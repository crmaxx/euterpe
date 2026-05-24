use std::sync::Arc;

use axum::Json;
use axum::extract::{Multipart, State};
use axum::http::StatusCode;
use bytes::Bytes;

use euterpe_torrent::TorrentEngine;

use crate::api::DownloadJobType;
use crate::api::{
    CreateDownloadResponse, TorrentConfirmRequest, TorrentInspectMagnetRequest,
    TorrentInspectResponse,
};
use crate::db::download_jobs;
use crate::error::ApiError;
use crate::services::download::payload::{DownloadJobPayload, TorrentJobPayload};
use crate::services::torrent_staging::StagingEntry;
use crate::state::AppState;

fn require_torrent(
    state: &AppState,
) -> Result<(&Arc<dyn TorrentEngine>, &std::path::PathBuf), ApiError> {
    let engine = state.torrent.as_ref().ok_or_else(|| {
        ApiError::Message("EUTERPE_TORRENT_INCOMING_DIR is not configured".into())
    })?;
    let dir = state.config.torrent_incoming_dir.as_ref().ok_or_else(|| {
        ApiError::Message("EUTERPE_TORRENT_INCOMING_DIR is not configured".into())
    })?;
    Ok((engine, dir))
}

/// Multipart field for `.torrent` upload — must match OpenAPI (`file`).
fn is_torrent_upload_field(name: Option<&str>) -> bool {
    name == Some("file")
}

async fn read_torrent_upload(multipart: &mut Multipart) -> Result<Bytes, ApiError> {
    let mut seen_fields: Vec<String> = Vec::new();
    let mut file_bytes: Option<Bytes> = None;

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        tracing::warn!(error = %e, "torrent inspect: invalid multipart");
        ApiError::bad_request(format!("invalid multipart body: {e}"))
    })? {
        let name = field.name().unwrap_or("").to_string();
        seen_fields.push(name.clone());
        if !is_torrent_upload_field(Some(name.as_str())) {
            continue;
        }
        let data = field.bytes().await.map_err(|e| {
            tracing::warn!(field = %name, error = %e, "torrent inspect: failed to read field");
            ApiError::bad_request(format!("failed to read upload field '{name}': {e}"))
        })?;
        if data.is_empty() {
            tracing::warn!(field = %name, "torrent inspect: empty .torrent file");
            return Err(ApiError::bad_request(format!(
                "upload field '{name}' is empty"
            )));
        }
        tracing::debug!(
            field = %name,
            bytes = data.len(),
            "torrent inspect: received .torrent file"
        );
        file_bytes = Some(data);
        break;
    }

    file_bytes.ok_or_else(|| {
        tracing::warn!(
            seen_fields = ?seen_fields,
            "torrent inspect: missing file field (expected multipart field 'file')"
        );
        ApiError::bad_request(format!(
            "torrent file field required (multipart field name must be 'file'; got: {seen_fields:?})"
        ))
    })
}

async fn write_staging_torrent(
    staging_dir: &std::path::Path,
    bytes: &[u8],
) -> Result<(), ApiError> {
    tokio::fs::create_dir_all(staging_dir).await.map_err(|e| {
        tracing::warn!(
            path = %staging_dir.display(),
            error = %e,
            "torrent inspect: cannot create staging dir"
        );
        ApiError::Message(format!("create staging dir {}: {e}", staging_dir.display()))
    })?;
    tokio::fs::write(staging_dir.join("data.torrent"), bytes)
        .await
        .map_err(|e| {
            tracing::warn!(
                path = %staging_dir.display(),
                error = %e,
                "torrent inspect: cannot write data.torrent"
            );
            ApiError::Message(format!("write staging torrent: {e}"))
        })
}

