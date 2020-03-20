pub struct ScreenSpace;
pub type ScreenPoint = euclid::Point2D<u32, ScreenSpace>;
pub type ScreenSize = euclid::Size2D<u32, ScreenSpace>;
pub type ScreenBlock = euclid::Box2D<u32, ScreenSpace>;

pub struct WorldSpace;
pub type WorldPoint = euclid::Point3D<f64, WorldSpace>;
pub type WorldVector = euclid::Vector3D<f64, WorldSpace>;
pub type WorldDistance = euclid::Length<f64, WorldSpace>;

pub struct Ray {
    pub origin: WorldPoint,
    pub direction: WorldVector,
}

#[cfg(test)]
pub mod test {
    use super::*;
    use proptest::prelude::*;

    #[derive(Copy, Clone, Debug)]
    pub struct ScreenBlockWrapper(pub ScreenBlock);
    impl Arbitrary for ScreenBlockWrapper {
        type Parameters = ();
        type Strategy = proptest::strategy::BoxedStrategy<Self>;
        fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
            const RANGE: std::ops::Range<u32> = 0..100u32;
            (RANGE, RANGE, RANGE, RANGE)
                .prop_map(|coords| {
                    ScreenBlockWrapper(ScreenBlock::new(
                        ScreenPoint::new(coords.0, coords.1),
                        ScreenPoint::new(coords.2, coords.3),
                    ))
                })
                .boxed()
        }
    }
}
