use std::{array, borrow::Borrow, fs, path::Path};

use crate::{
    geometry::{TexturePoint, Triangle, WorldBox, WorldBox8, WorldPoint, WorldPoint8, WorldVector},
    util::{collect_to_array, simba::simd_windows},
};

use arrayvec::ArrayVec;
use indexmap::IndexMap;
use itertools::Itertools as _;
use morton_encoding::morton_encode;
use simba::simd::{SimdValue, WideF32x8};
use thiserror::Error;

use super::{
    INNER_NODE_CHILDREN, InnerNode, LEAF_NODE_TRIANGLES, LEAF_NODE_VERTICES, LeafGeometry,
    LeafShadingData, NodeLink, TriangleBvh,
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
        let bounding_box = get_aabb(vertices_iter(&triangles, &vertices)).unwrap_or_default();

        let mut bvh = TriangleBvh {
            bounding_box: bounding_box.clone(),
            root: NodeLink::default(),

            inner_node_arena: Vec::new(),
            leaf_geometry_arena: Vec::new(),
            leaf_shading_data_arena: Vec::new(),
        };

        morton_sort(&mut triangles, &vertices);
        bvh.root = bvh.build_recursive(&mut triangles, &vertices, &bounding_box);

        bvh
    }

    fn build_recursive(
        &mut self,
        triangles: &mut [Triangle<usize>],
        vertices: &[VertexData],
        enclosing_box: &WorldBox,
    ) -> NodeLink {
        self.build_leaf(triangles, vertices, enclosing_box)
            .unwrap_or_else(|| self.build_inner_node(triangles, vertices, enclosing_box))
    }

    fn build_inner_node(
        &mut self,
        triangles: &mut [Triangle<usize>],
        vertices: &[VertexData],
        enclosing_box: &WorldBox,
    ) -> NodeLink {
        let split_indices = array::from_fn::<_, { INNER_NODE_CHILDREN + 1 }, _>(|i| {
            i * triangles.len() / INNER_NODE_CHILDREN
        });

        // TODO: Perf: Optimize the split
        // Be careful, that any optimization will destroy the morton ordering and we will need to re-sort.

        // Index of the node that we will be adding
        let node_index = self.inner_node_arena.len();

        // Create placeholder node that will be overwriten later
        self.inner_node_arena.push(InnerNode::default());

        let mut child_boxes = WorldBox8::default();
        for (i, (index1, index2)) in split_indices.iter().tuple_windows().enumerate() {
            let triangles = &mut triangles[*index1..*index2];
            if let Some(child_box) = get_aabb(vertices_iter(triangles, vertices)) {
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
        self.inner_node_arena[node_index] = InnerNode {
            child_bounds: compressed_child_boxes,
            child_links,
        };

        NodeLink::new_inner(node_index)
    }

    fn build_leaf(
        &mut self,
        triangles: &[Triangle<usize>],
        vertices: &[VertexData],
        enclosing_box: &WorldBox,
    ) -> Option<NodeLink> {
        if triangles.len() > LEAF_NODE_TRIANGLES {
            return None;
        }

        let enclosing_box = WorldBox8::splat(enclosing_box.clone());

        let link = NodeLink::new_leaf(self.leaf_geometry_arena.len());

        let geometry = collect_to_array(
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

        // Mapping of vertex indices -- values are indices into the `vertices` slice,
        // position determines the position in current leaf (= the index in current leaf)
        let mut vertex_mapping = ArrayVec::<usize, LEAF_NODE_VERTICES>::new();
        let mut too_many_vertices = false;

        let vertex_indices = collect_to_array(triangles.iter().map(|t| {
            t.map(|source_index| {
                vertex_mapping
                    .iter()
                    .position(|x| source_index == x)
                    .unwrap_or_else(|| {
                        let mapped_index = vertex_mapping.len();
                        vertex_mapping.try_push(*source_index).unwrap_or_else(|_| {
                            too_many_vertices = true;
                        });
                        mapped_index
                    })
            })
        }));

        if too_many_vertices {
            return None;
        }

        let flat_shading = collect_to_array(
            triangles
                .iter()
                .map(|t| t.iter().any(|i| vertices[*i].normal.norm() == 0.0)),
        );

        let normals = collect_to_array(vertex_mapping.iter().map(|index| vertices[*index].normal));
        let texture_coords =
            collect_to_array(vertex_mapping.iter().map(|index| vertices[*index].tex));

        self.leaf_geometry_arena.push(LeafGeometry {
            triangles: geometry,
        });
        self.leaf_shading_data_arena.push(LeafShadingData {
            material: 0, // TODO
            vertex_indices,
            flat_shading,

            normals,
            texture_coords,
        });
        Some(link)
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

    let Some(bounds) = get_aabb(centroids_iter(triangles, vertices)) else {
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

fn get_aabb<I>(points: I) -> Option<WorldBox>
where
    I: IntoIterator,
    I::Item: Borrow<WorldPoint>,
{
    let mut it = points.into_iter();
    let first = it.next()?.borrow().clone();
    Some(it.fold(WorldBox::new(first, first), |acc, p| {
        WorldBox::new(acc.min.inf(p.borrow()), acc.max.sup(p.borrow()))
    }))
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
