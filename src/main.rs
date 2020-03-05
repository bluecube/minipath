#![feature(specialization)]

mod image_window;
mod parallel_for_each;
mod screen_block;

use screen_block::ScreenBlockExt;

use euclid;
use rand;

type AnyError = Box<dyn std::error::Error + Send + Sync + 'static>;
type SimpleResult = Result<(), AnyError>;

fn run_all(
    output: image_window::ImageWindow,
    block_iterator: screen_block::SpiralChunks,
) -> SimpleResult {
    let output_writer = output.make_writer();

    parallel_for_each::parallel_for_each(
        block_iterator,
        |_worker_id| -> Result<_, parallel_for_each::NoError> { Ok(image::RgbaImage::new(50, 50)) },
        |_buffer, block| -> Result<_, AnyError> {
            // Pretend to render a block
            use rand::Rng;
            let mut rng = rand::thread_rng();
            std::thread::sleep(std::time::Duration::from_millis(rng.gen_range(500, 2000)));
            output_writer(
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
        || -> Result<_, AnyError> {
            output.run()?;
            Ok(parallel_for_each::Continue::Stop)
        },
        parallel_for_each::WorkerCount::Auto,
    )?;

    Ok(())
}

fn main() -> SimpleResult {
    let w = image_window::ImageWindow::new("minipath", 800, 600)?;
    run_all(w, euclid::rect(0, 0, 800, 600).to_box2d().spiral_chunks(50))?;
    Ok(())
}
