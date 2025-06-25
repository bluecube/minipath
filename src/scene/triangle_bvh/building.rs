use std::{array, fs, ops::Range, path::Path};

use crate::{
    geometry::{
        AABB, SimdMaskType, TexturePoint, Triangle, WorldBox, WorldBoxSized8, WorldPoint,
        WorldPoint8, WorldVector,
    },
    scene::triangle_bvh::TriangleShadingData,
    util::simba::simd_windows,
};

use arrayvec::ArrayVec;
use index_vec::IndexVec;
use indexmap::IndexMap;
use itertools::Itertools as _;
use nalgebra::Vector3;
use simba::simd::SimdValue as _;
use thiserror::Error;

use super::{
    CompressedNodeLink, INNER_NODE_CHILDREN, InnerNode, LEAF_NODE_MAX_TRIANGLES,
    LEAF_NODE_PACKET_SIZE, TriangleBvh,
    compressed_geometry::{RelativeBox8, RelativeTriangle8},
};

impl TriangleBvh {
    pub fn with_obj(p: impl AsRef<Path>) -> Result<TriangleBvh, ObjOpenError> {
        let content = fs::read_to_string(p)?;
        let parsed = wavefront_obj::obj::parse(content)?;

        let (triangles, vertices) = Self::load_obj(parsed);

        Ok(Self::build(triangles, vertices))
    }

    fn load_obj(obj: wavefront_obj::obj::ObjSet) -> (Vec<Triangle<usize>>, Vec<VertexData>) {
        let mut triangles = Vec::new();
        let mut vertices = IndexMap::new();

        for o in obj.objects.into_iter() {
            for geometry in o.geometry {
                for shape in geometry.shapes {
                    let wavefront_obj::obj::Primitive::Triangle(a, b, c) = shape.primitive else {
                        println!("non-triangle primitive!");
                        continue;
                    };

                    let mut handle_vertex = |vtindex: (usize, Option<usize>, Option<usize>)| {
                        let entry = vertices.entry(vtindex);
                        let index = entry.index();
                        entry.or_insert_with(|| {
                            let vertex = &o.vertices[vtindex.0];
                            let tex_vertex = vtindex.1.map(|i| &o.tex_vertices[i]);
                            let normal = vtindex.2.map(|i| &o.normals[i]);
                            VertexData {
                                pos: WorldPoint::new(
                                    vertex.x as f32,
                                    vertex.y as f32,
                                    vertex.z as f32,
                                ),
                                tex: tex_vertex.map_or_else(TexturePoint::origin, |v| {
                                    TexturePoint::new(v.u as f32, v.v as f32, v.w as f32)
                                }),
                                normal: normal.map_or_else(WorldVector::zeros, |v| {
                                    WorldVector::new(v.x as f32, v.y as f32, v.z as f32).normalize()
                                }),
                            }
                        });
                        index
                    };

                    let a = handle_vertex(a);
                    let b = handle_vertex(b);
                    let c = handle_vertex(c);

                    triangles.push(Triangle::new(a, b, c));
                }
            }
        }

        (triangles, vertices.into_iter().map(|(_k, v)| v).collect())
    }

    pub fn build(mut triangles: Vec<Triangle<usize>>, vertices: Vec<VertexData>) -> TriangleBvh {
        let bounding_box =
            WorldBox::from_points(vertices_iter(&triangles, &vertices)).unwrap_or_default();

        let mut bvh = TriangleBvh {
            bounding_box: bounding_box.clone(),
            root: CompressedNodeLink::default(),

            inner_nodes: IndexVec::new(),
            triangle_geometry: IndexVec::new(),
            triangle_shading_data: IndexVec::new(),
            vertex_data: IndexVec::new(),
        };

        bvh.root = bvh.build_recursive(&mut triangles, &vertices, &bounding_box);
        bvh.vertex_data = vertices
            .into_iter()
            .map(|v| super::VertexShadingData {
                normal: v.normal,
                texture_coords: v.tex,
            })
            .collect();

        bvh
    }

    fn build_recursive(
        &mut self,
        triangles: &mut [Triangle<usize>],
        vertices: &[VertexData],
        enclosing_box: &WorldBox,
    ) -> CompressedNodeLink {
        if triangles.len() <= LEAF_NODE_MAX_TRIANGLES {
            self.build_leaf(triangles, vertices, enclosing_box)
        } else {
            self.build_inner_node(triangles, vertices, enclosing_box)
        }
    }

