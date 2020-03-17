use crate::image_buffer;
use crate::screen_block;
use crate::util;

use image;
use parking_lot;

use image::GenericImage;

/// ImageBuffer that can only save its content to file.
pub struct ImageFileBuffer {
    img: parking_lot::Mutex<image::RgbaImage>,
}

impl ImageFileBuffer {
    /// Creates new image file buffer
    pub fn new(width: u32, height: u32) -> ImageFileBuffer {
        ImageFileBuffer {
            img: parking_lot::Mutex::new(image::RgbaImage::new(width, height)),
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
        self.img.lock().save(path)?;
        Ok(())
    }
}

pub struct Writer<'a>(&'a parking_lot::Mutex<image::RgbaImage>);

impl<'a> image_buffer::ImageBufferWriter for Writer<'a> {
    fn write(
        &self,
        block: screen_block::ScreenBlock,
        block_buffer: &image::RgbaImage,
    ) -> util::SimpleResult {
        debug_assert!(block.width() <= block_buffer.width());
        debug_assert!(block.height() <= block_buffer.height());

        self.0
            .lock()
            .copy_from(block_buffer, block.min.x, block.min.y)?;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use proptest_attr_macro::proptest;

    #[test]
    fn test_file_buffer() {
        const WIDTH: u32 = 200;
        const HEIGHT: u32 = 200;
        const CHUNK_SIZE: u32 = 51;

        let mut buffer = ImageFileBuffer::new(WIDTH, HEIGHT);
        image_buffer::test::test_image_buffer(WIDTH, HEIGHT, CHUNK_SIZE, &mut buffer);
    }
}
