pub struct ScreenSpace;
pub type ScreenPoint = euclid::Point2D<u32, ScreenSpace>;
pub type ScreenSize = euclid::Size2D<u32, ScreenSpace>;
pub type ScreenBlock = euclid::Box2D<u32, ScreenSpace>;

pub struct WorldSpace;
pub type WorldPoint = euclid::Point3D<f32, WorldSpace>;
pub type WorldVector = euclid::Vector3D<f32, WorldSpace>;
pub type WorldDistance = euclid::Length<f32, WorldSpace>;
pub type WorldBox = euclid::Box3D<f32, WorldSpace>;

#[derive(Copy, Clone, Debug)]
pub struct Ray {
    pub origin: WorldPoint,
    pub direction: WorldVector,
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
