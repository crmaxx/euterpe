use axum::Json;
use axum::body::{Body, Bytes};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::Response;
use serde::Deserialize;
use std::sync::Arc;

use crate::api::{
    AlbumCoverUploadResponse, ConvertAlbumResponse, ConvertJobResponse, CueAlbumResponse,
    CueJobResponse, CueSplitRequest, CueSplitResponse, CueValidateRequest, CueValidationResponse,
    LibraryAlbumDetailResponse, LibraryAlbumListResponse, LibraryAlbumTagsPatchRequest,
    LibraryScanLatestResponse, LibraryScanRunSummary, LibraryScanStartResponse,
    LibraryTrackDetailResponse, LibraryTrackItem, LibraryTrackTagsPatchRequest,
};
use crate::db::{albums, artists, convert_jobs, cue_jobs, library_scan_runs, tracks};
use crate::error::ApiError;
use crate::library::covers;
use crate::library::cue;
use crate::library::storage::{LibraryStorage, StoragePath};
use crate::library::stream;
use crate::library::tags::{
    self, AlbumTagsPatch, TrackTagsPatch, apply_album_patch, apply_patch, is_audio_file,
};
use crate::services::app_settings::StorageLocation;
use crate::services::convert::start_album_convert;
use crate::services::library_scan;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct StartLibraryScanQuery {
    /// Relative path under library root (e.g. `Artist/Album`) for subtree scan only.
    pub root: Option<String>,
}

pub async fn start_library_scan(
    State(state): State<AppState>,
    Query(q): Query<StartLibraryScanQuery>,
) -> Result<(StatusCode, Json<LibraryScanStartResponse>), ApiError> {
    let scan_cfg = state
        .runtime
        .read()
        .await
        .library_scan_config(state.config.debug)?;
    let location = state
        .runtime
        .read()
        .await
        .storage
        .library
        .clone()
        .ok_or_else(|| {
            ApiError::Message(
                "LIBRARY_STORAGE_NOT_CONFIGURED: configure library storage in Settings".into(),
            )
        })?;
    let scan_id = match location {
        StorageLocation::Local { .. } => {
            let library_path = state.require_local_library_path().await?;
            let scan_root =
                library_scan::resolve_scan_root_query(&library_path, q.root.as_deref())?;
            library_scan::start_scan(
                &state.db,
                library_path,
                state.scan_events.clone(),
                scan_cfg,
                scan_root,
                Some(state.convert_job_tx.clone()),
                Some(state.runtime.clone()),
            )
            .await?
        }
        StorageLocation::Smb { .. } => {
            let storage = state.library_storage().await?;
            let scan_root = match q.root.as_deref() {
                Some(root) => Some(StoragePath::parse(root)?),
                None => None,
            };
            library_scan::start_scan_storage(
                &state.db,
                storage,
                state.scan_events.clone(),
                scan_cfg,
                scan_root,
                Some(state.convert_job_tx.clone()),
                Some(state.runtime.clone()),
            )
            .await?
        }
    };
    Ok((
        StatusCode::ACCEPTED,
        Json(LibraryScanStartResponse { scan_id }),
    ))
}

