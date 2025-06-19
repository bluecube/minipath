use nalgebra::Unit;
use num_traits::zero;
use simba::simd::{SimdBool as _, SimdPartialOrd as _, SimdValue};

use super::{
    CompressedNodeLink, InnerNode, TriangleBvh, TriangleIdx, TrianglePackIdxRange,
    TriangleShadingData,
};
use crate::{
    geometry::{
        BarycentricCoordinates, FloatType, HitRecord, Ray, RayIntersectionExt as _, SimdFloatType,
        WorldBox, WorldBox8, WorldVector,
    },
    scene::Object,
    util::bit_iter,
};

impl Object for TriangleBvh {
    fn intersect(&self, ray: &Ray) -> Option<HitRecord> {
        let mut stack = vec![(self.root, self.bounding_box.clone())];

        let mut best = LeafHitRecord {
            t: FloatType::MAX,
            ..LeafHitRecord::default()
        };

        while let Some((link, enclosing_box)) = stack.pop() {
            // TODO: Perf: max_t might have decreased since we added this node to the queue -- is it worth re-checking box collision again?
            // For this the queue should also hold the box's t2 to make the check quick
            match link.decode() {
                super::NodeLink::Null => continue,
                super::NodeLink::Inner { index } => {
                    let node = &self.inner_nodes[index];
                    for (_t1, _t2, link, bb) in node.intersect(ray, &enclosing_box, best.t) {
                        // TODO: Perf: Sort based on t1 before inserting, lower t1 should be added last
                        // (= first to be popped, because it has the best chance of decreasing nearest_t)
                        stack.push((link, bb));
                    }
                }
                super::NodeLink::Leaf { indices } => {
                    let hit = self.intersect_triangles(indices, ray, &enclosing_box, best.t);

                    if hit.t < best.t {
                        best = hit;
                    }
                }
            }
        }

        if best.triangle_index == TriangleIdx::default() {
            None
        } else {
            let TriangleShadingData {
                ref vertex_indices,
                flat_shading,
                material,
            } = self.triangle_shading_data[best.triangle_index];
            let vertex_shading_data = vertex_indices.map(|i| &self.vertex_data[*i]);

            let tex = vertex_shading_data.map(|d| (*d).texture_coords);
            let normal = Unit::new_normalize(if flat_shading {
                best.geometric_normal
            } else {
                let normals = vertex_shading_data.map(|d| (*d).normal);
                best.uv.interpolate_triangle(&normals)
            });
            let texture_coords = best
                .uv
                .interpolate(&tex[0].coords, &tex[1].coords, &tex[2].coords)
                .into();

            Some(HitRecord {
                t: best.t,
                point: ray.point_at(best.t),
                normal,
                material,
                texture_coords,
            })
        }
    }

    fn get_bounding_box(&self) -> WorldBox {
        self.bounding_box.clone()
    }
}

impl TriangleBvh {
    fn intersect_triangles(
        &self,
        triangle_indices: TrianglePackIdxRange,
        ray: &Ray,
        enclosing_box: &WorldBox,
        max_t: f32,
    ) -> LeafHitRecord {
        let enclosing_box = WorldBox8::splat(enclosing_box.clone());
        let max_t = SimdFloatType::splat(max_t);

        let mut best = LeafHitRecord {
            t: FloatType::INFINITY,
            ..LeafHitRecord::default()
        };

        for (j, triangles) in triangle_indices
            .iter()
            .zip(self.triangle_geometry[triangle_indices.into_range()].iter())
        {
            let triangles = triangles.decompress(&enclosing_box);
            let (mask, t, uv) = triangles.intersect(ray);

            let mask = (mask & t.simd_ge(zero()) & t.simd_le(max_t)).bitmask();

            // TODO: Perf: Maybe a non-fancy bit_iter that just goes 0..8 could be faster because of inlining
            for i in bit_iter(mask) {
                let t = t.extract(i);
                if t < best.t {
                    best.t = t;
                    best.triangle_index = j.to_triangle_idx(i);
                    best.uv = uv.extract(i);
                    best.geometric_normal = triangles.extract(i).normal();
                }
            }
        }

        best
    }
}

impl InnerNode {
    /// Intersect this inner node with a ray.
    /// Returns an iterator of intersecting child boxes:
    /// (t1, t2, link to the child, Bounding box of the child)
    /// t1 and t2 are coordinates along the ray that are within min_t and max_t and where box intersects.
    /// All returned points have t1 < t2 (= nonempty intersection).
    fn intersect(
        &self,
        ray: &Ray,
        enclosing_box: &WorldBox,
        max_t: f32,
    ) -> impl Iterator<Item = (f32, f32, CompressedNodeLink, WorldBox)> {
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

#[derive(Clone, Debug, Default)]
struct LeafHitRecord {
    t: FloatType,
    uv: BarycentricCoordinates<FloatType>,
    geometric_normal: WorldVector,
    triangle_index: TriangleIdx,
}
