use crate::geometry::{Ray, SimdFloatType, Triangle8};

use simba::simd::{SimdPartialOrd as _, SimdValue};

impl Triangle8 {
    /// Calculates ray intersection with the (two sided) triangle pack.
    /// Returns mask of valid intersections, distance along ray, and barycentric uv coordinates.
    /// Adapted from https://en.wikipedia.org/wiki/M%C3%B6ller%E2%80%93Trumbore_intersection_algorithm#Rust_implementation
    pub fn intersect(
        &self,
        ray: &Ray,
    ) -> (
        <SimdFloatType as SimdValue>::SimdBool,
        SimdFloatType,
        SimdFloatType,
        SimdFloatType,
    ) {
        let origin = ray.origin.map(|x| SimdFloatType::splat(x));
        let direction = ray.direction.map(|x| SimdFloatType::splat(x));

        let ray_cross_e1 = direction.cross(&self.e[1]);
        let det = self.e[0].dot(&ray_cross_e1);

        let inv_det = SimdFloatType::ONE / det; // May be infinite
        let s = origin - self.a;
        let u = inv_det * s.dot(&ray_cross_e1);

        let s_cross_e0 = s.cross(&self.e[0]);
        let v = inv_det * direction.dot(&s_cross_e0);
        let t = inv_det * self.e[1].dot(&s_cross_e0);

        let mask = u.simd_ge(SimdFloatType::ZERO)
            & v.simd_ge(SimdFloatType::ZERO)
            & (u + v).simd_le(SimdFloatType::ONE);
        (mask, t, u, v)
    }
}
