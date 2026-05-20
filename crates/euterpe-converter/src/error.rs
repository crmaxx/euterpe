use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConvertError {
    #[error("unsupported format: {0}")]
    UnsupportedFormat(String),
    #[error("not a lossless ALAC stream")]
    NotAlac,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("decode: {0}")]
    Decode(String),
    #[error("encode: {0}")]
    Encode(String),
    #[error("tags: {0}")]
    Tags(String),
    #[error("invalid settings: {0}")]
    InvalidSettings(String),
}

pub type Result<T> = std::result::Result<T, ConvertError>;
