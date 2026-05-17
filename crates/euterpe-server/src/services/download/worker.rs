use std::path::PathBuf;
use std::sync::Arc;

use euterpe_qobuz::{AlbumDetail, QobuzApi, Quality, TrackSummary};
use futures_util::TryStreamExt;
use reqwest::Client;
use sqlx::SqlitePool;
use tokio::sync::{broadcast, Mutex, Semaphore};
use tokio_util::io::StreamReader;

use crate::api::JobProgressEvent;
use crate::config::AppConfig;
use crate::db::download_jobs;
use crate::error::ApiError;
use crate::library::paths::track_path;

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
    pub qobuz: Arc<Mutex<dyn QobuzApi>>,
    pub config: Arc<AppConfig>,
    pub events: broadcast::Sender<JobProgressEvent>,
    pub http: Client,
}

pub fn spawn_worker(
    mut job_rx: tokio::sync::mpsc::Receiver<i64>,
    deps: WorkerDeps,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(job_id) = job_rx.recv().await {
            if let Err(e) = run_job(job_id, &deps).await {
                tracing::error!(job_id, "download job failed: {e}");
            }
        }
    })
}

pub async fn run_job(job_id: i64, deps: &WorkerDeps) -> Result<(), ApiError> {
    if !download_jobs::claim_running(&deps.pool, job_id).await? {
        return Ok(());
    }

    let job = download_jobs::get(&deps.pool, job_id)
        .await?
        .ok_or_else(|| ApiError::Message(format!("job {job_id} not found")))?;

    let quality = quality_from_format_id(job.quality as u8)
        .ok_or_else(|| ApiError::bad_request("unsupported quality"))?;

    let result = match job.job_type {
        crate::api::DownloadJobType::Album => {
            run_album_job(job_id, job.qobuz_id as u64, quality, deps).await
        }
        _ => Err(ApiError::bad_request("only job_type=album is supported")),
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
    let album = {
        let guard = deps.qobuz.lock().await;
        guard.album(album_id).await?
    };

    let tracks = album
        .tracks
        .as_ref()
        .map(|t| t.items.clone())
        .unwrap_or_default();

    if tracks.is_empty() {
        return Err(ApiError::Message("album has no tracks".into()));
    }

    let total = tracks.len();
    let semaphore = Arc::new(Semaphore::new(deps.config.download_concurrency));
    let mut done = 0usize;

    for track in &tracks {
        if download_jobs::is_cancelled(&deps.pool, job_id).await? {
            return Err(ApiError::Message("job cancelled".into()));
        }

        let _permit = semaphore.acquire().await.map_err(|e| ApiError::Message(e.to_string()))?;
        download_track(job_id, &album, track, quality, deps).await?;
        done += 1;
        let progress = (done as f64 / total as f64) * 100.0;
        download_jobs::update_progress(&deps.pool, job_id, progress).await?;
        let _ = deps.events.send(JobProgressEvent {
            id: job_id,
            progress_pct: progress,
        });
    }

    download_jobs::finish_success(&deps.pool, job_id).await?;
    Ok(())
}

async fn download_track(
    job_id: i64,
    album: &AlbumDetail,
    track: &TrackSummary,
    quality: Quality,
    deps: &WorkerDeps,
) -> Result<(), ApiError> {
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

    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|e| {
            ApiError::Message(format!("mkdir {}: {e}", parent.display()))
        })?;
    }

    let part = dest.with_extension("part");
    download_url_to_file(&deps.http, &url, &part).await?;
    tokio::fs::rename(&part, &dest).await.map_err(|e| {
        ApiError::Message(format!("rename {}: {e}", dest.display()))
    })?;

    tracing::info!(job_id, track_id = track.id, path = %dest.display(), "track downloaded");
    Ok(())
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
    use axum::{routing::get, Router};
    use euterpe_qobuz::{
        AlbumDetail, AlbumSummary, ArtistRef, Page, PageRequest, QobuzApi,
        QobuzError, Quality, StreamUrl, TrackSummary,
    };
    use tempfile::tempdir;
    use tokio::sync::{broadcast, Mutex};

    use super::*;
    use crate::api::DownloadJobType;
    use crate::config::AppConfig;
    use crate::db;

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
            unimplemented!()
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
        async fn artist_albums(&self, _id: u64) -> Result<Vec<AlbumSummary>, QobuzError> {
            unimplemented!()
        }
    }

    #[tokio::test]
    async fn worker_downloads_album_tracks() {
        let dir = tempdir().unwrap();
        let body = b"fake-flac-bytes";
        let app = Router::new().route(
            "/stream",
            get(|| async { body.to_vec() }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let stream_url = format!("http://{addr}/stream");
        let album = AlbumDetail {
            summary: AlbumSummary {
                id: 99,
                title: "Album".into(),
                artist: Some(ArtistRef {
                    id: 1,
                    name: "Band".into(),
                }),
                artists: None,
                image: None,
                release_date_original: None,
                hires: None,
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
                    },
                    TrackSummary {
                        id: 2,
                        title: "Two".into(),
                        track_number: Some(2),
                        duration: None,
                        performer: None,
                        hires_streamable: None,
                    },
                ],
            }),
            description: None,
        };

        let pool = db::connect("sqlite::memory:").await.unwrap();
        db::migrate(&pool).await.unwrap();
        let job_id = download_jobs::insert_queued(&pool, DownloadJobType::Album, 99, 6)
            .await
            .unwrap();

        let (events, _) = broadcast::channel(8);
        let config = Arc::new(AppConfig {
            bind: "127.0.0.1:0".parse().unwrap(),
            database_url: "sqlite::memory:".into(),
            admin_password: None,
            master_key: None,
            qobuz_user_id: None,
            qobuz_auth_token: None,
            library_path: dir.path().to_path_buf(),
            download_concurrency: 2,
        });

        let deps = WorkerDeps {
            pool: pool.clone(),
            qobuz: Arc::new(Mutex::new(MockDownloadQobuz {
                album,
                stream_url,
            })),
            config,
            events,
            http: Client::new(),
        };

        run_job(job_id, &deps).await.unwrap();

        let job = download_jobs::get(&pool, job_id).await.unwrap().unwrap();
        assert_eq!(job.status, crate::api::DownloadJobStatus::Completed);
        assert!((job.progress_pct - 100.0).abs() < f64::EPSILON);

        let files: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .collect();
        assert!(!files.is_empty());
    }
}
