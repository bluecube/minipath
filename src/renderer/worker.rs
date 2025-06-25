use std::marker::PhantomData;

use image::RgbaImage;
use rand::{SeedableRng, rngs::SmallRng};

use crate::scene::triangle_bvh;
use crate::{
    camera::Camera,
    geometry::{ScreenBlock, ScreenPoint},
    renderer::RenderSettings,
    scene::{Object, Scene},
    util::Rgba,
};

pub struct Worker<O: Object> {
    rng: SmallRng,
    bvh_stack_cache: triangle_bvh::StackCache,
    _phantom: PhantomData<O>,
}

impl<O: Object + Sync> Worker<O> {
    pub fn new(_worker_id: usize) -> Self {
        Self {
            rng: SmallRng::from_os_rng(),
            bvh_stack_cache: Default::default(),
            _phantom: Default::default(),
        }
    }

    pub fn render_tile(
        &mut self,
        scene: &Scene<O>,
        camera: &Camera,
        settings: &RenderSettings,
        tile: &ScreenBlock,
        buffer: &mut RgbaImage,
    ) {
        for point in tile.internal_points() {
            let mut pixel_sum = Rgba::new(0.0, 0.0, 0.0, 0.0);
            for _i in 0..settings.sample_count.get() {
                pixel_sum += self.render_sample(scene, camera, settings, &point);
            }
            let pixel = pixel_sum * (1.0 / settings.sample_count.get() as f32);

            let buffer_position = point - tile.min;
            buffer.put_pixel(buffer_position.x, buffer_position.y, color_to_image(pixel));
        }
    }

    fn render_sample(
        &mut self,
        scene: &Scene<O>,
        camera: &Camera,
        _settings: &RenderSettings,
        point: &ScreenPoint,
    ) -> Rgba {
        let ray = camera.sample_ray(point, &mut self.rng);

        if let Some(intersection) = scene.object.intersect(&ray, &mut self.bvh_stack_cache) {
            let dot = ray.direction.dot(&intersection.normal).abs();
            Rgba::new(dot, dot, dot, 1.0)
        } else {
            Rgba::new(0.0, 0.0, 0.0, 0.0)
        }
    }
}

/// Maps a 0-1 f32 rgba pixel to pixel type compatible with module image.
pub fn color_to_image(color: Rgba) -> image::Rgba<u8> {
    image::Rgba([
        (color.r * 255.0).round().clamp(0.0, 255.0) as u8,
        (color.g * 255.0).round().clamp(0.0, 255.0) as u8,
        (color.b * 255.0).round().clamp(0.0, 255.0) as u8,
        (color.a * 255.0).round().clamp(0.0, 255.0) as u8,
    ])
}
