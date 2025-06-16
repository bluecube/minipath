use nalgebra::Unit;

use crate::geometry::{FloatType, HitRecord, Ray, TexturePoint, WorldBox, WorldPoint, WorldVector};

use super::Object;

pub struct Sphere {
    pub center: WorldPoint,
    pub radius: FloatType,
}

impl Object for Sphere {
    fn intersect(&self, ray: &Ray) -> Option<HitRecord> {
        let oc = ray.origin - self.center;
        let b = oc.dot(&ray.direction);
        let c = oc.dot(&oc) - self.radius * self.radius;
        let discriminant = b * b - c;

        if discriminant < 0.0 {
            return None;
        }

        let sqrt_disc = discriminant.sqrt();
        let t1 = -b - sqrt_disc;
        let t2 = -b + sqrt_disc;
        let t = if t1 > 0.0 {
            t1
        } else if t2 > 0.0 {
            t2
        } else {
            return None;
        };

        let point = ray.origin + ray.direction.as_ref() * t;
        let normal = Unit::new_normalize(point - self.center);

        Some(HitRecord {
            t,
            point,
            normal,
            material: 0,
            texture_coordinates: TexturePoint::origin(), // TODO?
        })
    }

    fn get_bounding_box(&self) -> crate::geometry::WorldBox {
        let r_vec = WorldVector::repeat(self.radius);
        WorldBox {
            min: self.center - r_vec,
            max: self.center + r_vec,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_direct_hit_through_center() {
        let sphere = Sphere {
            center: [1.0, 2.0, 3.0].into(),
            radius: 1.0,
        };
        let ray = Ray::new([1.0, 2.0, 0.0].into(), [0.0, 0.0, 1.0].into());
        let hit = sphere.intersect(&ray);

        let h = hit.expect("We should have a hit!");
        assert!((h.t - 2.0).abs() < 1e-6);
    }

    #[test]
    fn test_grazing_hit() {
        let sphere = Sphere {
            center: [1.0, 2.0, 3.0].into(),
            radius: 1.0,
        };
        let ray = Ray::new([2.0, 2.0, 0.0].into(), [0.0, 0.0, 1.0].into());
        let hit = sphere.intersect(&ray);

        let h = hit.expect("We should have a hit!");
        assert!((h.t - 3.0).abs() < 1e-6);
    }

    #[test]
    fn test_narrow_miss() {
        let sphere = Sphere {
            center: [1.0, 2.0, 3.0].into(),
            radius: 1.0,
        };
        let ray = Ray::new([2.0, 2.01, 0.0].into(), [0.0, 0.0, 1.0].into());
        let hit = sphere.intersect(&ray);
        assert!(hit.is_none());
    }
}
