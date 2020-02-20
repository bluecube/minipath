mod screen_block;
mod image_window;

use anyhow;

fn main() -> anyhow::Result<()> {
    let mut w = image_window::ImageWindow::new("Hello, world!", 800, 600)?;
    w.event_loop()?;
    Ok(())
}