pub async fn cancel_library_scan(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    library_scan::request_cancel(&state.db, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn library_scan_latest(
    State(state): State<AppState>,
) -> Result<Json<LibraryScanLatestResponse>, ApiError> {
    let run = library_scan_runs::latest(&state.db).await?;
    Ok(Json(LibraryScanLatestResponse { run }))
}

pub async fn get_library_scan(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<LibraryScanRunSummary>, ApiError> {
    let run = library_scan_runs::get_by_id(&state.db, id)
        .await?
        .ok_or_else(|| ApiError::Message("scan not found".into()))?;
    Ok(Json(run))
}

#[derive(Debug, Deserialize)]
pub struct AlbumListQuery {
    #[serde(default = "default_limit")]
    pub limit: u32,
    #[serde(default = "default_album_sort")]
    pub sort: String,
    #[serde(default)]
    pub order: Option<String>,
    pub cursor: Option<String>,
    pub q: Option<String>,
}

fn default_limit() -> u32 {
    50
}

fn default_album_sort() -> String {
    "title".to_string()
}

pub async fn list_library_albums(
    State(state): State<AppState>,
    Query(q): Query<AlbumListQuery>,
) -> Result<Json<LibraryAlbumListResponse>, ApiError> {
    use crate::api::SortOrder;
    use crate::api::keyset::parse_limit;
    use crate::db::albums::{AlbumsListParams, AlbumsSort};

    let limit = parse_limit(q.limit, 50, 500)?;
    let sort = AlbumsSort::parse(&q.sort)?;
    let order = match q.order.as_deref() {
        None => SortOrder::Asc,
        Some(s) => SortOrder::parse(s)?,
    };
    let page = albums::list_keyset(
        &state.db,
        AlbumsListParams {
            sort,
            order,
            limit,
            q: q.q,
            cursor: q.cursor,
        },
    )
    .await?;
    let location = state.runtime.read().await.storage.library.clone();
    let mut items = Vec::with_capacity(page.items.len());
    for r in page.items {
        let cover_path = album_cover_path_for_state(
            &state,
            location.as_ref(),
            r.id,
            r.path.as_deref(),
            r.cover_path.as_deref(),
        )
        .await?;
        items.push(crate::api::LibraryAlbumItem {
            id: r.id,
            title: r.title,
            artist_name: r.artist_name,
            year: r.year,
            track_count: r.track_count,
            cover_path,
            has_cue_files: album_has_cue_files_for_state(
                &state,
                location.as_ref(),
                r.path.as_deref(),
            )
            .await?,
        });
    }
    Ok(Json(LibraryAlbumListResponse {
        items,
        next_cursor: page.next_cursor,
        has_more: page.has_more,
    }))
}

pub async fn get_library_album(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<LibraryAlbumDetailResponse>, ApiError> {
    let location = state.runtime.read().await.storage.library.clone();
    let album = albums::get_by_id(&state.db, id)
        .await?
        .ok_or_else(|| ApiError::Message("album not found".into()))?;
    let artist_name = if let Some(aid) = album.artist_id {
        sqlx::query_as::<_, (String,)>("SELECT name FROM artists WHERE id = ?")
            .bind(aid)
            .fetch_optional(&state.db)
            .await?
            .map(|(n,)| n)
            .unwrap_or_default()
    } else {
        String::new()
    };
    let cover_path = album_cover_path_for_state(
        &state,
        location.as_ref(),
        album.id,
        album.path.as_deref(),
        album.cover_path.as_deref(),
    )
    .await?;
    let track_rows = tracks::list_by_album(&state.db, id).await?;
    let album_tags_from_file = match track_rows.first() {
        Some(first) => read_track_tags_for_state(&state, location.as_ref(), &first.path)
            .await
            .ok(),
        None => None,
    };
    let tracks_list: Vec<LibraryTrackItem> = track_rows
        .into_iter()
        .map(|t| LibraryTrackItem {
            id: t.id,
            title: t.title,
            track_number: t.track_number,
            year: t.year,
            disc_number: t.disc_number,
            genre: t.genre.clone(),
            path: t.path,
            duration_sec: t.duration_sec,
        })
        .collect();
    let has_convertible_tracks = tracks::album_has_convertible_tracks(&state.db, id).await?;
    let has_cue_files =
        album_has_cue_files_for_state(&state, location.as_ref(), album.path.as_deref()).await?;
    Ok(Json(LibraryAlbumDetailResponse {
        id: album.id,
        title: album.title,
        artist_name,
        year: album.year,
        cover_path,
        genre: album_tags_from_file.as_ref().and_then(|t| t.genre.clone()),
        has_convertible_tracks,
        has_cue_files,
        track_total: album_tags_from_file
            .as_ref()
            .and_then(|t| t.track_total.map(|n| n as i32)),
        disc_total: album_tags_from_file
            .as_ref()
            .and_then(|t| t.disc_total.map(|n| n as i32)),
        tracks: tracks_list,
    }))
}

pub async fn patch_library_album_tags(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<LibraryAlbumTagsPatchRequest>,
) -> Result<Json<LibraryAlbumDetailResponse>, ApiError> {
    let album = albums::get_by_id(&state.db, id)
        .await?
        .ok_or_else(|| ApiError::Message("album not found".into()))?;
    let track_rows = tracks::list_by_album(&state.db, id).await?;
    if track_rows.is_empty() {
        return Err(ApiError::bad_request("album has no tracks"));
    }

    let artist_name = body.artist_name.clone();
    let album_title = body.album_title.clone();
    let patch = AlbumTagsPatch {
        artist: artist_name.clone(),
        album: album_title.clone(),
        year: body.year.map(|y| y as u32),
        genre: body.genre.clone(),
        track_total: body.track_total.map(|n| n as u32),
        disc_total: body.disc_total.map(|n| n as u32),
    };
    let storage = state.library_storage().await?;

    for track in &track_rows {
        let storage_path = StoragePath::parse(&track.path)?;
        let current = tags::read_tags_storage(storage.as_ref(), &storage_path).await?;
        let updated = apply_album_patch(&current, &patch);
        tags::write_tags_storage(storage.as_ref(), &storage_path, &updated).await?;
        let meta = storage.metadata(&storage_path).await.ok();
        let file_size = meta.and_then(|m| i64::try_from(m.size).ok());
        tracks::update_metadata(
            &state.db,
            track.id,
            tracks::TrackMetadataUpdate {
                title: &track.title,
                track_number: track.track_number,
                year: updated.year.map(|y| y as i32),
                disc_number: track.disc_number,
                genre: updated
                    .genre
                    .as_deref()
                    .and_then(|g| if g.is_empty() { None } else { Some(g) }),
                file_mtime: None,
            },
        )
        .await?;
        if let Some(file_size) = file_size {
            sqlx::query("UPDATE tracks SET file_size = ? WHERE id = ?")
                .bind(file_size)
                .bind(track.id)
                .execute(&state.db)
                .await?;
        }
    }

    let album_year = body.year.or(album.year);
    if let Some(artist_name) = &artist_name {
        let artist_id = artists::upsert_by_name(&state.db, artist_name, None).await?;
        let title = album_title.as_deref().unwrap_or(album.title.as_str());
        let _ = albums::upsert(
            &state.db,
            albums::AlbumUpsert {
                artist_id: Some(artist_id),
                title,
                year: album_year,
                qobuz_album_id: album.qobuz_album_id,
                path: album.path.as_deref(),
                cover_path: album.cover_path.as_deref(),
            },
        )
        .await?;
    } else if album_title.is_some() || body.year.is_some() {
        let title = album_title.as_deref().unwrap_or(album.title.as_str());
        let _ = albums::upsert(
            &state.db,
            albums::AlbumUpsert {
                artist_id: album.artist_id,
                title,
                year: album_year,
                qobuz_album_id: album.qobuz_album_id,
                path: album.path.as_deref(),
                cover_path: album.cover_path.as_deref(),
            },
        )
        .await?;
    }

    get_library_album(State(state), Path(id)).await
}

pub async fn put_library_album_cover(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<AlbumCoverUploadResponse>, ApiError> {
    let album = albums::get_by_id(&state.db, id)
        .await?
        .ok_or_else(|| ApiError::Message("album not found".into()))?;
    let album_rel = album
        .path
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::bad_request("album has no directory path on disk"))?;
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let storage = state.library_storage().await?;
    let result = covers::write_album_cover_from_bytes_storage(
        &state.db,
        storage.as_ref(),
        id,
        album_rel,
        body,
        content_type,
    )
    .await?;
    Ok(Json(AlbumCoverUploadResponse {
        cover_path: result.cover_path,
        tracks_embedded: result.tracks_embedded,
    }))
}

pub async fn get_library_album_cover(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Response, ApiError> {
    let location = state.runtime.read().await.storage.library.clone();
    let album = albums::get_by_id(&state.db, id)
        .await?
        .ok_or_else(|| ApiError::Message("album not found".into()))?;
    let rel = album_cover_path_for_state(
        &state,
        location.as_ref(),
        album.id,
        album.path.as_deref(),
        album.cover_path.as_deref(),
    )
    .await?
    .ok_or_else(|| ApiError::Message("album cover not found".into()))?;
    let storage = state.library_storage().await?;
    let path = StoragePath::parse(&rel)?;
    let bytes = storage.read(&path).await?;
    let ct = covers::image_content_type(std::path::Path::new(path.as_str()));
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, ct)
        .body(Body::from(bytes))
        .map_err(|e| ApiError::Message(e.to_string()))
}

pub async fn get_library_track(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<LibraryTrackDetailResponse>, ApiError> {
    let detail = track_detail(&state, id).await?;
    Ok(Json(detail))
}

pub async fn get_library_track_stream(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let track = tracks::get_by_id(&state.db, id)
        .await?
        .ok_or_else(|| ApiError::Message("track not found".into()))?;
    let rel = track.path.trim();
    if rel.is_empty() {
        return Err(ApiError::bad_request("track has no file path"));
    }
    let rel_path = std::path::Path::new(rel);
    if !is_audio_file(rel_path) {
        return Err(ApiError::bad_request("not an audio file"));
    }
    let path = StoragePath::parse(rel)?;
    let storage = state.library_storage().await?;
    let range = headers
        .get(axum::http::header::RANGE)
        .and_then(|v| v.to_str().ok());
    stream::audio_storage_response(storage.as_ref(), &path, range).await
}

pub async fn patch_library_track_tags(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<LibraryTrackTagsPatchRequest>,
) -> Result<Json<LibraryTrackDetailResponse>, ApiError> {
    let track = tracks::get_by_id(&state.db, id)
        .await?
        .ok_or_else(|| ApiError::Message("track not found".into()))?;
    let storage = state.library_storage().await?;
    let storage_path = StoragePath::parse(&track.path)?;
    let current = tags::read_tags_storage(storage.as_ref(), &storage_path).await?;
    let artist_name = body.artist_name.clone();
    let album_title = body.album_title.clone();
    let patch = TrackTagsPatch {
        title: body.title,
        artist: artist_name.clone(),
        album: album_title.clone(),
        track_number: body.track_number.map(|n| n as u32),
        year: body.year.map(|y| y as u32),
        disc_number: body.disc_number.map(|d| d as u32),
        genre: body.genre.clone(),
    };
    let updated = apply_patch(&current, &patch);
    tags::write_tags_storage(storage.as_ref(), &storage_path, &updated).await?;

    let album_year = body.year.or(updated.year.map(|y| y as i32));

    if let Some(artist_name) = &artist_name {
        let artist_id = artists::upsert_by_name(&state.db, artist_name, None).await?;
        if let Some(album_title) = &album_title {
            let album = albums::get_by_id(&state.db, track.album_id)
                .await?
                .ok_or_else(|| ApiError::Message("album not found".into()))?;
            let _ = albums::upsert(
                &state.db,
                albums::AlbumUpsert {
                    artist_id: Some(artist_id),
                    title: album_title,
                    year: album_year.or(album.year),
                    qobuz_album_id: album.qobuz_album_id,
                    path: album.path.as_deref(),
                    cover_path: album.cover_path.as_deref(),
                },
            )
            .await?;
        }
    } else if body.year.is_some() {
        let album = albums::get_by_id(&state.db, track.album_id)
            .await?
            .ok_or_else(|| ApiError::Message("album not found".into()))?;
        let _ = albums::upsert(
            &state.db,
            albums::AlbumUpsert {
                artist_id: album.artist_id,
                title: &album.title,
                year: album_year.or(album.year),
                qobuz_album_id: album.qobuz_album_id,
                path: album.path.as_deref(),
                cover_path: album.cover_path.as_deref(),
            },
        )
        .await?;
    }

    let meta = storage.metadata(&storage_path).await.ok();
    let file_size = meta.and_then(|m| i64::try_from(m.size).ok());
    tracks::update_metadata(
        &state.db,
        id,
        tracks::TrackMetadataUpdate {
            title: &updated.title,
            track_number: updated.track_number.map(|n| n as i32),
            year: updated.year.map(|y| y as i32),
            disc_number: updated.disc_number.map(|d| d as i32),
            genre: updated
                .genre
                .as_deref()
                .and_then(|g| if g.is_empty() { None } else { Some(g) }),
            file_mtime: None,
        },
    )
    .await?;
    if let Some(file_size) = file_size {
        sqlx::query("UPDATE tracks SET file_size = ? WHERE id = ?")
            .bind(file_size)
            .bind(id)
            .execute(&state.db)
            .await?;
    }

    let detail = track_detail(&state, id).await?;
    Ok(Json(detail))
}

async fn track_detail(state: &AppState, id: i64) -> Result<LibraryTrackDetailResponse, ApiError> {
    let location = state.runtime.read().await.storage.library.clone();
    let track = tracks::get_by_id(&state.db, id)
        .await?
        .ok_or_else(|| ApiError::Message("track not found".into()))?;
    let t = read_track_tags_for_state(state, location.as_ref(), &track.path).await?;
    Ok(LibraryTrackDetailResponse {
        id: track.id,
        album_id: track.album_id,
        title: t.title,
        artist_name: t.artist,
        album_title: t.album,
        track_number: t.track_number.map(|n| n as i32),
        year: t.year.map(|y| y as i32),
        disc_number: t.disc_number.map(|d| d as i32),
        genre: t.genre.clone(),
        path: track.path,
        duration_sec: t.duration_sec.map(|d| d as i32),
    })
}

pub async fn post_library_album_convert(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<(StatusCode, Json<ConvertAlbumResponse>), ApiError> {
    albums::get_by_id(&state.db, id)
        .await?
        .ok_or_else(|| ApiError::Message("album not found".into()))?;
    let job_id = start_album_convert(&state.db, id, &state.convert_job_tx).await?;
    Ok((StatusCode::ACCEPTED, Json(ConvertAlbumResponse { job_id })))
}

#[derive(Debug, Deserialize)]
pub struct CueQuery {
    pub cue_path: Option<String>,
}

pub async fn get_library_album_cue(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(q): Query<CueQuery>,
) -> Result<Json<CueAlbumResponse>, ApiError> {
    let album = albums::get_by_id(&state.db, id)
        .await?
        .ok_or_else(|| ApiError::Message("album not found".into()))?;
    let album_path = album
        .path
        .as_deref()
        .ok_or_else(|| ApiError::bad_request("album has no directory path"))?;
    let storage = state.library_storage().await?;
    let response =
        cue::load_album_cue_storage(storage.as_ref(), album_path, q.cue_path.as_deref()).await?;
    Ok(Json(response))
}

pub async fn validate_library_album_cue(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<CueValidateRequest>,
) -> Result<Json<CueValidationResponse>, ApiError> {
    albums::get_by_id(&state.db, id)
        .await?
        .ok_or_else(|| ApiError::Message("album not found".into()))?;
    Ok(Json(cue::validate_api_document(&body.document)))
}

pub async fn split_library_album_cue(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<CueSplitRequest>,
) -> Result<(StatusCode, Json<CueSplitResponse>), ApiError> {
    let album = albums::get_by_id(&state.db, id)
        .await?
        .ok_or_else(|| ApiError::Message("album not found".into()))?;
    let album_path = album.path.clone();
    let album_rel = StoragePath::parse(
        album_path
            .as_deref()
            .ok_or_else(|| ApiError::bad_request("album has no directory path"))?,
    )?;
    let validation = cue::validate_api_document(&body.document);
    if !validation.valid {
        return Err(ApiError::bad_request("CUE has validation errors"));
    }
    if !matches!(
        body.source_file_policy.as_str(),
        "keep" | "delete_after_success"
    ) {
        return Err(ApiError::bad_request("invalid source_file_policy"));
    }
    let cue_rel = StoragePath::parse(&body.document.cue_path)?;
    if !storage_path_is_under(&cue_rel, &album_rel) {
        return Err(ApiError::bad_request("CUE path is outside album directory"));
    }
    reject_unsafe_cue_audio_path(&body.document.audio_path)?;
    state.library_storage().await?.metadata(&cue_rel).await?;
    let payload = cue_jobs::CueJobPayload {
        cue_path: body.document.cue_path.clone(),
        audio_path: body.document.audio_path.clone(),
        source_file_policy: body.source_file_policy.clone(),
    };
    let payload_json =
        serde_json::to_string(&payload).map_err(|e| ApiError::Message(e.to_string()))?;
    let tracks_total = body.document.tracks.iter().filter(|t| t.selected).count() as i64;
    let job_id = cue_jobs::create_queued(&state.db, id, tracks_total, Some(&payload_json)).await?;
    spawn_cue_split_job(state, job_id, body, album_path);
    Ok((StatusCode::ACCEPTED, Json(CueSplitResponse { job_id })))
}

fn reject_unsafe_cue_audio_path(audio_path: &str) -> Result<(), ApiError> {
    let path = std::path::Path::new(audio_path);
    if path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(ApiError::bad_request("invalid CUE audio path"));
    }
    Ok(())
}

fn storage_path_is_under(path: &StoragePath, base: &StoragePath) -> bool {
    if base.is_root() || path.as_str() == base.as_str() {
        return true;
    }
    path.as_str()
        .strip_prefix(base.as_str())
        .is_some_and(|rest| rest.starts_with('/'))
}

fn spawn_cue_split_job(
    state: AppState,
    job_id: i64,
    body: CueSplitRequest,
    album_path: Option<String>,
) {
    tokio::spawn(async move {
        if let Err(e) = run_cue_split_job(state, job_id, body, album_path).await {
            tracing::error!(job_id, error = %e, "CUE split job failed");
        }
    });
}

async fn run_cue_split_job(
    state: AppState,
    job_id: i64,
    body: CueSplitRequest,
    album_path: Option<String>,
) -> Result<(), ApiError> {
    cue_jobs::mark_running(&state.db, job_id).await?;
    let document = cue::document_to_core(&body.document);
    let selected_total = document.tracks.iter().filter(|t| t.selected).count() as i64;
    let source_file_policy = if body.source_file_policy == "delete_after_success" {
        euterpe_cue::SourceFilePolicy::DeleteAfterSuccess
    } else {
        euterpe_cue::SourceFilePolicy::Keep
    };
    let progress_pool = state.db.clone();
    let progress_handle = tokio::runtime::Handle::current();
    let on_progress = std::sync::Arc::new(move |p: euterpe_cue::SplitProgress| {
        let pool = progress_pool.clone();
        let tracks_done = p.tracks_done as i64;
        let tracks_total = p.tracks_total as i64;
        let _ = progress_handle.block_on(cue_jobs::update_progress(
            &pool,
            job_id,
            tracks_done,
            tracks_total,
        ));
    });
    let options = euterpe_cue::SplitOptions {
        source_file_policy,
        file_mask: body.file_mask.clone(),
        on_progress: Some(on_progress),
    };
    let storage = state.library_storage().await?;
    let cue_rel = StoragePath::parse(&body.document.cue_path)?;
    let cue_dir_rel = cue_rel.parent().unwrap_or_else(StoragePath::root);
    let output_dir_rel = std::path::PathBuf::from(cue_dir_rel.as_str());
    let handle = tokio::runtime::Handle::current();
    let mut split_io = StorageCueSplitIo {
        storage: storage.clone(),
        handle,
        base_dir: cue_dir_rel.clone(),
    };
    let split = tokio::task::spawn_blocking(move || {
        euterpe_cue::split_flac_image_io(&document, &mut split_io, &output_dir_rel, &options)
    })
    .await
    .map_err(|e| ApiError::Message(e.to_string()))?;

    match split {
        Ok(result) => {
            cue_jobs::finish_success(
                &state.db,
                job_id,
                selected_total.min(result.output_paths.len() as i64),
            )
            .await?;
            if body.source_file_policy == "delete_after_success" {
                let _ = storage.delete(&cue_rel).await;
            }
            if let Some(album_path) = album_path
                && let Ok(scan_root) = StoragePath::parse(album_path)
            {
                let scan_cfg = state
                    .runtime
                    .read()
                    .await
                    .library_scan_config(state.config.debug)?;
                let _ = library_scan::start_scan_storage(
                    &state.db,
                    storage,
                    state.scan_events.clone(),
                    scan_cfg,
                    Some(scan_root),
                    Some(state.convert_job_tx.clone()),
                    Some(state.runtime.clone()),
                )
                .await;
            }
        }
        Err(e) => {
            cue_jobs::finish_failed(&state.db, job_id, &e.to_string()).await?;
        }
    }
    Ok(())
}

struct StorageCueSplitIo {
    storage: Arc<dyn LibraryStorage>,
    handle: tokio::runtime::Handle,
    base_dir: StoragePath,
}

impl StorageCueSplitIo {
    fn cue_path_error(message: impl Into<String>) -> euterpe_cue::CueError {
        euterpe_cue::CueError::Invalid(message.into())
    }

    fn api_error(error: ApiError) -> euterpe_cue::CueError {
        euterpe_cue::CueError::Io(std::io::Error::other(error.to_string()))
    }

    fn rel_from_path(path: &std::path::Path) -> euterpe_cue::Result<String> {
        if path.is_absolute() {
            return Err(Self::cue_path_error("CUE path must be library-relative"));
        }
        let mut parts = Vec::new();
        for component in path.components() {
            match component {
                std::path::Component::Normal(part) => {
                    let part = part.to_str().ok_or_else(|| {
                        Self::cue_path_error("CUE path contains non-UTF-8 component")
                    })?;
                    parts.push(part);
                }
                std::path::Component::CurDir => {}
                std::path::Component::ParentDir
                | std::path::Component::Prefix(_)
                | std::path::Component::RootDir => {
                    return Err(Self::cue_path_error(
                        "CUE path must not escape library storage",
                    ));
                }
            }
        }
        Ok(parts.join("/"))
    }
}

impl euterpe_cue::SplitIo for StorageCueSplitIo {
    fn read_source(&mut self, audio_path: &std::path::Path) -> euterpe_cue::Result<Vec<u8>> {
        let rel = Self::rel_from_path(audio_path)?;
        let path = self
            .base_dir
            .join(&rel)
            .map_err(|e| Self::cue_path_error(e.to_string()))?;
        let bytes = self
            .handle
            .block_on(self.storage.read(&path))
            .map_err(Self::api_error)?;
        Ok(bytes.to_vec())
    }

    fn write_output(
        &mut self,
        rel_path: &std::path::Path,
        bytes: Vec<u8>,
    ) -> euterpe_cue::Result<()> {
        let rel = Self::rel_from_path(rel_path)?;
        let path = StoragePath::parse(&rel).map_err(|e| Self::cue_path_error(e.to_string()))?;
        if let Some(parent) = path.parent() {
            self.handle
                .block_on(self.storage.create_dir_all(&parent))
                .map_err(Self::api_error)?;
        }
        self.handle
            .block_on(self.storage.atomic_write(&path, Bytes::from(bytes)))
            .map_err(Self::api_error)
    }

    fn delete_source(&mut self, rel_path: &std::path::Path) -> euterpe_cue::Result<()> {
        let rel = Self::rel_from_path(rel_path)?;
        let path = self
            .base_dir
            .join(&rel)
            .map_err(|e| Self::cue_path_error(e.to_string()))?;
        self.handle
            .block_on(self.storage.delete(&path))
            .map_err(Self::api_error)
    }
}

pub async fn get_library_album_cue_latest(
    State(state): State<AppState>,
    Path(album_id): Path<i64>,
) -> Result<Json<CueJobResponse>, ApiError> {
    albums::get_by_id(&state.db, album_id)
        .await?
        .ok_or_else(|| ApiError::Message("album not found".into()))?;
    let job = cue_jobs::latest_for_album(&state.db, album_id)
        .await?
        .map(cue::cue_job_to_api);
    Ok(Json(CueJobResponse { job }))
}

pub async fn get_library_album_convert_latest(
    State(state): State<AppState>,
    Path(album_id): Path<i64>,
) -> Result<Json<ConvertJobResponse>, ApiError> {
    let row = convert_jobs::latest_for_album(&state.db, album_id)
        .await?
        .ok_or_else(|| ApiError::Message("no convert job for album".into()))?;
    let job = convert_jobs::row_to_summary(row).await?;
    Ok(Json(ConvertJobResponse { job }))
}

pub async fn get_convert_job(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<ConvertJobResponse>, ApiError> {
    let row = convert_jobs::get_by_id(&state.db, id)
        .await?
        .ok_or_else(|| ApiError::Message("convert job not found".into()))?;
    let job = convert_jobs::row_to_summary(row).await?;
    Ok(Json(ConvertJobResponse { job }))
}

async fn album_cover_path_for_state(
    state: &AppState,
    location: Option<&StorageLocation>,
    album_id: i64,
    album_path: Option<&str>,
    cover_path: Option<&str>,
) -> Result<Option<String>, ApiError> {
    match location {
        Some(StorageLocation::Local { .. }) => {
            let library_path = state.require_local_library_path().await?;
            covers::ensure_album_cover_path(
                &state.db,
                &library_path,
                album_id,
                album_path,
                cover_path,
            )
            .await
        }
        Some(StorageLocation::Smb { .. }) => {
            if let Some(path) = cover_path.filter(|p| !p.trim().is_empty()) {
                return Ok(Some(path.to_string()));
            }
            let Some(album_path) = album_path.filter(|p| !p.trim().is_empty()) else {
                return Ok(None);
            };
            let storage = state.library_storage().await?;
            covers::discover_album_cover_rel_storage(storage.as_ref(), album_path).await
        }
        None => Err(ApiError::Message(
            "LIBRARY_STORAGE_NOT_CONFIGURED: configure library storage in Settings".into(),
        )),
    }
}

async fn album_has_cue_files_for_state(
    state: &AppState,
    location: Option<&StorageLocation>,
    album_path: Option<&str>,
) -> Result<bool, ApiError> {
    match location {
        Some(StorageLocation::Local { .. }) | Some(StorageLocation::Smb { .. }) => {
            let storage = state.library_storage().await?;
            cue::album_has_cue_files_storage(storage.as_ref(), album_path).await
        }
        None => Err(ApiError::Message(
            "LIBRARY_STORAGE_NOT_CONFIGURED: configure library storage in Settings".into(),
        )),
    }
}

async fn read_track_tags_for_state(
    state: &AppState,
    location: Option<&StorageLocation>,
    rel: &str,
) -> Result<tags::TrackTags, ApiError> {
    match location {
        Some(StorageLocation::Local { .. }) => {
            let library_path = state.require_local_library_path().await?;
            let file_path = library_path.join(rel);
            tags::read_tags(&file_path)
        }
        Some(StorageLocation::Smb { .. }) => {
            let storage = state.library_storage().await?;
            let path = StoragePath::parse(rel)?;
            let bytes = storage.read(&path).await?;
            tags::read_tags_from_bytes_with_rel(bytes.to_vec(), Some(rel))
        }
        None => Err(ApiError::Message(
            "LIBRARY_STORAGE_NOT_CONFIGURED: configure library storage in Settings".into(),
        )),
    }
}
