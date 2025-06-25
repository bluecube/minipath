mod building;
mod compressed_geometry;
mod printing;
mod ray_bvh_intersection;

use compressed_geometry::{RelativeBox8, RelativeTriangle8};
use simba::simd::WideBoolF32x8;
use wide::u32x8;

use crate::geometry::{SimdMaskType, TexturePoint, Triangle, WorldBox, WorldVector};

use index_vec::IndexVec;

pub use ray_bvh_intersection::StackCache;

const INNER_NODE_CHILDREN: usize = 8;
const LEAF_NODE_PACKET_SIZE: usize = 8;
const LEAF_NODE_MAX_TRIANGLES: usize = LEAF_NODE_PACKET_SIZE * NodeLink::MAX_COUNT as usize;

#[derive(Clone, Debug)]
pub struct TriangleBvh {
    bounding_box: WorldBox,
    root: NodeLink,

    inner_nodes: IndexVec<InnerNodeIdx, InnerNode>,

    triangle_geometry: IndexVec<TrianglePackIdx, RelativeTriangle8>,
    triangle_shading_data: IndexVec<TriangleIdx, TriangleShadingData>,

    vertex_data: IndexVec<VertexIdx, VertexShadingData>,
}

#[derive(Clone, Debug, Default)]
struct InnerNode {
    child_bounds: RelativeBox8,
    child_links: CompressedNodeLink8,
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
#[derive(Clone, Copy, Debug)]
struct CompressedNodeLink8(u32x8);

impl CompressedNodeLink8 {
    pub const COUNT_BITS: u32 = 3;
    pub const COUNT_MASK: u32 = (1 << Self::COUNT_BITS) - 1;
    pub const NULL_VALUE: u32 = (u32::MAX >> Self::COUNT_BITS) << Self::COUNT_BITS;

    /// Returns mask of elements that represent inner nodes.
    pub fn inner_node_mask(&self) -> SimdMaskType {
        !self.null_mask() & WideBoolF32x8(bytemuck::cast(self.count().cmp_eq(u32x8::ZERO)))
    }

    /// Returns mask of elements that represent leaf nodes.
    pub fn leaf_node_mask(&self) -> SimdMaskType {
        WideBoolF32x8(bytemuck::cast(self.count().cmp_gt(u32x8::ZERO)))
    }

    /// Returns mask of elements that represent null links.
    pub fn null_mask(&self) -> SimdMaskType {
        WideBoolF32x8(bytemuck::cast(
            self.0.cmp_eq(u32x8::splat(Self::NULL_VALUE)),
        ))
    }

    /// Returns the index of non-null links (Self::MAX_COUNT + 1 for null links).
    pub fn index(&self) -> u32x8 {
        self.0 >> u32x8::splat(Self::COUNT_BITS)
    }

    /// Returns the packet count for leaf nodes (zero for non-leaf nodes).
    pub fn count(&self) -> u32x8 {
        self.0 & u32x8::splat(Self::COUNT_MASK)
    }

