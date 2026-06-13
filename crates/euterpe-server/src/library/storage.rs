use std::path::{Component, Path, PathBuf};
use std::pin::Pin;

use async_trait::async_trait;
use bytes::Bytes;
use futures_util::{Stream, StreamExt, TryStreamExt};
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
    /// `YYYY-MM-DD HH:MM:SS` from listing when available (SMB/local).
    pub mtime: Option<String>,
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
    async fn atomic_write_stream(
        &self,
        path: &StoragePath,
        stream: StorageByteStream,
    ) -> Result<(), ApiError> {
        let bytes = stream
            .try_fold(Vec::new(), |mut acc, chunk| async move {
                acc.extend_from_slice(&chunk);
                Ok(acc)
            })
            .await
            .map_err(|e| ApiError::Message(format!("storage write: {e}")))?;
        self.atomic_write(path, Bytes::from(bytes)).await
    }
    async fn atomic_write(&self, path: &StoragePath, bytes: Bytes) -> Result<(), ApiError> {
        self.atomic_write_stream(
            path,
            Box::pin(futures_util::stream::once(async move { Ok(bytes) })),
        )
        .await
    }
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
        Self::with_client(
            root,
            credentials,
            Arc::new(euterpe_smb::SmbStorageClient::new()),
        )
    }

    pub fn with_client(
        root: euterpe_smb::SmbShareLocation,
        credentials: euterpe_smb::SmbCredentials,
        client: Arc<euterpe_smb::SmbStorageClient>,
    ) -> Self {
        Self {
            client,
            root,
            credentials,
        }
    }

    pub fn with_session(
        root: euterpe_smb::SmbShareLocation,
        credentials: euterpe_smb::SmbCredentials,
        session: Arc<euterpe_smb::SmbSession>,
    ) -> Self {
        Self::with_client(
            root,
            credentials,
            Arc::new(euterpe_smb::SmbStorageClient::with_session(session)),
        )
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

    fn entry_path_relative_to_library_root(&self, entry_path: &str) -> String {
        smb_entry_path_relative_to_library_root(&self.root.path, entry_path)
    }
}