    fn build_inner_node(
        &mut self,
        triangles: &mut [Triangle<usize>],
        vertices: &[VertexData],
        enclosing_box: &WorldBox,
    ) -> CompressedNodeLink {
        let split_indices = split_triangles(triangles, vertices, enclosing_box);

        // Create placeholder node that will be overwriten later
        self.inner_nodes.push(InnerNode::default());
        let node_index = self.inner_nodes.last_idx();

        let enclosing_box = WorldBoxSized8::splat(enclosing_box.into());
        let compressed_child_boxes = simd_windows(
            split_indices
                .iter()
                .map(|(_range, child_box)| child_box.clone()),
        )
        .map(|(child_boxes, mask)| {
            RelativeBox8::compress_round_out(child_boxes, &enclosing_box, &mask)
        })
        .exactly_one()
        .unwrap_or_else(|_| unreachable!());

        // Compression is lossy, so the bounding box will change.
        // We have to use the decompressed value for the children, same as what is used when
        // traversing the tree
        let decompressed_child_boxes = compressed_child_boxes.decompress(&enclosing_box);

        // Insert the children
        let child_links = array::from_fn(|i| {
            if let Some((range, _child_box)) = split_indices.get(i) {
                let triangles = &mut triangles[range.clone()];
                self.build_recursive(triangles, vertices, &decompressed_child_boxes.extract(i))
            } else {
                CompressedNodeLink::NULL
            }
        });

        // Replace the placeholder with an actual inner node
        self.inner_nodes[node_index] = InnerNode {
            child_bounds: compressed_child_boxes,
            child_links,
        };

        CompressedNodeLink::new_inner(node_index)
    }

    fn build_leaf(
        &mut self,
        triangles: &[Triangle<usize>],
        vertices: &[VertexData],
        enclosing_box: &WorldBox,
    ) -> CompressedNodeLink {
        let enclosing_box = WorldBoxSized8::splat(enclosing_box.into());

        assert!(!triangles.is_empty());
        let packet_count = triangles.len().div_ceil(LEAF_NODE_PACKET_SIZE);
        let padded_triangle_count = packet_count * LEAF_NODE_PACKET_SIZE;
        let padding = padded_triangle_count - triangles.len();

        let link =
            CompressedNodeLink::new_leaf(self.triangle_geometry.next_idx(), packet_count as u32);

        self.triangle_geometry.extend(
            simd_windows(
                triangles
                    .iter()
                    .map(|t: &Triangle<usize>| t.map(|i| vertices[*i].pos)),
            )
            .map(|(t, mask): (Triangle<WorldPoint8>, SimdMaskType)| {
                RelativeTriangle8::compress(&t, &enclosing_box, &mask)
            }),
        );

        self.triangle_shading_data
            .extend(triangles.iter().map(|t| TriangleShadingData {
                vertex_indices: t.clone(),
                flat_shading: t.iter().any(|i| vertices[*i].normal.norm_squared() == 0.0),
                material: 0,
            }));
        self.triangle_shading_data
            .extend((0..padding).map(|_| Default::default()));

        link
    }
}

#[derive(Debug, Error)]
pub enum ObjOpenError {
    #[error("Failed to read file: {0}")]
    ReadError(#[from] std::io::Error),

    #[error("Failed to parse file: {0}")]
    ParseError(#[from] wavefront_obj::ParseError),
}

/// Per-vertex data of the model.
pub struct VertexData {
    pos: WorldPoint,
    tex: TexturePoint,
    normal: WorldVector,
}

/// Iterates over vertices of indexed triangles
fn vertices_iter<'a>(
    triangles: &[Triangle<usize>],
    vertices: &'a [VertexData],
) -> impl Iterator<Item = &'a WorldPoint> {
    triangles
        .iter()
        .flat_map(|t| t.iter())
        .map(|i| &vertices[*i].pos)
}

/// Reorders the triangles and returns index range and a bounding box for child of the node.
fn split_triangles(
    triangles: &mut [Triangle<usize>],
    vertices: &[VertexData],
    enclosing_box: &WorldBox,
) -> ArrayVec<(Range<usize>, WorldBox), INNER_NODE_CHILDREN> {
    let bin_count = (triangles.len() / 64).clamp(128, 1024);
    let bin_grid = BinGrid::with_approximate_bin_count(enclosing_box.clone(), bin_count);

    let mut bins = Vec::new();
    bins.extend((0..bin_grid.bin_count()).map(|i| SplittingBin {
        parent: i,
        ..Default::default()
    }));

    for triangle in triangles.iter() {
        let triangle = triangle.map(|i| vertices[*i].pos);
        let centroid = triangle.centroid();

        let bin = &mut bins[bin_grid.bin_index(&centroid)];
        bin.bounding_box.extend_points(triangle.iter());
        bin.count += 1;
    }

    let mut groups: Vec<_> = bins
        .iter()
        .filter(|bin| bin.count > 0)
        .map(Clone::clone)
        .collect();

    // We can't merge any more if there's only two groups
    while groups.len() > 2 {
        let (i1, i2, sah_improvement) = find_best_bin_merge(&groups);

        if sah_improvement < 0.0 && groups.len() <= INNER_NODE_CHILDREN {
            // If the best merge is disadvantageous and we already have
            // small enough number of children to fit in a node, stop merging
            break;
        }

        let g1 = &groups[i1];
        let g2 = &groups[i2];
        bins[g2.parent].parent = g1.parent;
        let merged = g1.merge(g2);
        groups[i1] = merged;
        groups.swap_remove(i2);
    }

    triangles.sort_unstable_by_key(|triangle| {
        let centroid = triangle.map(|i| vertices[*i].pos).centroid();
        let mut i = bin_grid.bin_index(&centroid);

        // Disjoint-set data structure

        let mut root = i;
        while bins[root].parent != root {
            root = bins[root].parent;
        }

        while bins[i].parent != i {
            let tmp = bins[i].parent;
            bins[i].parent = root;
            i = tmp;
        }

        root
    });

    let chunked_triangles = triangles
        .iter()
        .map(|triangle| triangle.map(|i| vertices[*i].pos))
        .chunk_by(|triangle| {
            let centroid = triangle.centroid();
            let grid_index = bin_grid.bin_index(&centroid);
            bins[grid_index].parent
        });
    chunked_triangles
        .into_iter()
        .map(|(_k, mut chunk)| {
            let first_triangle = chunk.next().unwrap();
            chunk.fold(
                (
                    1usize,
                    WorldBox::from_points(first_triangle.iter()).unwrap(),
                ),
                |(chunk_count, mut chunk_box), triangle| {
                    chunk_box.extend_points(triangle.iter());
                    (chunk_count + 1, chunk_box)
                },
            )
        })
        .scan(0usize, |state, (chunk_count, chunk_box)| {
            let chunk_offset = *state;
            *state += chunk_count;
            let result = (chunk_offset..(chunk_offset + chunk_count), chunk_box);
            Some(result)
        })
        .collect()
}

