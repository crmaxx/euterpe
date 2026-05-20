use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use euterpe_qobuz::{AlbumDetail, QobuzApi, QobuzError, Quality, TrackSummary};
use euterpe_torrent::TorrentEngine;
use futures_util::TryStreamExt;
use reqwest::Client;
use sqlx::SqlitePool;
use tokio::sync::{broadcast, mpsc, Mutex, Semaphore};
use tokio_util::io::StreamReader;

use crate::api::{JobProgressEvent, ScanProgressEvent};
use crate::config::AppConfig;
use crate::db::{download_jobs, favorites};
use crate::error::ApiError;
use crate::library::paths::track_path;
use crate::services::download::resolve::{push_album_api_candidate, resolve_from_qobuz_favorites};
use crate::services::download::format_album_display_title;

macro_rules! dl_debug {
    ($deps:expr, $($arg:tt)*) => {
        if $deps.config.debug {
            tracing::debug!($($arg)*);
        }
    };
}

pub fn quality_from_format_id(id: u8) -> Option<Quality> {
    match id {
        5 => Some(Quality::Mp3_320),
        6 => Some(Quality::FlacCd),
        7 => Some(Quality::FlacHiRes),
        27 => Some(Quality::FlacHiResPlus),
        _ => None,
    }
}

pub struct WorkerDeps {
    pub pool: SqlitePool,
    pub qobuz: Arc<Mutex<Box<dyn QobuzApi + Send + Sync>>>,
    pub config: Arc<AppConfig>,
    pub events: broadcast::Sender<JobProgressEvent>,
    pub http: Client,
    pub torrent: Option<Arc<dyn TorrentEngine>>,
    pub torrent_semaphore: Option<Arc<Semaphore>>,
    pub scan_events: broadcast::Sender<ScanProgressEvent>,
    pub job_tx: mpsc::Sender<i64>,
}

fn wake_scheduler(job_tx: &mpsc::Sender<i64>) {
    let _ = job_tx.try_send(0);
}

pub fn spawn_worker(
    mut job_rx: mpsc::Receiver<i64>,
    deps: WorkerDeps,
) -> tokio::task::JoinHandle<()> {
    let deps = Arc::new(deps);
    let deps_dispatch = Arc::clone(&deps);
    tokio::spawn(async move {
        let _ = try_dispatch(&deps_dispatch).await;
        while job_rx.recv().await.is_some() {
            if let Err(e) = try_dispatch(&deps_dispatch).await {
                tracing::error!("download scheduler dispatch failed: {e}");
            }
        }
    })
}

async fn try_dispatch(deps: &Arc<WorkerDeps>) -> Result<(), ApiError> {
    use crate::api::DownloadJobType;

    loop {
        let mut dispatched = false;

        let album_running =
            download_jobs::count_running_by_type(&deps.pool, DownloadJobType::Album).await?;
        if album_running == 0 {
            if let Some(id) =
                download_jobs::next_queued_id(&deps.pool, DownloadJobType::Album).await?
            {
                if download_jobs::claim_running(&deps.pool, id).await? {
                    dispatched = true;
                    let deps = Arc::clone(deps);
                    tokio::spawn(async move {
                        let result = execute_job(id, &deps).await;
                        if let Err(e) = result {
                            tracing::error!(job_id = id, "download job failed: {e}");
                        }
                        wake_scheduler(&deps.job_tx);
                    });
                }
            }
        }

        let torrent_max = if deps.torrent.is_some() {
            deps.config.torrent_max_active
        } else {
            0
        };
        if torrent_max > 0 {
            let torrent_running =
                download_jobs::count_running_by_type(&deps.pool, DownloadJobType::Torrent)
                    .await?;
            if torrent_running < torrent_max as u64 {
                if let Some(id) =
                    download_jobs::next_queued_id(&deps.pool, DownloadJobType::Torrent).await?
                {
                    if download_jobs::claim_running(&deps.pool, id).await? {
                        dispatched = true;
                        let deps = Arc::clone(deps);
                        tokio::spawn(async move {
                            let result = execute_job(id, &deps).await;
                            if let Err(e) = result {
                                tracing::error!(job_id = id, "download job failed: {e}");
                            }
                            wake_scheduler(&deps.job_tx);
                        });
                    }
                }
            }
        }

        if !dispatched {
            break;
        }
    }
    Ok(())
}

