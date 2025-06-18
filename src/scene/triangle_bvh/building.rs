use std::{array, fs, path::Path};

use crate::{
    geometry::{TexturePoint, Triangle, WorldBox, WorldBox8, WorldPoint, WorldPoint8, WorldVector},
    scene::triangle_bvh::TriangleShadingData,
    util::simba::simd_windows,
};

use index_vec::IndexVec;
use indexmap::IndexMap;
use itertools::Itertools as _;
use morton_encoding::morton_encode;
use simba::simd::{SimdValue, WideF32x8};
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

        morton_sort(&mut triangles, &vertices);
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

        let mut child_boxes = WorldBox8::default();
        for (i, (index1, index2)) in split_indices.iter().tuple_windows().enumerate() {
            let triangles = &mut triangles[*index1..*index2];
            if let Some(child_box) = WorldBox::from_points(vertices_iter(triangles, vertices)) {
                child_boxes.replace(i, child_box);
            }
        }
        let enclosing_box = WorldBox8::splat(enclosing_box.clone());
        let compressed_child_boxes = RelativeBox8::compress_round_out(child_boxes, &enclosing_box);
        // Compression is lossy, so the bounding box will change.
        // We have to use the decompressed value for the children, same as what is used when
        // traversing the tree
        let decompressed_child_boxes = compressed_child_boxes.decompress(&enclosing_box);

        // Insert the children
        let child_links = array::from_fn(|i| {
            let triangles = &mut triangles[split_indices[i]..split_indices[i + 1]];
            self.build_recursive(triangles, vertices, &decompressed_child_boxes.extract(i))
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
        let enclosing_box = WorldBox8::splat(enclosing_box.clone());

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
                    .map(|t: &Triangle<usize>| t.map(|i| vertices[*i].pos.clone())),
            )
            .map(
                |(t, mask): (Triangle<WorldPoint8>, <WideF32x8 as SimdValue>::SimdBool)| {
                    let masked: Triangle<WorldPoint8> = t.map(|p| {
                        p.coords
                            .zip_map(&enclosing_box.min.coords, |x, box_min| {
                                x.select(mask, box_min)
                            })
                            .into()
                    });
                    RelativeTriangle8::compress(&masked, &enclosing_box)
                },
            ),
        );

        self.triangle_shading_data
            .extend(triangles.iter().map(|t| TriangleShadingData {
                vertex_indices: t.clone(),
                flat_shading: t.iter().any(|i| vertices[*i].normal.norm_squared() == 0.0),
                material: 0,
            }));
        self.triangle_shading_data
            .extend((0..padding).into_iter().map(|_| Default::default()));

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

fn morton_sort(triangles: &mut [Triangle<usize>], vertices: &[VertexData]) {
    const GRID_BITS: usize = 10;

    let Some(bounds) = WorldBox::from_points(centroids_iter(triangles, vertices)) else {
        return;
    };
    let min = bounds.min;
    let scale = WorldVector::repeat((1 >> GRID_BITS) as f32).component_div(&bounds.size());

    // TODO: Perf: sort_unstable? is caching helpful?
    triangles.sort_by_cached_key(|triangle| {
        let centroid = triangle_centroid(triangle, vertices);
        let grid_coordinates: [u32; 3] = (centroid - min)
            .component_mul(&scale)
            .map(|x| x.round() as u32)
            .into();

        morton_encode(grid_coordinates)
    });
}

fn triangle_centroid(triangle: &Triangle<usize>, vertices: &[VertexData]) -> WorldPoint {
    WorldPoint {
        coords: triangle
            .iter()
            .map(|i| vertices[*i].pos.coords)
            .sum::<WorldVector>()
            / (triangle.len() as f32),
    }
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

/// Iterates over centroids of indexed triangles
fn centroids_iter(
    triangles: &[Triangle<usize>],
    vertices: &[VertexData],
) -> impl Iterator<Item = WorldPoint> {
    triangles
        .iter()
        .map(|triangle| triangle_centroid(triangle, vertices))
}

/// Reorder the triangles and return an array of indices in the triangle array, where the
/// output bins should be split. Array is one larger than INNER_NODE_CHILDREN, first item is always 0,
/// last item is always triangles.size().
fn split_triangles(
    triangles: &mut [Triangle<usize>],
    _vertices: &[VertexData],
    _enclosing_box: &WorldBox,
) -> [usize; INNER_NODE_CHILDREN + 1] {
    // const TARGET_BIN_COUNT: usize = 128;
    // let bin_size = (enclosing_box.volume() / (TARGET_BIN_COUNT as f32)).cbrt();
    // let bin_counts = (enclosing_box.size() / bin_size).map(|x| x.ceil() as usize);
    // dbg!(enclosing_box);
    // dbg!(bin_size);
    // dbg!(bin_counts);

    array::from_fn::<_, { INNER_NODE_CHILDREN + 1 }, _>(|i| {
        i * triangles.len() / INNER_NODE_CHILDREN
    })
}
