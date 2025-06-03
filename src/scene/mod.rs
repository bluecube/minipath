pub mod primitives;

use crate::geometry::{Intersection, Ray, WorldBox};

/// Renderable object
pub trait Object {
    fn intersect(&self, ray: &Ray) -> Option<Intersection>;
    fn get_bounding_box(&self) -> WorldBox;
}

pub struct Scene<O: Object> {
    pub object: O,
}
