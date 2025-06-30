use num_traits::One;
use ordered_float::FloatCore;
use std::{
    borrow::Borrow,
    ops::{Add, Sub},
};

use nalgebra::{
    ClosedAddAssign, ClosedDivAssign, DefaultAllocator, DimName, OPoint, OVector, Point, Point2,
    Scalar, allocator::Allocator,
};

use simba::simd::SimdValue;

use crate::util::simba::{SimbaWorkarounds as _, fast_max, fast_min};

use super::{Ray, SimdFloatType, WorldPoint, WorldPoint8};

#[derive(Clone, Debug)]
pub struct AABB<Point> {
    pub min: Point,
    pub max: Point,
}

impl<Point> AABB<Point> {
    pub fn new(min: Point, max: Point) -> AABB<Point> {
        AABB { min, max }
    }

    pub fn with_size<S>(min: Point, size: &S) -> AABB<Point>
    where
        for<'a> &'a Point: Add<&'a S, Output = Point>,
    {
        let max = &min + size;
        AABB { min, max }
    }

    pub fn map<Point2, F: FnMut(&Point) -> Point2>(&self, mut f: F) -> AABB<Point2> {
        AABB {
            min: f(&self.min),
            max: f(&self.max),
        }
    }

    pub fn zip_map<Point2, Point3, F: FnMut(&Point, &Point2) -> Point3>(
        &self,
        rhs: &AABB<Point2>,
        mut f: F,
    ) -> AABB<Point3> {
        AABB {
            min: f(&self.min, &rhs.min),
            max: f(&self.max, &rhs.max),
        }
    }

    // pub fn apply<F: FnMut(&mut Point)>(&mut self, mut f: F) {
    //     f(&mut self.min);
    //     f(&mut self.max);
    // }

    pub fn zip_apply<Point2, F: FnMut(&mut Point, &Point2)>(
        &mut self,
        rhs: &AABB<Point2>,
        mut f: F,
    ) {
        f(&mut self.min, &rhs.min);
        f(&mut self.max, &rhs.max);
    }
}

impl<T: Scalar, D: DimName> AABB<OPoint<T, D>>
where
    DefaultAllocator: Allocator<D>,
{
    pub fn map_coords<T2: Scalar, F: FnMut(T) -> T2>(&self, mut f: F) -> AABB<OPoint<T2, D>> {
        self.map(|x| x.map(&mut f))
    }

    pub fn zip_map_coords<T2: Scalar, T3: Scalar, F: FnMut(T, T2) -> T3>(
        &self,
        rhs: &AABB<OPoint<T2, D>>,
        mut f: F,
    ) -> AABB<OPoint<T3, D>> {
        self.zip_map(rhs, |x, y| OPoint {
            coords: x.coords.zip_map(&y.coords, &mut f),
        })
    }

    pub fn zip_apply_coords<T2: Scalar, F: FnMut(&mut T, T2)>(
        &mut self,
        rhs: &AABB<OPoint<T2, D>>,
        mut f: F,
    ) {
        self.zip_apply(rhs, |x, y| x.coords.zip_apply(&y.coords, &mut f))
    }
}

impl<Point: Sub + Copy> AABB<Point> {
    pub fn size(&self) -> Point::Output {
        self.max - self.min
    }
}

impl<T: Scalar + Copy + Sub> AABB<Point2<T>> {
    pub fn width(&self) -> T::Output {
        self.max[0] - self.min[0]
    }

    pub fn height(&self) -> T::Output {
        self.max[1] - self.min[1]
    }
}

impl<T: Scalar + ClosedAddAssign + ClosedDivAssign + One, const D: usize> AABB<Point<T, D>> {
    pub fn center(&self) -> Point<T, D> {
        let two = T::one() + T::one();
        let avg_coords = (&self.min.coords + &self.max.coords) / two;
        Point::from(avg_coords)
    }
}

impl<Point> From<[Point; 2]> for AABB<Point> {
    fn from(value: [Point; 2]) -> Self {
        let [min, max] = value;
        AABB { min, max }
    }
}

impl<Point> From<(Point, Point)> for AABB<Point> {
    fn from(value: (Point, Point)) -> Self {
        let (min, max) = value;
        AABB { min, max }
    }
}

impl<T, D> Default for AABB<OPoint<T, D>>
where
    D: DimName,
    DefaultAllocator: Allocator<D>,
    T: Scalar + SimdValue,
    T::Element: num_traits::float::FloatCore,
{
    fn default() -> Self {
        Self {
            min: OPoint {
                coords: OVector::repeat(T::splat(T::Element::infinity())),
            },
            max: OPoint {
                coords: OVector::repeat(T::splat(T::Element::neg_infinity())),
            },
        }
    }
}

