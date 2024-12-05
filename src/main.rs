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
fn make_output(size: ScreenSize) -> anyhow::Result<Box<dyn image_buffer::ImageBuffer>> {
    Ok(Box::new(image_window::ImageWindow::new(
        "minipath",
        size.width,
        size.height,
    )?))
}

#[cfg(not(feature = "gui"))]
fn make_output(size: ScreenSize) -> anyhow::Result<Box<dyn image_buffer::ImageBuffer>> {
    Ok(Box::new(image_file_buffer::ImageFileBuffer::new(
        size.width,
        size.height,
    )))
}

fn main() -> anyhow::Result<()> {
    let camera = camera::Camera::new(
        WorldPoint::new(0.0, 0.0, 2.0),
        WorldVector::new(0.0, 1.0, 0.0),
        WorldVector::new(0.0, 0.0, 1.0),
        ScreenSize::new(800, 600),
        WorldDistance::new(36e-3),
        WorldDistance::new(50e-3),
        4.8,
        WorldDistance::new(5.0),
    );
    let settings = renderer::RenderSettings {
        block_size: std::num::NonZeroU32::new(50).unwrap(),
        sample_count: std::num::NonZeroU32::new(100).unwrap(),
    };
    renderer::render(&camera, &settings, make_output)
}
