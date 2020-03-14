#![feature(specialization)]

mod image_buffer;
mod image_file_buffer;
#[cfg(feature = "gui")]
mod image_window;
mod parallel_for_each;
mod renderer;
mod screen_block;
mod util;

#[cfg(feature = "gui")]
fn make_output(w: u32, h: u32) -> util::SimpleResult<Box<dyn image_buffer::ImageBuffer>> {
    Ok(Box::new(image_window::ImageWindow::new("minipath", w, h)?))
}

#[cfg(not(feature = "gui"))]
fn make_output(w: u32, h: u32) -> util::SimpleResult<Box<dyn image_buffer::ImageBuffer>> {
    Ok(Box::new(image_file_buffer::ImageFileBuffer::new(w, h)))
}

fn main() -> util::SimpleResult {
    renderer::render(800, 600, 50, make_output)
}
