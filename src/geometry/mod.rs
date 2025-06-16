mod aabb;
mod ray_box_intersection;
mod ray_triangle_intersection;
mod triangle;

use std::ops::{Mul, Sub};

use nalgebra::{Point2, Point3, Unit, Vector2, Vector3};

pub use aabb::AABB;
use num_traits::One;
pub use ray_box_intersection::RayIntersectionExt;
use simba::{scalar::ClosedAdd, simd::SimdValue};
pub use triangle::Triangle;

pub type FloatType = f32;
pub type SimdFloatType = simba::simd::WideF32x8;

/// Error tolerance for general purpose calculations in the raytracer.
/// This is not the same as machine epsilon (FloatType::EPSILON).
pub const EPSILON: FloatType = 1e-6;

pub type ScreenPoint = Point2<u32>;
pub type ScreenSize = Vector2<u32>;
pub type ScreenBlock = AABB<ScreenPoint>;

pub type WorldPoint = Point3<FloatType>;
pub type WorldVector = Vector3<FloatType>;
pub type WorldBox = AABB<WorldPoint>;
pub type WorldPoint8 = Point3<SimdFloatType>;
pub type WorldVector8 = Vector3<SimdFloatType>;
pub type WorldBox8 = AABB<WorldPoint8>;

pub type TexturePoint = Point3<f32>;

/// Ray going through the world. Only positive direction is considered to be on the ray.
#[derive(Copy, Clone, Debug)]
pub struct Ray {
    pub origin: WorldPoint,
    /// Normalized direction of the ray
    pub direction: Unit<WorldVector>,

    /// Componentwise inverse of the ray direction
    /// Zeros in direction get turned into positive infinity regardless of the sign of the zero
    pub inv_direction: WorldVector,
}

impl Ray {
    pub fn new(origin: WorldPoint, direction: WorldVector) -> Ray {
        let direction = Unit::new_normalize(direction);
        let inv_direction = direction.map(|x| if x == 0.0 { f32::INFINITY } else { 1.0 / x });

        Ray {
            origin,
            direction,
            inv_direction,
        }
    }

    pub fn point_at(&self, distance: f32) -> WorldPoint {
        self.origin + self.direction.as_ref() * distance
    }

    pub fn advance_by(&self, distance: f32) -> Ray {
        Ray {
            origin: self.point_at(distance),
            direction: self.direction,
            inv_direction: self.inv_direction,
        }
    }
}

/// Intersection of ray and scene
#[derive(Copy, Clone, Debug)]
pub struct HitRecord {
    /// Position along the ray
    pub t: f32,
    /// Point where the ray hit the geometry
    pub point: WorldPoint,
    /// Normalized normal vector
    pub normal: Unit<WorldVector>,
    pub material: u32,
    pub texture_coordinates: TexturePoint,
}

#[derive(Copy, Clone, Debug, Default)]
pub struct BarycentricCoordinates<T: SimdValue> {
    pub u: T,
    pub v: T,
}

impl<T> BarycentricCoordinates<T>
where
    T: SimdValue + One + Copy + Sub<Output = T>,
{
    pub fn interpolate<T2>(&self, a: &T2, b: &T2, c: &T2) -> T2
    where
        for<'a> &'a T2: Mul<T, Output = T2>,
        T2: ClosedAdd,
    {
        let w = T::one() - self.u - self.v;
        a * w + b * self.u + c * self.v
    }

    pub fn interpolate_triangle<T2>(&self, triangle: &Triangle<T2>) -> T2
    where
        for<'a> &'a T2: Mul<T, Output = T2>,
        T2: ClosedAdd,
    {
        self.interpolate(&triangle[0], &triangle[1], &triangle[2])
    }
}

impl<T: SimdValue> SimdValue for BarycentricCoordinates<T> {
    const LANES: usize = T::LANES;

    type Element = BarycentricCoordinates<T::Element>;

    type SimdBool = T::SimdBool;

    fn splat(val: Self::Element) -> Self {
        BarycentricCoordinates {
            u: T::splat(val.u),
            v: T::splat(val.v),
        }
    }

    fn extract(&self, i: usize) -> Self::Element {
        BarycentricCoordinates {
            u: self.u.extract(i),
            v: self.v.extract(i),
        }
    }

    unsafe fn extract_unchecked(&self, i: usize) -> Self::Element {
        unsafe {
            BarycentricCoordinates {
                u: self.u.extract_unchecked(i),
                v: self.v.extract_unchecked(i),
            }
        }
    }

    fn replace(&mut self, i: usize, val: Self::Element) {
        self.u.replace(i, val.u);
        self.v.replace(i, val.v);
    }

    unsafe fn replace_unchecked(&mut self, i: usize, val: Self::Element) {
        unsafe {
            self.u.replace_unchecked(i, val.u);
            self.v.replace_unchecked(i, val.v);
        }
    }

    fn select(self, cond: Self::SimdBool, other: Self) -> Self {
        BarycentricCoordinates {
            u: self.u.select(cond, other.u),
            v: self.v.select(cond, other.v),
        }
    }
}
