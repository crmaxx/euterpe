use std::path::{Path, PathBuf};

use crate::api::{
    CueAlbumResponse, CueDocument, CueExtraField, CueFileChoice, CueIssue, CueJobSummary,
    CueValidationResponse,
};
use crate::db::cue_jobs::CueJobRow;
use crate::error::ApiError;
use crate::library::storage::{LibraryStorage, StorageEntryKind, StoragePath};

pub fn album_has_cue_files(library_root: &Path, album_path_rel: Option<&str>) -> bool {
    album_path_rel
        .and_then(|rel| discover_cue_files(library_root, rel).ok())
        .is_some_and(|files| !files.is_empty())
}

pub async fn album_has_cue_files_storage(
    storage: &dyn LibraryStorage,
    album_path_rel: Option<&str>,
) -> Result<bool, ApiError> {
    let Some(rel) = album_path_rel.filter(|rel| !rel.trim().is_empty()) else {
        return Ok(false);
    };
    Ok(discover_cue_files_storage(storage, rel)
        .await
        .map(|files| !files.is_empty())
        .unwrap_or(false))
}

pub fn discover_cue_files(
    library_root: &Path,
    album_path_rel: &str,
) -> Result<Vec<String>, ApiError> {
    let album_dir = safe_join(library_root, album_path_rel)?;
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&album_dir)
        .map_err(|e| ApiError::Message(format!("read_dir {}: {e}", album_dir.display())))?
    {
        let entry = entry.map_err(|e| ApiError::Message(e.to_string()))?;
        let path = entry.path();
        if path.is_file()
            && path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("cue"))
        {
            let rel = path
                .strip_prefix(library_root)
                .map_err(|_| ApiError::Message("cue path outside library".into()))?
                .to_string_lossy()
                .replace('\\', "/");
            out.push(rel);
        }
    }
    out.sort();
    Ok(out)
}

pub fn load_album_cue(
    library_root: &Path,
    album_path_rel: &str,
    cue_path_query: Option<&str>,
) -> Result<CueAlbumResponse, ApiError> {
    let cue_files = discover_cue_files(library_root, album_path_rel)?;
    if cue_files.is_empty() {
        return Err(ApiError::Message("album has no CUE files".into()));
    }
    let selected = cue_path_query
        .filter(|q| cue_files.iter().any(|c| c == q))
        .unwrap_or(&cue_files[0]);
    let selected_owned = selected.to_string();
    let cue_abs = safe_join(library_root, selected)?;
    let cue_text = std::fs::read_to_string(&cue_abs)
        .map_err(|e| ApiError::Message(format!("read {}: {e}", cue_abs.display())))?;
    let parsed = euterpe_cue::parse_cue(&cue_text, Path::new(selected))
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    let document = cue_document_to_api(parsed);
    let validation =
        cue_validation_to_api(euterpe_cue::validate_document(&document_to_core(&document)));
    Ok(CueAlbumResponse {
        cue_files: cue_files
            .into_iter()
            .map(|path| CueFileChoice {
                selected: path == selected_owned,
                path,
            })
            .collect(),
        document,
        validation,
    })
}

pub async fn load_album_cue_storage(
    storage: &dyn LibraryStorage,
    album_path_rel: &str,
    cue_path_query: Option<&str>,
) -> Result<CueAlbumResponse, ApiError> {
    let cue_files = discover_cue_files_storage(storage, album_path_rel).await?;
    if cue_files.is_empty() {
        return Err(ApiError::Message("album has no CUE files".into()));
    }
    let selected = cue_path_query
        .filter(|q| cue_files.iter().any(|c| c == q))
        .unwrap_or(&cue_files[0]);
    let selected_owned = selected.to_string();
    let cue_path = StoragePath::parse(selected)?;
    let cue_bytes = storage.read(&cue_path).await?;
    let cue_text = std::str::from_utf8(&cue_bytes)
        .map_err(|e| ApiError::Message(format!("read {}: {e}", cue_path.as_str())))?;
    let parsed = euterpe_cue::parse_cue(cue_text, Path::new(selected))
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    let document = cue_document_to_api(parsed);
    let validation =
        cue_validation_to_api(euterpe_cue::validate_document(&document_to_core(&document)));
    Ok(CueAlbumResponse {
        cue_files: cue_files
            .into_iter()
            .map(|path| CueFileChoice {
                selected: path == selected_owned,
                path,
            })
            .collect(),
        document,
        validation,
    })
}

pub fn validate_api_document(document: &CueDocument) -> CueValidationResponse {
    cue_validation_to_api(euterpe_cue::validate_document(&document_to_core(document)))
}

pub fn document_to_core(document: &CueDocument) -> euterpe_cue::CueDocument {
    euterpe_cue::CueDocument {
        cue_path: document.cue_path.clone(),
        audio_path: document.audio_path.clone(),
        album_title: document.album_title.clone(),
        album_artist: document.album_artist.clone(),
        year: document.year.map(|y| y as u32),
        genre: document.genre.clone(),
        comment: document.comment.clone(),
        extra_fields: document
            .extra_fields
            .iter()
            .map(|f| euterpe_cue::CueExtraField {
                scope: if f.scope == "track" {
                    euterpe_cue::CueFieldScope::Track
                } else {
                    euterpe_cue::CueFieldScope::Album
                },
                track_number: f.track_number.map(|n| n as u32),
                key: f.key.clone(),
                value: f.value.clone(),
            })
            .collect(),
        tracks: document
            .tracks
            .iter()
            .map(|t| euterpe_cue::CueTrack {
                number: t.number as u32,
                artist: t.artist.clone(),
                title: t.title.clone(),
                genre: t.genre.clone(),
                start_index: t.start_index.clone(),
                pregap: t.pregap.clone(),
                duration: t.duration.clone(),
                selected: t.selected,
            })
            .collect(),
    }
}

