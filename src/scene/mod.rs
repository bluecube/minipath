pub mod primitives;

use crate::geometry::{HitRecord, Ray, WorldBox};

/// Renderable object
pub trait Object {
    fn intersect(&self, ray: &Ray) -> Option<HitRecord>;
    fn get_bounding_box(&self) -> WorldBox;
}

pub struct Scene<O: Object> {
    pub object: O,
}
