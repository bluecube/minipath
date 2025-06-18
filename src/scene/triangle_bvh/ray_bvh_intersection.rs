use nalgebra::Unit;
use num_traits::zero;
use simba::simd::{SimdBool as _, SimdPartialOrd as _, SimdValue};

use super::{InnerNode, LeafGeometry, NodeLink, TriangleBvh};
use crate::{
    geometry::{
        BarycentricCoordinates, HitRecord, Ray, RayIntersectionExt as _, SimdFloatType,
        TexturePoint, WorldBox, WorldBox8, WorldVector,
    },
    scene::Object,
    util::bit_iter,
};

impl Object for TriangleBvh {
    fn intersect(&self, ray: &Ray) -> Option<HitRecord> {
        let mut queue = vec![(self.root, self.bounding_box.clone())];

        let mut nearest_t = f32::INFINITY;
        let mut nearest_uv = Default::default();
        let mut nearest_normal = Default::default();
        let mut nearest_leaf_index = usize::MAX;
        let mut nearest_triangle_index: usize = 0;

        while let Some((link, enclosing_box)) = queue.pop() {
            // TODO: Perf: max_t might have decreased since we added this node to the queue -- is it worth re-checking box collision again?
            // For this the queue should also hold the box's t2 to make the check quick
            let index = link.index();
            if link.is_leaf() {
                let node = &self.leaf_geometry_arena[index];
                let (t, uv, normal, i) = node.intersect(ray, &enclosing_box, nearest_t);

                if t < nearest_t {
                    nearest_t = t;
                    nearest_uv = uv;
                    nearest_normal = normal;
                    nearest_leaf_index = index;
                    nearest_triangle_index = i;
                }
            } else {
                let node = &self.inner_node_arena[index];
                for (_t1, _t2, link, bb) in node.intersect(ray, &enclosing_box, nearest_t) {
                    // TODO: Perf: Sort based on t1 before inserting, lower t1 should be added last
                    // (= first to be popped, because it has the best chance of decreasing nearest_t)
                    queue.push((link, bb));
                }
            }
        }

        if nearest_leaf_index == usize::MAX {
            None
        } else {
            let shading_data = &self.leaf_shading_data_arena[nearest_leaf_index];

            let vertex_indices = &shading_data.vertex_indices[nearest_triangle_index];

            let tex = vertex_indices.map(|i| shading_data.texture_coords[*i]);

            let normal =
                Unit::new_normalize(if shading_data.flat_shading[nearest_triangle_index] {
                    nearest_normal
                } else {
                    let normals = vertex_indices.map(|i| shading_data.normals[*i]);
                    nearest_uv.interpolate_triangle(&normals)
                });

            let texture_coordinates = TexturePoint {
                coords: nearest_uv.interpolate(&tex[0].coords, &tex[1].coords, &tex[2].coords),
            };

            Some(HitRecord {
                t: nearest_t,
                point: ray.point_at(nearest_t),
                normal,
                material: shading_data.material,
                texture_coords: texture_coordinates,
            })
        }
    }

    fn get_bounding_box(&self) -> WorldBox {
        self.bounding_box.clone()
    }
}

impl InnerNode {
    /// Intersect this inne node with a ray.
    /// Returns an iterator of intersecting child boxes:
    /// (t1, t2, link to the child, Bounding box of the child)
    /// t1 and t2 are coordinates along the ray that are within min_t and max_t and where box intersects.
    /// All returned points have t1 < t2 (= nonempty intersection).
    fn intersect(
        &self,
        ray: &Ray,
        enclosing_box: &WorldBox,
        max_t: f32,
    ) -> impl Iterator<Item = (f32, f32, NodeLink, WorldBox)> {
        let boxes = self
            .child_bounds
            .decompress(&enclosing_box.map_coords(|x| SimdFloatType::splat(x)));
        let (t1, t2) = boxes.intersect(ray);
        // TODO: Perf: wide types support fast_min and fast_max which disregard NaNs.
        // Since we know that there will not be any, we could use that -- verify if it is worth it
        let t1 = t1.simd_max(SimdFloatType::ZERO);
        let t2 = t2.simd_min(SimdFloatType::splat(max_t));
        let mask = t1.simd_le(t2).bitmask();

        // TODO: Perf: Maybe a non-fancy bit_iter that just goes 0..8 could be faster because of inlining
        bit_iter(mask).map(move |i| {
            (
                t1.extract(i),
                t2.extract(i),
                self.child_links[i],
                boxes.extract(i),
            )
        })
    }
}

impl LeafGeometry {
    /// Returns intersection distance, triangle barycentric coords and index of the triangle in current leaf
    fn intersect(
        &self,
        ray: &Ray,
        enclosing_box: &WorldBox,
        max_t: f32,
    ) -> (f32, BarycentricCoordinates<f32>, WorldVector, usize) {
        let enclosing_box = WorldBox8::splat(enclosing_box.clone());
        let max_t = SimdFloatType::splat(max_t);
        let mut nearest_t: f32 = f32::INFINITY;
        let mut nearest_uv: BarycentricCoordinates<f32> = Default::default();
        let mut nearest_normal = Default::default();
        let mut nearest_i: usize = 0;

        for (j, triangles) in self.triangles.iter().enumerate() {
            let triangles = triangles.decompress(&enclosing_box);
            let (mask, t, uv) = triangles.intersect(ray);

            let mask = (mask & t.simd_ge(zero()) & t.simd_le(max_t)).bitmask();

            // TODO: Perf: Maybe a non-fancy bit_iter that just goes 0..8 could be faster because of inlining
            for i in bit_iter(mask) {
                let t = t.extract(i);
                if t < nearest_t {
                    nearest_i = i + 8 * j;
                    nearest_t = t;
                    nearest_uv = uv.extract(i);
                    nearest_normal = triangles.extract(i).normal();
                }
            }
        }

        (nearest_t, nearest_uv, nearest_normal, nearest_i)
    }
}