pub fn cue_job_to_api(row: CueJobRow) -> CueJobSummary {
    let row = crate::db::cue_jobs::row_to_summary(row);
    CueJobSummary {
        id: row.id,
        album_id: row.album_id,
        status: row.status,
        tracks_total: row.tracks_total,
        tracks_done: row.tracks_done,
        progress_pct: row.progress_pct,
        error_message: row.error_message,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

fn cue_document_to_api(document: euterpe_cue::CueDocument) -> CueDocument {
    let audio_format = audio_format(&document.audio_path);
    CueDocument {
        cue_path: document.cue_path,
        audio_path: document.audio_path,
        audio_format,
        album_title: document.album_title,
        album_artist: document.album_artist,
        year: document.year.map(|y| y as i32),
        genre: document.genre,
        comment: document.comment,
        extra_fields: document
            .extra_fields
            .into_iter()
            .map(|f| CueExtraField {
                scope: match f.scope {
                    euterpe_cue::CueFieldScope::Album => "album".into(),
                    euterpe_cue::CueFieldScope::Track => "track".into(),
                },
                track_number: f.track_number.map(|n| n as i32),
                key: f.key,
                value: f.value,
            })
            .collect(),
        tracks: document
            .tracks
            .into_iter()
            .map(|t| crate::api::CueTrack {
                number: t.number as i32,
                artist: t.artist,
                title: t.title,
                genre: t.genre,
                start_index: t.start_index,
                pregap: t.pregap,
                duration: t.duration,
                selected: t.selected,
            })
            .collect(),
    }
}

fn cue_validation_to_api(validation: euterpe_cue::CueValidation) -> CueValidationResponse {
    CueValidationResponse {
        valid: validation.valid,
        issues: validation
            .issues
            .into_iter()
            .map(|i| CueIssue {
                code: i.code,
                message: i.message,
                severity: "error".into(),
                field: i.field,
                track_number: i.track_number.map(|n| n as i32),
                line: i.line.map(|n| n as i32),
                column: i.column.map(|n| n as i32),
            })
            .collect(),
    }
}

fn audio_format(path: &str) -> String {
    match Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("flac") => "flac",
        Some("wav") | Some("wave") => "wav",
        Some("ape") => "ape",
        Some("m4a") | Some("mp4") => "m4a",
        Some("wv") | Some("wavpack") => "wv",
        _ => "unknown",
    }
    .into()
}

async fn discover_cue_files_storage(
    storage: &dyn LibraryStorage,
    album_path_rel: &str,
) -> Result<Vec<String>, ApiError> {
    let album = StoragePath::parse(album_path_rel)?;
    let entries = storage.list_dir(&album).await?;
    let mut out: Vec<String> = entries
        .into_iter()
        .filter(|entry| entry.kind == StorageEntryKind::File)
        .filter(|entry| {
            Path::new(entry.path.as_str())
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("cue"))
        })
        .map(|entry| entry.path.as_str().to_string())
        .collect();
    out.sort();
    Ok(out)
}

fn safe_join(root: &Path, rel: &str) -> Result<PathBuf, ApiError> {
    let joined = root.join(rel);
    if rel.contains("..") || joined.is_absolute() && !joined.starts_with(root) {
        return Err(ApiError::bad_request("path outside library"));
    }
    Ok(joined)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::storage::LocalStorage;

    const VALID_CUE: &str = r#"PERFORMER "Artist"
TITLE "Album"
FILE "album.flac" WAVE
  TRACK 01 AUDIO
    TITLE "First"
    PERFORMER "Artist"
    INDEX 01 00:00:00
"#;

    #[tokio::test]
    async fn album_has_cue_files_storage_finds_case_insensitive_cue_files() {
        let dir = tempfile::tempdir().unwrap();
        let album_dir = dir.path().join("Artist").join("Album");
        std::fs::create_dir_all(&album_dir).unwrap();
        std::fs::write(album_dir.join("Album.CUE"), VALID_CUE).unwrap();
        std::fs::write(album_dir.join("notes.txt"), "ignore").unwrap();
        let storage = LocalStorage::new(dir.path());

        assert!(
            album_has_cue_files_storage(&storage, Some("Artist/Album"))
                .await
                .unwrap()
        );
        assert!(
            !album_has_cue_files_storage(&storage, Some("Artist/Missing"))
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn load_album_cue_storage_reads_selected_cue_from_storage() {
        let dir = tempfile::tempdir().unwrap();
        let album_dir = dir.path().join("Artist").join("Album");
        std::fs::create_dir_all(&album_dir).unwrap();
        std::fs::write(
            album_dir.join("a.cue"),
            VALID_CUE.replace("Album", "First Album"),
        )
        .unwrap();
        std::fs::write(
            album_dir.join("b.cue"),
            VALID_CUE.replace("Album", "Second Album"),
        )
        .unwrap();
        let storage = LocalStorage::new(dir.path());

        let response = load_album_cue_storage(&storage, "Artist/Album", Some("Artist/Album/b.cue"))
            .await
            .unwrap();

        assert_eq!(response.document.cue_path, "Artist/Album/b.cue");
        assert_eq!(response.document.album_title, "Second Album");
        assert_eq!(response.cue_files.len(), 2);
        assert_eq!(response.cue_files[0].path, "Artist/Album/a.cue");
        assert!(!response.cue_files[0].selected);
        assert_eq!(response.cue_files[1].path, "Artist/Album/b.cue");
        assert!(response.cue_files[1].selected);
    }
}
