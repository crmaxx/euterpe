//! BitTorrent engine wrapper around [librqbit](https://github.com/ikatson/rqbit).

mod engine;
mod error;
mod types;

pub use engine::{LibrqbitEngine, TorrentEngineConfig};
pub use librqbit::limits::LimitsConfig;
pub use error::TorrentError;
pub use types::{
    InspectFile, InspectResult, JobHandle, JobStats, LibrqbitState, SessionSettings,
    StartJobRequest,
};

#[async_trait::async_trait]
pub trait TorrentEngine: Send + Sync {
    async fn inspect_magnet(
        &self,
        magnet: &str,
        staging_output: std::path::PathBuf,
    ) -> Result<InspectResult, TorrentError>;
    async fn inspect_bytes(
        &self,
        torrent_file: &[u8],
        staging_output: std::path::PathBuf,
    ) -> Result<InspectResult, TorrentError>;
    async fn start_job(&self, req: StartJobRequest) -> Result<JobHandle, TorrentError>;
    async fn job_stats(&self, handle: &JobHandle) -> Result<JobStats, TorrentError>;
    async fn wait_until_completed(&self, handle: &JobHandle) -> Result<(), TorrentError>;
    async fn cancel(&self, handle: &JobHandle) -> Result<(), TorrentError>;
    async fn remove_from_session(&self, handle: &JobHandle) -> Result<(), TorrentError>;
    fn apply_ratelimits(&self, settings: SessionSettings);
}
