use thiserror::Error;

#[derive(Debug, Error)]
pub enum TorrentError {
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Librqbit(#[from] librqbit::Error),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl TorrentError {
    pub fn msg(s: impl Into<String>) -> Self {
        Self::Message(s.into())
    }
}
