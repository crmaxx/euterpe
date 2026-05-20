pub mod payload;
pub mod progress;
pub mod resolve;
pub mod torrent_job;
pub mod worker;

pub use payload::{format_album_display_title, DownloadJobPayload};
pub use resolve::{resolve_album_api_id_for_state, resolve_from_qobuz_favorites};
pub use worker::{execute_job, quality_from_format_id, run_job, spawn_worker, WorkerDeps};
