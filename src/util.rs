#[derive(Copy, Clone, Debug)]
pub enum NoError {}

impl std::fmt::Display for NoError {
    fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        unreachable!()
    }
}

impl std::error::Error for NoError {}

pub type Rgba = rgb::RGBA<f32>;
