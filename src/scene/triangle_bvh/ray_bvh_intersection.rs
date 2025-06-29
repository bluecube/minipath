use assert2::debug_assert;
use nalgebra::Unit;
use num_traits::zero;
use simba::simd::{SimdPartialOrd, SimdValue};

use super::{
    CompressedNodeLink, InnerNode, TriangleBvh, TriangleIdx, TrianglePackIdxRange,
    TriangleShadingData,
};
use crate::{
    geometry::{
        BarycentricCoordinates, FloatType, HitRecord, Ray, RayIntersectionExt as _, SimdFloatType,
        WorldBox, WorldBoxSized, WorldBoxSized8, WorldVector,
    },
    scene::Object,
    util::{
        bit_iter,
        simba::{fast_max, fast_min},
    },
};

#[derive(Clone, Default)]
#[repr(transparent)]
pub struct StackCache {
    stack: Vec<(CompressedNodeLink, WorldBoxSized, FloatType)>,
}

impl Object for TriangleBvh {
    fn intersect(&self, ray: &Ray, stack: &mut StackCache) -> Option<HitRecord> {
        debug_assert!(stack.stack.is_empty());
        stack.stack.push((
            self.root,
            (&self.bounding_box).into(),
            FloatType::NEG_INFINITY,
        ));

        let mut best = LeafHitRecord {
            t: FloatType::MAX,
            ..LeafHitRecord::default()
        };

        while let Some((link, enclosing_box, node_t1)) = stack.stack.pop() {
            if node_t1 > best.t {
                // If the node's minimum intersection distance is further away than the best
                // hit found so far, the node can't do any good any more and we can skip it.
                continue;
            }

            let enclosing_box = WorldBoxSized8::splat(enclosing_box);

            match link.decode() {
                super::NodeLink::Null => continue,
                super::NodeLink::Inner { index } => {
                    let node = &self.inner_nodes[index];
                    for (link, bb, t1) in node.intersect(ray, &enclosing_box, best.t) {
                        // TODO: Perf: Sort based on t1 before inserting, lower t1 should be added last
                        // (= first to be popped, because it has the best chance of decreasing nearest_t)
                        stack.stack.push((link, bb, t1));
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

            let tex = vertex_shading_data.map(|d| d.texture_coords);
            let normal = Unit::new_normalize(if flat_shading {
                best.geometric_normal
            } else {
                let normals = vertex_shading_data.map(|d| d.normal);
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
        enclosing_box: &WorldBoxSized8,
        max_t: f32,
    ) -> LeafHitRecord {
        let max_t = SimdFloatType::splat(max_t);

        let mut best = LeafHitRecord {
            t: FloatType::INFINITY,
            ..LeafHitRecord::default()
        };

        for (j, triangles) in triangle_indices
            .iter()
            .zip(self.triangle_geometry[triangle_indices.into_range()].iter())
        {
            let triangles = triangles.decompress(enclosing_box);
            let (mask, t, uv) = triangles.intersect(ray);

            let mask = (mask & t.simd_ge(zero()) & t.simd_le(max_t)).0.move_mask() as u64;

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
        enclosing_box: &WorldBoxSized8,
        max_t: f32,
    ) -> impl Iterator<Item = (CompressedNodeLink, WorldBoxSized, FloatType)> {
        let boxes = self.child_bounds.decompress(enclosing_box);
        let (t1, t2) = boxes.intersect(ray);
        let t1 = fast_max(t1, SimdFloatType::ZERO);
        let t2 = fast_min(t2, SimdFloatType::splat(max_t));
        let boxes: WorldBoxSized8 = (&boxes).into();
        let mask = t1.simd_le(t2).0.move_mask() as u64;

        // TODO: Perf: Maybe a non-fancy bit_iter that just goes 0..8 could be faster because of inlining
        bit_iter(mask).map(move |i| (self.child_links[i], boxes.extract(i), t1.extract(i)))
    }
}

#[derive(Clone, Debug, Default)]
struct LeafHitRecord {
    t: FloatType,
    uv: BarycentricCoordinates<FloatType>,
    geometric_normal: WorldVector,
    triangle_index: TriangleIdx,
}
