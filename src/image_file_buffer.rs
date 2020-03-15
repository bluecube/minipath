use crate::image_buffer;
use crate::screen_block;
use crate::util;

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

impl image_buffer::ImageBuffer for ImageFileBuffer {
    fn run(&self) -> util::SimpleResult {
        Ok(())
    }

    /// Creates a writer function that can write data into the window from different thread.
    fn make_writer<'a>(&'a self) -> Box<dyn image_buffer::ImageBufferWriter + 'a> {
        Box::new(Writer(&self.img))
    }

    fn save(&self, path: &std::path::Path) -> util::SimpleResult {
        (*self.img.lock().unwrap()).save(path)?;
        Ok(())
    }
}

pub struct Writer<'a>(&'a sync::Mutex<image::RgbaImage>);

impl<'a> image_buffer::ImageBufferWriter for Writer<'a> {
    fn write(
        &self,
        block: screen_block::ScreenBlock,
        block_buffer: &image::RgbaImage,
    ) -> util::SimpleResult {
        debug_assert_eq!(block_buffer.width(), block.width());
        debug_assert_eq!(block_buffer.height(), block.width());

        (*self.0.lock().unwrap()).copy_from(block_buffer, block.min.x, block.min.y)?;

        Ok(())
    }
}
