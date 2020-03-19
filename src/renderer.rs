use crate::camera;
use crate::geometry::*;
use crate::image_buffer;
use crate::parallel_for_each;
use crate::screen_block;
use crate::util;

use screen_block::ScreenBlockExt;

#[derive(Copy, Clone, Debug)]
pub struct RenderSettings {
    pub block_size: std::num::NonZeroU32,
    pub sample_count: std::num::NonZeroU32,
}

pub fn render<F>(
    camera: &camera::Camera,
    settings: &RenderSettings,
    buffer_factory: F,
) -> util::SimpleResult
where
    F: FnOnce(ScreenSize) -> util::SimpleResult<Box<dyn image_buffer::ImageBuffer>>,
{
    let block_size = settings.block_size.get();
    let buffer = buffer_factory(camera.get_resolution())?;
    let block_iterator = ScreenBlock::from_size(camera.get_resolution()).spiral_chunks(block_size);

    let buffer_writer = buffer.make_writer();

    parallel_for_each::parallel_for_each(
        block_iterator,
        |_worker_id| -> Result<_, util::NoError> {
            use rand::SeedableRng;
            Ok((
                rand::rngs::SmallRng::from_entropy(),
                image::RgbaImage::new(block_size, block_size),
            ))
        },
        |state, block| -> util::SimpleResult<_> {
            let (ref mut rng, ref mut buffer) = state;
            render_block(block, camera, settings, rng, buffer);
            buffer_writer.write(block, buffer)?;

            Ok(())
        },
        || -> util::SimpleResult<_> {
            buffer.run()?;
            Ok(parallel_for_each::Continue::Stop)
        },
        || {
            // TODO: Notify the background task that we are finished
        },
        parallel_for_each::WorkerCount::Auto,
    )?;

    Ok(())
}

fn render_block(
    block: ScreenBlock,
    camera: &camera::Camera,
    settings: &RenderSettings,
    rng: &mut impl rand::Rng,
    output_buffer: &mut image::RgbaImage,
) {
    for point in block.internal_points() {
        let mut pixel_sum = util::Rgba::new(0f64, 0f64, 0f64, 0f64);
        for _i in 0..settings.sample_count.get() {
            pixel_sum += render_sample(point, camera, settings, rng);
        }
        let pixel = pixel_sum * (1.0 / settings.sample_count.get() as f64);
        let buffer_position = point - block.min;
        output_buffer.put_pixel(buffer_position.x, buffer_position.y, color_to_image(pixel));
    }
}

fn render_sample(
    point: ScreenPoint,
    camera: &camera::Camera,
    settings: &RenderSettings,
    rng: &mut impl rand::Rng,
) -> util::Rgba {
    util::Rgba::new(
        rng.gen_range(0.0, 1.0),
        rng.gen_range(0.0, 1.0),
        rng.gen_range(0.0, 1.0),
        rng.gen_range(0.0, 1.0),
    )
}

/// Maps a 0-1 f64 rgba pixel to pixel type compatible with module image.
pub fn color_to_image(color: util::Rgba) -> image::Rgba<u8> {
    image::Rgba([
        (color.r * 255.0).round().max(0.0).min(255.0) as u8,
        (color.g * 255.0).round().max(0.0).min(255.0) as u8,
        (color.b * 255.0).round().max(0.0).min(255.0) as u8,
        (color.a * 255.0).round().max(0.0).min(255.0) as u8,
    ])
}
