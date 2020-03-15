use crate::screen_block;
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
    fn write(
        &self,
        block: screen_block::ScreenBlock,
        block_buffer: &image::RgbaImage,
    ) -> util::SimpleResult;
}

/// This is an implementation of the unit tests that is shared for all impls of
/// this trait. That's why the test mod is public and there is no actual #[test] inside.
#[cfg(test)]
pub mod test {
    use super::*;

    /// Tests that the buffer correctly reconstruct an image from small writes
    fn test_image_buffer() {}
}
