use thiserror::Error;

#[derive(Debug, Error)]
pub enum InternalError {
    #[error("")]
    InquireError(),
}
