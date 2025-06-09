mod aabb;
mod ray_box_intersection;
mod ray_triangle_intersection;

use nalgebra::Unit;

pub use aabb::AABB;
pub use ray_box_intersection::RayIntersectionExt;

pub type FloatType = f32;
pub type SimdFloatType = simba::simd::WideF32x8;

/// Error tolerance for general purpose calculations in the raytracer.
/// This is not the same as machine epsilon (FloatType::EPSILON).
pub const EPSILON: FloatType = 1e-6;

pub type ScreenPoint = nalgebra::Point2<u32>;
pub type ScreenSize = nalgebra::Vector2<u32>;
pub type ScreenBlock = AABB<ScreenPoint>;

pub type WorldPoint = nalgebra::Point3<FloatType>;
pub type WorldVector = nalgebra::Vector3<FloatType>;
pub type WorldBox = AABB<WorldPoint>;
pub type WorldPoint8 = nalgebra::Point3<SimdFloatType>;
pub type WorldVector8 = nalgebra::Vector3<SimdFloatType>;
pub type WorldBox8 = AABB<WorldPoint8>;

pub type TexturePoint = nalgebra::Point2<f32>;

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
pub struct Intersection {
    /// Position along the ray
    pub t: f32,
    /// Point where the ray hit the geometry
    pub point: WorldPoint,
    /// Normalized normal vector
    pub normal: WorldVector,
    pub material: u32,
    pub texture_coordinates: TexturePoint,
}

pub struct Triangle8 {
    pub a: WorldPoint8,
    pub e: [WorldVector8; 2],
}

impl Triangle8 {
    fn new(p: &[WorldPoint8; 3]) -> Triangle8 {
        Triangle8 {
            a: p[0],
            e: [p[1] - p[0], p[2] - p[0]],
        }
    }
}
