use std::path::{Component, Path, PathBuf};
use std::pin::Pin;

use async_trait::async_trait;
use bytes::Bytes;
use futures_util::Stream;
use std::sync::Arc;

use crate::crypto::MasterKey;
use crate::error::ApiError;
use crate::services::app_settings::StorageLocation;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StoragePath(String);

impl StoragePath {
    pub fn parse(input: impl AsRef<str>) -> Result<Self, ApiError> {
        let raw = input.as_ref().trim().replace('\\', "/");
        if raw.starts_with('/') || raw.starts_with("//") || looks_like_windows_drive(&raw) {
            return Err(ApiError::bad_request("invalid library-relative path"));
        }
        let mut parts = Vec::new();
        for part in raw.split('/') {
            let part = part.trim();
            if part.is_empty() || part == "." {
                continue;
            }
            if part == ".." {
                return Err(ApiError::bad_request("library path must not escape root"));
            }
            parts.push(part);
        }
        Ok(Self(parts.join("/")))
    }

    pub fn root() -> Self {
        Self(String::new())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_root(&self) -> bool {
        self.0.is_empty()
    }

    pub fn parent(&self) -> Option<Self> {
        let (parent, _) = self.0.rsplit_once('/')?;
        Some(Self(parent.to_string()))
    }

    pub fn file_name(&self) -> Option<&str> {
        if self.0.is_empty() {
            return None;
        }
        Some(self.0.rsplit('/').next().unwrap_or(self.0.as_str()))
    }

    pub fn join(&self, child: &str) -> Result<Self, ApiError> {
        let child = StoragePath::parse(child)?;
        if self.is_root() {
            return Ok(child);
        }
        if child.is_root() {
            return Ok(self.clone());
        }
        StoragePath::parse(format!("{}/{}", self.0, child.0))
    }

    pub fn to_local_path(&self, root: &Path) -> PathBuf {
        let mut out = root.to_path_buf();
        for part in self.0.split('/').filter(|p| !p.is_empty()) {
            out.push(part);
        }
        out
    }
}

fn looks_like_windows_drive(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageEntryKind {
    File,
    Directory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageEntry {
    pub name: String,
    pub path: StoragePath,
    pub kind: StorageEntryKind,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageMetadata {
    pub kind: StorageEntryKind,
    pub size: u64,
    pub mtime: Option<String>,
}

pub type StorageByteStream =
    Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static>>;

#[async_trait]
pub trait LibraryStorage: Send + Sync {
    async fn metadata(&self, path: &StoragePath) -> Result<StorageMetadata, ApiError>;
    async fn list_dir(&self, path: &StoragePath) -> Result<Vec<StorageEntry>, ApiError>;
    async fn read(&self, path: &StoragePath) -> Result<Bytes, ApiError>;
    async fn read_at(&self, path: &StoragePath, offset: u64, len: usize)
    -> Result<Bytes, ApiError>;
    async fn read_stream(
        &self,
        path: &StoragePath,
        offset: u64,
        len: Option<u64>,
    ) -> Result<StorageByteStream, ApiError>;
    async fn atomic_write(&self, path: &StoragePath, bytes: Bytes) -> Result<(), ApiError>;
    async fn create_dir_all(&self, path: &StoragePath) -> Result<(), ApiError>;
    async fn rename(&self, from: &StoragePath, to: &StoragePath) -> Result<(), ApiError>;
    async fn delete(&self, path: &StoragePath) -> Result<(), ApiError>;
}

#[derive(Debug, Clone)]
pub struct LocalStorage {
    root: PathBuf,
}

#[derive(Clone)]
pub struct SmbStorage {
    client: Arc<euterpe_smb::SmbStorageClient>,
    root: euterpe_smb::SmbShareLocation,
    credentials: euterpe_smb::SmbCredentials,
}

impl SmbStorage {
    pub fn new(
        root: euterpe_smb::SmbShareLocation,
        credentials: euterpe_smb::SmbCredentials,
    ) -> Self {
        Self {
            client: Arc::new(euterpe_smb::SmbStorageClient::new()),
            root,
            credentials,
        }
    }

    fn location(&self, path: &StoragePath) -> euterpe_smb::SmbShareLocation {
        let root = euterpe_smb::normalize_remote_path(&self.root.path);
        let rel = path.as_str();
        let joined = match (root.is_empty(), rel.is_empty()) {
            (true, true) => String::new(),
            (true, false) => rel.to_string(),
            (false, true) => root,
            (false, false) => format!("{root}/{rel}"),
        };
        euterpe_smb::SmbShareLocation {
            path: joined,
            ..self.root.clone()
        }
    }
}

pub fn storage_from_location(
    location: &StorageLocation,
    master_key: Option<&MasterKey>,
) -> Result<Arc<dyn LibraryStorage>, ApiError> {
    match location {
        StorageLocation::Local { path } => Ok(Arc::new(LocalStorage::new(path))),
        StorageLocation::Smb {
            host,
            port,
            share,
            path,
            username,
            password_encrypted,
            ..
        } => {
            let password = match password_encrypted {
                Some(value) => master_key
                    .ok_or_else(|| {
                        ApiError::Message(
                            "EUTERPE_MASTER_KEY is required for SMB library storage".into(),
                        )
                    })?
                    .decrypt(value)?,
                None => String::new(),
            };
            Ok(Arc::new(SmbStorage::new(
                euterpe_smb::SmbShareLocation {
                    host: host.clone(),
                    port: *port,
                    share: share.clone(),
                    path: path.clone(),
                },
                euterpe_smb::SmbCredentials {
                    username: username.clone().unwrap_or_default(),
                    password,
                },
            )))
        }
    }
}

impl LocalStorage {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn abs(&self, path: &StoragePath) -> PathBuf {
        path.to_local_path(&self.root)
    }
}

fn format_mtime(t: std::time::SystemTime) -> String {
    let dt: chrono::DateTime<chrono::Utc> = t.into();
    dt.format("%Y-%m-%d %H:%M:%S").to_string()
}

fn ensure_local_child(root: &Path, path: &Path) -> Result<(), ApiError> {
    for component in path.components() {
        match component {
            Component::ParentDir | Component::Prefix(_) => {
                return Err(ApiError::bad_request("invalid library path"));
            }
            _ => {}
        }
    }
    if !path.starts_with(root) {
        return Err(ApiError::bad_request("library path outside root"));
    }
    Ok(())
}

#[async_trait]
impl LibraryStorage for LocalStorage {
    async fn metadata(&self, path: &StoragePath) -> Result<StorageMetadata, ApiError> {
        let abs = self.abs(path);
        ensure_local_child(&self.root, &abs)?;
        let meta = tokio::fs::metadata(&abs)
            .await
            .map_err(|e| ApiError::Message(format!("storage metadata: {e}")))?;
        Ok(StorageMetadata {
            kind: if meta.is_dir() {
                StorageEntryKind::Directory
            } else {
                StorageEntryKind::File
            },
            size: meta.len(),
            mtime: meta.modified().ok().map(format_mtime),
        })
    }

    async fn list_dir(&self, path: &StoragePath) -> Result<Vec<StorageEntry>, ApiError> {
        let abs = self.abs(path);
        ensure_local_child(&self.root, &abs)?;
        let mut read_dir = tokio::fs::read_dir(&abs)
            .await
            .map_err(|e| ApiError::Message(format!("storage list: {e}")))?;
        let mut entries = Vec::new();
        while let Some(entry) = read_dir
            .next_entry()
            .await
            .map_err(|e| ApiError::Message(format!("storage list: {e}")))?
        {
            let name = entry.file_name().to_string_lossy().to_string();
            let meta = entry
                .metadata()
                .await
                .map_err(|e| ApiError::Message(format!("storage metadata: {e}")))?;
            entries.push(StorageEntry {
                path: path.join(&name)?,
                name,
                kind: if meta.is_dir() {
                    StorageEntryKind::Directory
                } else {
                    StorageEntryKind::File
                },
                size: if meta.is_dir() {
                    None
                } else {
                    Some(meta.len())
                },
            });
        }
        entries.sort_by(|a, b| {
            (b.kind == StorageEntryKind::Directory)
                .cmp(&(a.kind == StorageEntryKind::Directory))
                .then_with(|| a.name.cmp(&b.name))
        });
        Ok(entries)
    }

    async fn read(&self, path: &StoragePath) -> Result<Bytes, ApiError> {
        let abs = self.abs(path);
        ensure_local_child(&self.root, &abs)?;
        tokio::fs::read(abs)
            .await
            .map(Bytes::from)
            .map_err(|e| ApiError::Message(format!("storage read: {e}")))
    }

    async fn read_at(
        &self,
        path: &StoragePath,
        offset: u64,
        len: usize,
    ) -> Result<Bytes, ApiError> {
        use tokio::io::{AsyncReadExt, AsyncSeekExt};
        let abs = self.abs(path);
        ensure_local_child(&self.root, &abs)?;
        let mut file = tokio::fs::File::open(abs)
            .await
            .map_err(|e| ApiError::Message(format!("storage read: {e}")))?;
        file.seek(std::io::SeekFrom::Start(offset))
            .await
            .map_err(|e| ApiError::Message(format!("storage seek: {e}")))?;
        let mut buf = vec![0; len];
        let n = file
            .read(&mut buf)
            .await
            .map_err(|e| ApiError::Message(format!("storage read: {e}")))?;
        buf.truncate(n);
        Ok(Bytes::from(buf))
    }

    async fn read_stream(
        &self,
        path: &StoragePath,
        offset: u64,
        len: Option<u64>,
    ) -> Result<StorageByteStream, ApiError> {
        use tokio::io::{AsyncReadExt, AsyncSeekExt};
        use tokio_util::io::ReaderStream;
        let abs = self.abs(path);
        ensure_local_child(&self.root, &abs)?;
        let mut file = tokio::fs::File::open(abs)
            .await
            .map_err(|e| ApiError::Message(format!("storage read: {e}")))?;
        file.seek(std::io::SeekFrom::Start(offset))
            .await
            .map_err(|e| ApiError::Message(format!("storage seek: {e}")))?;
        let stream: StorageByteStream = match len {
            Some(len) => Box::pin(ReaderStream::new(file.take(len))),
            None => Box::pin(ReaderStream::new(file)),
        };
        Ok(stream)
    }

    async fn atomic_write(&self, path: &StoragePath, bytes: Bytes) -> Result<(), ApiError> {
        let abs = self.abs(path);
        ensure_local_child(&self.root, &abs)?;
        if let Some(parent) = abs.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| ApiError::Message(format!("storage mkdir: {e}")))?;
        }
        let tmp = abs.with_file_name(format!(
            ".{}.euterpe-part",
            abs.file_name().and_then(|n| n.to_str()).unwrap_or("file")
        ));
        tokio::fs::write(&tmp, bytes)
            .await
            .map_err(|e| ApiError::Message(format!("storage write: {e}")))?;
        tokio::fs::rename(&tmp, &abs)
            .await
            .map_err(|e| ApiError::Message(format!("storage rename: {e}")))?;
        Ok(())
    }

    async fn create_dir_all(&self, path: &StoragePath) -> Result<(), ApiError> {
        let abs = self.abs(path);
        ensure_local_child(&self.root, &abs)?;
        tokio::fs::create_dir_all(abs)
            .await
            .map_err(|e| ApiError::Message(format!("storage mkdir: {e}")))
    }

    async fn rename(&self, from: &StoragePath, to: &StoragePath) -> Result<(), ApiError> {
        let from_abs = self.abs(from);
        let to_abs = self.abs(to);
        ensure_local_child(&self.root, &from_abs)?;
        ensure_local_child(&self.root, &to_abs)?;
        tokio::fs::rename(from_abs, to_abs)
            .await
            .map_err(|e| ApiError::Message(format!("storage rename: {e}")))
    }

    async fn delete(&self, path: &StoragePath) -> Result<(), ApiError> {
        let abs = self.abs(path);
        ensure_local_child(&self.root, &abs)?;
        let meta = tokio::fs::metadata(&abs)
            .await
            .map_err(|e| ApiError::Message(format!("storage delete: {e}")))?;
        if meta.is_dir() {
            tokio::fs::remove_dir_all(abs).await
        } else {
            tokio::fs::remove_file(abs).await
        }
        .map_err(|e| ApiError::Message(format!("storage delete: {e}")))
    }
}

#[async_trait]
impl LibraryStorage for SmbStorage {
    async fn metadata(&self, path: &StoragePath) -> Result<StorageMetadata, ApiError> {
        let meta = self
            .client
            .metadata(&self.location(path), &self.credentials)
            .await
            .map_err(|e| ApiError::Message(format!("SMB_STORAGE_METADATA_FAILED: {e}")))?;
        Ok(StorageMetadata {
            kind: match meta.kind {
                euterpe_smb::SmbEntryKind::File => StorageEntryKind::File,
                euterpe_smb::SmbEntryKind::Directory => StorageEntryKind::Directory,
            },
            size: meta.size,
            mtime: None,
        })
    }

    async fn list_dir(&self, path: &StoragePath) -> Result<Vec<StorageEntry>, ApiError> {
        self.client
            .list_directory(&self.location(path), &self.credentials)
            .await
            .map_err(|e| ApiError::Message(format!("SMB_STORAGE_LIST_FAILED: {e}")))?
            .into_iter()
            .map(|entry| {
                Ok(StorageEntry {
                    name: entry.name,
                    path: StoragePath::parse(entry.path)?,
                    kind: if entry.is_dir {
                        StorageEntryKind::Directory
                    } else {
                        StorageEntryKind::File
                    },
                    size: entry.size,
                })
            })
            .collect()
    }

    async fn read(&self, path: &StoragePath) -> Result<Bytes, ApiError> {
        self.client
            .read_all(&self.location(path), &self.credentials)
            .await
            .map_err(|e| ApiError::Message(format!("SMB_STORAGE_READ_FAILED: {e}")))
    }

    async fn read_at(
        &self,
        path: &StoragePath,
        offset: u64,
        len: usize,
    ) -> Result<Bytes, ApiError> {
        self.client
            .read_at(&self.location(path), &self.credentials, offset, len)
            .await
            .map_err(|e| ApiError::Message(format!("SMB_STORAGE_READ_FAILED: {e}")))
    }

    async fn read_stream(
        &self,
        path: &StoragePath,
        offset: u64,
        len: Option<u64>,
    ) -> Result<StorageByteStream, ApiError> {
        let location = self.location(path);
        let credentials = self.credentials.clone();
        let client = self.client.clone();
        let stream = futures_util::stream::unfold(
            (client, location, credentials, offset, len),
            |(client, location, credentials, mut cursor, remaining)| async move {
                if remaining == Some(0) {
                    return None;
                }
                let want = remaining
                    .map(|v| v.min(64 * 1024) as usize)
                    .unwrap_or(64 * 1024);
                let chunk = client.read_at(&location, &credentials, cursor, want).await;
                match chunk {
                    Ok(bytes) if bytes.is_empty() => None,
                    Ok(bytes) => {
                        cursor += bytes.len() as u64;
                        let remaining = remaining.map(|v| v.saturating_sub(bytes.len() as u64));
                        Some((
                            Ok(bytes),
                            (client, location, credentials, cursor, remaining),
                        ))
                    }
                    Err(e) => Some((
                        Err(std::io::Error::other(e.to_string())),
                        (client, location, credentials, cursor, remaining),
                    )),
                }
            },
        );
        Ok(Box::pin(stream))
    }

    async fn atomic_write(&self, path: &StoragePath, bytes: Bytes) -> Result<(), ApiError> {
        self.client
            .atomic_write(&self.location(path), &self.credentials, bytes)
            .await
            .map_err(|e| ApiError::Message(format!("SMB_STORAGE_WRITE_FAILED: {e}")))
    }

    async fn create_dir_all(&self, path: &StoragePath) -> Result<(), ApiError> {
        self.client
            .create_dir_all(&self.location(path), &self.credentials)
            .await
            .map_err(|e| ApiError::Message(format!("SMB_STORAGE_MKDIR_FAILED: {e}")))
    }

    async fn rename(&self, from: &StoragePath, to: &StoragePath) -> Result<(), ApiError> {
        self.client
            .rename(
                &self.location(from),
                &self.location(to),
                &self.credentials,
                true,
            )
            .await
            .map_err(|e| ApiError::Message(format!("SMB_STORAGE_RENAME_FAILED: {e}")))
    }

    async fn delete(&self, path: &StoragePath) -> Result<(), ApiError> {
        self.client
            .delete(&self.location(path), &self.credentials)
            .await
            .map_err(|e| ApiError::Message(format!("SMB_STORAGE_DELETE_FAILED: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_path_normalizes_separators() {
        let path = StoragePath::parse(r"Artist\Album//01.flac").unwrap();
        assert_eq!(path.as_str(), "Artist/Album/01.flac");
        assert_eq!(path.parent().unwrap().as_str(), "Artist/Album");
        assert_eq!(path.file_name().unwrap(), "01.flac");
    }

    #[test]
    fn storage_path_rejects_escape_paths() {
        assert!(StoragePath::parse("../outside.flac").is_err());
        assert!(StoragePath::parse("/absolute.flac").is_err());
        assert!(StoragePath::parse(r"\\nas\share\file.flac").is_err());
        assert!(StoragePath::parse("C:/music/file.flac").is_err());
    }

    #[tokio::test]
    async fn local_storage_round_trips_bytes_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let storage = LocalStorage::new(dir.path());
        let path = StoragePath::parse("Artist/Album/01.flac").unwrap();
        storage
            .atomic_write(&path, Bytes::from_static(b"abcdef"))
            .await
            .unwrap();
        assert_eq!(
            storage.read_at(&path, 2, 3).await.unwrap(),
            Bytes::from_static(b"cde")
        );
        assert_eq!(storage.metadata(&path).await.unwrap().size, 6);
    }
}
