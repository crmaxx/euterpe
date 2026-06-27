use axum::Json;
use axum::body::{Body, Bytes};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::Response;
use serde::Deserialize;

use crate::api::keyset::{
    decode_cursor, ensure_cursor_matches, fingerprint_json, finish_keyset_page,
};
use crate::api::{
    AlbumCoverUploadResponse, ConvertAlbumResponse, ConvertJobResponse, CueAlbumResponse,
    CueJobResponse, CueSplitRequest, CueSplitResponse, CueValidateRequest, CueValidationResponse,
    LibraryAlbumDetailResponse, LibraryAlbumListResponse, LibraryAlbumTagsPatchRequest,
    LibraryScanLatestResponse, LibraryScanRunSummary, LibraryScanStartResponse,
    LibraryTrackDetailResponse, LibraryTrackItem, LibraryTrackTagsPatchRequest,
};
use crate::api::{KeysetPage, SortKeyKind, SortKeyValue, SortOrder};
use crate::error::ApiError;
use crate::library::covers;
use crate::library::cue;
use crate::library::storage::StoragePath;
use crate::library::stream;
use crate::library::tags::{
    self, AlbumTagsPatch, TrackTagsPatch, apply_album_patch, apply_patch, is_audio_file,
    is_convertible_path,
};
use crate::services::app_settings::StorageLocation;
use crate::services::convert::start_album_convert;
use crate::services::library_scan;
use crate::state::AppState;
use euterpe_data::DataHandle;
use euterpe_data::repositories::{catalog, convert_jobs, cue_jobs, library_scan_runs};
use serde_json::json;

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
    if state.runtime.read().await.storage.library.is_none() {
        return Err(ApiError::Message(
            "LIBRARY_STORAGE_NOT_CONFIGURED: configure library storage in Settings".into(),
        ));
    }
    let storage = state.library_storage().await?;
    let scan_root = match q.root.as_deref() {
        Some(root) => Some(StoragePath::parse(root)?),
        None => None,
    };
    state.storage_watch.pause_for_scan().await;
    let scan_id = match library_scan::start_scan_storage(
        &state.data,
        storage,
        state.scan_events.clone(),
        scan_cfg,
        scan_root,
        Some(state.convert_job_tx.clone()),
        Some(state.runtime.clone()),
    )
    .await
    {
        Ok(scan_id) => scan_id,
        Err(error) => {
            state.storage_watch.restart().await;
            return Err(error);
        }
    };
    let data = state.data.clone();
    let watch = state.storage_watch.clone();
    tokio::spawn(async move {
        library_scan::wait_scan_finished(&data, scan_id).await;
        watch.restart().await;
    });
    Ok((
        StatusCode::ACCEPTED,
        Json(LibraryScanStartResponse { scan_id }),
    ))
}