impl<T: SimdValue + Scalar, D: DimName> SimdValue for AABB<OPoint<T, D>>
where
    T::Element: SimdValue + Scalar,
    DefaultAllocator: Allocator<D>,
{
    const LANES: usize = T::LANES;

    type Element = AABB<OPoint<T::Element, D>>;

    type SimdBool = T::SimdBool;

    fn splat(val: Self::Element) -> Self {
        val.map_coords(T::splat)
    }

    fn extract(&self, i: usize) -> Self::Element {
        self.map_coords(|x| x.extract(i))
    }

    unsafe fn extract_unchecked(&self, i: usize) -> Self::Element {
        unsafe { self.map_coords(|x| x.extract_unchecked(i)) }
    }

    fn replace(&mut self, i: usize, val: Self::Element) {
        self.zip_apply_coords(&val, |x, y| x.replace(i, y.clone()));
    }

    unsafe fn replace_unchecked(&mut self, i: usize, val: Self::Element) {
        unsafe {
            self.zip_apply_coords(&val, |x, y| x.replace_unchecked(i, y.clone()));
        }
    }

    fn select(self, cond: Self::SimdBool, other: Self) -> Self {
        self.zip_map_coords(&other, |x, y| x.select(cond, y.clone()))
    }
}

impl<T, D: DimName> AABB<OPoint<T, D>>
where
    DefaultAllocator: Allocator<D>,
    T: Scalar + nalgebra::SimdPartialOrd,
{
    pub fn intersection(&self, other: &AABB<OPoint<T, D>>) -> AABB<OPoint<T, D>> {
        AABB {
            min: self.min.sup(&other.min),
            max: self.max.inf(&other.max),
        }
    }

    pub fn union(&self, other: &AABB<OPoint<T, D>>) -> AABB<OPoint<T, D>> {
        AABB {
            min: self.min.inf(&other.min),
            max: self.max.sup(&other.max),
        }
    }
}

impl AABB<WorldPoint> {
    pub fn extend_point(&mut self, p: &WorldPoint) {
        self.min = self.min.inf(p);
        self.max = self.max.sup(p);
    }

    pub fn extend_points<I>(&mut self, points: I)
    where
        I: IntoIterator,
        I::Item: Borrow<WorldPoint>,
    {
        for p in points.into_iter() {
            self.extend_point(p.borrow());
        }
    }

    pub fn from_points<I>(points: I) -> Option<AABB<WorldPoint>>
    where
        I: IntoIterator,
        I::Item: Borrow<WorldPoint>,
    {
        let mut it = points.into_iter();
        let first = *it.next()?.borrow();

        let mut b = AABB::new(first, first);
        b.extend_points(it);

        Some(b)
    }

    pub fn volume(&self) -> f32 {
        self.size().product()
    }

    pub fn surface_area(&self) -> f32 {
        let size = self.size();

        2.0 * (size.x * (size.y + size.z) + size.y * size.z)
    }
}

impl AABB<WorldPoint8> {
    /// Calculates first and last intersection point of a ray with a AABB pack.
    /// Intersection ray coordinate are clamped to the range [0, max_t]
    pub fn intersect(&self, ray: &Ray, max_t: SimdFloatType) -> (SimdFloatType, SimdFloatType) {
        let ray_origin = ray.origin.map(SimdFloatType::splat);
        let ray_inv_direction = ray.inv_direction.map(SimdFloatType::splat);

        // Componentwise distances along the ray to the box's min and max corners
        let to_box_min = (self.min - ray_origin)
            .component_mul(&ray_inv_direction)
            .map(|x| SimdFloatType::neg_infinity().select(x.is_nan(), x));
        let to_box_max = (self.max - ray_origin)
            .component_mul(&ray_inv_direction)
            .map(|x| SimdFloatType::infinity().select(x.is_nan(), x));

        // Correctly ordered (min_t <= max_t)
        let componentwise_min_t = to_box_min.zip_map(&to_box_max, fast_min);
        let componentwise_max_t = to_box_min.zip_map(&to_box_max, fast_max);

        let min_t = fast_max(
            fast_max(componentwise_min_t.x, SimdFloatType::ZERO),
            fast_max(componentwise_min_t.y, componentwise_min_t.z),
        );
        let max_t = fast_min(
            fast_min(componentwise_max_t.x, max_t),
            fast_min(componentwise_max_t.y, componentwise_max_t.z),
        );

        (min_t, max_t)
    }
}

#[derive(Debug, Clone)]
pub struct AABBSized<T, U> {
    pub min: T,
    pub size: U,
}