async fn fetch_album_detail(
    job_id: i64,
    catalog_id: u64,
    meta: Option<&favorites::FavoriteAlbumMeta>,
    stored_api_id: Option<&str>,
    deps: &WorkerDeps,
) -> Result<AlbumDetail, ApiError> {
    let guard = deps.qobuz.lock().await;
    let mut last_err: Option<QobuzError> = None;

    if let Some(api_id) = stored_api_id.map(str::trim).filter(|s| !s.is_empty()) {
        dl_debug!(
            deps,
            job_id,
            qobuz_id = catalog_id,
            api_album_id = %api_id,
            "album/get attempt"
        );
        match guard.album_ref(api_id).await {
            Ok(album) => {
                tracing::info!(
                    job_id,
                    api_album_id = %api_id,
                    resolved_id = album.summary.id,
                    tracks = album.tracks.as_ref().map(|t| t.items.len()).unwrap_or(0),
                    "album/get ok"
                );
                return Ok(album);
            }
            Err(e @ QobuzError::NotFound { .. }) => {
                tracing::warn!(
                    job_id,
                    api_album_id = %api_id,
                    error = %e,
                    "stored album_api_id not found, trying fallbacks"
                );
                last_err = Some(e);
            }
            Err(e) => return Err(e.into()),
        }
    }

    let mut candidates: Vec<String> = Vec::new();
    if let Some(m) = meta {
        if let Some(slug) = &m.slug {
            push_album_api_candidate(&mut candidates, slug);
        }
    }

    if candidates.is_empty() {
        if let Ok(Some(api_id)) = resolve_from_qobuz_favorites(&**guard, catalog_id).await {
            tracing::info!(
                job_id,
                qobuz_id = catalog_id,
                album_api_id = %api_id,
                "resolved album_api_id from Qobuz favorites (legacy job)"
            );
            push_album_api_candidate(&mut candidates, &api_id);
        }
    }

    let numeric = catalog_id.to_string();
    push_album_api_candidate(&mut candidates, &numeric);

    for api_id in &candidates {
        dl_debug!(deps, job_id, qobuz_id = catalog_id, api_album_id = %api_id, "album/get attempt");
        match guard.album_ref(api_id).await {
            Ok(album) => {
                tracing::info!(
                    job_id,
                    api_album_id = %api_id,
                    resolved_id = album.summary.id,
                    tracks = album.tracks.as_ref().map(|t| t.items.len()).unwrap_or(0),
                    "album/get ok"
                );
                return Ok(album);
            }
            Err(e) => {
                tracing::warn!(
                    job_id,
                    api_album_id = %api_id,
                    error = %e,
                    "album/get failed"
                );
                last_err = Some(e);
            }
        }
    }

    if let Some(m) = meta {
        let query = format!("{} {}", m.title, m.artist_name);
        dl_debug!(deps, job_id, %query, "album/search fallback");
        let results = guard.album_search(&query, 25).await?;
        dl_debug!(
            deps,
            job_id,
            hits = results.len(),
            "album/search results"
        );
        for hit in &results {
            dl_debug!(
                deps,
                job_id,
                hit_id = hit.id,
                hit_api_id = %hit.api_album_id(),
                hit_title = %hit.title,
                "album/search candidate"
            );
        }

        let pick = results
            .iter()
            .find(|a| a.id == catalog_id)
            .or_else(|| {
                results.iter().find(|a| {
                    m.title.eq_ignore_ascii_case(&a.title)
                        || a.title.contains(m.title.as_str())
                        || m.title.contains(&a.title)
                })
            })
            .or(results.first());

        if let Some(hit) = pick {
            tracing::info!(
                job_id,
                qobuz_id = catalog_id,
                hit_id = hit.id,
                preferred = %hit.preferred_album_get_id(),
                "album/search match, retry album/get"
            );
            for api_id in hit.album_get_candidate_ids() {
                dl_debug!(deps, job_id, api_album_id = %api_id, "album/get attempt (search hit)");
                match guard.album_ref(&api_id).await {
                    Ok(album) => {
                        tracing::info!(
                            job_id,
                            api_album_id = %api_id,
                            resolved_id = album.summary.id,
                            "album/get ok (search hit)"
                        );
                        return Ok(album);
                    }
                    Err(e @ QobuzError::NotFound { .. }) => {
                        tracing::warn!(job_id, api_album_id = %api_id, error = %e, "album/get failed");
                        last_err = Some(e);
                    }
                    Err(e) => return Err(e.into()),
                }
            }
        }
    }

    if !candidates.iter().any(|c| c == &numeric) {
        dl_debug!(deps, job_id, api_album_id = %numeric, "album/get attempt (numeric fallback)");
        match guard.album_ref(&numeric).await {
            Ok(album) => return Ok(album),
            Err(e) => last_err = Some(e),
        }
    }

    Err(last_err.map(Into::into).unwrap_or_else(|| {
        ApiError::Message(format!(
            "album not found for qobuz_id {catalog_id}; run POST /api/v1/qobuz/sync or pass album_api_id (e.g. zg7pv28g4mldg)"
        ))
    }))
}

/// Claim a queued job and run it (used in tests; production uses scheduler + `execute_job`).
pub async fn run_job(job_id: i64, deps: &WorkerDeps) -> Result<(), ApiError> {
    if !download_jobs::claim_running(&deps.pool, job_id).await? {
        return Ok(());
    }
    execute_job(job_id, deps).await
}

