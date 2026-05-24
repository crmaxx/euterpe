use axum::Json;
use axum::body::{Body, Bytes};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::Response;
use serde::Deserialize;

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
use crate::library::stream;
use crate::library::tags::{
    self, AlbumTagsPatch, TrackTagsPatch, apply_album_patch, apply_patch, is_audio_file,
};
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
    let scan_root =
        library_scan::resolve_scan_root_query(&state.config.library_path, q.root.as_deref())?;
    let scan_cfg = state
        .runtime
        .read()
        .await
        .library_scan_config(state.config.debug)?;
    let scan_id = library_scan::start_scan(
        &state.db,
        state.config.library_path.clone(),
        state.scan_events.clone(),
        scan_cfg,
        scan_root,
        Some(state.convert_job_tx.clone()),
        Some(state.runtime.clone()),
    )
    .await?;
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
    let mut items = Vec::with_capacity(page.items.len());
    for r in page.items {
        let cover_path = covers::ensure_album_cover_path(
            &state.db,
            &state.config.library_path,
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
            has_cue_files: cue::album_has_cue_files(&state.config.library_path, r.path.as_deref()),
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
    let cover_path = covers::ensure_album_cover_path(
        &state.db,
        &state.config.library_path,
        album.id,
        album.path.as_deref(),
        album.cover_path.as_deref(),
    )
    .await?;
    let track_rows = tracks::list_by_album(&state.db, id).await?;
    let album_tags_from_file = track_rows.first().and_then(|first| {
        let file_path = state.config.library_path.join(&first.path);
        tags::read_tags(&file_path).ok()
    });
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
    let has_cue_files = cue::album_has_cue_files(&state.config.library_path, album.path.as_deref());
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

    for track in &track_rows {
        let file_path = state.config.library_path.join(&track.path);
        let current = tags::read_tags(&file_path)?;
        let updated = apply_album_patch(&current, &patch);
        tags::write_tags(&file_path, &updated)?;
        let mtime = file_metadata_iso(&file_path)?;
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
                file_mtime: mtime.as_deref(),
            },
        )
        .await?;
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
        .and_then(|v| v.to_str().ok());
    let result = covers::write_album_cover_from_bytes(
        &state.db,
        &state.config.library_path,
        id,
        album_rel,
        &body,
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
    let album = albums::get_by_id(&state.db, id)
        .await?
        .ok_or_else(|| ApiError::Message("album not found".into()))?;
    let rel = covers::ensure_album_cover_path(
        &state.db,
        &state.config.library_path,
        album.id,
        album.path.as_deref(),
        album.cover_path.as_deref(),
    )
    .await?
    .ok_or_else(|| ApiError::Message("album cover not found".into()))?;
    let path = covers::resolve_library_relative_file(&state.config.library_path, &rel)?;
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|_| ApiError::Message("cover file not found".into()))?;
    let ct = covers::image_content_type(&path);
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
    let path = covers::resolve_library_relative_file(&state.config.library_path, rel)?;
    let range = headers
        .get(axum::http::header::RANGE)
        .and_then(|v| v.to_str().ok());
    stream::audio_file_response(&path, range).await
}

pub async fn patch_library_track_tags(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<LibraryTrackTagsPatchRequest>,
) -> Result<Json<LibraryTrackDetailResponse>, ApiError> {
    let track = tracks::get_by_id(&state.db, id)
        .await?
        .ok_or_else(|| ApiError::Message("track not found".into()))?;
    let file_path = state.config.library_path.join(&track.path);
    let current = tags::read_tags(&file_path)?;
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
    tags::write_tags(&file_path, &updated)?;

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

    let mtime = file_metadata_iso(&file_path)?;
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
            file_mtime: mtime.as_deref(),
        },
    )
    .await?;

    let detail = track_detail(&state, id).await?;
    Ok(Json(detail))
}

async fn track_detail(state: &AppState, id: i64) -> Result<LibraryTrackDetailResponse, ApiError> {
    let track = tracks::get_by_id(&state.db, id)
        .await?
        .ok_or_else(|| ApiError::Message("track not found".into()))?;
    let file_path = state.config.library_path.join(&track.path);
    let t = tags::read_tags(&file_path)?;
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
    let response = cue::load_album_cue(
        &state.config.library_path,
        album_path,
        q.cue_path.as_deref(),
    )?;
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
    let cue_abs =
        covers::resolve_library_relative_file(&state.config.library_path, &body.document.cue_path)?;
    reject_unsafe_cue_audio_path(&body.document.audio_path)?;
    let payload = cue_jobs::CueJobPayload {
        cue_path: body.document.cue_path.clone(),
        audio_path: body.document.audio_path.clone(),
        source_file_policy: body.source_file_policy.clone(),
    };
    let payload_json =
        serde_json::to_string(&payload).map_err(|e| ApiError::Message(e.to_string()))?;
    let tracks_total = body.document.tracks.iter().filter(|t| t.selected).count() as i64;
    let job_id = cue_jobs::create_queued(&state.db, id, tracks_total, Some(&payload_json)).await?;
    spawn_cue_split_job(state, job_id, body, cue_abs, album_path);
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

fn spawn_cue_split_job(
    state: AppState,
    job_id: i64,
    body: CueSplitRequest,
    cue_abs: std::path::PathBuf,
    album_path: Option<String>,
) {
    tokio::spawn(async move {
        if let Err(e) = run_cue_split_job(state, job_id, body, cue_abs, album_path).await {
            tracing::error!(job_id, error = %e, "CUE split job failed");
        }
    });
}

async fn run_cue_split_job(
    state: AppState,
    job_id: i64,
    body: CueSplitRequest,
    cue_abs: std::path::PathBuf,
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
    let cue_dir = cue_abs
        .parent()
        .ok_or_else(|| ApiError::Message("CUE file has no parent directory".into()))?
        .to_path_buf();
    let output_dir = cue_dir.clone();
    let split = tokio::task::spawn_blocking(move || {
        euterpe_cue::split_flac_image(&document, &cue_dir, &output_dir, &options)
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
                let _ = tokio::fs::remove_file(&cue_abs).await;
            }
            if let Some(album_path) = album_path
                && let Ok(scan_root) = library_scan::resolve_scan_root_query(
                    &state.config.library_path,
                    Some(album_path.as_str()),
                )
            {
                let scan_cfg = state
                    .runtime
                    .read()
                    .await
                    .library_scan_config(state.config.debug)?;
                let _ = library_scan::start_scan(
                    &state.db,
                    state.config.library_path.clone(),
                    state.scan_events.clone(),
                    scan_cfg,
                    scan_root,
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

fn file_metadata_iso(path: &std::path::Path) -> Result<Option<String>, ApiError> {
    let meta = std::fs::metadata(path).map_err(|e| ApiError::Message(e.to_string()))?;
    Ok(meta.modified().ok().map(|t| {
        let dt: chrono::DateTime<chrono::Utc> = t.into();
        dt.format("%Y-%m-%d %H:%M:%S").to_string()
    }))
}
