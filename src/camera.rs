use crate::geometry::*;
use rand_distr::{self, Distribution as _};

#[derive(Copy, Clone, Debug)]
pub struct Camera {
    center: WorldPoint,

    resolution: ScreenSize,

    forward: WorldVector,
    up: WorldVector,
    right: WorldVector,
    film_origin_offset: WorldVector,
    pixel_scale: euclid::Scale<f32, ScreenSpace, WorldSpace>,
    lens_radius: WorldDistance,
    lens_weight: euclid::Scale<f32, WorldSpace, WorldSpace>,
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
        f_number: f32,
        focus_distance: WorldDistance,
    ) -> Self {
        assert_ne!(forward, WorldVector::zero());
        let forward = forward.normalize();
        assert_ne!(up, WorldVector::zero());
        let up = up.normalize();
        let right = forward.cross(up);
        assert_ne!(
            right,
            WorldVector::zero(),
            "`up` and `forward` must be linearly independent"
        );
        let right = right.normalize();
        let up = right.cross(forward).normalize();

        assert!(resolution.width > 0);
        assert!(resolution.height > 0);
        assert!(film_width.get() > 0.0);
        assert!(focal_length.get() > 0.0);
        assert!(f_number > 0.0);
        assert!(focus_distance.get() > 0.0);

        let pixel_scale = film_width / euclid::Length::new(resolution.width as f32);
        let resolution_minus_one = ScreenSize::new(resolution.width - 1, resolution.height - 1);
        let film_origin_uv = resolution_minus_one.to_f32().to_vector() * pixel_scale / 2.0;
        let film_origin_offset =
            -forward * focal_length.get() + right * film_origin_uv.x - up * film_origin_uv.y;

        let lens_radius = focal_length / (2.0 * f_number);
        let lens_weight = focal_length / focus_distance;

        Camera {
            center,

            resolution,

            forward,
            up,
            right,
            film_origin_offset,
            pixel_scale,
            lens_radius,
            lens_weight,
        }
    }

    pub fn get_resolution(&self) -> ScreenSize {
        self.resolution
    }

    /// Samples a new ray from the camera for the given image pixel.
    pub fn sample_ray(&self, point: &ScreenPoint, rng: &mut impl rand::Rng) -> Ray {
        //TODO: Figure out a better reconstruction kernel for the pixel than a square
        let film_u = euclid::Length::new(point.x as f32 + rng.random_range(-0.5..=0.5));
        let film_v = euclid::Length::new(point.y as f32 + rng.random_range(-0.5..=0.5));
        let film_point_offset = self.film_origin_offset
            + self.up * (film_v * self.pixel_scale).get()
            - self.right * (film_u * self.pixel_scale).get();

        let lens_uv: [f32; 2] = rand_distr::UnitDisc.sample(rng);
        let lens_vector = self.right * (self.lens_radius * lens_uv[0]).get()
            + self.up * (self.lens_radius * lens_uv[1]).get();

        let direction = lens_vector * self.lens_weight - film_point_offset;

        let ray = Ray {
            origin: self.center + lens_vector,
            direction: direction.normalize(),
        };
        ray
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use assert2::assert;
    use proptest::prelude::*;
    use test_strategy::proptest;

    use crate::geometry::test::*;

    impl Arbitrary for Camera {
        type Parameters = ();
        type Strategy = proptest::strategy::BoxedStrategy<Self>;
        fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
            (
                any::<WorldPointWrapper>(),
                any::<NonzeroWorldVectorWrapper>(),
                any::<NonzeroWorldVectorWrapper>(),
                any::<ScreenSizeWrapper>(),
                any::<PositiveWorldDistanceWrapper>(),
                any::<PositiveWorldDistanceWrapper>(),
                any::<PositiveWorldDistanceWrapper>(), // Because of the conditioning we already have for postivie world distance
                any::<PositiveWorldDistanceWrapper>(),
            )
                .prop_filter_map(
                    "camera up and forward vectors are linearly dependent",
                    |tuple| {
                        if tuple.1.normalize().cross(tuple.2.normalize()) == WorldVector::zero() {
                            None
                        } else {
                            Some(Camera::new(
                                *tuple.0,
                                *tuple.1,
                                *tuple.2,
                                *tuple.3,
                                *tuple.4,
                                *tuple.5,
                                tuple.6.get(),
                                *tuple.7,
                            ))
                        }
                    },
                )
                .boxed()
        }
    }

    #[derive(Copy, Clone, Debug)]
    struct CameraAndPoint(Camera, ScreenPoint);

    impl Arbitrary for CameraAndPoint {
        type Parameters = ();
        type Strategy = proptest::strategy::BoxedStrategy<Self>;
        fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
            any::<Camera>()
                .prop_flat_map(|camera| {
                    (
                        Just(camera),
                        0..camera.get_resolution().width,
                        0..camera.get_resolution().height,
                    )
                })
                .prop_map(|camera_and_xy| {
                    CameraAndPoint(
                        camera_and_xy.0,
                        ScreenPoint::new(camera_and_xy.1, camera_and_xy.2),
                    )
                })
                .boxed()
        }
    }

    /// Checks that output ray always aims roughly along the forward vector
    #[proptest]
    fn correct_direction(camera_and_point: CameraAndPoint) {
        let camera = camera_and_point.0;
        let point = camera_and_point.1;
        let ray = camera.sample_ray(&point, &mut rand::rng());

        assert!(
            ray.direction.dot(camera.forward) > 0.0,
            "ray = {:?}, camera = {:?}",
            ray,
            camera
        );
    }

    #[test]
    fn left_right_up_down() {
        // X goes right, Y goes away, Z goes up
        let camera = Camera::new(
            WorldPoint::new(0.0, 0.0, 0.0),
            WorldVector::new(0.0, 1.0, 0.0),
            WorldVector::new(0.0, 0.0, 1.0),
            ScreenSize::new(800, 600),
            WorldDistance::new(36e-3),
            WorldDistance::new(50e-3),
            std::f32::INFINITY,
            WorldDistance::new(2.0),
        );
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
