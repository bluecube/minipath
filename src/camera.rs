use crate::geometry::*;
use rand_distr;

#[derive(Copy, Clone, Debug)]
pub struct Camera {
    center: WorldPoint,

    // Direction vectors are always perpendicular to each other and normalized
    forward: WorldVector,
    up: WorldVector,
    right: WorldVector,

    resolution: ScreenSize,
    focus_distance: WorldDistance,
    pixel_scale: euclid::Scale<f64, ScreenSpace, WorldSpace>,
    film_origin_offset: WorldVector,
    lens_radius: WorldDistance,
}

impl Camera {
    /// Creates new camera and precomputes what needs to be precomputed.
    /// `forward` and `up` must be nonzero and non colinear.
    pub fn new(
        center: WorldPoint,
        forward: WorldVector,
        up: WorldVector,
        resolution: ScreenSize,
        film_width: WorldDistance,
        focal_length: WorldDistance,
        f_number: f64,
        focus_distance: WorldDistance,
    ) -> Self {
        assert_ne!(forward, WorldVector::zero());
        let forward = forward.normalize();
        assert_ne!(up, WorldVector::zero());
        let right = forward.cross(up).normalize();
        assert_ne!(
            right,
            WorldVector::zero(),
            "`up` and `forward` must be linearly independent"
        );
        let up = right.cross(forward).normalize();

        assert!(resolution.width > 0);
        assert!(resolution.height > 0);
        assert!(film_width.get() > 0.0);
        assert!(focal_length.get() > 0.0);
        assert!(f_number > 0.0);
        assert!(focus_distance.get() > 0.0);

        let pixel_scale = film_width / euclid::Length::new(resolution.width as f64);
        let film_origin_uv = resolution.to_vector().to_f64() * pixel_scale / 2.0;
        let film_origin_offset =
            forward * focal_length.get() + up * film_origin_uv.x - right * film_origin_uv.y;

        let lens_radius = focal_length / (2.0 * f_number);

        Camera {
            center,
            forward,
            up,
            right,
            resolution,
            focus_distance,
            pixel_scale,
            film_origin_offset,
            lens_radius,
        }
    }

    pub fn get_resolution(&self) -> ScreenSize {
        self.resolution
    }

    /// Samples a new ray from the camera for the given image pixel.
    pub fn sample_ray(&self, point: ScreenPoint, rng: &mut impl rand::Rng) -> Ray {
        use rand::distributions::Distribution;

        //TODO: Figure out a better reconstruction kernel for the pixel than a square
        let film_u = euclid::Length::new(point.x as f64 + rng.gen_range(-0.5, 0.5));
        let film_v = euclid::Length::new(point.y as f64 + rng.gen_range(-0.5, 0.5));
        let film_point_offset = self.film_origin_offset
            - self.up * (film_v * self.pixel_scale).get()
            + self.right * (film_u * self.pixel_scale).get();

        // The point that in focus for this film point for all points on the lens
        let focus_vector =
            film_point_offset * (self.focus_distance / film_point_offset.dot(self.forward)).get();

        let lens_uv: [f64; 2] = rand_distr::UnitDisc.sample(rng);
        let lens_vector =
            self.right * (self.lens_radius * lens_uv[0]).get() + self.up * (self.lens_radius * lens_uv[1]).get();

        Ray {
            origin: self.center + lens_vector,
            direction: (focus_vector - lens_vector).normalize(),
        }
    }
}
