use thiserror::Error;

#[derive(Debug, Error)]
pub enum DecodeError {
    #[error("buffer truncated")]
    Truncated,

    #[error("corrupted data: {0}")]
    Corrupted(String),

    #[error("invalid value")]
    InvalidValue,

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("protocol not found: {0}")]
    ProtocolMissing(u32),

    #[error("protocol JSON parse error: {0}")]
    Json(String),

    #[error("unexpected type: {0}")]
    UnexpectedType(String),
}
