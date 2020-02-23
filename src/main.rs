mod image_window;
mod screen_block;

use screen_block::ScreenBlockExt;

use anyhow;
use crossbeam_utils;
use euclid;
use num_cpus;
use rand;

use std::sync;
use std::thread;
use std::time;

fn run_all(
    output: image_window::ImageWindow,
    block_iterator: screen_block::SpiralChunks,
) -> anyhow::Result<()> {
    let block_iterator = sync::Mutex::new(block_iterator);

    crossbeam_utils::thread::scope(|scope| -> anyhow::Result<()> {
        for _worker_id in 0..num_cpus::get() {
            let block_iterator = &block_iterator;
            let output_writer = output.make_writer();

            scope.spawn(move |_| {
                loop {
                    let block = match (*(block_iterator.lock().unwrap())).next() {
                        Some(block) => block,
                        None => break,
                    };

                    // Pretend to render a block
                    use rand::Rng;
                    let mut rng = rand::thread_rng();
                    thread::sleep(time::Duration::from_millis(rng.gen_range(500, 2000)));
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
                    )
                    .unwrap();
                }
            });
        }
        let run_result = output.run();
        // When the event loop finishes, kill the block iterator to stop any further blocks from being rendered
        (*(block_iterator.lock().unwrap())).kill();
        run_result?;

        Ok(())
    })
    .unwrap()?; // Propagate panics and unwrap internal errors

    Ok(())
}

fn main() -> anyhow::Result<()> {
    let w = image_window::ImageWindow::new("minipath", 800, 600)?;
    run_all(w, euclid::rect(0, 0, 800, 600).to_box2d().spiral_chunks(50))?;
    Ok(())
}
