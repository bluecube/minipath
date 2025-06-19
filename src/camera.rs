use assert2::assert;
use bon::bon;
use nalgebra::Unit;
use rand_distr::Distribution as _;

use crate::geometry::{EPSILON, FloatType, Ray, ScreenPoint, ScreenSize, WorldPoint, WorldVector};

#[derive(Copy, Clone, Debug)]
pub struct Camera {
    center: WorldPoint,

    resolution: ScreenSize,

    up: Unit<WorldVector>,
    right: Unit<WorldVector>,
    film_origin_offset: WorldVector,

    /// Distance between pixels in meters
    pixel_pitch: FloatType,

    /// Lens radius in meters
    lens_radius: FloatType,
    lens_weight: FloatType,
}

#[bon]
impl Camera {
    #[builder]
    pub fn new(
        center: WorldPoint,
        forward: WorldVector,
        up: WorldVector,
        resolution: ScreenSize,
        film_width: FloatType,
        focal_length: FloatType,
        f_number: f32,
        focus_distance: FloatType,
    ) -> Self {
        let forward = Unit::try_new(forward, EPSILON).expect("Forward vector must be non-zero");
        let up = Unit::try_new(up, EPSILON).expect("Up vector must be no-zero");
        let right = Unit::try_new(forward.cross(&up), EPSILON)
            .expect("`up` and `forward` must be linearly independent");
        let up = Unit::new_normalize(right.cross(&forward));

        assert!(resolution.x > 0);
        assert!(resolution.y > 0);
        assert!(film_width > 0.0);
        assert!(focal_length > 0.0);
        assert!(f_number > 0.0);
        assert!(focus_distance > 0.0);

        let pixel_scale = film_width / (resolution.x as f32);
        let resolution_minus_one = ScreenSize::new(resolution.x - 1, resolution.y - 1);
        let film_origin_uv = resolution_minus_one.cast::<FloatType>() * pixel_scale / 2.0;
        let film_origin_offset = -forward.as_ref() * focal_length
            + right.as_ref() * film_origin_uv.x
            - up.as_ref() * film_origin_uv.y;

        Camera {
            center,

            resolution,

            up,
            right,
            film_origin_offset,
            pixel_pitch: pixel_scale,
            lens_radius: focal_length / (2.0 * f_number),
            lens_weight: focal_length / focus_distance,
        }
    }
}

impl Camera {
    pub fn get_resolution(&self) -> ScreenSize {
        self.resolution
    }

    /// Samples a new ray from the camera for the given image pixel.
    pub fn sample_ray(&self, point: &ScreenPoint, rng: &mut impl rand::Rng) -> Ray {
        //TODO: Figure out a better reconstruction kernel for the pixel than a square
        let film_u = point.x as f32 + rng.random_range(-0.5..=0.5);
        let film_v = point.y as f32 + rng.random_range(-0.5..=0.5);
        let film_point_offset = self.film_origin_offset
            + self.up.as_ref() * (film_v * self.pixel_pitch)
            - self.right.as_ref() * (film_u * self.pixel_pitch);

        let lens_uv: [f32; 2] = rand_distr::UnitDisc.sample(rng);
        let lens_vector = self.right.as_ref() * (self.lens_radius * lens_uv[0])
            + self.up.as_ref() * (self.lens_radius * lens_uv[1]);

        let direction = lens_vector * self.lens_weight - film_point_offset;

        let ray = Ray::new(self.center + lens_vector, direction);
        ray
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use assert2::assert;

    #[test]
    fn left_right_up_down() {
        // X goes right, Y goes away, Z goes up
        let camera = Camera::builder()
            .center(WorldPoint::new(0.0, 0.0, 0.0))
            .forward(WorldVector::new(0.0, 1.0, 0.0))
            .up(WorldVector::new(0.0, 0.0, 1.0))
            .resolution(ScreenSize::new(800, 600))
            .film_width(36e-3)
            .focal_length(50e-3)
            .f_number(std::f32::INFINITY)
            .focus_distance(2.0)
            .build();
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