impl<T, U> From<&AABB<T>> for AABBSized<T, U>
where
    T: Clone,
    for<'a> &'a T: Sub<&'a T, Output = U>,
{
    fn from(value: &AABB<T>) -> Self {
        AABBSized {
            min: value.min.clone(),
            size: &value.max - &value.min,
        }
    }
}

impl<T, U> From<&AABBSized<T, U>> for AABB<T>
where
    T: Clone,
    for<'a> &'a T: Add<&'a U, Output = T>,
{
    fn from(value: &AABBSized<T, U>) -> Self {
        AABB {
            min: value.min.clone(),
            max: &value.min + &value.size,
        }
    }
}

impl<T: SimdValue + Scalar, D: DimName> SimdValue for AABBSized<OPoint<T, D>, OVector<T, D>>
where
    T::Element: SimdValue + Scalar,
    DefaultAllocator: Allocator<D>,
{
    const LANES: usize = T::LANES;

    type Element = AABBSized<OPoint<T::Element, D>, OVector<T::Element, D>>;

    type SimdBool = T::SimdBool;

    fn splat(val: Self::Element) -> Self {
        AABBSized {
            min: val.min.map(T::splat),
            size: val.size.map(T::splat),
        }
    }

    fn extract(&self, i: usize) -> Self::Element {
        AABBSized {
            min: self.min.map(|x| x.extract(i)),
            size: self.size.map(|x| x.extract(i)),
        }
    }

    unsafe fn extract_unchecked(&self, _i: usize) -> Self::Element {
        unimplemented!()
    }

    fn replace(&mut self, _i: usize, _val: Self::Element) {
        unimplemented!()
    }

    unsafe fn replace_unchecked(&mut self, _i: usize, _val: Self::Element) {
        unimplemented!()
    }

    fn select(self, _cond: Self::SimdBool, _other: Self) -> Self {
        unimplemented!()
    }
}

#[cfg(test)]
pub mod test {
    use assert2::assert;
    use simba::simd::{SimdBool as _, SimdPartialOrd as _};
    use test_case::{test_case, test_matrix};

    use super::*;

    use crate::{
        geometry::{Ray, WorldBox, WorldBox8, WorldPoint, WorldVector},
        util::simba::SimbaWorkarounds,
    };

