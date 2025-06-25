mod building;
mod compressed_geometry;
mod printing;
mod ray_bvh_intersection;

use compressed_geometry::{RelativeBox8, RelativeTriangle8};

use crate::geometry::{TexturePoint, Triangle, WorldBox, WorldVector};

use index_vec::IndexVec;

pub use ray_bvh_intersection::StackCache;

const INNER_NODE_CHILDREN: usize = 8;
const LEAF_NODE_PACKET_SIZE: usize = 8;
const LEAF_NODE_MAX_TRIANGLES: usize =
    LEAF_NODE_PACKET_SIZE * CompressedNodeLink::MAX_COUNT as usize;

#[derive(Clone, Debug)]
pub struct TriangleBvh {
    bounding_box: WorldBox,
    root: CompressedNodeLink,

    inner_nodes: IndexVec<InnerNodeIdx, InnerNode>,

    triangle_geometry: IndexVec<TrianglePackIdx, RelativeTriangle8>,
    triangle_shading_data: IndexVec<TriangleIdx, TriangleShadingData>,

    vertex_data: IndexVec<VertexIdx, VertexShadingData>,
}

#[derive(Clone, Debug, Default)]
struct InnerNode {
    child_bounds: RelativeBox8,
    // TODO: Perf: NodeLink might also be SIMD types, that way we can filter out
    // NULLs faster?
    child_links: [CompressedNodeLink; 8],
}

#[derive(Clone, Debug, Default)]
struct TriangleShadingData {
    vertex_indices: Triangle<usize>,
    flat_shading: bool,
    material: usize,
}

/// Additional data for triangle shading.
/// normals and texture_coords are indexed using LeafNodeGeomerty::vertex_indices
#[derive(Clone, Debug)]
struct VertexShadingData {
    normal: WorldVector,
    texture_coords: TexturePoint,
}

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq)]
struct CompressedNodeLink(u32);

#[derive(Clone, Debug, PartialEq, Eq)]
enum NodeLink {
    Null,
    Inner { index: InnerNodeIdx },
    Leaf { indices: TrianglePackIdxRange },
}

impl CompressedNodeLink {
    const COUNT_BITS: u32 = 3;
    const COUNT_MASK: u32 = (1 << Self::COUNT_BITS) - 1;
    const NULL_VALUE: u32 = (u32::MAX >> Self::COUNT_BITS) << Self::COUNT_BITS;

    pub const MAX_INDEX: u32 = (u32::MAX >> Self::COUNT_BITS) - 1;
    pub const MIN_COUNT: u32 = 1;
    pub const MAX_COUNT: u32 = (1 << Self::COUNT_BITS) - 1;

    pub const NULL: Self = Self(Self::NULL_VALUE);

    /// Create a new leaf link, panics if size or count are out of range
    fn new_leaf(index: TrianglePackIdx, count: u32) -> Self {
        assert!(count >= Self::MIN_COUNT);
        assert!(count <= Self::MAX_COUNT);
        Self(index.raw() << Self::COUNT_BITS | count)
    }

    /// Create a new inner node link, panics if size is out of range
    fn new_inner(index: InnerNodeIdx) -> Self {
        Self(index.raw() << Self::COUNT_BITS)
    }

    fn decode(&self) -> NodeLink {
        if self.is_null() {
            NodeLink::Null
        } else {
            let count = self.0 & Self::COUNT_MASK;
            let index = self.0 >> Self::COUNT_BITS;

            if count == 0 {
                NodeLink::Inner {
                    index: InnerNodeIdx::from_raw_unchecked(index),
                }
            } else {
                NodeLink::Leaf {
                    indices: TrianglePackIdxRange::new(
                        TrianglePackIdx::from_raw_unchecked(index),
                        count,
                    ),
                }
            }
        }
    }

    fn is_null(&self) -> bool {
        self.0 == Self::NULL_VALUE
    }
}

impl Default for CompressedNodeLink {
    fn default() -> Self {
        Self::NULL
    }
}

impl std::fmt::Debug for CompressedNodeLink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NodeLink")
            .field("0", &self.0)
            .field("<decoded>", &self.decode())
            .finish()
    }
}

index_vec::define_index_type! {
    struct InnerNodeIdx = u32;
    MAX_INDEX = CompressedNodeLink::MAX_INDEX as usize;
    IMPL_RAW_CONVERSIONS = true;
}

index_vec::define_index_type! {
    struct TrianglePackIdx = u32;
    MAX_INDEX = CompressedNodeLink::MAX_INDEX as usize;
    IMPL_RAW_CONVERSIONS = true;
}

index_vec::define_index_type! {
    struct TriangleIdx = usize;
    MAX_INDEX = usize::MAX - 1;
    DEFAULT = TriangleIdx::from_raw_unchecked(usize::MAX);
}

index_vec::define_index_type! {
    struct VertexIdx = usize;
}

impl TrianglePackIdx {
    fn to_triangle_idx(self, lane: usize) -> TriangleIdx {
        ((self.raw() as usize) * LEAF_NODE_PACKET_SIZE + lane).into()
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct TrianglePackIdxRange {
    pub first: TrianglePackIdx,
    pub last: TrianglePackIdx,
}

impl TrianglePackIdxRange {
    pub fn new(first: TrianglePackIdx, count: u32) -> TrianglePackIdxRange {
        TrianglePackIdxRange {
            first,
            last: first + (count as usize),
        }
    }

    pub fn into_range(self) -> std::ops::Range<TrianglePackIdx> {
        self.first..self.last
    }

    pub fn iter(&self) -> impl Iterator<Item = TrianglePackIdx> {
        (u32::from(self.first)..u32::from(self.last)).map(TrianglePackIdx::from)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use assert2::{assert, let_assert};
    use test_strategy::proptest;

    #[proptest]
    fn node_link_construction_leaf(
        #[strategy(0u32..=CompressedNodeLink::MAX_INDEX)] index: u32,
        #[strategy(1u32..=CompressedNodeLink::MAX_COUNT)] count: u32,
    ) {
        dbg!(CompressedNodeLink::MAX_INDEX);
        let tag = CompressedNodeLink::new_leaf(index.into(), count);
        let_assert!(NodeLink::Leaf { indices } = tag.decode());
        assert!(indices.first.raw() == index);
        assert!(indices.iter().count() == count as usize);
    }

    #[proptest]
    fn node_link_construction_inner(#[strategy(0u32..=CompressedNodeLink::MAX_INDEX)] index: u32) {
        let tag = CompressedNodeLink::new_inner(index.into());
        let_assert!(NodeLink::Inner { index: decoded } = tag.decode());
        assert!(decoded.raw() == index);
    }

    #[test]
    fn node_link_construction_null() {
        let tag = CompressedNodeLink::NULL;
        assert!(tag.decode() == NodeLink::Null);
    }

    #[test]
    #[should_panic]
    fn node_link_invalid_leaf_packet_count_zero() {
        CompressedNodeLink::new_leaf(0u32.into(), 0);
    }

    #[test]
    #[should_panic]
    fn node_link_invalid_leaf_packet_count_too_high() {
        CompressedNodeLink::new_leaf(0u32.into(), CompressedNodeLink::MAX_COUNT + 1);
    }

    #[test]
    #[should_panic]
    fn node_link_leaf_index_out_of_range() {
        CompressedNodeLink::new_leaf((CompressedNodeLink::MAX_INDEX + 1).into(), 1);
    }

    #[test]
    #[should_panic]
    fn node_link_inner_index_out_of_range() {
        CompressedNodeLink::new_inner((CompressedNodeLink::MAX_INDEX + 1).into());
    }
}
