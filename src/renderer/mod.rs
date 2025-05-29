mod machinery;
mod worker;

pub use crate::renderer::machinery::{RenderProgress, render};

#[derive(Copy, Clone, Debug)]
pub struct RenderSettings {
    pub tile_size: std::num::NonZeroU32,
    pub sample_count: std::num::NonZeroU32,
}