pub async fn cancel_library_scan(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    library_scan::request_cancel(&state.data, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn library_scan_latest(
    State(state): State<AppState>,
) -> Result<Json<LibraryScanLatestResponse>, ApiError> {
    let run = library_scan_runs::latest(&state.data)
        .await?
        .map(scan_run_to_api);
    Ok(Json(LibraryScanLatestResponse { run }))
}

pub async fn get_library_scan(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<LibraryScanRunSummary>, ApiError> {
    let run = library_scan_runs::get_by_id(&state.data, id)
        .await?
        .map(scan_run_to_api)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AlbumSort {
    Title,
    Artist,
    Year,
}

impl AlbumSort {
    fn as_str(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Artist => "artist",
            Self::Year => "year",
        }
    }

    fn key_kind(self) -> SortKeyKind {
        match self {
            Self::Year => SortKeyKind::Int,
            _ => SortKeyKind::Text,
        }
    }

    fn primary_key(self, row: &catalog::AlbumListRow) -> SortKeyValue {
        match self {
            Self::Title => SortKeyValue::Text(row.title.clone()),
            Self::Artist => SortKeyValue::Text(row.artist_name.clone()),
            Self::Year => SortKeyValue::Int(row.year.unwrap_or(-1) as i64),
        }
    }
}

struct AlbumListApiParams {
    sort: AlbumSort,
    order: SortOrder,
    limit: u32,
    q: Option<String>,
    cursor: Option<String>,
}

fn parse_album_sort(value: &str) -> Result<AlbumSort, ApiError> {
    match value {
        "title" => Ok(AlbumSort::Title),
        "artist" => Ok(AlbumSort::Artist),
        "year" => Ok(AlbumSort::Year),
        _ => Err(ApiError::bad_request("sort must be title, artist, or year")),
    }
}

async fn list_albums_keyset_for_api(
    data: &DataHandle,
    params: AlbumListApiParams,
) -> Result<KeysetPage<catalog::AlbumListRow>, ApiError> {
    let fingerprint = fingerprint_json(&json!({ "q": params.q }));
    let after = if let Some(ref cursor_str) = params.cursor {
        let payload = decode_cursor(cursor_str)?;
        let (primary, tie) = ensure_cursor_matches(
            &payload,
            params.sort.as_str(),
            params.order,
            &fingerprint,
            params.sort.key_kind(),
        )?;
        Some(catalog::AlbumListCursor {
            primary: match primary {
                SortKeyValue::Text(value) => catalog::AlbumListSortValue::Text(value),
                SortKeyValue::Int(value) => catalog::AlbumListSortValue::Int(value),
                SortKeyValue::Bool(value) => catalog::AlbumListSortValue::Int(value as i64),
            },
            tie_id: tie,
        })
    } else {
        None
    };
    let page = catalog::list_albums_keyset(
        data,
        catalog::AlbumListParams {
            sort: match params.sort {
                AlbumSort::Title => catalog::AlbumListSort::Title,
                AlbumSort::Artist => catalog::AlbumListSort::Artist,
                AlbumSort::Year => catalog::AlbumListSort::Year,
            },
            order: match params.order {
                SortOrder::Asc => catalog::AlbumListOrder::Asc,
                SortOrder::Desc => catalog::AlbumListOrder::Desc,
            },
            limit: params.limit as usize + 1,
            q: params.q.clone(),
            after,
        },
    )
    .await?;
    let sort = params.sort;
    Ok(finish_keyset_page(
        page.items,
        params.limit as usize,
        sort.as_str(),
        params.order,
        &fingerprint,
        |row| (sort.primary_key(row), row.id),
    ))
}

fn convert_job_to_api(row: convert_jobs::ConvertJobRow) -> crate::api::ConvertJobSummary {
    crate::api::ConvertJobSummary {
        id: row.id,
        album_id: row.album_id,
        status: row.status.as_str().to_string(),
        trigger: row.trigger.as_str().to_string(),
        files_total: row.files_total,
        files_done: row.files_done,
        progress_pct: row.progress_pct,
        error_message: row.error_message,
        payload_json: row.payload_json,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

fn scan_run_to_api(
    row: euterpe_data::repositories::library_scan_runs::LibraryScanRunSummary,
) -> LibraryScanRunSummary {
    LibraryScanRunSummary {
        id: row.id,
        status: row.status,
        files_seen: row.files_seen,
        files_processed: row.files_processed,
        files_indexed: row.files_indexed,
        files_total: row.files_total,
        started_at: row.started_at,
        finished_at: row.finished_at,
        error_message: row.error_message,
    }
}

pub async fn list_library_albums(
    State(state): State<AppState>,
    Query(q): Query<AlbumListQuery>,
) -> Result<Json<LibraryAlbumListResponse>, ApiError> {
    use crate::api::keyset::parse_limit;

    let limit = parse_limit(q.limit, 50, 500)?;
    let sort = parse_album_sort(&q.sort)?;
    let order = match q.order.as_deref() {
        None => SortOrder::Asc,
        Some(s) => SortOrder::parse(s)?,
    };
    let page = list_albums_keyset_for_api(
        &state.data,
        AlbumListApiParams {
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
    let album = catalog::get_album_by_id(&state.data, id)
        .await?
        .ok_or_else(|| ApiError::Message("album not found".into()))?;
    let artist_name = if let Some(aid) = album.artist_id {
        catalog::get_artist_name_by_id(&state.data, aid)
            .await?
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
    let track_rows = catalog::list_tracks_by_album(&state.data, id).await?;
    let album_tags_from_file = match track_rows.first() {
        Some(first) => read_track_tags_for_state(&state, location.as_ref(), &first.path)
            .await
            .ok(),
        None => None,
    };
    let has_convertible_tracks = track_rows
        .iter()
        .any(|track| is_convertible_path(std::path::Path::new(&track.path)));
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
    let album = catalog::get_album_by_id(&state.data, id)
        .await?
        .ok_or_else(|| ApiError::Message("album not found".into()))?;
    let track_rows =
        catalog::list_tracks_by_album_or_path_prefix(&state.data, id, album.path.as_deref())
            .await?;
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
        catalog::update_track_metadata(
            &state.data,
            track.id,
            catalog::TrackMetadataUpdate {
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
            catalog::set_track_file_size(&state.data, track.id, file_size).await?;
        }
    }

    let album_year = body.year.or(album.year);
    if let Some(artist_name) = &artist_name {
        let artist_id = catalog::upsert_artist_by_name(&state.data, artist_name, None).await?;
        let title = album_title.as_deref().unwrap_or(album.title.as_str());
        let _ = catalog::upsert_album(
            &state.data,
            catalog::AlbumUpsert {
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
        let _ = catalog::upsert_album(
            &state.data,
            catalog::AlbumUpsert {
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
    let album = catalog::get_album_by_id(&state.data, id)
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
        &state.data,
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
    let album = catalog::get_album_by_id(&state.data, id)
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
    let track = catalog::get_track_by_id(&state.data, id)
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
    let track = catalog::get_track_by_id(&state.data, id)
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
        let artist_id = catalog::upsert_artist_by_name(&state.data, artist_name, None).await?;
        if let Some(album_title) = &album_title {
            let album = catalog::get_album_by_id(&state.data, track.album_id)
                .await?
                .ok_or_else(|| ApiError::Message("album not found".into()))?;
            let _ = catalog::upsert_album(
                &state.data,
                catalog::AlbumUpsert {
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
        let album = catalog::get_album_by_id(&state.data, track.album_id)
            .await?
            .ok_or_else(|| ApiError::Message("album not found".into()))?;
        let _ = catalog::upsert_album(
            &state.data,
            catalog::AlbumUpsert {
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
    catalog::update_track_metadata(
        &state.data,
        id,
        catalog::TrackMetadataUpdate {
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
        catalog::set_track_file_size(&state.data, id, file_size).await?;
    }

    let detail = track_detail(&state, id).await?;
    Ok(Json(detail))
}

async fn track_detail(state: &AppState, id: i64) -> Result<LibraryTrackDetailResponse, ApiError> {
    let location = state.runtime.read().await.storage.library.clone();
    let track = catalog::get_track_by_id(&state.data, id)
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
    catalog::get_album_by_id(&state.data, id)
        .await?
        .ok_or_else(|| ApiError::Message("album not found".into()))?;
    let job_id = start_album_convert(&state.data, id, &state.convert_job_tx).await?;
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
    let album = catalog::get_album_by_id(&state.data, id)
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
    catalog::get_album_by_id(&state.data, id)
        .await?
        .ok_or_else(|| ApiError::Message("album not found".into()))?;
    Ok(Json(cue::validate_api_document(&body.document)))
}

pub async fn split_library_album_cue(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<CueSplitRequest>,
) -> Result<(StatusCode, Json<CueSplitResponse>), ApiError> {
    let album = catalog::get_album_by_id(&state.data, id)
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
    let tracks_total = body.document.tracks.iter().filter(|t| t.selected).count() as i64;
    let job_id = cue_jobs::create_queued(&state.data, id, tracks_total, Some(&payload)).await?;
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
    let storage = state.library_storage().await?;
    cue::run_storage_cue_split_job(&state.data, storage.clone(), job_id, body, None).await?;
    if let Some(album_path) = album_path
        && let Ok(scan_root) = StoragePath::parse(album_path)
    {
        let scan_cfg = state
            .runtime
            .read()
            .await
            .library_scan_config(state.config.debug)?;
        state.storage_watch.pause_for_scan().await;
        match library_scan::start_scan_storage(
            &state.data,
            storage,
            state.scan_events.clone(),
            scan_cfg,
            Some(scan_root),
            Some(state.convert_job_tx.clone()),
            Some(state.runtime.clone()),
        )
        .await
        {
            Ok(scan_id) => {
                let data = state.data.clone();
                let watch = state.storage_watch.clone();
                tokio::spawn(async move {
                    library_scan::wait_scan_finished(&data, scan_id).await;
                    watch.restart().await;
                });
            }
            Err(error) => {
                state.storage_watch.restart().await;
                tracing::warn!(
                    error = %error,
                    "CUE split follow-up scan failed to start; restarted storage watch"
                );
            }
        }
    }
    Ok(())
}

pub async fn get_library_album_cue_latest(
    State(state): State<AppState>,
    Path(album_id): Path<i64>,
) -> Result<Json<CueJobResponse>, ApiError> {
    catalog::get_album_by_id(&state.data, album_id)
        .await?
        .ok_or_else(|| ApiError::Message("album not found".into()))?;
    let job = cue_jobs::latest_for_album(&state.data, album_id)
        .await?
        .map(cue::cue_job_to_api);
    Ok(Json(CueJobResponse { job }))
}

pub async fn get_library_album_convert_latest(
    State(state): State<AppState>,
    Path(album_id): Path<i64>,
) -> Result<Json<ConvertJobResponse>, ApiError> {
    let row = convert_jobs::latest_for_album(&state.data, album_id)
        .await?
        .ok_or_else(|| ApiError::Message("no convert job for album".into()))?;
    let job = convert_job_to_api(row);
    Ok(Json(ConvertJobResponse { job }))
}

pub async fn get_convert_job(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<ConvertJobResponse>, ApiError> {
    let row = convert_jobs::get_by_id(&state.data, id)
        .await?
        .ok_or_else(|| ApiError::Message("convert job not found".into()))?;
    let job = convert_job_to_api(row);
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
        Some(_) => {
            let storage = state.library_storage().await?;
            covers::ensure_album_cover_path_storage(
                &state.data,
                storage.as_ref(),
                album_id,
                album_path,
                cover_path,
            )
            .await
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
        Some(_) => {
            let storage = state.library_storage().await?;
            let path = StoragePath::parse(rel)?;
            tags::read_tags_storage(storage.as_ref(), &path).await
        }
        None => Err(ApiError::Message(
            "LIBRARY_STORAGE_NOT_CONFIGURED: configure library storage in Settings".into(),
        )),
    }
}
