pub struct ScreenSpace;
pub type ScreenPoint = euclid::Point2D<u32, ScreenSpace>;
pub type ScreenSize = euclid::Size2D<u32, ScreenSpace>;
pub type ScreenBlock = euclid::Box2D<u32, ScreenSpace>;

pub struct WorldSpace;
pub type WorldPoint = euclid::Point3D<f64, WorldSpace>;
pub type WorldVector = euclid::Point3D<f64, WorldSpace>;

pub struct Ray {
    pub origin: WorldPoint,
    pub direction: WorldVector,
}
