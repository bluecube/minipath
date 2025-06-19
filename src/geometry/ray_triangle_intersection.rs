use crate::{
    geometry::{Ray, SimdFloatType},
    util::simba::fma_dot,
};

use simba::simd::{SimdPartialOrd as _, SimdValue};

use super::{BarycentricCoordinates, Triangle, WorldPoint8};

impl Triangle<WorldPoint8> {
    /// Calculates ray intersection with the (two sided) triangle pack.
    /// Returns mask of valid intersections, distance along ray, and barycentric uv coordinates.
    /// Adapted from https://en.wikipedia.org/wiki/M%C3%B6ller%E2%80%93Trumbore_intersection_algorithm#Rust_implementation
    pub fn intersect(
        &self,
        ray: &Ray,
    ) -> (
        <SimdFloatType as SimdValue>::SimdBool,
        SimdFloatType,
        BarycentricCoordinates<SimdFloatType>,
    ) {
        let origin = ray.origin.map(|x| SimdFloatType::splat(x));
        let direction = ray.direction.map(|x| SimdFloatType::splat(x));

        let e1 = self[1] - self[0];
        let e2 = self[2] - self[0];

        let ray_cross_e2 = direction.cross(&e2);
        let det = fma_dot(&e1, &ray_cross_e2);

        let inv_det = SimdFloatType::ONE / det; // May be infinite
        let s = origin - self[0];
        let u = inv_det * fma_dot(&s, &ray_cross_e2);

        let s_cross_e1 = s.cross(&e1);
        let v = inv_det * fma_dot(&direction, &s_cross_e1);
        let t = inv_det * fma_dot(&e2, &s_cross_e1);

        let mask = u.simd_ge(SimdFloatType::ZERO)
            & v.simd_ge(SimdFloatType::ZERO)
            & (u + v).simd_le(SimdFloatType::ONE);
        (mask, t, BarycentricCoordinates { u, v })
    }
}
