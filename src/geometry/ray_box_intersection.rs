use simba::simd::SimdValue;

use crate::{
    geometry::{Ray, WorldBox8},
    util::simba::{SimbaWorkarounds as _, fast_max, fast_min},
};

use super::SimdFloatType;

pub trait RayIntersectionExt {
    type DistanceType;
    /// Calculate first and last ray intersection with the box
    fn intersect(&self, ray: &Ray) -> (Self::DistanceType, Self::DistanceType);
}

impl RayIntersectionExt for WorldBox8 {
    type DistanceType = SimdFloatType;

    /// Calculates ray intersection with the box pack.
    /// Returns minimum and maximum distance along the ray, ray intersects is min <= max.
    fn intersect(&self, ray: &Ray) -> (SimdFloatType, SimdFloatType) {
        let ray_origin = ray.origin.map(|x| SimdFloatType::splat(x));
        let ray_inv_direction = ray.inv_direction.map(|x| SimdFloatType::splat(x));

        // Componentwise distances along the ray to the box's min and max corners
        // TODO: Perf: Try storing pre-multiplied ray origin in the ray, use FMA (mul_add)
        // The multiplication is NAN if the ray is starting inside the slab bounding plane
        // and is parallel to it. In this case we blend to +-infinity, so that the range becomes infinite
        let to_box_min = (self.min - ray_origin)
            .component_mul(&ray_inv_direction)
            .map(|x| SimdFloatType::neg_infinity().select(x.is_nan(), x));
        let to_box_max = (self.max - ray_origin)
            .component_mul(&ray_inv_direction)
            .map(|x| SimdFloatType::infinity().select(x.is_nan(), x));

        // Correctly ordered (min_t <= max_t)
        let componentwise_min_t = to_box_min.zip_map(&to_box_max, |a, b| fast_min(a, b));
        let componentwise_max_t = to_box_min.zip_map(&to_box_max, |a, b| fast_max(a, b));

        let min_t = fast_max(
            componentwise_min_t.x,
            fast_max(componentwise_min_t.y, componentwise_min_t.z),
        );
        let max_t = fast_min(
            componentwise_max_t.x,
            fast_min(componentwise_max_t.y, componentwise_max_t.z),
        );

        (min_t, max_t)
    }
}

#[cfg(test)]
pub mod test {
    use assert2::assert;
    use simba::simd::{SimdBool as _, SimdPartialOrd as _};
    use test_case::{test_case, test_matrix};

    use super::*;

    use crate::geometry::{Ray, WorldBox, WorldBox8, WorldPoint, WorldVector};

    /// Checks cases when the ray hits the box, including some corner cases.
    #[test_matrix(
        [5.0, 7.0, 10.0],
        [5.0, 7.0, 10.0],
        [5.0, 7.0, 10.0],
        [-1.0, 0.0, 2.0],
        [-1.0, 0.0, 2.0],
        [-1.0, 0.0, 2.0],
        [-10.0, -1.0, 0.0, 2.0, 5.0, 20.0]
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
        let origin = temp_r.point_at(origin_pos);
        let r = Ray::new(origin, d);

        let result = simd_result_to_scalar(b_simd.intersect(&r));

        let (t1, t2) =
            result.expect("The ray origin is in/on the box, we should always have an intersection");

        let p1 = r.point_at(t1);
        let p2 = r.point_at(t2);

        assert!(point_is_on_box_surface(&p1, &b), "{p1:?} must be in {b:?}");
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

        let result = simd_result_to_scalar(b_simd.intersect(&r));

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

        let result = simd_result_to_scalar(b_simd.intersect(&r));

        assert!(result == None);
    }
    fn point_is_on_box_surface(p: &WorldPoint, b: &WorldBox) -> bool {
        const TOLERANCE: f32 = 1e-3;

        // Check if point is within the box's bounds (inclusive, with tolerance)
        let inside_x = p.x >= b.min.x - TOLERANCE && p.x <= b.max.x + TOLERANCE;
        let inside_y = p.y >= b.min.y - TOLERANCE && p.y <= b.max.y + TOLERANCE;
        let inside_z = p.z >= b.min.z - TOLERANCE && p.z <= b.max.z + TOLERANCE;

        if !(inside_x && inside_y && inside_z) {
            return false; // outside the box entirely
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
}
