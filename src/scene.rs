pub type Scene = ();
/*

TODO: Cache-aware vectorized BVH

use packed_simd::Simd;

struct SceneSpace;
struct BoxCornerSpace;
struct BoxSizeSpace;

const NODE_VECTOR_SIZE: usize = 32;
const LEAF_VECTOR_SIZE: usize = 8;

struct SimdBvhInnerNode {
    min: Point3D<Simd<[u16; NODE_VECTOR_SIZE], BoxCornerSpace>, // 3*512b = 3 cacheline
    size: Size3D<Simd<[u8, NODE_VECTOR_SIZE], BoxSizeSpace>, // 3*256b = 1.5 cachelines
    children: Box<SimdBvhNodeGroup>, // 64b = 0.25 cacheline
    // + 64b to spare
}

type SimdBvhInnerNodeGroup = [SimdBvhInnerNode; NODE_VECTOR_SIZE];

struct TrianglePack {
    origin: Point3D<f32x8, SceneSpace>; // 3 * 256b = 1.5 cachelines
    edge1: Vector3D<f32x8, SceneSpace>; // 3 * 256b = 1.5 cachelines
    edge2: Vector3D<f32x8, SceneSpace>; // 3 * 256b = 1.5 cachelines
}

pub struct Triangle {
    origin: Point3D<f32>,
    edge1: Vector3D<f32>,
    edge2: Vector3D<f32>,
}

pub type Scene = Vec<Triangle>;

*/
