#![feature(specialization)]

mod parallel_for_each;
mod screen_block;
mod util;

mod image_buffer;
mod image_file_buffer;
#[cfg(feature = "gui")]
mod image_window;

use screen_block::ScreenBlockExt;

use euclid;
use rand;

#[cfg(feature = "gui")]
fn make_output(w: u32, h: u32) -> util::SimpleResult<Box<dyn image_buffer::ImageBuffer>> {
    Ok(Box::new(image_window::ImageWindow::new("minipath", w, h)?))
}

#[cfg(not(feature = "gui"))]
fn make_output(w: u32, h: u32) -> util::SimpleResult<Box<dyn image_buffer::ImageBuffer>> {
    Ok(Box::new(image_file_buffer::ImageFileBuffer::new(w, h)))
}

fn run_all(
    output: Box<dyn image_buffer::ImageBuffer>,
    block_iterator: screen_block::SpiralChunks,
) -> util::SimpleResult {
    let output_writer = output.make_writer();

    parallel_for_each::parallel_for_each(
        block_iterator,
        |_worker_id| -> Result<_, parallel_for_each::NoError> { Ok(image::RgbaImage::new(50, 50)) },
        |_buffer, block| -> util::SimpleResult<_> {
            // Pretend to render a block
            use rand::Rng;

            let mut rng = rand::thread_rng();
            std::thread::sleep(std::time::Duration::from_millis(rng.gen_range(500, 2000)));
            output_writer.write(
                block,
                image::RgbaImage::from_pixel(
                    50,
                    50,
                    image::Rgba([
                        rng.gen_range(0, 255),
                        rng.gen_range(0, 255),
                        rng.gen_range(0, 255),
                        rng.gen_range(128, 255),
                    ]),
                ),
            )?;

            Ok(())
        },
        || -> util::SimpleResult<_> {
            output.run()?;
            Ok(parallel_for_each::Continue::Stop)
        },
        || {
            // TODO: Notify the background task that we are finished
        },
        parallel_for_each::WorkerCount::Auto,
    )?;

    Ok(())
}

fn main() -> util::SimpleResult {
    let w = make_output(800, 600)?;
    run_all(w, euclid::rect(0, 0, 800, 600).to_box2d().spiral_chunks(50))?;
    Ok(())
}