pub async fn inspect_torrent_magnet(
    State(state): State<AppState>,
    Json(body): Json<TorrentInspectMagnetRequest>,
) -> Result<Json<TorrentInspectResponse>, ApiError> {
    let magnet = body.magnet.trim();
    if magnet.is_empty() {
        tracing::warn!("torrent inspect magnet: empty magnet link");
        return Err(ApiError::bad_request("magnet must not be empty"));
    }
    let (engine, incoming) = require_torrent(&state).map_err(|e| {
        tracing::warn!(error = %e, "torrent inspect magnet: engine not configured");
        e
    })?;
    let inspect_id = uuid::Uuid::new_v4().to_string();
    let staging_dir = incoming.join(".staging").join("inspect").join(&inspect_id);
    let result = engine
        .inspect_magnet(magnet, staging_dir.clone())
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, magnet_len = magnet.len(), "torrent inspect magnet failed");
            ApiError::Message(e.to_string())
        })?;

    if let Some(bytes) = &result.torrent_bytes {
        write_staging_torrent(&staging_dir, bytes).await?;
    }

    let files: Vec<crate::api::TorrentInspectFile> = result
        .files
        .into_iter()
        .map(|f| crate::api::TorrentInspectFile {
            index: f.index,
            path: f.path,
            size_bytes: f.size_bytes,
            selected: f.selected,
        })
        .collect();

    let post_download_capability = detect_post_download_capability(&files);
    let response = TorrentInspectResponse {
        inspect_id: inspect_id.clone(),
        name: result.name.clone(),
        total_size_bytes: result.total_size_bytes,
        info_hash_v1: result.info_hash_v1.clone(),
        info_hash_v2: None,
        comment: result.comment.clone(),
        free_space_bytes: free_space_bytes(incoming).await,
        files: files.clone(),
        post_download_capability,
    };

    state.torrent_staging.insert(
        inspect_id,
        StagingEntry::new(
            result.name,
            result.info_hash_v1,
            result.total_size_bytes,
            result.comment,
            files,
            Some(magnet.to_string()),
            result.torrent_bytes,
            staging_dir,
        ),
    );

    Ok(Json(response))
}

