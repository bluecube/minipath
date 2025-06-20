use std::ops::{Index, IndexMut, Mul, Sub};

use nalgebra::{
    ClosedAddAssign, ClosedDivAssign, ClosedMulAssign, ClosedSubAssign, DefaultAllocator, DimName,
    OPoint, OVector, Scalar, allocator::Allocator,
};
use num_traits::{One, Zero};
use simba::{scalar::ClosedAdd, simd::SimdValue};

#[derive(Clone, Debug)]
pub struct Triangle<Point>([Point; 3]);

impl<Point> Triangle<Point> {
    pub fn new(a: Point, b: Point, c: Point) -> Triangle<Point> {
        Triangle([a, b, c])
    }

    pub fn iter<'a>(&'a self) -> impl Iterator<Item = &'a Point> {
        self.0.iter()
    }

    pub fn len(&self) -> usize {
        3
    }
}

impl<Point: Default> Default for Triangle<Point> {
    fn default() -> Self {
        Triangle([Default::default(), Default::default(), Default::default()])
    }
}

impl<Point> Index<usize> for Triangle<Point> {
    type Output = Point;

    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}

impl<Point> IndexMut<usize> for Triangle<Point> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.0[index]
    }
}

impl<Point> Triangle<Point> {
    pub fn map<Point2, F: FnMut(&Point) -> Point2>(&self, mut f: F) -> Triangle<Point2> {
        Triangle([f(&self[0]), f(&self[1]), f(&self[2])])
    }

    pub fn zip_map<Point2, Point3, F: FnMut(&Point, &Point2) -> Point3>(
        &self,
        rhs: &Triangle<Point2>,
        mut f: F,
    ) -> Triangle<Point3> {
        Triangle([
            f(&self.0[0], &rhs.0[0]),
            f(&self.0[1], &rhs.0[1]),
            f(&self.0[2], &rhs.0[2]),
        ])
    }

    pub fn zip_apply<Point2, F: FnMut(&mut Point, &Point2)>(
        &mut self,
        rhs: &Triangle<Point2>,
        mut f: F,
    ) {
        f(&mut self.0[0], &rhs.0[0]);
        f(&mut self.0[1], &rhs.0[1]);
        f(&mut self.0[2], &rhs.0[2]);
    }
}

impl<T: Scalar, D: DimName> Triangle<OPoint<T, D>>
where
    DefaultAllocator: Allocator<D>,
{
    pub fn map_coords<T2: Scalar, F: FnMut(T) -> T2>(&self, mut f: F) -> Triangle<OPoint<T2, D>> {
        self.map(|x| x.map(&mut f))
    }

    pub fn zip_map_coords<T2: Scalar, T3: Scalar, F: FnMut(T, T2) -> T3>(
        &self,
        rhs: &Triangle<OPoint<T2, D>>,
        mut f: F,
    ) -> Triangle<OPoint<T3, D>> {
        self.zip_map(rhs, |x, y| OPoint {
            coords: x.coords.zip_map(&y.coords, &mut f),
        })
    }

    pub fn zip_apply_coords<T2: Scalar, F: FnMut(&mut T, T2)>(
        &mut self,
        rhs: &Triangle<OPoint<T2, D>>,
        mut f: F,
    ) {
        self.zip_apply(rhs, |x, y| x.coords.zip_apply(&y.coords, &mut f))
    }
}

impl<T: Scalar, D: DimName> Triangle<OPoint<T, D>>
where
    DefaultAllocator: Allocator<D>,
    T: ClosedAddAssign + ClosedDivAssign + Zero + From<u16>,
{
    pub fn centroid(&self) -> OPoint<T, D> {
        OPoint {
            coords: self.0.iter().map(|p| &p.coords).sum::<OVector<T, D>>()
                / T::from(self.0.len() as u16),
        }
    }
}

impl<T: Scalar, D: DimName> Triangle<OPoint<T, D>>
where
    DefaultAllocator: Allocator<D>,
    for<'a> &'a OPoint<T, D>: Sub<Output = OVector<T, D>>,
{
    /// Returns edge vectors, coming from self[0]
    pub fn edges(&self) -> [OVector<T, D>; 2] {
        [&self.0[1] - &self.0[0], &self.0[2] - &self.0[0]]
    }
}

impl<T: Scalar, D: DimName> Triangle<OPoint<T, D>>
where
    DefaultAllocator: Allocator<D>,
    for<'a> &'a OPoint<T, D>: Sub<Output = OVector<T, D>>,
    T: ClosedAddAssign + ClosedSubAssign + ClosedMulAssign,
{
    /// Returns a normal vector of the triangle, not normalized.
    pub fn normal(&self) -> OVector<T, D> {
        let [e1, e2] = self.edges();
        e1.cross(&e2)
    }
}

impl<T: SimdValue + Scalar, D: DimName> SimdValue for Triangle<OPoint<T, D>>
where
    T::Element: Scalar,
    DefaultAllocator: Allocator<D>,
{
    const LANES: usize = T::LANES;
    type Element = Triangle<OPoint<T::Element, D>>;
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
