use crate::image_buffer;
use crate::parallel_for_each;
use crate::screen_block;
use crate::util;

use screen_block::ScreenBlockExt;

pub fn render<F>(width: u32, height: u32, chunk_size: u32, buffer_factory: F) -> util::SimpleResult
where
    F: FnOnce(u32, u32) -> util::SimpleResult<Box<dyn image_buffer::ImageBuffer>>,
{
    let buffer = buffer_factory(width, height)?;
    let size = screen_block::ScreenSize::new(width, height);
    let block_iterator = screen_block::ScreenBlock::from_size(size).spiral_chunks(chunk_size);

    let buffer_writer = buffer.make_writer();

    parallel_for_each::parallel_for_each(
        block_iterator,
        |_worker_id| -> Result<_, util::NoError> {
            use rand::SeedableRng;
            Ok((rand::rngs::SmallRng::from_entropy(), image::RgbaImage::new(chunk_size, chunk_size)))
        },
        |state, block| -> util::SimpleResult<_> {
            let (ref mut rng, ref mut buffer) = state;
            render_block(block, rng, buffer);
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

fn render_block<T: rand::Rng>(
    block: screen_block::ScreenBlock,
    rng: &mut T,
    output_buffer: &mut image::RgbaImage) {
    // Pretend to render a block
    std::thread::sleep(std::time::Duration::from_millis(rng.gen_range(500, 2000)));

    *output_buffer = image::RgbaImage::from_pixel(
        block.width(),
        block.height(),
        image::Rgba([
            rng.gen_range(0, 255),
            rng.gen_range(0, 255),
            rng.gen_range(0, 255),
            rng.gen_range(128, 255),
        ]),
    );
}
