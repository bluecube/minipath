pub type AnyError = Box<dyn std::error::Error + Send + Sync + 'static>;
pub type SimpleResult<T = ()> = Result<T, AnyError>;
