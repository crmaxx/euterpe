use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use tokio::sync::broadcast;
use walkdir::WalkDir;

use crate::api::ScanProgressEvent;
use crate::db::{albums, artists, library_scan_runs, tracks};
use crate::error::ApiError;
use crate::library::covers::discover_album_cover_rel;
use crate::library::fs::file_mtime_sync;
use crate::library::tags::{self, is_audio_file};

const PROGRESS_EVERY: usize = 5;

pub struct ScanDeps {
    pub pool: SqlitePool,
    pub library_path: PathBuf,
    pub events: broadcast::Sender<ScanProgressEvent>,
}

pub async fn run_scan(scan_id: i64, deps: ScanDeps) {
    if let Err(e) = run_scan_inner(scan_id, &deps).await {
        tracing::error!(scan_id, error = %e, "library scan failed");
        let _ = library_scan_runs::finish_failed(&deps.pool, scan_id, &e.to_string()).await;
    }
}

async fn run_scan_inner(scan_id: i64, deps: &ScanDeps) -> Result<(), ApiError> {
    let root = &deps.library_path;
    if !root.is_dir() {
        return Err(ApiError::Message(format!(
            "library path is not a directory: {}",
            root.display()
        )));
    }

    let mut files_seen = 0i64;
    let mut files_indexed = 0i64;

    for entry in WalkDir::new(root).follow_links(false) {
        let entry = entry.map_err(|e| ApiError::Message(e.to_string()))?;
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if !is_audio_file(path) {
            continue;
        }
        files_seen += 1;

        if library_scan_runs::is_cancelled(&deps.pool, scan_id).await? {
            return Ok(());
        }

        match index_file(&deps.pool, root, path).await {
            Ok(()) => files_indexed += 1,
            Err(e) => tracing::warn!(path = %path.display(), error = %e, "skip file during scan"),
        }

        if (files_seen as usize).is_multiple_of(PROGRESS_EVERY) {
            library_scan_runs::update_progress(&deps.pool, scan_id, files_seen, files_indexed)
                .await?;
            let _ = deps.events.send(ScanProgressEvent {
                scan_id,
                files_seen,
                files_indexed,
            });
        }
    }

    library_scan_runs::update_progress(&deps.pool, scan_id, files_seen, files_indexed).await?;
    let _ = deps.events.send(ScanProgressEvent {
        scan_id,
        files_seen,
        files_indexed,
    });
    library_scan_runs::finish_success(&deps.pool, scan_id).await?;
    Ok(())
}

async fn index_file(pool: &SqlitePool, root: &Path, path: &Path) -> Result<(), ApiError> {
    let tags = tags::read_tags(path)?;
    let rel = path
        .strip_prefix(root)
        .map_err(|_| ApiError::Message("path outside library root".into()))?;
    let path_str = rel.to_string_lossy().replace('\\', "/");
    let album_dir = path.parent().ok_or_else(|| ApiError::Message("no parent".into()))?;
    let album_path_str = album_dir
        .strip_prefix(root)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| album_dir.to_string_lossy().into_owned());

    let artist_id = artists::upsert_by_name(pool, &tags.artist, None).await?;
    let year = tags.year.map(|y| y as i32);
    let cover_path = discover_album_cover_rel(root, &album_path_str);
    let album_id = albums::upsert(
        pool,
        albums::AlbumUpsert {
            artist_id: Some(artist_id),
            title: &tags.album,
            year,
            qobuz_album_id: tags.qobuz_album_id.map(|id| id as i64),
            path: Some(&album_path_str),
            cover_path: cover_path.as_deref(),
        },
    )
    .await?;

    let (mtime, hash) = file_metadata(path)?;
    tracks::upsert(
        pool,
        tracks::TrackUpsert {
            album_id,
            title: &tags.title,
            track_number: tags.track_number.map(|n| n as i32),
            year: tags.year.map(|y| y as i32),
            disc_number: tags.disc_number.map(|d| d as i32),
            genre: tags
                .genre
                .as_deref()
                .and_then(|g| if g.is_empty() { None } else { Some(g) }),
            qobuz_track_id: tags.qobuz_track_id.map(|id| id as i64),
            path: &path_str,
            duration_sec: tags.duration_sec.map(|d| d as i32),
            file_mtime: mtime.as_deref(),
            file_hash: hash.as_deref(),
        },
    )
    .await?;
    Ok(())
}

fn file_metadata(path: &Path) -> Result<(Option<String>, Option<String>), ApiError> {
    let mtime = file_mtime_sync(path);
    let mut hasher = Sha256::new();
    let bytes = std::fs::read(path).map_err(|e| ApiError::Message(e.to_string()))?;
    hasher.update(bytes);
    let hash = hex::encode(hasher.finalize());
    Ok((mtime, Some(hash)))
}

pub fn spawn_scan(scan_id: i64, deps: ScanDeps) {
    tokio::spawn(async move {
        run_scan(scan_id, deps).await;
    });
}

pub async fn start_scan(
    pool: &SqlitePool,
    library_path: PathBuf,
    events: broadcast::Sender<ScanProgressEvent>,
) -> Result<i64, ApiError> {
    if library_scan_runs::has_running(pool).await? {
        return Err(ApiError::Message("SCAN_ALREADY_RUNNING".into()));
    }
    let scan_id = library_scan_runs::start(pool).await?;
    spawn_scan(
        scan_id,
        ScanDeps {
            pool: pool.clone(),
            library_path,
            events,
        },
    );
    Ok(scan_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{connect, migrate};
    use tempfile::TempDir;
    use tokio::sync::broadcast;

    #[tokio::test]
    async fn scan_indexes_audio_files() {
        let dir = TempDir::new().unwrap();
        let artist_dir = dir.path().join("Artist A").join("Album One");
        std::fs::create_dir_all(&artist_dir).unwrap();
        let track_path = artist_dir.join("01.wav");
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 44100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&track_path, spec).unwrap();
        for _ in 0..512 {
            writer.write_sample(0i16).unwrap();
        }
        writer.finalize().unwrap();
        let tags = tags::TrackTags {
            title: "Song".into(),
            artist: "Artist A".into(),
            album: "Album One".into(),
            track_number: Some(1),
            year: Some(2020),
            disc_number: None,
            genre: None,
            duration_sec: None,
            qobuz_track_id: None,
            qobuz_album_id: None,
            label: None,
            isrc: None,
            composer: None,
        };
        tags::write_tags(&track_path, &tags).unwrap();

        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        let (events, _) = broadcast::channel(8);
        let scan_id = library_scan_runs::start(&pool).await.unwrap();
        run_scan(
            scan_id,
            ScanDeps {
                pool: pool.clone(),
                library_path: dir.path().to_path_buf(),
                events,
            },
        )
        .await;

        let run = library_scan_runs::get_by_id(&pool, scan_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(run.status, "success");
        assert!(run.files_indexed >= 1);

        use crate::api::SortOrder;
        use crate::db::albums::{AlbumsListParams, AlbumsSort};
        let page = albums::list_keyset(
            &pool,
            AlbumsListParams {
                sort: AlbumsSort::Title,
                order: SortOrder::Asc,
                limit: 10,
                q: None,
                cursor: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].title, "Album One");
    }
}