/// Runs a job that is already in `running` state (claimed by the scheduler).
pub async fn execute_job(job_id: i64, deps: &WorkerDeps) -> Result<(), ApiError> {
    dl_debug!(deps, job_id, "download job executing");

    let job = download_jobs::get(&deps.pool, job_id)
        .await?
        .ok_or_else(|| ApiError::Message(format!("job {job_id} not found")))?;

    let result = match job.job_type {
        crate::api::DownloadJobType::Album => {
            let quality = quality_from_format_id(job.quality as u8)
                .ok_or_else(|| ApiError::bad_request("unsupported quality"))?;
            run_album_job(job_id, job.qobuz_id as u64, quality, deps).await
        }
        crate::api::DownloadJobType::Torrent => {
            super::torrent_job::run_torrent_job(job_id, deps).await
        }
        _ => Err(ApiError::bad_request("unsupported job_type")),
    };

    if let Err(e) = result {
        let _ = download_jobs::finish_failed(&deps.pool, job_id, &e.to_string()).await;
        return Err(e);
    }
    Ok(())
}

async fn run_album_job(
    job_id: i64,
    album_id: u64,
    quality: Quality,
    deps: &WorkerDeps,
) -> Result<(), ApiError> {
    let meta = if album_id > 0 {
        favorites::album_meta(&deps.pool, album_id).await?
    } else {
        None
    };
    let payload = download_jobs::get_payload(&deps.pool, job_id).await?;
    let stored_api_id = payload.as_ref().and_then(|p| p.album_api_id.clone());
    tracing::info!(
        job_id,
        qobuz_id = album_id,
        album_api_id = stored_api_id.as_deref(),
        slug = meta.as_ref().and_then(|m| m.slug.as_deref()),
        title = meta.as_ref().map(|m| m.title.as_str()),
        "download job: resolving album"
    );

    let album = fetch_album_detail(
        job_id,
        album_id,
        meta.as_ref(),
        stored_api_id.as_deref().map(str::as_ref),
        deps,
    )
    .await?;

    let artist = album
        .summary
        .artist
        .as_ref()
        .map(|a| a.name.as_str())
        .unwrap_or("");
    let label = format_album_display_title(artist, &album.summary.title);
    let mut job_payload = payload.unwrap_or_default();
    job_payload.display_title = Some(label);
    download_jobs::set_payload(&deps.pool, job_id, &job_payload).await?;

    let tracks = album
        .tracks
        .as_ref()
        .map(|t| t.items.clone())
        .unwrap_or_default();

    if tracks.is_empty() {
        return Err(ApiError::Message("album has no tracks".into()));
    }

    let total = tracks.len();
    dl_debug!(
        deps,
        job_id,
        qobuz_id = album_id,
        tracks = total,
        concurrency = deps.config.download_concurrency,
        quality = ?quality,
        "album resolved, downloading tracks"
    );
    let semaphore = Arc::new(Semaphore::new(deps.config.download_concurrency));
    let mut done = 0usize;

    for track in &tracks {
        if download_jobs::is_cancelled(&deps.pool, job_id).await? {
            dl_debug!(deps, job_id, "download job stopped (cancelled)");
            tracing::info!(job_id, "download job stopped (cancelled)");
            return Ok(());
        }

        dl_debug!(
            deps,
            job_id,
            track_id = track.id,
            track_number = track.track_number,
            title = %track.title,
            index = done + 1,
            total,
            "downloading track"
        );
        let _permit = semaphore.acquire().await.map_err(|e| ApiError::Message(e.to_string()))?;
        let speed_bps = download_track(job_id, album_id, &album, track, quality, deps).await?;
        done += 1;
        let progress = (done as f64 / total as f64) * 100.0;
        download_jobs::update_progress_and_speed(
            &deps.pool,
            job_id,
            progress,
            Some(speed_bps),
        )
        .await?;
        let _ = deps.events.send(JobProgressEvent {
            id: job_id,
            progress_pct: progress,
            download_speed_bps: speed_bps,
            torrent_detail: None,
        });
    }

    if let Err(e) = crate::library::register_download::register_album_from_qobuz_download(
        &deps.pool,
        &deps.config.library_path,
        album_id,
        &album,
        quality,
    )
    .await
    {
        tracing::warn!(
            job_id,
            error = %e,
            "register downloaded album in library index failed"
        );
    }

    if let Err(e) = crate::library::covers::apply_album_cover_after_download(
        &deps.http,
        &deps.pool,
        &deps.config.library_path,
        &album,
        quality,
        Some(album_id),
    )
    .await
    {
        tracing::warn!(job_id, error = %e, "album cover download/embed failed");
    }

    download_jobs::finish_success(&deps.pool, job_id).await?;
    dl_debug!(deps, job_id, qobuz_id = album_id, tracks = total, "download job finished");
    Ok(())
}

/// Skip re-download when a local file exists and its size matches the remote `Content-Length`.
pub(crate) fn existing_file_matches_remote_size(local_len: u64, remote_len: Option<u64>) -> bool {
    remote_len.is_some_and(|remote| remote == local_len)
}

async fn http_content_length(http: &Client, url: &str) -> Result<Option<u64>, ApiError> {
    let response = http
        .head(url)
        .send()
        .await
        .map_err(|e| ApiError::Message(e.to_string()))?;
    if !response.status().is_success() {
        return Ok(None);
    }
    Ok(response.content_length())
}

