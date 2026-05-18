use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::Response;
use axum::Json;
use serde::Deserialize;

use crate::api::{
    LibraryAlbumDetailResponse, LibraryAlbumListResponse, LibraryScanLatestResponse,
    LibraryScanStartResponse, LibraryScanRunSummary, LibraryTrackDetailResponse,
    LibraryTrackItem, LibraryTrackTagsPatchRequest,
};
use crate::db::{albums, artists, library_scan_runs, tracks};
use crate::error::ApiError;
use crate::library::covers;
use crate::library::tags::{self, apply_patch, TrackTagsPatch};
use crate::services::library_scan;
use crate::state::AppState;

pub async fn start_library_scan(
    State(state): State<AppState>,
) -> Result<(StatusCode, Json<LibraryScanStartResponse>), ApiError> {
    let scan_id = library_scan::start_scan(
        &state.db,
        state.config.library_path.clone(),
        state.scan_events.clone(),
    )
    .await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(LibraryScanStartResponse { scan_id }),
    ))
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
    use crate::api::keyset::parse_limit;
    use crate::api::SortOrder;
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
    Ok(Json(LibraryAlbumDetailResponse {
        id: album.id,
        title: album.title,
        artist_name,
        year: album.year,
        cover_path,
        tracks: tracks_list,
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

fn file_metadata_iso(path: &std::path::Path) -> Result<Option<String>, ApiError> {
    let meta = std::fs::metadata(path).map_err(|e| ApiError::Message(e.to_string()))?;
    Ok(meta.modified().ok().map(|t| {
        let dt: chrono::DateTime<chrono::Utc> = t.into();
        dt.format("%Y-%m-%d %H:%M:%S").to_string()
    }))
}