pub async fn inspect_torrent_file(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<TorrentInspectResponse>, ApiError> {
    tracing::debug!("torrent inspect file: parsing multipart");
    let (engine, incoming) = require_torrent(&state).map_err(|e| {
        tracing::warn!(error = %e, "torrent inspect file: engine not configured");
        e
    })?;
    let file_bytes = read_torrent_upload(&mut multipart).await?;
    let inspect_id = uuid::Uuid::new_v4().to_string();
    let staging_dir = incoming.join(".staging").join("inspect").join(&inspect_id);
    write_staging_torrent(&staging_dir, &file_bytes).await?;

    let result = engine
        .inspect_bytes(&file_bytes, staging_dir.clone())
        .await
        .map_err(|e| {
            tracing::warn!(
                error = %e,
                bytes = file_bytes.len(),
                "torrent inspect file: librqbit rejected .torrent"
            );
            ApiError::Message(e.to_string())
        })?;

    let files: Vec<_> = result
        .files
        .into_iter()
        .map(|f| crate::api::TorrentInspectFile {
            index: f.index,
            path: f.path,
            size_bytes: f.size_bytes,
            selected: f.selected,
        })
        .collect();
    let response = TorrentInspectResponse {
        inspect_id: inspect_id.clone(),
        name: result.name.clone(),
        total_size_bytes: result.total_size_bytes,
        info_hash_v1: result.info_hash_v1.clone(),
        info_hash_v2: None,
        comment: result.comment.clone(),
        free_space_bytes: free_space_bytes(incoming).await,
        post_download_capability: detect_post_download_capability(&files),
        files,
    };

    state.torrent_staging.insert(
        inspect_id,
        StagingEntry::new(
            result.name,
            result.info_hash_v1,
            result.total_size_bytes,
            result.comment,
            response.files.clone(),
            None,
            Some(file_bytes),
            staging_dir,
        ),
    );

    Ok(Json(response))
}

pub async fn confirm_torrent(
    State(state): State<AppState>,
    Json(body): Json<TorrentConfirmRequest>,
) -> Result<(StatusCode, Json<CreateDownloadResponse>), ApiError> {
    if body.auto_index_after_import && !body.copy_to_library {
        return Err(ApiError::bad_request(
            "auto_index_after_import requires copy_to_library",
        ));
    }

    let staging = state.torrent_staging.get(&body.inspect_id)?;
    let selected: Vec<usize> = body
        .files
        .iter()
        .filter(|f| f.selected)
        .map(|f| f.index)
        .collect();
    if selected.is_empty() {
        return Err(ApiError::bad_request("at least one file must be selected"));
    }
    if let Some(post) = &body.post_download {
        if (post.split_after_conversion || post.split_after_download) && post.cue_path.is_none() {
            return Err(ApiError::bad_request("cue_path is required for CUE split"));
        }
        if post.split_after_conversion && !post.convert_after_download {
            return Err(ApiError::bad_request(
                "split_after_conversion requires convert_after_download",
            ));
        }
        if post.split_after_download && post.split_after_conversion {
            return Err(ApiError::bad_request(
                "choose direct split or split after conversion, not both",
            ));
        }
        if let Some(policy) = &post.source_file_policy
            && !matches!(policy.as_str(), "keep" | "delete_after_success")
        {
            return Err(ApiError::bad_request("invalid source_file_policy"));
        }
    }

    let (_engine, incoming) = require_torrent(&state)?;

    let payload = TorrentJobPayload {
        display_name: staging.name.clone(),
        info_hash: staging.info_hash_v1.clone(),
        selected_file_indices: selected,
        copy_to_library: body.copy_to_library,
        auto_index_after_import: body.auto_index_after_import,
        post_download: body.post_download.clone(),
        magnet: staging.magnet.clone(),
        save_dir_incoming: String::new(),
        library_dest_rel: None,
        librqbit_id: None,
        runtime: None,
    };

    let job_id = download_jobs::insert_queued(
        &state.db,
        DownloadJobType::Torrent,
        0,
        0,
        Some(&DownloadJobPayload {
            torrent: Some(payload),
            ..Default::default()
        }),
    )
    .await?;

    let job_dir = incoming.join(job_id.to_string());
    tokio::fs::create_dir_all(&job_dir)
        .await
        .map_err(|e| ApiError::Message(e.to_string()))?;

    if staging.torrent_bytes.is_some() || staging_dir_has_torrent(&staging.staging_dir).await {
        let src = staging.staging_dir.join("data.torrent");
        tokio::fs::copy(&src, job_dir.join("seed.torrent"))
            .await
            .map_err(|e| ApiError::Message(e.to_string()))?;
    }

    let mut payload = download_jobs::get_payload(&state.db, job_id)
        .await?
        .and_then(|p| p.torrent)
        .ok_or_else(|| ApiError::Message("torrent payload missing after insert".into()))?;
    payload.save_dir_incoming = job_dir.display().to_string();
    download_jobs::set_payload(
        &state.db,
        job_id,
        &DownloadJobPayload {
            torrent: Some(payload),
            ..Default::default()
        },
    )
    .await?;

    state.torrent_staging.remove(&body.inspect_id);

    state
        .job_tx
        .send(job_id)
        .await
        .map_err(|e| ApiError::Message(format!("job queue closed: {e}")))?;

    Ok((
        StatusCode::ACCEPTED,
        Json(CreateDownloadResponse { job_id }),
    ))
}

async fn staging_dir_has_torrent(dir: &std::path::Path) -> bool {
    tokio::fs::metadata(dir.join("data.torrent"))
        .await
        .map(|m| m.is_file())
        .unwrap_or(false)
}

async fn free_space_bytes(_path: &std::path::Path) -> Option<u64> {
    None
}

fn detect_post_download_capability(
    files: &[crate::api::TorrentInspectFile],
) -> Option<crate::api::TorrentPostDownloadCapability> {
    use std::path::Path;
    let cue_files = files
        .iter()
        .filter(|f| {
            Path::new(&f.path)
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("cue"))
        })
        .collect::<Vec<_>>();
    if cue_files.is_empty() {
        return None;
    }
    let audio_files = files
        .iter()
        .filter_map(|f| audio_format(&f.path).map(|fmt| (f, fmt)))
        .collect::<Vec<_>>();
    let mut candidates = Vec::new();
    for cue in cue_files {
        let cue_parent = Path::new(&cue.path).parent();
        let matching = audio_files
            .iter()
            .find(|(audio, _)| Path::new(&audio.path).parent() == cue_parent)
            .or_else(|| audio_files.first());
        if let Some((audio, fmt)) = matching {
            candidates.push(crate::api::TorrentCueCandidate {
                cue_path: cue.path.clone(),
                audio_path: audio.path.clone(),
                audio_format: (*fmt).to_string(),
                direct_split_supported: *fmt == "flac",
                convert_required_for_split: *fmt != "flac",
            });
        }
    }
    if candidates.is_empty() {
        return None;
    }
    let has_flac_image_cue = candidates.iter().any(|c| c.direct_split_supported);
    let has_convertible_image_cue = candidates.iter().any(|c| c.convert_required_for_split);
    Some(crate::api::TorrentPostDownloadCapability {
        cue_candidates: candidates,
        has_flac_image_cue,
        has_convertible_image_cue,
    })
}

fn audio_format(path: &str) -> Option<&'static str> {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())?
        .to_ascii_lowercase();
    match ext.as_str() {
        "flac" => Some("flac"),
        "wav" | "wave" => Some("wav"),
        "ape" => Some("ape"),
        "m4a" | "mp4" => Some("m4a"),
        "wv" | "wavpack" => Some("wv"),
        _ => None,
    }
}