async fn download_track(
    job_id: i64,
    catalog_album_id: u64,
    album: &AlbumDetail,
    track: &TrackSummary,
    quality: Quality,
    deps: &WorkerDeps,
) -> Result<u64, ApiError> {
    let stream = {
        let mut guard = deps.qobuz.lock().await;
        guard.track_stream_url(track.id, quality).await?
    };

    let url = stream
        .url
        .filter(|u| !u.is_empty())
        .ok_or_else(|| ApiError::Message("empty stream url".into()))?;

    let dest = track_path(
        &deps.config.library_path,
        album,
        track,
        quality.format_id(),
    );

    if dest.is_file() {
        let local_len = tokio::fs::metadata(&dest)
            .await
            .map_err(|e| ApiError::Message(format!("stat {}: {e}", dest.display())))?
            .len();
        let remote_len = http_content_length(&deps.http, &url).await?;
        if existing_file_matches_remote_size(local_len, remote_len) {
            dl_debug!(
                deps,
                job_id,
                track_id = track.id,
                path = %dest.display(),
                local_len,
                remote_len = remote_len.unwrap(),
                "track file exists with matching size, skipping download"
            );
            return Ok(0);
        }
        dl_debug!(
            deps,
            job_id,
            track_id = track.id,
            path = %dest.display(),
            local_len,
            ?remote_len,
            "track file exists but size differs or unknown, re-downloading"
        );
    }

    dl_debug!(
        deps,
        job_id,
        track_id = track.id,
        path = %dest.display(),
        "fetching stream and writing file"
    );

    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|e| {
            ApiError::Message(format!("mkdir {}: {e}", parent.display()))
        })?;
    }

    let part = dest.with_extension("part");
    let started = Instant::now();
    download_url_to_file(&deps.http, &url, &part).await?;
    tokio::fs::rename(&part, &dest).await.map_err(|e| {
        ApiError::Message(format!("rename {}: {e}", dest.display()))
    })?;
    let size = tokio::fs::metadata(&dest)
        .await
        .map_err(|e| ApiError::Message(format!("stat {}: {e}", dest.display())))?
        .len();
    let elapsed = started.elapsed().as_secs_f64().max(0.001);
    let speed_bps = (size as f64 / elapsed) as u64;

    let tags = crate::library::qobuz_tags::track_tags_from_qobuz(album, track, catalog_album_id);
    if let Err(e) = crate::library::tags::write_qobuz_tags_async(&dest, tags).await {
        tracing::warn!(
            job_id,
            track_id = track.id,
            path = %dest.display(),
            error = %e,
            "write qobuz tags after download failed"
        );
    }

    tracing::info!(job_id, track_id = track.id, path = %dest.display(), "track downloaded");
    Ok(speed_bps)
}

