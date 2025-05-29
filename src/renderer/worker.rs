use image::RgbaImage;
use rand::{SeedableRng, rngs::SmallRng};

use crate::{
    camera::Camera,
    geometry::{ScreenBlock, ScreenPoint},
    renderer::RenderSettings,
    scene::Scene,
    screen_block::ScreenBlockExt as _,
    util::Rgba,
};

pub struct Worker {
    rng: SmallRng,
}

impl Worker {
    pub fn new(_worker_id: usize) -> Self {
        Self {
            rng: SmallRng::from_os_rng(),
        }
    }

    pub fn render_tile(
        &mut self,
        scene: &Scene,
        camera: &Camera,
        settings: &RenderSettings,
        tile: &ScreenBlock,
        buffer: &mut RgbaImage,
    ) {
        for point in tile.internal_points() {
            let mut pixel_sum = Rgba::new(0f64, 0f64, 0f64, 0f64);
            for _i in 0..settings.sample_count.get() {
                pixel_sum += self.render_sample(scene, camera, settings, &point);
            }
            let pixel = pixel_sum * (1.0 / settings.sample_count.get() as f64);

            let buffer_position = point - tile.min;
            buffer.put_pixel(buffer_position.x, buffer_position.y, color_to_image(pixel));
        }
    }

    fn render_sample(
        &mut self,
        scene: &Scene,
        camera: &Camera,
        settings: &RenderSettings,
        point: &ScreenPoint,
    ) -> Rgba {
        let ray = camera.sample_ray(&point, &mut self.rng);

        let floor_hit_distance = -ray.origin.z / ray.direction.z;
        if floor_hit_distance < 0.0 {
            Rgba::new(0.0, 0.0, 0.0, 0.0)
        } else {
            const TILE_SIZE: f64 = 1.0;
            const LINE_WIDTH: f64 = 1e-2;
            let floor_hit_point = ray.origin + ray.direction * floor_hit_distance;
            if floor_hit_point.x.abs() % TILE_SIZE < LINE_WIDTH
                || floor_hit_point.y.abs() % TILE_SIZE < LINE_WIDTH
            {
                Rgba::new(0.0, 0.0, 0.0, 1.0)
            } else {
                Rgba::new(0.7, 0.8, 1.0, 1.0)
            }
        }
    }
}

/// Maps a 0-1 f64 rgba pixel to pixel type compatible with module image.
pub fn color_to_image(color: Rgba) -> image::Rgba<u8> {
    image::Rgba([
        (color.r * 255.0).round().max(0.0).min(255.0) as u8,
        (color.g * 255.0).round().max(0.0).min(255.0) as u8,
        (color.b * 255.0).round().max(0.0).min(255.0) as u8,
        (color.a * 255.0).round().max(0.0).min(255.0) as u8,
    ])
}