    /// Checks cases when the ray hits the box, including some corner cases.
    #[test_matrix(
        [5.0, 7.0, 10.0],
        [5.0, 7.0, 10.0],
        [5.0, 7.0, 10.0],
        [-1.0, 0.0, 2.0],
        [-1.0, 0.0, 2.0],
        [-1.0, 0.0, 2.0],
        [0.0, 2.0, 5.0, 20.0]
    )]
    fn hit(px: f32, py: f32, pz: f32, dx: f32, dy: f32, dz: f32, origin_pos: f32) {
        if dx == 0.0 && dy == 0.0 && dz == 0.0 {
            return;
        }

        let b = WorldBox::new([5.0, 5.0, 5.0].into(), [10.0, 10.0, 10.0].into());
        let b_simd = WorldBox8::splat(b.clone());

        let p = WorldPoint::new(px, py, pz);
        let d = WorldVector::new(dx, dy, dz);
        let temp_r = Ray::new(p, d);
        let origin = temp_r.point_at(-origin_pos);
        let r = Ray::new(origin, d);

        let result = simd_result_to_scalar(b_simd.intersect(&r, SimdFloatType::infinity()));
        dbg!(result);
        let (t1, t2) =
            result.expect("The ray origin is in/on the box, we should always have an intersection");

        let p1 = r.point_at(t1);
        if t1 > 0.0 {
            assert!(point_is_on_box_surface(&p1, &b), "{p1:?} must be in {b:?}");
        } else {
            assert!(point_is_inside_box(&p1, &b), "{p1:?} must be in {b:?}");
        }

        let p2 = r.point_at(t2);
        assert!(point_is_on_box_surface(&p2, &b), "{p2:?} must be in {b:?}");
    }

    /// Asserts that all lanes have identical data and returns the intersection if one was found
    fn simd_result_to_scalar(result_simd: (SimdFloatType, SimdFloatType)) -> Option<(f32, f32)> {
        const TOLERANCE: f32 = 1e-3;

        let t1 = result_simd.0.extract(0);
        let t2 = result_simd.1.extract(0);
        assert!(result_simd.0.simd_eq(SimdFloatType::splat(t1)).all());
        assert!(result_simd.1.simd_eq(SimdFloatType::splat(t2)).all());

        if t1 <= t2 {
            Some((t1, t2))
        } else if t1 <= t2 + TOLERANCE {
            let t = (t1 + t2) / 2.0;
            Some((t, t))
        } else {
            None
        }
    }

    /// Just a manual example of ray grazing along an edge.
    #[test]
    fn hit_along_edge() {
        let b = WorldBox::new([5.0, 5.0, 5.0].into(), [10.0, 10.0, 10.0].into());
        let b_simd = WorldBox8::splat(b);

        let r = Ray::new(
            WorldPoint::new(5.0, 5.0, 0.0),
            WorldVector::new(0.0, 0.0, 1.0),
        );

        let result = simd_result_to_scalar(b_simd.intersect(&r, SimdFloatType::infinity()));

        assert!(result == Some((5.0, 10.0)))
    }

    /// Rays that lie parallel to one axis and start outside the corresponding slab
    /// must miss, even if they move toward the box on other axes or remain unchanged.
    #[test_case( 0.0,  7.0,  7.0,   0.0, 1.0, 0.0,   0.0 ; "low_x_parallel_miss")]
    #[test_case(12.0,  7.0,  7.0,   0.0, 1.0, 0.0,   0.0 ; "high_x_parallel_miss")]
    #[test_case( 7.0,  0.0,  7.0,   1.0, 0.0, 0.0,   0.0 ; "low_y_parallel_miss")]
    #[test_case( 7.0, 12.0,  7.0,   1.0, 0.0, 0.0,   0.0 ; "high_y_parallel_miss")]
    #[test_case( 7.0,  7.0,  0.0,   1.0, 0.0, 0.0,   0.0 ; "low_z_parallel_miss")]
    #[test_case( 7.0,  7.0, 12.0,   1.0, 0.0, 0.0,   0.0 ; "high_z_parallel_miss")]
    #[test_case( 0.0,  5.0,  7.0,   1.0, 0.0, 1.0,   0.0 ; "corner_miss")]
    #[test_case( 0.0,  0.0,  0.0,  -1.0, 1.0, 1.0,   0.0 ; "corner_miss2")]
    fn only_misses(px: f32, py: f32, pz: f32, dx: f32, dy: f32, dz: f32, origin_pos: f32) {
        let b = WorldBox::new([5.0, 5.0, 5.0].into(), [10.0, 10.0, 10.0].into());
        let b_simd = WorldBox8::splat(b);

        let p = WorldPoint::new(px, py, pz);
        let d = WorldVector::new(dx, dy, dz);
        let temp_r = Ray::new(p, d);
        let origin = temp_r.point_at(origin_pos);
        let r = Ray::new(origin, d);

        let result = simd_result_to_scalar(b_simd.intersect(&r, SimdFloatType::infinity()));

        assert!(result == None);
    }

    fn point_is_on_box_surface(p: &WorldPoint, b: &WorldBox) -> bool {
        const TOLERANCE: f32 = 1e-3;

        if !point_is_inside_box(p, b) {
            return false;
        }

        // Check if the point lies on any of the six faces (within tolerance)
        let on_x_face = ((p.x - b.min.x).abs() <= TOLERANCE || (p.x - b.max.x).abs() <= TOLERANCE)
            && (p.y >= b.min.y - TOLERANCE && p.y <= b.max.y + TOLERANCE)
            && (p.z >= b.min.z - TOLERANCE && p.z <= b.max.z + TOLERANCE);

        let on_y_face = ((p.y - b.min.y).abs() <= TOLERANCE || (p.y - b.max.y).abs() <= TOLERANCE)
            && (p.x >= b.min.x - TOLERANCE && p.x <= b.max.x + TOLERANCE)
            && (p.z >= b.min.z - TOLERANCE && p.z <= b.max.z + TOLERANCE);

        let on_z_face = ((p.z - b.min.z).abs() <= TOLERANCE || (p.z - b.max.z).abs() <= TOLERANCE)
            && (p.x >= b.min.x - TOLERANCE && p.x <= b.max.x + TOLERANCE)
            && (p.y >= b.min.y - TOLERANCE && p.y <= b.max.y + TOLERANCE);

        on_x_face || on_y_face || on_z_face
    }

    fn point_is_inside_box(p: &WorldPoint, b: &WorldBox) -> bool {
        const TOLERANCE: f32 = 1e-3;

        // Check if point is within the box's bounds (inclusive, with tolerance)
        let inside_x = p.x >= b.min.x - TOLERANCE && p.x <= b.max.x + TOLERANCE;
        let inside_y = p.y >= b.min.y - TOLERANCE && p.y <= b.max.y + TOLERANCE;
        let inside_z = p.z >= b.min.z - TOLERANCE && p.z <= b.max.z + TOLERANCE;

        inside_x && inside_y && inside_z
    }
}
