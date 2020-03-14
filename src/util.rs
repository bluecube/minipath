pub type AnyError = Box<dyn std::error::Error + Send + Sync + 'static>;
pub type SimpleResult<T = ()> = Result<T, AnyError>;

#[derive(Copy, Clone, Debug)]
pub enum NoError {}

impl std::fmt::Display for NoError {
    fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        unreachable!()
    }
}

impl std::error::Error for NoError {}
