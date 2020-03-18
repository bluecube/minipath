#![feature(specialization)]

mod camera;
mod geometry;
mod image_buffer;
mod image_file_buffer;
#[cfg(feature = "gui")]
mod image_window;
mod parallel_for_each;
mod renderer;
mod scene;
mod screen_block;
mod util;

use geometry::*;

#[cfg(feature = "gui")]
fn make_output(size: ScreenSize) -> util::SimpleResult<Box<dyn image_buffer::ImageBuffer>> {
    Ok(Box::new(image_window::ImageWindow::new(
        "minipath",
        size.width,
        size.height,
    )?))
}

#[cfg(not(feature = "gui"))]
fn make_output(size: ScreenSize) -> util::SimpleResult<Box<dyn image_buffer::ImageBuffer>> {
    Ok(Box::new(image_file_buffer::ImageFileBuffer::new(
        size.width,
        size.height,
    )))
}

fn main() -> util::SimpleResult {
    let settings = renderer::RenderSettings {
        image_size: euclid::size2(800, 600),
        block_size: std::num::NonZeroU32::new(50).unwrap(),
        sample_count: std::num::NonZeroU32::new(100).unwrap(),
    };
    renderer::render(&settings, make_output)
}