/// Maps SMB paths (relative to share) to paths relative to the configured library root.
pub(crate) fn smb_entry_path_relative_to_library_root(
    library_root: &str,
    entry_path: &str,
) -> String {
    let root = euterpe_smb::normalize_remote_path(library_root);
    let path = euterpe_smb::normalize_remote_path(entry_path);
    if root.is_empty() {
        return path;
    }
    if path == root {
        return String::new();
    }
    let prefix = format!("{root}/");
    if let Some(suffix) = path.strip_prefix(&prefix) {
        return suffix.to_string();
    }
    path
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
            workgroup,
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
            let username = euterpe_smb::format_smb_username(
                workgroup.as_deref(),
                username.as_deref().unwrap_or_default(),
            );
            Ok(Arc::new(SmbStorage::new(
                euterpe_smb::SmbShareLocation {
                    host: host.clone(),
                    port: *port,
                    share: share.clone(),
                    path: path.clone(),
                },
                euterpe_smb::SmbCredentials { username, password },
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

    async fn canonical_root(&self) -> Result<PathBuf, ApiError> {
        tokio::fs::canonicalize(&self.root)
            .await
            .map_err(|e| ApiError::Message(format!("storage root: {e}")))
    }

    async fn existing_abs(&self, path: &StoragePath) -> Result<PathBuf, ApiError> {
        let root = self.canonical_root().await?;
        let abs = self.abs(path);
        ensure_local_child(&self.root, &abs)?;
        let canonical = tokio::fs::canonicalize(&abs)
            .await
            .map_err(|e| ApiError::Message(format!("storage path: {e}")))?;
        if !canonical.starts_with(&root) {
            return Err(ApiError::bad_request("library path outside root"));
        }
        Ok(canonical)
    }

    async fn create_dir_all_contained(&self, path: &StoragePath) -> Result<PathBuf, ApiError> {
        tokio::fs::create_dir_all(&self.root)
            .await
            .map_err(|e| ApiError::Message(format!("storage mkdir: {e}")))?;
        let root = self.canonical_root().await?;
        let mut cursor = root.clone();
        for part in path.as_str().split('/').filter(|part| !part.is_empty()) {
            cursor.push(part);
            match tokio::fs::symlink_metadata(&cursor).await {
                Ok(meta) if meta.file_type().is_symlink() => {
                    let canonical = tokio::fs::canonicalize(&cursor)
                        .await
                        .map_err(|e| ApiError::Message(format!("storage symlink: {e}")))?;
                    if !canonical.starts_with(&root) {
                        return Err(ApiError::bad_request("library path outside root"));
                    }
                    if !tokio::fs::metadata(&canonical)
                        .await
                        .map_err(|e| ApiError::Message(format!("storage metadata: {e}")))?
                        .is_dir()
                    {
                        return Err(ApiError::bad_request("storage path is not a directory"));
                    }
                    cursor = canonical;
                }
                Ok(meta) => {
                    if !meta.is_dir() {
                        return Err(ApiError::bad_request("storage path is not a directory"));
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    match tokio::fs::create_dir(&cursor).await {
                        Ok(()) => {}
                        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                            if !tokio::fs::metadata(&cursor)
                                .await
                                .map_err(|e| ApiError::Message(format!("storage metadata: {e}")))?
                                .is_dir()
                            {
                                return Err(ApiError::bad_request(
                                    "storage path is not a directory",
                                ));
                            }
                        }
                        Err(e) => return Err(ApiError::Message(format!("storage mkdir: {e}"))),
                    }
                }
                Err(e) => return Err(ApiError::Message(format!("storage metadata: {e}"))),
            }
        }
        let canonical = tokio::fs::canonicalize(&cursor)
            .await
            .map_err(|e| ApiError::Message(format!("storage path: {e}")))?;
        if !canonical.starts_with(&root) {
            return Err(ApiError::bad_request("library path outside root"));
        }
        Ok(canonical)
    }

    async fn writable_parent_abs(&self, path: &StoragePath) -> Result<PathBuf, ApiError> {
        let parent = path.parent().unwrap_or_else(StoragePath::root);
        self.create_dir_all_contained(&parent).await
    }
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
        let abs = self.existing_abs(path).await?;
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
            mtime: meta.modified().ok().map(crate::library::fs::format_mtime),
        })
    }

    async fn list_dir(&self, path: &StoragePath) -> Result<Vec<StorageEntry>, ApiError> {
        let abs = self.existing_abs(path).await?;
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
                mtime: meta.modified().ok().map(crate::library::fs::format_mtime),
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
        let abs = self.existing_abs(path).await?;
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
        let abs = self.existing_abs(path).await?;
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
        let abs = self.existing_abs(path).await?;
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

    async fn atomic_write_stream(
        &self,
        path: &StoragePath,
        mut stream: StorageByteStream,
    ) -> Result<(), ApiError> {
        use tokio::io::AsyncWriteExt;

        let parent = self.writable_parent_abs(path).await?;
        let file_name = path.file_name().unwrap_or("file");
        let abs = parent.join(file_name);
        let tmp = abs.with_file_name(format!(
            ".{}.euterpe-part",
            abs.file_name().and_then(|n| n.to_str()).unwrap_or("file")
        ));

        let mut file = tokio::fs::File::create(&tmp)
            .await
            .map_err(|e| ApiError::Message(format!("storage write: {e}")))?;
        while let Some(chunk) = stream.next().await {
            let chunk = match chunk {
                Ok(chunk) => chunk,
                Err(e) => {
                    let _ = tokio::fs::remove_file(&tmp).await;
                    return Err(ApiError::Message(format!("storage write: {e}")));
                }
            };
            if let Err(e) = file.write_all(&chunk).await {
                let _ = tokio::fs::remove_file(&tmp).await;
                return Err(ApiError::Message(format!("storage write: {e}")));
            }
        }
        if let Err(e) = file.flush().await {
            let _ = tokio::fs::remove_file(&tmp).await;
            return Err(ApiError::Message(format!("storage flush: {e}")));
        }
        drop(file);
        if let Err(e) = tokio::fs::rename(&tmp, &abs).await {
            let _ = tokio::fs::remove_file(&tmp).await;
            return Err(ApiError::Message(format!("storage rename: {e}")));
        }
        Ok(())
    }

    async fn create_dir_all(&self, path: &StoragePath) -> Result<(), ApiError> {
        self.create_dir_all_contained(path).await.map(|_| ())
    }

    async fn rename(&self, from: &StoragePath, to: &StoragePath) -> Result<(), ApiError> {
        let from_abs = self.existing_abs(from).await?;
        let parent = self.writable_parent_abs(to).await?;
        let to_abs = parent.join(to.file_name().unwrap_or("file"));
        tokio::fs::rename(from_abs, to_abs)
            .await
            .map_err(|e| ApiError::Message(format!("storage rename: {e}")))
    }

    async fn delete(&self, path: &StoragePath) -> Result<(), ApiError> {
        let abs = self.existing_abs(path).await?;
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
            mtime: meta.mtime,
        })
    }

    async fn list_dir(&self, path: &StoragePath) -> Result<Vec<StorageEntry>, ApiError> {
        let loc = self.location(path);
        tracing::info!(
            rel = %path.as_str(),
            smb = %loc.path,
            "smb storage list_dir"
        );
        let entries = self
            .client
            .list_directory(&loc, &self.credentials)
            .await
            .map_err(|e| ApiError::Message(format!("SMB_STORAGE_LIST_FAILED: {e}")))?;
        entries
            .into_iter()
            .map(|entry| {
                let rel = self.entry_path_relative_to_library_root(&entry.path);
                Ok(StorageEntry {
                    name: entry.name,
                    path: StoragePath::parse(rel)?,
                    kind: if entry.is_dir {
                        StorageEntryKind::Directory
                    } else {
                        StorageEntryKind::File
                    },
                    size: entry.size,
                    mtime: entry.mtime,
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
        let file = self
            .client
            .open_file_for_read(&self.location(path), &self.credentials)
            .await
            .map_err(|e| ApiError::Message(format!("SMB_STORAGE_READ_FAILED: {e}")))?;
        let stream = file
            .byte_stream(offset, len)
            .map(|chunk| chunk.map_err(|e| std::io::Error::other(e.to_string())));
        Ok(Box::pin(stream))
    }

    async fn atomic_write_stream(
        &self,
        path: &StoragePath,
        stream: StorageByteStream,
    ) -> Result<(), ApiError> {
        self.client
            .atomic_write_stream(&self.location(path), &self.credentials, stream)
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
            .delete_tree(&self.location(path), &self.credentials)
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
    fn smb_entry_path_strips_library_root_prefix() {
        assert_eq!(
            smb_entry_path_relative_to_library_root("Musik/Flac", "Musik/Flac/Aarni"),
            "Aarni"
        );
        assert_eq!(
            smb_entry_path_relative_to_library_root("Musik/Flac", "Musik/Flac"),
            ""
        );
        assert_eq!(
            smb_entry_path_relative_to_library_root("", "Album/track.flac"),
            "Album/track.flac"
        );
    }

    #[test]
    fn storage_path_rejects_escape_paths() {
        assert!(StoragePath::parse("../outside.flac").is_err());
        assert!(StoragePath::parse("/absolute.flac").is_err());
        assert!(StoragePath::parse(r"\\nas\share\file.flac").is_err());
        assert!(StoragePath::parse("C:/music/file.flac").is_err());
    }

    #[test]
    fn format_smb_username_applies_workgroup_for_plain_user() {
        assert_eq!(
            euterpe_smb::format_smb_username(Some("WORKGROUP"), "roon"),
            r"WORKGROUP\roon"
        );
    }

    #[test]
    fn format_smb_username_leaves_domain_qualified_user_unchanged() {
        assert_eq!(
            euterpe_smb::format_smb_username(Some("WORKGROUP"), r"NAS\roon"),
            r"NAS\roon"
        );
        assert_eq!(
            euterpe_smb::format_smb_username(Some("WORKGROUP"), "roon@nas.local"),
            "roon@nas.local"
        );
    }

    #[test]
    fn format_smb_username_without_workgroup_is_unchanged() {
        assert_eq!(euterpe_smb::format_smb_username(None, "roon"), "roon");
        assert_eq!(euterpe_smb::format_smb_username(Some(""), "roon"), "roon");
        assert_eq!(
            euterpe_smb::format_smb_username(Some("   "), "roon"),
            "roon"
        );
    }

    #[test]
    fn format_smb_username_trims_inputs() {
        assert_eq!(
            euterpe_smb::format_smb_username(Some("  WORKGROUP  "), "  roon  "),
            r"WORKGROUP\roon"
        );
    }

    #[tokio::test]
    async fn smb_storage_stream_reuses_file_handle() {
        use futures_util::StreamExt;
        use std::sync::Arc;
        use std::sync::atomic::Ordering;

        let (client, connect_counter) = euterpe_smb::SmbStorageClient::new_for_connect_tests();
        let client = Arc::new(client);
        let storage = SmbStorage::with_client(
            euterpe_smb::SmbShareLocation {
                host: "nas".into(),
                port: 445,
                share: "music".into(),
                path: "library".into(),
            },
            euterpe_smb::SmbCredentials {
                username: "user".into(),
                password: "pass".into(),
            },
            client.clone(),
        );
        let track = StoragePath::parse("track.flac").unwrap();
        let mut stream = storage
            .read_stream(&track, 0, Some(256 * 1024))
            .await
            .unwrap();
        let mut chunks = 0usize;
        while let Some(chunk) = stream.next().await {
            chunk.unwrap();
            chunks += 1;
            if chunks >= 4 {
                break;
            }
        }
        assert_eq!(chunks, 4);
        assert_eq!(connect_counter.load(Ordering::SeqCst), 1);
        assert_eq!(client.open_resource_count(), 1);
    }

    #[tokio::test]
    async fn smb_storage_burst_reuses_share_connect() {
        use std::sync::Arc;
        use std::sync::atomic::Ordering;

        let (session, counter) = euterpe_smb::SmbSession::new_for_connect_tests();
        let storage = SmbStorage::with_session(
            euterpe_smb::SmbShareLocation {
                host: "nas".into(),
                port: 445,
                share: "music".into(),
                path: "library".into(),
            },
            euterpe_smb::SmbCredentials {
                username: "user".into(),
                password: "pass".into(),
            },
            Arc::new(session),
        );
        storage.list_dir(&StoragePath::root()).await.unwrap();
        let track = StoragePath::parse("track.flac").unwrap();
        storage.read_at(&track, 0, 64).await.unwrap();
        storage.read_at(&track, 64, 64).await.unwrap();
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn local_storage_list_dir_includes_file_mtime() {
        let dir = tempfile::tempdir().unwrap();
        let storage = LocalStorage::new(dir.path());
        let path = StoragePath::parse("Artist/Album/01.flac").unwrap();
        storage
            .atomic_write(&path, Bytes::from_static(b"audio"))
            .await
            .unwrap();
        let entries = storage
            .list_dir(&StoragePath::parse("Artist/Album").unwrap())
            .await
            .unwrap();
        let track = entries
            .iter()
            .find(|e| e.name == "01.flac")
            .expect("track in listing");
        assert_eq!(track.size, Some(5));
        assert!(track.mtime.is_some());
        assert_eq!(
            track.mtime.as_deref(),
            storage.metadata(&path).await.unwrap().mtime.as_deref()
        );
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

    #[tokio::test]
    async fn local_storage_atomic_write_creates_missing_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("library");
        let storage = LocalStorage::new(&root);
        let path = StoragePath::parse("Artist/Album/01.flac").unwrap();

        storage
            .atomic_write(&path, Bytes::from_static(b"audio"))
            .await
            .unwrap();

        assert_eq!(
            tokio::fs::read(root.join("Artist/Album/01.flac"))
                .await
                .unwrap(),
            b"audio"
        );
    }

    #[tokio::test]
    async fn local_storage_streaming_atomic_write_publishes_final_and_removes_temp() {
        let dir = tempfile::tempdir().unwrap();
        let storage = LocalStorage::new(dir.path());
        let path = StoragePath::parse("Artist/Album/stream.flac").unwrap();
        let chunks = vec![
            Ok(Bytes::from_static(b"abc")),
            Ok(Bytes::from_static(b"def")),
            Ok(Bytes::from_static(b"ghi")),
        ];
        let stream: StorageByteStream = Box::pin(futures_util::stream::iter(chunks));

        storage.atomic_write_stream(&path, stream).await.unwrap();

        assert_eq!(
            tokio::fs::read(dir.path().join("Artist/Album/stream.flac"))
                .await
                .unwrap(),
            b"abcdefghi"
        );
        assert!(
            !dir.path()
                .join("Artist/Album/.stream.flac.euterpe-part")
                .exists()
        );
    }

    #[tokio::test]
    async fn local_storage_streaming_atomic_write_cleans_temp_after_midstream_error() {
        let dir = tempfile::tempdir().unwrap();
        let storage = LocalStorage::new(dir.path());
        let path = StoragePath::parse("Artist/Album/broken.flac").unwrap();
        let chunks = vec![
            Ok(Bytes::from_static(b"abc")),
            Err(std::io::Error::other("stream failed")),
        ];
        let stream: StorageByteStream = Box::pin(futures_util::stream::iter(chunks));

        let err = storage
            .atomic_write_stream(&path, stream)
            .await
            .unwrap_err();

        assert!(err.to_string().contains("stream failed"));
        assert!(!dir.path().join("Artist/Album/broken.flac").exists());
        assert!(
            !dir.path()
                .join("Artist/Album/.broken.flac.euterpe-part")
                .exists()
        );
    }

    #[tokio::test]
    async fn local_storage_streaming_atomic_write_cleans_temp_after_rename_error() {
        let dir = tempfile::tempdir().unwrap();
        let storage = LocalStorage::new(dir.path());
        let path = StoragePath::parse("Artist/Album/final.flac").unwrap();
        tokio::fs::create_dir_all(dir.path().join("Artist/Album/final.flac"))
            .await
            .unwrap();
        let stream: StorageByteStream = Box::pin(futures_util::stream::iter(vec![Ok(
            Bytes::from_static(b"abc"),
        )]));

        let err = storage
            .atomic_write_stream(&path, stream)
            .await
            .unwrap_err();

        assert!(err.to_string().contains("storage rename"));
        assert!(
            !dir.path()
                .join("Artist/Album/.final.flac.euterpe-part")
                .exists()
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn local_storage_rejects_read_through_symlink_escape() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.flac"), b"secret").unwrap();
        symlink(outside.path(), root.path().join("outside")).unwrap();

        let storage = LocalStorage::new(root.path());
        let path = StoragePath::parse("outside/secret.flac").unwrap();
        let err = storage.read(&path).await.unwrap_err();
        assert!(err.to_string().contains("outside root"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn local_storage_rejects_write_through_symlink_parent_escape() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        symlink(outside.path(), root.path().join("outside")).unwrap();

        let storage = LocalStorage::new(root.path());
        let path = StoragePath::parse("outside/new.flac").unwrap();
        let err = storage
            .atomic_write(&path, Bytes::from_static(b"data"))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("outside root"));
        assert!(!outside.path().join("new.flac").exists());
    }
}
