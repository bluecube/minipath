use wide::f32x8;

pub struct ScreenSpace;
pub type ScreenPoint = euclid::Point2D<u32, ScreenSpace>;
pub type ScreenSize = euclid::Size2D<u32, ScreenSpace>;
pub type ScreenBlock = euclid::Box2D<u32, ScreenSpace>;

pub struct WorldSpace;
pub type WorldPoint = euclid::Point3D<f32, WorldSpace>;
pub type WorldVector = euclid::Vector3D<f32, WorldSpace>;
pub type WorldDistance = euclid::Length<f32, WorldSpace>;
pub type WorldBox = euclid::Box3D<f32, WorldSpace>;
pub type WorldPoint8 = euclid::Point3D<f32x8, WorldSpace>;
pub type WorldVector8 = euclid::Vector3D<f32x8, WorldSpace>;
pub type WorldDistance8 = euclid::Length<f32x8, WorldSpace>;
pub type WorldBox8 = euclid::Box3D<f32x8, WorldSpace>;

pub struct TextureSpace;
pub type TexturePoint = euclid::Point2D<f32, TextureSpace>;

/// Ray going through the world. Only positive direction is considered to be on the ray.
#[derive(Copy, Clone, Debug)]
pub struct Ray {
    pub origin: WorldPoint,
    /// Normalized direction of the ray
    pub direction: WorldVector,

    /// Componentwise inverse of the ray direction
    /// Zeros in direction get turned into positive infinity regardless of the sign of the zero
    pub inv_direction: WorldVector,
}

impl Ray {
    pub fn new(origin: WorldPoint, direction: WorldVector) -> Ray {
        let direction = direction.normalize();
        let inv_direction = direction.map(|x| if x == 0.0 { f32::INFINITY } else { 1.0 / x });

        Ray {
            origin,
            direction,
            inv_direction,
        }
    }

    pub fn point_at(&self, distance: f32) -> WorldPoint {
        self.origin + self.direction * distance
    }

    pub fn advance_by(&self, distance: f32) -> Ray {
        Ray {
            origin: self.point_at(distance),
            direction: self.direction,
            inv_direction: self.inv_direction,
        }
    }
}

/// Intersection of ray and scene
#[derive(Copy, Clone, Debug)]
pub struct Intersection {
    /// Position along the ray
    pub t: f32,
    /// Point where the ray hit the geometry
    pub point: WorldPoint,
    /// Normalized normal vector
    pub normal: WorldVector,
    pub material: u32,
    pub texture_coordinates: TexturePoint,
}

pub trait SimdSplat {
    type VectorType;

    fn simd_splat(&self) -> Self::VectorType;
}

impl SimdSplat for WorldPoint {
    type VectorType = WorldPoint8;

    fn simd_splat(&self) -> WorldPoint8 {
        self.map(f32x8::splat)
    }
}

impl SimdSplat for WorldVector {
    type VectorType = WorldVector8;

    fn simd_splat(&self) -> WorldVector8 {
        self.map(f32x8::splat)
    }
}

impl SimdSplat for WorldBox {
    type VectorType = WorldBox8;

    fn simd_splat(&self) -> WorldBox8 {
        WorldBox8::new(self.min.simd_splat(), self.max.simd_splat())
    }
}

pub trait SimdLaneAccess {
    type ScalarType;

    fn get_lane(&self, i: usize) -> Self::ScalarType;
}

impl SimdLaneAccess for WorldBox8 {
    type ScalarType = WorldBox;

    fn get_lane(&self, i: usize) -> WorldBox {
        WorldBox::new(self.min.get_lane(i), self.max.get_lane(i))
    }
}

impl SimdLaneAccess for WorldPoint8 {
    type ScalarType = WorldPoint;

    fn get_lane(&self, i: usize) -> WorldPoint {
        self.map(|x| x.as_array_ref()[i])
    }
}

#[cfg(test)]
pub mod test {
    use super::*;
    use proptest::prelude::*;

    /// Helper macro that creates a wrapper arnound a type that implemetns Deref and Arbitary
    macro_rules! arbitrary_wrapper {
        ( $wrapper_name:ident ( $type:ty ) -> $block:block ) => {
            #[derive(Copy, Clone, Debug)]
            pub struct $wrapper_name(pub $type);

            impl std::ops::Deref for $wrapper_name {
                type Target = $type;
                fn deref(&self) -> &$type {
                    &self.0
                }
            }

            impl Arbitrary for $wrapper_name {
                type Parameters = ();
                type Strategy = proptest::strategy::BoxedStrategy<Self>;
                fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
                    $block.prop_map(|x| $wrapper_name(x)).boxed()
                }
            }
        };
    }

    fn simple_float() -> BoxedStrategy<f32> {
        any::<i64>().prop_map(|n| n as f32 * 1e-6).boxed()
    }

    fn simple_positive_float() -> BoxedStrategy<f32> {
        any::<u64>().prop_map(|n| n as f32 * 1e-6).boxed()
    }

    arbitrary_wrapper! {
        ScreenBlockWrapper(ScreenBlock) -> {
            const RANGE: std::ops::Range<u32> = 0..100u32;
            (RANGE, RANGE, RANGE, RANGE)
                .prop_map(|coords| {
                    ScreenBlock::new(
                        ScreenPoint::new(coords.0, coords.1),
                        ScreenPoint::new(coords.2, coords.3),
                    )
                })
        }
    }

    arbitrary_wrapper! {
        ScreenSizeWrapper(ScreenSize) -> {
            const RANGE: std::ops::Range<u32> = 1..100u32;
            (RANGE, RANGE)
                .prop_map(|coords| ScreenSize::new(coords.0, coords.1))
        }
    }

    arbitrary_wrapper! {
        NonzeroWorldVectorWrapper(WorldVector) -> {
            (simple_float(), simple_float(), simple_float())
                .prop_filter_map(
                    "vector is zero",
                    |coords| {
                        let vector = WorldVector::new(coords.0, coords.1, coords.2);
                        if vector.length() < 1e-6 {
                            None
                        } else {
                            Some(vector)
                        }
                    })

        }
    }

    arbitrary_wrapper! {
        WorldPointWrapper(WorldPoint) -> {
            (simple_float(), simple_float(), simple_float())
                .prop_map(|coords| {
                    WorldPoint::new(coords.0, coords.1, coords.2)
                })
        }
    }

    arbitrary_wrapper! {
        PositiveWorldDistanceWrapper(WorldDistance) -> {
            simple_positive_float()
                .prop_map(|x| WorldDistance::new(x))
        }
    }
}
