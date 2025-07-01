use assert2::assert;
use nalgebra::{Isometry3, Unit};
use rand_distr::Distribution as _;

use crate::geometry::{FloatType, Ray, ScreenPoint, ScreenSize, WorldPoint, WorldVector};

/// Represents camera looking at the scene
#[derive(Copy, Clone, Debug)]
pub struct Camera {
    pub world_to_camera: Isometry3<FloatType>,

    pub focus_distance: FloatType,

    pub sensor_size: SensorSize,
    pub focal_length: FloatType,
    pub f_number: FloatType,
}

#[derive(Copy, Clone, Debug)]
pub enum SensorSize {
    Width(FloatType),
    Height(FloatType),
}

#[derive(Copy, Clone, Debug)]
pub struct CameraSampler {
    center: WorldPoint,

    up: Unit<WorldVector>,
    right: Unit<WorldVector>,
    film_origin_offset: WorldVector,

    /// Distance between pixels in meters
    pixel_scale: FloatType,

    /// Lens radius in meters
    lens_radius: FloatType,
    lens_weight: FloatType,
}

/// Default camera is a 35mm camera with 50mm f/9 lens, looks along Z, focuses at infinity.
impl Default for Camera {
    fn default() -> Self {
        Camera {
            world_to_camera: Default::default(),
            focus_distance: FloatType::INFINITY,
            sensor_size: SensorSize::Height(24e-3),
            focal_length: 50e-3,
            f_number: 9.0,
        }
    }
}

impl Camera {
    /// Creates a new camera that replaces its transform with the argument
    pub fn with_transform(&self, transform: Isometry3<FloatType>) -> Camera {
        Camera {
            world_to_camera: transform,
            ..*self
        }
    }

    pub fn focus_distance(&self, focus_distance: FloatType) -> Camera {
        assert!(focus_distance >= 0.0);
        Camera {
            focus_distance,
            ..*self
        }
    }

    pub fn sensor_width(&self, sensor_width: FloatType) -> Camera {
        assert!(sensor_width > 0.0);
        Camera {
            sensor_size: SensorSize::Width(sensor_width),
            ..*self
        }
    }

    pub fn sensor_height(&self, sensor_height: FloatType) -> Camera {
        assert!(sensor_height > 0.0);
        Camera {
            sensor_size: SensorSize::Height(sensor_height),
            ..*self
        }
    }

    pub fn f_number(&self, f_number: FloatType) -> Camera {
        assert!(f_number > 0.0);
        Camera { f_number, ..*self }
    }

    /// Creates a new camera that looks from `center` to `look_at` and also focuses at `look_at`
    pub fn look_at(&self, center: WorldPoint, look_at: WorldPoint, up: WorldVector) -> Camera {
        let transform = Isometry3::look_at_rh(&center, &look_at, &up);

        Camera {
            world_to_camera: transform,
            focus_distance: (look_at - center).norm(),
            ..*self
        }
    }

    /// Creates a new camera that looks from `center` looking `forward`.
    pub fn look_direction(
        &self,
        center: WorldPoint,
        forward: WorldVector,
        up: WorldVector,
    ) -> Camera {
        let transform = Isometry3::look_at_rh(&center, &(center + forward), &up);

        Camera {
            world_to_camera: transform,
            ..*self
        }
    }

    pub fn build_sampler(&self, resolution: ScreenSize) -> CameraSampler {
        let t = self.world_to_camera.inverse();
        let center = t * WorldPoint::origin();
        let forward = Unit::new_unchecked(t * WorldVector::new(0.0, 0.0, -1.0));
        let up = Unit::new_unchecked(t * WorldVector::new(0.0, 1.0, 0.0));
        let right = Unit::new_unchecked(t * WorldVector::new(1.0, 0.0, 0.0));

        let resolution = resolution.cast::<FloatType>();
        let pixel_scale = match self.sensor_size {
            SensorSize::Width(w) => w / resolution.x,
            SensorSize::Height(h) => h / resolution.y,
        };

        let film_origin_uv = (resolution.map(|x| x - 1.0) * pixel_scale) / 2.0;
        let film_origin_offset = -forward.as_ref() * self.focal_length
            + right.as_ref() * film_origin_uv.x
            - up.as_ref() * film_origin_uv.y;

        CameraSampler {
            center,
            up,
            right,
            film_origin_offset,
            pixel_scale,
            lens_radius: self.focal_length / (2.0 * self.f_number),
            lens_weight: self.focal_length / self.focus_distance,
        }
    }
}

impl CameraSampler {
    /// Samples a new ray from the camera for the given image pixel.
    pub fn sample_ray(&self, point: &ScreenPoint, rng: &mut impl rand::Rng) -> Ray {
        //TODO: Figure out a better reconstruction kernel for the pixel than a square
        let film_u = point.x as f32 + rng.random_range(-0.5..=0.5);
        let film_v = point.y as f32 + rng.random_range(-0.5..=0.5);
        let film_point_offset = self.film_origin_offset
            + self.up.as_ref() * (film_v * self.pixel_scale)
            - self.right.as_ref() * (film_u * self.pixel_scale);

        let lens_uv: [f32; 2] = rand_distr::UnitDisc.sample(rng);
        let lens_vector = self.right.as_ref() * (self.lens_radius * lens_uv[0])
            + self.up.as_ref() * (self.lens_radius * lens_uv[1]);

        let direction = lens_vector * self.lens_weight - film_point_offset;

        Ray::new(self.center + lens_vector, direction)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use assert2::assert;

    #[test]
    fn left_right_up_down() {
        // X goes right, Y goes away, Z goes up
        let camera = Camera::default()
            .look_direction(
                WorldPoint::new(0.0, 0.0, 0.0),
                WorldVector::new(0.0, 1.0, 0.0), /* forward */
                WorldVector::new(0.0, 0.0, 1.0), /* up */
            )
            .focus_distance(2.0)
            .build_sampler(ScreenSize::new(800, 600));

        let mut rng = rand::rng();

        let ray_center = camera.sample_ray(&ScreenPoint::new(400, 300), &mut rng);
        let ray_left = camera.sample_ray(&ScreenPoint::new(0, 300), &mut rng);
        let ray_right = camera.sample_ray(&ScreenPoint::new(799, 300), &mut rng);
        let ray_up = camera.sample_ray(&ScreenPoint::new(400, 0), &mut rng);
        let ray_down = camera.sample_ray(&ScreenPoint::new(400, 599), &mut rng);

        assert!(ray_center.direction.x.abs() < 1e-3);
        assert!(ray_center.direction.z.abs() < 1e-3);
        assert!(ray_left.direction.x < ray_center.direction.x);
        assert!(ray_right.direction.x > ray_center.direction.x);
        assert!(ray_up.direction.z > ray_center.direction.z);
        assert!(ray_down.direction.z < ray_center.direction.z);
    }
}
