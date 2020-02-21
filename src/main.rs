mod screen_block;
mod image_window;

use screen_block::ScreenBlockExt;

use anyhow;

use rayon::iter::ParallelBridge;
use rayon::iter::ParallelIterator;
use std::time;
use std::thread;

use euclid;

fn run_all(mut output: image_window::ImageWindow, block_iterator: screen_block::SpiralChunks) -> anyhow::Result<()> {
    let output_writer = output.make_writer();

    std::thread::spawn(move || {
        block_iterator.par_bridge()
            .try_for_each_with(&output_writer, |output_writer, block| {
                println!("Rendering on block {:?}!", block);
                thread::sleep(time::Duration::from_millis(1000));
                output_writer(block)
            });
        println!("Rendering done!");
    });
    output.event_loop()?;
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let mut w = image_window::ImageWindow::new("minipath", 800, 600)?;
    run_all(w, euclid::rect(0, 0, 800, 600).to_box2d().spiral_chunks(50))?;
    Ok(())
}
