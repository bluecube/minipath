mod building;
mod compressed_geometry;
mod printing;
mod ray_bvh_intersection;

use compressed_geometry::{RelativeBox8, RelativeTriangle8};

use crate::geometry::{TexturePoint, Triangle, WorldBox, WorldVector};

const INNER_NODE_CHILDREN: usize = 8;
const LEAF_NODE_TRIANGLES: usize = 16;
const LEAF_NODE_VERTICES: usize = 16;

#[derive(Clone, Debug)]
pub struct TriangleBvh {
    bounding_box: WorldBox,
    root: NodeLink,

    inner_node_arena: Vec<InnerNode>,
    leaf_geometry_arena: Vec<LeafGeometry>,
    leaf_shading_data_arena: Vec<LeafShadingData>,
    //material_arena: Vec<Material>,
}

#[derive(Clone, Debug, Default)]
struct InnerNode {
    child_bounds: RelativeBox8,
    child_links: [NodeLink; 8],
}

/// Contains data needed to calculate intersection distance with a ray.
/// After an intersection is found, additional data is found in LeafNodeShadingData
#[derive(Clone, Debug)]
struct LeafGeometry {
    // TODO: Perf: Check if 32 triangles per leaf is better or worse.
    triangles: [RelativeTriangle8; 2],
}

/// Additional data for triangle shading.
/// normals and texture_coords are indexed using LeafNodeGeomerty::vertex_indices
#[derive(Clone, Debug, Default)]
struct LeafShadingData {
    material: usize,

    /// Vertex indices -- Fields correspond to triangles in LeafGeometry,
    /// indices point into normals and texture_coords of this struct.
    vertex_indices: [Triangle<usize>; LEAF_NODE_TRIANGLES],
    flat_shading: [bool; LEAF_NODE_TRIANGLES],

    normals: [WorldVector; LEAF_NODE_VERTICES],
    texture_coords: [TexturePoint; LEAF_NODE_VERTICES],
}

#[derive(Copy, Clone, Debug, Default)]
#[repr(transparent)]
struct NodeLink(u32);

impl NodeLink {
    const LEAF_MASK: u32 = 1 << 31;
    const MAX_INDEX: usize = (Self::LEAF_MASK - 1) as usize;

    /// Create a new size tag, panics if size is out of range (> 127)
    fn new_leaf(index: usize) -> Self {
        assert!(index <= Self::MAX_INDEX);
        Self((index as u32) | Self::LEAF_MASK)
    }

    fn new_inner(index: usize) -> Self {
        assert!(index < Self::MAX_INDEX);
        Self(index as u32)
    }

    fn index(&self) -> usize {
        (self.0 & (!Self::LEAF_MASK)) as usize
    }

    fn is_leaf(&self) -> bool {
        (self.0 & Self::LEAF_MASK) != 0
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use assert2::assert;
    use test_strategy::proptest;

    #[proptest]
    fn node_link_construction_leaf(#[strategy(0usize..=0x7fff_ffffusize)] index: usize) {
        let tag = NodeLink::new_leaf(index);
        assert!(tag.index() == index as usize);
        assert!(tag.is_leaf());
    }

    #[proptest]
    fn node_link_construction_inner(#[strategy(0usize..=0x7fff_ffffusize)] index: usize) {
        let tag = NodeLink::new_inner(index);
        assert!(tag.index() == index as usize);
        assert!(!tag.is_leaf());
    }

    #[test]
    #[should_panic]
    fn node_link_panics_on_invalid_size_leaf() {
        let _ = NodeLink::new_leaf(0x8000_0000usize);
    }

    #[test]
    #[should_panic]
    fn node_link_panics_on_invalid_size_inner() {
        let _ = NodeLink::new_inner(0x8000_0000usize);
    }
}
