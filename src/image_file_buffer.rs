use crate::image_buffer;
use crate::screen_block;

use image;
use std::sync;

use image::GenericImage;

/// ImageBuffer that can only save its content to file.
pub struct ImageFileBuffer {
    img: sync::Mutex<image::RgbaImage>,
}

impl ImageFileBuffer {
    /// Creates new image file buffer
    pub fn new(width: u32, height: u32) -> ImageFileBuffer {
        ImageFileBuffer {
            img: sync::Mutex::new(image::RgbaImage::new(width, height)),
        }
    }
}

impl<'a> image_buffer::ImageBuffer<'a> for ImageFileBuffer {
    type Writer = Writer<'a>;
    type RunError = ();
    type SaveError = image::error::ImageError;

    fn run(&self) -> Result<(), Self::RunError> {
        Ok(())
    }

    /// Creates a writer function that can write data into the window from different thread.
    fn make_writer(&'a self) -> Writer<'a> {
        Writer(&self.img)
    }

    fn save(&self, path: &std::path::Path) -> Result<(), Self::SaveError> {
        (*self.img.lock().unwrap()).save(path)?;
        Ok(())
    }
}

pub struct Writer<'a>(&'a sync::Mutex<image::RgbaImage>);

impl<'a> image_buffer::ImageBufferWriter for Writer<'a> {
    type WriteError = image::error::ImageError;

    fn write(
        &self,
        block: screen_block::ScreenBlock,
        block_buffer: image::RgbaImage,
    ) -> Result<(), Self::WriteError> {
        debug_assert_eq!(block_buffer.width(), block.width());
        debug_assert_eq!(block_buffer.height(), block.width());

        (*self.0.lock().unwrap()).copy_from(&block_buffer, block.min.x, block.min.y)?;

        Ok(())
    }
}