    /// Extracts and decompresses a link from a given lane.
    pub fn extract(&self, i: usize) -> NodeLink {
        let value = self.0.as_array_ref()[i];
        if value == Self::NULL_VALUE {
            NodeLink::Null
        } else {
            let count = value & Self::COUNT_MASK;
            let index = value >> Self::COUNT_BITS;

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

    pub fn replace(&mut self, i: usize, value: NodeLink) {
        let value = match value {
            NodeLink::Null => Self::NULL_VALUE,
            NodeLink::Inner { index } => index.raw() << Self::COUNT_BITS,
            NodeLink::Leaf { indices } => {
                (indices.first.raw() << Self::COUNT_BITS) | indices.count()
            }
        };

        self.0.as_array_mut()[i] = value;
    }
}

impl Default for CompressedNodeLink8 {
    fn default() -> Self {
        CompressedNodeLink8(u32x8::splat(CompressedNodeLink8::NULL_VALUE))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum NodeLink {
    Null,
    Inner { index: InnerNodeIdx },
    Leaf { indices: TrianglePackIdxRange },
}

impl NodeLink {
    pub const MAX_INDEX: u32 = (u32::MAX >> CompressedNodeLink8::COUNT_BITS) - 1;
    pub const MIN_COUNT: u32 = 1;
    pub const MAX_COUNT: u32 = (1 << CompressedNodeLink8::COUNT_BITS) - 1;

    /// Create a new leaf link, panics if size or count are out of range
    fn new_leaf(index: TrianglePackIdx, count: u32) -> Self {
        assert!(count >= Self::MIN_COUNT);
        assert!(count <= Self::MAX_COUNT);
        Self::Leaf {
            indices: TrianglePackIdxRange {
                first: index,
                last: index + count as usize,
            },
        }
    }

    /// Create a new inner node link, panics if size is out of range
    fn new_inner(index: InnerNodeIdx) -> Self {
        Self::Inner { index }
    }
}

impl Default for NodeLink {
    fn default() -> Self {
        NodeLink::Null
    }
}

index_vec::define_index_type! {
    struct InnerNodeIdx = u32;
    MAX_INDEX = NodeLink::MAX_INDEX as usize;
    IMPL_RAW_CONVERSIONS = true;
}

index_vec::define_index_type! {
    struct TrianglePackIdx = u32;
    MAX_INDEX = NodeLink::MAX_INDEX as usize;
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
    fn to_triangle_idx(&self, lane: usize) -> TriangleIdx {
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
        (u32::from(self.first)..u32::from(self.last))
            .into_iter()
            .map(TrianglePackIdx::from)
    }

    pub fn count(&self) -> u32 {
        self.last.raw() - self.first.raw()
    }
}

#[cfg(test)]
mod test {
    use std::array;

    use super::*;

    use assert2::assert;
    use proptest::{
        prelude::{Just, Strategy},
        prop_assert_eq, prop_oneof,
    };
    use simba::simd::SimdBool as _;
    use test_strategy::proptest;

    fn node_link_strategy() -> impl Strategy<Value = NodeLink> {
        prop_oneof![
            Just(NodeLink::Null),
            (0u32..=NodeLink::MAX_INDEX).prop_map(|i| NodeLink::Inner { index: i.into() }),
            (0u32..=NodeLink::MAX_INDEX, 1u32..NodeLink::MAX_COUNT).prop_map(|(i, count)| {
                let first = i.into();
                NodeLink::Leaf {
                    indices: TrianglePackIdxRange {
                        first,
                        last: first + count as usize,
                    },
                }
            }),
        ]
    }

    #[proptest]
    fn node_link_round_trip(
        #[strategy(node_link_strategy())] link: NodeLink,
        #[strategy(0usize..8usize)] i: usize,
    ) {
        let mut links_simd = CompressedNodeLink8::default();
        links_simd.replace(i, link.clone());

        let links_expected: [NodeLink; 8] =
            array::from_fn(|j| if j == i { link.clone() } else { NodeLink::Null });
        let links_decoded = array::from_fn(|i| links_simd.extract(i));

        prop_assert_eq!(links_decoded, links_expected);
    }

    #[test]
    fn node_link_simd_accessors_leaf() {
        let mut links_simd = CompressedNodeLink8::default();
        links_simd.replace(
            3,
            NodeLink::Leaf {
                indices: TrianglePackIdxRange {
                    first: TrianglePackIdx::from_raw(10),
                    last: TrianglePackIdx::from_raw(15),
                },
            },
        );

        let inner_node_mask = links_simd.inner_node_mask();
        assert!(inner_node_mask.none(), "{:?}", inner_node_mask);

        let leaf_node_mask = links_simd.leaf_node_mask();
        assert!(leaf_node_mask.bitmask() == 0x08);

        let null_mask = links_simd.null_mask();
        assert!(null_mask.bitmask() == 0xf7);

        let index = links_simd.index();
        assert!(index.as_array_ref()[3] == 10, "{:?}", index);

        let count = links_simd.count();
        assert!(count.as_array_ref()[3] == 5, "{:?}", count);
    }

    #[test]
    fn node_link_simd_accessors_inner() {
        let mut links_simd = CompressedNodeLink8::default();
        links_simd.replace(
            3,
            NodeLink::Inner {
                index: InnerNodeIdx::from_raw(10),
            },
        );

        let inner_node_mask = links_simd.inner_node_mask();
        assert!(inner_node_mask.bitmask() == 0x08);

        let leaf_node_mask = links_simd.leaf_node_mask();
        assert!(leaf_node_mask.none(), "{:?}", leaf_node_mask);

        let null_mask = links_simd.null_mask();
        assert!(null_mask.bitmask() == 0xf7);

        let index = links_simd.index();
        assert!(index.as_array_ref()[3] == 10, "{:?}", index);
    }

    #[test]
    #[should_panic]
    fn node_link_invalid_leaf_packet_count_zero() {
        NodeLink::new_leaf(0u32.into(), 0);
    }

    #[test]
    #[should_panic]
    fn node_link_invalid_leaf_packet_count_too_high() {
        NodeLink::new_leaf(0u32.into(), NodeLink::MAX_COUNT + 1);
    }

    #[test]
    #[should_panic]
    fn node_link_leaf_index_out_of_range() {
        NodeLink::new_leaf((NodeLink::MAX_INDEX + 1).into(), 1);
    }

    #[test]
    #[should_panic]
    fn node_link_inner_index_out_of_range() {
        NodeLink::new_inner((NodeLink::MAX_INDEX + 1).into());
    }
}
