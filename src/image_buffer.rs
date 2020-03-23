use crate::geometry::*;
use crate::util;

/// Trait for an image buffer that can be accessed from multiple threads
pub trait ImageBuffer {
    /// Runs event loop belonging to this image, if necessary.
    fn run(&self) -> util::SimpleResult;

    /// Creates a writer function that can write data into the image from different thread.
    fn make_writer<'a>(&'a self) -> Box<dyn ImageBufferWriter + 'a>;

    /// Saves the content of the buffer to a file
    fn save(&self, path: &std::path::Path) -> util::SimpleResult;
}

pub trait ImageBufferWriter: Sync + Send {
    fn write(&self, block: ScreenBlock, block_buffer: &image::RgbaImage) -> util::SimpleResult;
}

/// This is an implementation of the unit tests that is shared for all impls of
/// this trait. That's why the test mod is public and there is no actual #[test] inside.
#[cfg(test)]
pub mod test {
    use super::*;
    use crate::screen_block;
    use assert2::assert;

    fn create_test_pattern(block: ScreenBlock) -> image::RgbaImage {
        assert!(!block.is_empty_or_negative());
        image::RgbaImage::from_fn(block.width(), block.height(), |x, y| {
            let x = (x + block.min.x) as f64;
            let y = (y + block.min.y) as f64;

            image::Rgba([
                ((x / 50.0).sin() * 127.0 + 127.0) as u8,
                ((y / 50.0).sin() * 127.0 + 127.0) as u8,
                (((x + y) / 50.0).sin() * 127.0 + 127.0) as u8,
                (((x - y) / 50.0).sin() * 50.0 + 200.0) as u8,
            ])
        })
    }

    /// Creates an image buffer and randomly (but single threadedly) fills it with test patern.
    fn fill_image_buffer(block: ScreenBlock, chunk_size: u32, buffer: &mut dyn ImageBuffer) {
        assert!(block.min.x == 0);
        assert!(block.min.y == 0);

        use rand::seq::SliceRandom;
        use rand::SeedableRng;
        use screen_block::ScreenBlockExt;

        let mut blocks: Vec<_> = block.spiral_chunks(chunk_size).collect();
        let mut rng = rand::rngs::StdRng::seed_from_u64(0); // we just need a single non-trivial shuffle
        blocks.shuffle(&mut rng);

        let writer = buffer.make_writer();

        crossbeam_utils::thread::scope(|scope| {
            scope.spawn(|_| {
                for block in blocks {
                    writer.write(block, &create_test_pattern(block)).unwrap();
                }
            });
            buffer.run().unwrap();
        })
        .unwrap();
    }

    /// Tests that the buffer correctly reconstruct an image from small writes and that saves /
    /// loads work.
    pub fn test_image_buffer(
        width: u32,
        height: u32,
        chunk_size: u32,
        buffer: &mut dyn ImageBuffer,
    ) {
        let size = ScreenSize::new(width, height);
        let block = ScreenBlock::from_size(size);

        fill_image_buffer(block, chunk_size, buffer);

        let file = tempfile::Builder::new()
            .suffix(".png")
            .tempfile()
            .unwrap()
            .into_temp_path();

        buffer.save(&file).unwrap();

        let img_dyn = image::open(&file).unwrap();
        let pattern = create_test_pattern(block);

        let img_rgba = img_dyn.as_rgba8().unwrap();
        assert!(img_rgba.dimensions() == pattern.dimensions());
        assert!(img_rgba
            .pixels()
            .zip(pattern.pixels())
            .all(|pair| pair.0 == pair.1));
    }
}
