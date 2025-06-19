pub mod primitives;
pub mod triangle_bvh;

use crate::geometry::{HitRecord, Ray, WorldBox};

/// Renderable object
pub trait Object {
    fn intersect(&self, ray: &Ray, cache: &mut triangle_bvh::StackCache) -> Option<HitRecord>;
    fn get_bounding_box(&self) -> WorldBox;
}

#[derive(Clone)]
pub struct Scene<O: Object> {
    pub object: O,
}