#[derive(Clone, Debug, Default)]
struct SplittingBin {
    bounding_box: WorldBox,
    count: usize,
    /// Index of bin that represents this bin or group
    parent: usize,
}

impl SplittingBin {
    /// Evaluate surface area heuristic component for a single box.
    /// The value is scaled relative to the parent box.
    fn sah(&self) -> f32 {
        // TODO: Perf: This is just a second stab at what the node traversal cost might look like.
        // It performs better than plain area * self.count, but not by much and behaves weirdly with
        // changes in C_LEAF_PACKET. Investigate more.

        const B: f32 = INNER_NODE_CHILDREN as f32;

        const C_INNER: f32 = 1.0;
        const C_LEAF_PACKET: f32 = 0.75;

        let packet_count = self.count.div_ceil(LEAF_NODE_PACKET_SIZE);

        let leaf_cost = if packet_count <= CompressedNodeLink::MAX_COUNT as usize {
            C_LEAF_PACKET * packet_count as f32
        } else {
            f32::INFINITY
        };

        let packet_count = packet_count as f32;
        let depth = packet_count.log(B).floor();
        let tree_cost =
            C_INNER * depth + C_LEAF_PACKET * (packet_count / B.powi(depth as i32)).ceil();

        self.bounding_box.surface_area() * leaf_cost.min(tree_cost)
    }

    fn merge(&self, other: &SplittingBin) -> SplittingBin {
        SplittingBin {
            bounding_box: AABB::union(&self.bounding_box, &other.bounding_box),
            count: self.count + other.count,
            parent: self.parent,
        }
    }
}

fn find_best_bin_merge(groups: &[SplittingBin]) -> (usize, usize, f32) {
    let mut best_i1 = 0;
    let mut best_i2 = 0;
    let mut best_sah_improvement = f32::NEG_INFINITY;
    for i1 in 0..groups.len() {
        for i2 in (i1 + 1)..groups.len() {
            let g1 = &groups[i1];
            let g2 = &groups[i2];
            let merged_sah = g1.merge(g2).sah();
            let sah_improvement = g1.sah() + g2.sah() - merged_sah;

            if sah_improvement > best_sah_improvement {
                best_i1 = i1;
                best_i2 = i2;
                best_sah_improvement = sah_improvement;
            }
        }
    }

    (best_i1, best_i2, best_sah_improvement)
}

struct BinGrid {
    enclosing_box: WorldBox,
    bin_size: f32,
    bin_counts: Vector3<usize>,
}

impl BinGrid {
    pub fn with_approximate_bin_count(enclosing_box: WorldBox, bin_count: usize) -> Self {
        let bin_size = (enclosing_box.volume() / (bin_count as f32)).cbrt();
        Self::with_bin_size(enclosing_box, bin_size)
    }

    pub fn with_bin_size(enclosing_box: WorldBox, bin_size: f32) -> Self {
        let bin_counts = (enclosing_box.size() / bin_size).map(|x| x.ceil() as usize);

        BinGrid {
            enclosing_box,
            bin_size,
            bin_counts,
        }
    }

    pub fn bin_count(&self) -> usize {
        self.bin_counts.product()
    }

    pub fn bin_coords(&self, p: &WorldPoint) -> Vector3<usize> {
        (p - self.enclosing_box.min).map(|x| (x / self.bin_size).floor() as usize)
    }

    pub fn bin_index(&self, p: &WorldPoint) -> usize {
        let coords = self.bin_coords(p);
        coords.x + coords.y * self.bin_counts.x + coords.z * self.bin_counts.xy().product()
    }
}
