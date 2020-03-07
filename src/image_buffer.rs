use crate::screen_block;

/// Trait for an image buffer that can be accessed from multiple threads
pub trait ImageBuffer<'a> {
    type RunError;
    type SaveError;
    type Writer: ImageBufferWriter;

    /// Runs event loop belonging to this image, if necessary.
    fn run(&self) -> Result<(), Self::RunError>;

    /// Creates a writer function that can write data into the image from different thread.
    fn make_writer(&'a self) -> Self::Writer;

    /// Saves the content of the buffer to a file
    fn save(&self, path: &std::path::Path) -> Result<(), Self::SaveError>;
}

pub trait ImageBufferWriter: Sync + Send {
    type WriteError: Send;
    fn write(
        &self,
        block: screen_block::ScreenBlock,
        block_buffer: image::RgbaImage,
    ) -> Result<(), Self::WriteError>;
}
