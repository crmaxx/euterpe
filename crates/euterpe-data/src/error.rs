pub type Result<T> = std::result::Result<T, DataError>;

#[derive(Debug, thiserror::Error)]
pub enum DataError {
    #[error("database configuration error: {0}")]
    Config(String),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Welds(#[from] welds::WeldsError),
}