pub async fn download_url_to_file(
    http: &Client,
    url: &str,
    dest: &PathBuf,
) -> Result<(), ApiError> {
    let response = http.get(url).send().await.map_err(|e| ApiError::Message(e.to_string()))?;
    if !response.status().is_success() {
        return Err(ApiError::Message(format!(
            "download HTTP {}",
            response.status()
        )));
    }

    let mut file = tokio::fs::File::create(dest).await.map_err(|e| ApiError::Message(e.to_string()))?;
    let stream = response
        .bytes_stream()
        .map_err(std::io::Error::other);
    let mut reader = StreamReader::new(stream);
    tokio::io::copy(&mut reader, &mut file)
        .await
        .map_err(|e| ApiError::Message(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use axum::{
        body::Body,
        http::{header, StatusCode},
        response::Response,
        routing::get,
        Router,
    };
    use euterpe_qobuz::{
        AlbumDetail, AlbumSummary, ArtistRef, GenreRef, LabelRef, Page, PageRequest, QobuzApi,
        QobuzError, Quality, StreamUrl, TrackSummary,
    };
    use tempfile::tempdir;
    use tokio::sync::{broadcast, Mutex};

    use super::*;
    use crate::api::DownloadJobType;
    use crate::config::AppConfig;
    use crate::db;

    fn stream_mock_router(body: &'static [u8]) -> Router {
        let content_len = body.len();
        let get_body = body.to_vec();
        Router::new().route(
            "/stream",
            get({
                let get_body = get_body.clone();
                move || {
                    let get_body = get_body.clone();
                    async move { get_body }
                }
            })
            .head(move || async move {
                Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_LENGTH, content_len.to_string())
                    .body(Body::empty())
                    .unwrap()
            }),
        )
    }

    #[test]
    fn existing_file_matches_remote_size_only_when_equal() {
        assert!(existing_file_matches_remote_size(100, Some(100)));
        assert!(!existing_file_matches_remote_size(100, Some(99)));
        assert!(!existing_file_matches_remote_size(100, None));
    }

    struct MockDownloadQobuz {
        album: AlbumDetail,
        stream_url: String,
    }

    #[async_trait]
    impl QobuzApi for MockDownloadQobuz {
        async fn favorites_albums(
            &self,
            _p: PageRequest,
        ) -> Result<Page<AlbumSummary>, QobuzError> {
            unimplemented!()
        }
        async fn favorites_all_albums(&self) -> Result<Vec<AlbumSummary>, QobuzError> {
            Ok(vec![])
        }
        async fn favorites_album_api_id_for_catalog(
            &self,
            _catalog_id: u64,
        ) -> Result<Option<String>, QobuzError> {
            Ok(None)
        }
        async fn favorite_add_albums(&self, _ids: &[u64]) -> Result<(), QobuzError> {
            Ok(())
        }
        async fn favorite_remove_albums(&self, _ids: &[u64]) -> Result<(), QobuzError> {
            Ok(())
        }
        async fn track_stream_url(
            &mut self,
            _track_id: u64,
            _quality: Quality,
        ) -> Result<StreamUrl, QobuzError> {
            Ok(StreamUrl {
                url: Some(self.stream_url.clone()),
                format_id: Some(6),
                sampling_rate: None,
                bit_depth: None,
                restrictions: None,
            })
        }
        async fn album(&self, _album_id: u64) -> Result<AlbumDetail, QobuzError> {
            Ok(self.album.clone())
        }
        async fn album_ref(&self, _album_id: &str) -> Result<AlbumDetail, QobuzError> {
            Ok(self.album.clone())
        }
        async fn album_search(&self, _query: &str, _limit: u32) -> Result<Vec<AlbumSummary>, QobuzError> {
            Ok(vec![])
        }
        async fn artist_albums(&self, _id: u64) -> Result<Vec<AlbumSummary>, QobuzError> {
            unimplemented!()
        }
    }

    #[tokio::test]
    async fn worker_downloads_album_tracks() {
        let dir = tempdir().unwrap();
        let body = include_bytes!("../../../tests/fixtures/silent.flac");
        let app = stream_mock_router(body);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let stream_url = format!("http://{addr}/stream");
        let album = AlbumDetail {
            summary: AlbumSummary {
                id: 99,
                qobuz_id: None,
                title: "Album".into(),
                artist: Some(ArtistRef {
                    id: 1,
                    name: "Band".into(),
                }),
                artists: None,
                image: None,
                release_date_original: Some("2020-01-15".into()),
                hires: None,
                album_ref: None,
                slug: None,
                list_id: None,
                product_id: None,
                genre: Some(GenreRef {
                    id: Some(1),
                    name: "Rock".into(),
                }),
                label: Some(LabelRef {
                    id: Some(2),
                    name: "Indie".into(),
                }),
            },
            tracks: Some(euterpe_qobuz::AlbumTracks {
                items: vec![
                    TrackSummary {
                        id: 1,
                        title: "One".into(),
                        track_number: Some(1),
                        duration: None,
                        performer: None,
                        hires_streamable: None,
                        media_number: Some(1),
                        genre: Some(GenreRef {
                            id: None,
                            name: "Orchestral".into(),
                        }),
                        isrc: Some("XX-1".into()),
                        composer: Some(ArtistRef {
                            id: 3,
                            name: "Composer".into(),
                        }),
                    },
                    TrackSummary {
                        id: 2,
                        title: "Two".into(),
                        track_number: Some(2),
                        duration: None,
                        performer: None,
                        hires_streamable: None,
                        media_number: None,
                        genre: None,
                        isrc: None,
                        composer: None,
                    },
                ],
            }),
            description: None,
        };

        let pool = db::connect("sqlite::memory:").await.unwrap();
        db::migrate(&pool).await.unwrap();
        let job_id = download_jobs::insert_queued(
            &pool,
            DownloadJobType::Album,
            99,
            6,
            Some(&crate::services::download::DownloadJobPayload {
                album_api_id: Some("99".into()),
                display_title: None,
                torrent: None,
            }),
        )
        .await
        .unwrap();

        let (events, _) = broadcast::channel(8);
        let config = Arc::new(AppConfig {
            bind: "127.0.0.1:0".parse().unwrap(),
            database_url: "sqlite::memory:".into(),
            admin_password: None,
            master_key: None,
            public_base_url: "http://127.0.0.1:0".into(),
            oauth_state_ttl: std::time::Duration::from_secs(600),
            qobuz_api_base: None,
            qobuz_play_base: None,
            library_path: dir.path().to_path_buf(),
            torrent_incoming_dir: None,
            torrent_max_active: 2,
            torrent_enable_upnp: false,
            download_concurrency: 2,
            library_scan: crate::config::LibraryScanConfig::default(),
            debug: false,
            static_dir: std::path::PathBuf::new(),
        });

        let (scan_events, _) = broadcast::channel(8);
        let (job_tx, _job_rx) = mpsc::channel(8);
        let album_for_assert = album.clone();
        let deps = WorkerDeps {
            pool: pool.clone(),
            qobuz: Arc::new(Mutex::new(Box::new(MockDownloadQobuz {
                album,
                stream_url,
            }))),
            config,
            events,
            http: Client::new(),
            torrent: None,
            torrent_semaphore: None,
            scan_events,
            job_tx,
        };

        run_job(job_id, &deps).await.unwrap();

        let job = download_jobs::get(&pool, job_id).await.unwrap().unwrap();
        assert_eq!(job.status, crate::api::DownloadJobStatus::Completed);
        assert!((job.progress_pct - 100.0).abs() < f64::EPSILON);

        let track1 = track_path(
            dir.path(),
            &album_for_assert,
            &album_for_assert.tracks.as_ref().unwrap().items[0],
            6,
        );
        assert!(track1.is_file());
        let tags = crate::library::tags::read_tags(&track1).unwrap();
        assert_eq!(tags.title, "One");
        assert_eq!(tags.artist, "Band");
        assert_eq!(tags.album, "Album");
        assert_eq!(tags.track_number, Some(1));
        assert_eq!(tags.year, Some(2020));
        assert_eq!(tags.disc_number, Some(1));
        assert_eq!(tags.genre.as_deref(), Some("Orchestral"));
        assert_eq!(tags.qobuz_track_id, Some(1));
        assert_eq!(tags.qobuz_album_id, Some(99));
        assert_eq!(tags.label.as_deref(), Some("Indie"));
        assert_eq!(tags.isrc.as_deref(), Some("XX-1"));
        assert_eq!(tags.composer.as_deref(), Some("Composer"));

        let lib_album_id = crate::db::albums::find_id_by_qobuz_album_id(&pool, 99)
            .await
            .unwrap()
            .expect("album row for favorites in_library JOIN (qobuz_album_id=job.qobuz_id)");

        let indexed = crate::db::tracks::list_by_album(&pool, lib_album_id)
            .await
            .unwrap();
        assert_eq!(
            indexed.len(),
            album_for_assert.tracks.as_ref().unwrap().items.len(),
            "tracks indexed without library/scan"
        );
    }

    /// Favorites / download job use catalog id 99; `album/get` may use a different `summary.id`.
    /// `albums.qobuz_album_id` must still match the job for `in_library`.
    #[tokio::test]
    async fn worker_registers_album_with_job_qobuz_id_not_only_summary_id() {
        let dir = tempdir().unwrap();
        let body = b"x";
        let app = stream_mock_router(body);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let stream_url = format!("http://{addr}/stream");
        let album = AlbumDetail {
            summary: AlbumSummary {
                id: 2000,
                qobuz_id: None,
                title: "Other Id Album".into(),
                artist: Some(ArtistRef {
                    id: 1,
                    name: "Band".into(),
                }),
                artists: None,
                image: None,
                release_date_original: None,
                hires: None,
                album_ref: None,
                slug: None,
                list_id: None,
                product_id: None,
                genre: None,
                label: None,
            },
            tracks: Some(euterpe_qobuz::AlbumTracks {
                items: vec![TrackSummary {
                    id: 1,
                    title: "One".into(),
                    track_number: Some(1),
                    duration: None,
                    performer: None,
                    hires_streamable: None,
                    media_number: None,
                    genre: None,
                    isrc: None,
                    composer: None,
                }],
            }),
            description: None,
        };

        let pool = db::connect("sqlite::memory:").await.unwrap();
        db::migrate(&pool).await.unwrap();
        let job_id = download_jobs::insert_queued(
            &pool,
            DownloadJobType::Album,
            99,
            6,
            Some(&crate::services::download::DownloadJobPayload {
                album_api_id: Some("ref".into()),
                display_title: None,
                torrent: None,
            }),
        )
        .await
        .unwrap();

        let (events, _) = broadcast::channel(8);
        let config = Arc::new(AppConfig {
            bind: "127.0.0.1:0".parse().unwrap(),
            database_url: "sqlite::memory:".into(),
            admin_password: None,
            master_key: None,
            public_base_url: "http://127.0.0.1:0".into(),
            oauth_state_ttl: std::time::Duration::from_secs(600),
            qobuz_api_base: None,
            qobuz_play_base: None,
            library_path: dir.path().to_path_buf(),
            torrent_incoming_dir: None,
            torrent_max_active: 2,
            torrent_enable_upnp: false,
            download_concurrency: 2,
            library_scan: crate::config::LibraryScanConfig::default(),
            debug: false,
            static_dir: std::path::PathBuf::new(),
        });

        let (job_tx, _job_rx) = mpsc::channel(8);
        let deps = WorkerDeps {
            pool: pool.clone(),
            qobuz: Arc::new(Mutex::new(Box::new(MockDownloadQobuz {
                album,
                stream_url,
            }))),
            config,
            events,
            http: Client::new(),
            torrent: None,
            torrent_semaphore: None,
            scan_events: {
                let (scan_events, _) = broadcast::channel(8);
                scan_events
            },
            job_tx,
        };

        run_job(job_id, &deps).await.unwrap();

        assert!(
            crate::db::albums::find_id_by_qobuz_album_id(&pool, 99)
                .await
                .unwrap()
                .is_some()
        );
        assert!(
            crate::db::albums::find_id_by_qobuz_album_id(&pool, 2000)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn worker_skips_existing_track_files() {
        let dir = tempdir().unwrap();
        let body = b"downloaded";
        let app = stream_mock_router(body);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let album = AlbumDetail {
            summary: AlbumSummary {
                id: 99,
                qobuz_id: None,
                title: "Album".into(),
                artist: Some(ArtistRef {
                    id: 1,
                    name: "Band".into(),
                }),
                artists: None,
                image: None,
                release_date_original: None,
                hires: None,
                album_ref: None,
                slug: None,
                list_id: None,
                product_id: None,
                genre: None,
                label: None,
            },
            tracks: Some(euterpe_qobuz::AlbumTracks {
                items: vec![
                    TrackSummary {
                        id: 1,
                        title: "One".into(),
                        track_number: Some(1),
                        duration: None,
                        performer: None,
                        hires_streamable: None,
                        media_number: None,
                        genre: None,
                        isrc: None,
                        composer: None,
                    },
                    TrackSummary {
                        id: 2,
                        title: "Two".into(),
                        track_number: Some(2),
                        duration: None,
                        performer: None,
                        hires_streamable: None,
                        media_number: None,
                        genre: None,
                        isrc: None,
                        composer: None,
                    },
                ],
            }),
            description: None,
        };

        let existing = track_path(dir.path(), &album, &album.tracks.as_ref().unwrap().items[0], 6);
        tokio::fs::create_dir_all(existing.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(&existing, body).await.unwrap();

        let pool = db::connect("sqlite::memory:").await.unwrap();
        db::migrate(&pool).await.unwrap();
        let job_id = download_jobs::insert_queued(
            &pool,
            DownloadJobType::Album,
            99,
            6,
            Some(&crate::services::download::DownloadJobPayload {
                album_api_id: Some("99".into()),
                display_title: None,
                torrent: None,
            }),
        )
        .await
        .unwrap();

        let (events, _) = broadcast::channel(8);
        let config = Arc::new(AppConfig {
            bind: "127.0.0.1:0".parse().unwrap(),
            database_url: "sqlite::memory:".into(),
            admin_password: None,
            master_key: None,
            public_base_url: "http://127.0.0.1:0".into(),
            oauth_state_ttl: std::time::Duration::from_secs(600),
            qobuz_api_base: None,
            qobuz_play_base: None,
            library_path: dir.path().to_path_buf(),
            torrent_incoming_dir: None,
            torrent_max_active: 2,
            torrent_enable_upnp: false,
            download_concurrency: 2,
            library_scan: crate::config::LibraryScanConfig::default(),
            debug: false,
            static_dir: std::path::PathBuf::new(),
        });

        let (job_tx, _job_rx) = mpsc::channel(8);
        let deps = WorkerDeps {
            pool: pool.clone(),
            qobuz: Arc::new(Mutex::new(Box::new(MockDownloadQobuz {
                album: album.clone(),
                stream_url: format!("http://{addr}/stream"),
            }))),
            config,
            events,
            http: Client::new(),
            torrent: None,
            torrent_semaphore: None,
            scan_events: {
                let (scan_events, _) = broadcast::channel(8);
                scan_events
            },
            job_tx,
        };

        run_job(job_id, &deps).await.unwrap();

        assert_eq!(std::fs::read(&existing).unwrap(), body);
        let track2 = track_path(dir.path(), &album, &album.tracks.as_ref().unwrap().items[1], 6);
        assert_eq!(std::fs::read(&track2).unwrap(), body);

        let lib_album_id = crate::db::albums::find_id_by_qobuz_album_id(&pool, 99)
            .await
            .unwrap()
            .expect("album indexed after skip-download job");
        let indexed = crate::db::tracks::list_by_album(&pool, lib_album_id)
            .await
            .unwrap();
        assert_eq!(indexed.len(), 2);
        assert!(
            indexed.iter().any(|t| t.qobuz_track_id == Some(1)),
            "skipped download still indexes track from API + on-disk path"
        );
    }

    #[tokio::test]
    async fn worker_redownloads_when_existing_size_mismatches() {
        let dir = tempdir().unwrap();
        let body = b"downloaded";
        let app = stream_mock_router(body);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let album = AlbumDetail {
            summary: AlbumSummary {
                id: 99,
                qobuz_id: None,
                title: "Album".into(),
                artist: Some(ArtistRef {
                    id: 1,
                    name: "Band".into(),
                }),
                artists: None,
                image: None,
                release_date_original: None,
                hires: None,
                album_ref: None,
                slug: None,
                list_id: None,
                product_id: None,
                genre: None,
                label: None,
            },
            tracks: Some(euterpe_qobuz::AlbumTracks {
                items: vec![TrackSummary {
                    id: 1,
                    title: "One".into(),
                    track_number: Some(1),
                    duration: None,
                    performer: None,
                    hires_streamable: None,
                    media_number: None,
                    genre: None,
                    isrc: None,
                    composer: None,
                }],
            }),
            description: None,
        };

        let existing = track_path(dir.path(), &album, &album.tracks.as_ref().unwrap().items[0], 6);
        tokio::fs::create_dir_all(existing.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(&existing, b"short").await.unwrap();

        let pool = db::connect("sqlite::memory:").await.unwrap();
        db::migrate(&pool).await.unwrap();
        let job_id = download_jobs::insert_queued(
            &pool,
            DownloadJobType::Album,
            99,
            6,
            Some(&crate::services::download::DownloadJobPayload {
                album_api_id: Some("99".into()),
                display_title: None,
                torrent: None,
            }),
        )
        .await
        .unwrap();

        let (events, _) = broadcast::channel(8);
        let config = Arc::new(AppConfig {
            bind: "127.0.0.1:0".parse().unwrap(),
            database_url: "sqlite::memory:".into(),
            admin_password: None,
            master_key: None,
            public_base_url: "http://127.0.0.1:0".into(),
            oauth_state_ttl: std::time::Duration::from_secs(600),
            qobuz_api_base: None,
            qobuz_play_base: None,
            library_path: dir.path().to_path_buf(),
            torrent_incoming_dir: None,
            torrent_max_active: 2,
            torrent_enable_upnp: false,
            download_concurrency: 2,
            library_scan: crate::config::LibraryScanConfig::default(),
            debug: false,
            static_dir: std::path::PathBuf::new(),
        });

        let (job_tx, _job_rx) = mpsc::channel(8);
        let deps = WorkerDeps {
            pool: pool.clone(),
            qobuz: Arc::new(Mutex::new(Box::new(MockDownloadQobuz {
                album: album.clone(),
                stream_url: format!("http://{addr}/stream"),
            }))),
            config,
            events,
            http: Client::new(),
            torrent: None,
            torrent_semaphore: None,
            scan_events: {
                let (scan_events, _) = broadcast::channel(8);
                scan_events
            },
            job_tx,
        };

        run_job(job_id, &deps).await.unwrap();

        assert_eq!(std::fs::read(&existing).unwrap(), body);
    }

    #[tokio::test]
    async fn cancelled_job_stays_cancelled_not_failed() {
        let dir = tempdir().unwrap();
        let body = b"downloaded";
        let app = stream_mock_router(body);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let album = AlbumDetail {
            summary: AlbumSummary {
                id: 99,
                qobuz_id: None,
                title: "Album".into(),
                artist: Some(ArtistRef {
                    id: 1,
                    name: "Band".into(),
                }),
                artists: None,
                image: None,
                release_date_original: None,
                hires: None,
                album_ref: None,
                slug: None,
                list_id: None,
                product_id: None,
                genre: None,
                label: None,
            },
            tracks: Some(euterpe_qobuz::AlbumTracks {
                items: vec![TrackSummary {
                    id: 1,
                    title: "One".into(),
                    track_number: Some(1),
                    duration: None,
                    performer: None,
                    hires_streamable: None,
                    media_number: None,
                    genre: None,
                    isrc: None,
                    composer: None,
                }],
            }),
            description: None,
        };

        let pool = db::connect("sqlite::memory:").await.unwrap();
        db::migrate(&pool).await.unwrap();
        let job_id = download_jobs::insert_queued(
            &pool,
            DownloadJobType::Album,
            99,
            6,
            Some(&crate::services::download::DownloadJobPayload {
                album_api_id: Some("99".into()),
                display_title: None,
                torrent: None,
            }),
        )
        .await
        .unwrap();
        download_jobs::claim_running(&pool, job_id).await.unwrap();
        download_jobs::cancel(&pool, job_id).await.unwrap();

        let (events, _) = broadcast::channel(8);
        let config = Arc::new(AppConfig {
            bind: "127.0.0.1:0".parse().unwrap(),
            database_url: "sqlite::memory:".into(),
            admin_password: None,
            master_key: None,
            public_base_url: "http://127.0.0.1:0".into(),
            oauth_state_ttl: std::time::Duration::from_secs(600),
            qobuz_api_base: None,
            qobuz_play_base: None,
            library_path: dir.path().to_path_buf(),
            torrent_incoming_dir: None,
            torrent_max_active: 2,
            torrent_enable_upnp: false,
            download_concurrency: 2,
            library_scan: crate::config::LibraryScanConfig::default(),
            debug: false,
            static_dir: std::path::PathBuf::new(),
        });

        let (scan_events, _) = broadcast::channel(8);
        let (job_tx, _job_rx) = mpsc::channel(8);
        let deps = WorkerDeps {
            pool: pool.clone(),
            qobuz: Arc::new(Mutex::new(Box::new(MockDownloadQobuz {
                album,
                stream_url: format!("http://{addr}/stream"),
            }))),
            config,
            events,
            http: Client::new(),
            torrent: None,
            torrent_semaphore: None,
            scan_events,
            job_tx,
        };

        run_album_job(job_id, 99, Quality::FlacCd, &deps)
            .await
            .unwrap();

        let job = download_jobs::get(&pool, job_id).await.unwrap().unwrap();
        assert_eq!(job.status, crate::api::DownloadJobStatus::Cancelled);
        assert!(job.error_message.is_none());
    }
}
