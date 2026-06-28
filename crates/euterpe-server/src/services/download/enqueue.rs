use euterpe_data::DataHandle;
use euterpe_data::repositories::download_jobs as data_download_jobs;
use tokio::sync::mpsc;

use crate::error::ApiError;
use crate::services::download::DownloadJobPayload;
use crate::state::AppState;

pub async fn queue_album_download_for_state(
    state: &AppState,
    album_api_id: &str,
    quality: u8,
    qobuz_id: Option<u64>,
    display_title: Option<String>,
) -> Result<i64, ApiError> {
    queue_album_download(
        &state.data,
        &state.job_tx,
        album_api_id,
        quality,
        qobuz_id,
        display_title,
    )
    .await
}

pub async fn queue_album_download(
    data: &DataHandle,
    job_tx: &mpsc::Sender<i64>,
    album_api_id: &str,
    quality: u8,
    qobuz_id: Option<u64>,
    display_title: Option<String>,
) -> Result<i64, ApiError> {
    let qobuz_for_dedup = qobuz_id.filter(|id| *id > 0);
    if data_download_jobs::has_active_album(data, album_api_id, qobuz_for_dedup, quality).await? {
        return Err(ApiError::Message(
            "JOB_ALREADY_RUNNING: album download in progress".into(),
        ));
    }

    let payload = DownloadJobPayload {
        album_api_id: Some(album_api_id.to_string()),
        display_title: display_title.filter(|s| !s.trim().is_empty()),
        torrent: None,
    };
    let job_id = data_download_jobs::insert_queued(
        data,
        data_download_jobs::DownloadJobType::Album,
        qobuz_id.filter(|id| *id > 0).map(|id| id as i64),
        quality,
        Some(&payload),
    )
    .await?;

    tracing::debug!(
        job_id,
        qobuz_id = ?qobuz_id,
        quality,
        album_api_id = %album_api_id,
        "download job queued"
    );

    if let Err(error) = job_tx.send(job_id).await {
        tracing::warn!(job_id, error = %error, "download job queued but worker wake-up failed");
    }

    Ok(job_id)
}

pub async fn queue_album_download_if_missing_for_state(
    state: &AppState,
    album_api_id: &str,
    quality: u8,
    qobuz_id: Option<u64>,
    display_title: Option<String>,
) -> Result<Option<i64>, ApiError> {
    queue_album_download_if_missing(
        &state.data,
        &state.job_tx,
        album_api_id,
        quality,
        qobuz_id,
        display_title,
    )
    .await
}

pub async fn queue_album_download_if_missing(
    data: &DataHandle,
    job_tx: &mpsc::Sender<i64>,
    album_api_id: &str,
    quality: u8,
    qobuz_id: Option<u64>,
    display_title: Option<String>,
) -> Result<Option<i64>, ApiError> {
    let qobuz_for_dedup = qobuz_id.filter(|id| *id > 0);
    if data_download_jobs::has_active_album(data, album_api_id, qobuz_for_dedup, quality).await? {
        return Ok(None);
    }
    queue_album_download(data, job_tx, album_api_id, quality, qobuz_id, display_title)
        .await
        .map(Some)
}
