use assert2::debug_assert;
use nalgebra::Unit;
use num_traits::zero;
use simba::simd::{SimdPartialOrd, SimdValue};

use super::{
    CompressedNodeLink8, InnerNode, InnerNodeIdx, TriangleBvh, TriangleIdx, TrianglePackIdxRange,
    TriangleShadingData,
};
use crate::{
    geometry::{
        BarycentricCoordinates, FloatType, HitRecord, Ray, RayIntersectionExt as _, SimdFloatType,
        SimdMaskType, WorldBox, WorldBoxSized, WorldBoxSized8, WorldVector,
    },
    scene::{Object, triangle_bvh::TrianglePackIdx},
    util::{
        Stats, bit_iter,
        simba::{fast_max, fast_min},
    },
};

#[derive(Clone, Default)]
pub struct StackCache {
    stack: Vec<(InnerNodeIdx, WorldBoxSized, FloatType)>,

    effective_branching_stats: Stats,
    max_stack_depth: usize,
}

impl StackCache {
    pub fn print_stats(&self) {
        println!(
            "Children visited per node: {}",
            self.effective_branching_stats
        );
        println!("Max stack depth: {}", self.max_stack_depth);
    }
}

impl Object for TriangleBvh {
    fn intersect(&self, ray: &Ray, stack: &mut StackCache) -> Option<HitRecord> {
        debug_assert!(stack.stack.is_empty());

        let sized_box: WorldBoxSized = (&self.bounding_box).into();

        let best = match self.root {
            crate::scene::triangle_bvh::NodeLink::Null => LeafHitRecord {
                t: FloatType::MAX,
                ..LeafHitRecord::default()
            },
            crate::scene::triangle_bvh::NodeLink::Inner { index } => {
                self.intersect_recursive(index, sized_box, ray, stack)
            }
            crate::scene::triangle_bvh::NodeLink::Leaf { indices } => {
                self.intersect_leaf(indices, sized_box, ray, FloatType::MAX)
            }
        };

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
    fn intersect_recursive(
        &self,
        root_index: InnerNodeIdx,
        enclosing_box: WorldBoxSized,
        ray: &Ray,
        stack: &mut StackCache,
    ) -> LeafHitRecord {
        stack
            .stack
            .push((root_index, enclosing_box, FloatType::NEG_INFINITY));

        let mut best = LeafHitRecord {
            t: FloatType::MAX,
            ..LeafHitRecord::default()
        };

        while let Some((node_index, enclosing_box, node_t1)) = stack.stack.pop() {
            if node_t1 > best.t {
                // If the node's minimum intersection distance is further away than the best
                // hit found so far, the node can't do any good any more and we can skip it.
                continue;
            }

            let enclosing_box = WorldBoxSized8::splat(enclosing_box);

            let node = &self.inner_nodes[node_index];
            let intersection_result = node.intersect_inner(ray, &enclosing_box, best.t);

            for (triangle_indices, enclosing_box) in intersection_result.leaf_node_child_iter() {
                let hit = self.intersect_leaf(triangle_indices, enclosing_box, ray, best.t);

                if hit.t < best.t {
                    best = hit;
                }
            }

            let saved_index = stack.stack.len();
            stack
                .stack
                .extend(intersection_result.inner_node_child_iter());
            stack.stack[saved_index..].sort_unstable_by(|a, b| b.2.total_cmp(&a.2));
            // stack.stack.sort_unstable_by(|a, b| b.2.total_cmp(&a.2));

            // Statistics
            stack
                .effective_branching_stats
                .add_sample(stack.stack.len() - saved_index);
            stack.max_stack_depth = stack.max_stack_depth.max(stack.stack.len());
        }

        best
    }

    fn intersect_leaf(
        &self,
        triangle_indices: TrianglePackIdxRange,
        enclosing_box: WorldBoxSized,
        ray: &Ray,
        max_t: f32,
    ) -> LeafHitRecord {
        let enclosing_box = WorldBoxSized8::splat(enclosing_box);
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
    fn intersect_inner(
        &self,
        ray: &Ray,
        enclosing_box: &WorldBoxSized8,
        max_t: f32,
    ) -> InnerNodeIntersectResult {
        let boxes = self.child_bounds.decompress(enclosing_box);
        let (t1, t2) = boxes.intersect(ray);
        let t1 = fast_max(t1, SimdFloatType::ZERO);
        let t2 = fast_min(t2, SimdFloatType::splat(max_t));

        InnerNodeIntersectResult {
            t1,
            boxes: (&boxes).into(),
            child_links: self.child_links,

            mask: t1.simd_le(t2),
        }
    }
}

struct InnerNodeIntersectResult {
    t1: SimdFloatType,
    boxes: WorldBoxSized8,
    child_links: CompressedNodeLink8,

    mask: SimdMaskType,
}

impl InnerNodeIntersectResult {
    fn inner_node_child_iter(
        &self,
    ) -> impl Iterator<Item = (InnerNodeIdx, WorldBoxSized, FloatType)> {
        let indices = self.child_links.index();
        let mask = self.mask & self.child_links.inner_node_mask();
        let bitmask = mask.0.move_mask() as u64;

        // TODO: Perf: Maybe a non-fancy bit_iter that just goes 0..8 could be faster because of inlining
        bit_iter(bitmask).map(move |i| {
            (
                InnerNodeIdx::from_raw(indices.as_array_ref()[i]),
                self.boxes.extract(i),
                self.t1.extract(i),
            )
        })
    }

    fn leaf_node_child_iter(&self) -> impl Iterator<Item = (TrianglePackIdxRange, WorldBoxSized)> {
        let indices = self.child_links.index();
        let counts = self.child_links.count();
        let mask = self.mask & self.child_links.leaf_node_mask();
        let bitmask = mask.0.move_mask() as u64;

        // TODO: Perf: Maybe a non-fancy bit_iter that just goes 0..8 could be faster because of inlining
        bit_iter(bitmask).map(move |i| {
            let first = TrianglePackIdx::from_raw(indices.as_array_ref()[i]);
            (
                TrianglePackIdxRange {
                    first,
                    last: first + counts.as_array_ref()[i] as usize,
                },
                self.boxes.extract(i),
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
