use num_traits::One;
use std::ops::{Add, Sub};

use nalgebra::{
    ClosedAddAssign, ClosedDivAssign, DefaultAllocator, DimName, OPoint, Point, Point2, Scalar,
    allocator::Allocator,
};

use simba::simd::SimdValue;

#[derive(Clone, Debug, Default)]
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

impl<T: SimdValue + Scalar, D: DimName> SimdValue for AABB<OPoint<T, D>>
where
    T::Element: SimdValue + Scalar,
    DefaultAllocator: Allocator<D>,
{
    const LANES: usize = T::LANES;

    type Element = AABB<OPoint<T::Element, D>>;

    type SimdBool = T::SimdBool;

    fn splat(val: Self::Element) -> Self {
        val.map_coords(|x| T::splat(x))
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
