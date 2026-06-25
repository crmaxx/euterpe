mod srvsvc_ndr;

use bytes::Bytes;
use futures_util::{Stream, StreamExt};
use smb::FileBasicInformation;
use smb::FileStandardInformation;
use smb::binrw_util::prelude::SizedWideString;
use smb::{CreateDisposition, FileAccessMask, FileAttributes, FileCreateArgs, Resource, UncPath};
use smb::{CreateOptions, FileDispositionInformation, FileRenameInformation};
use std::pin::Pin;
use std::sync::Arc;
#[cfg(any(test, feature = "test-hooks"))]
use std::sync::Mutex as StdMutex;
#[cfg(any(test, feature = "test-hooks"))]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::sync::Mutex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmbShareLocation {
    pub host: String,
    pub port: u16,
    pub share: String,
    pub path: String,
}

#[derive(Clone, PartialEq, Eq)]
pub struct SmbCredentials {
    pub username: String,
    pub password: String,
}

impl std::fmt::Debug for SmbCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SmbCredentials")
            .field("username", &self.username)
            .field("password", &"<redacted>")
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmbDirectoryEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: Option<u64>,
    /// `YYYY-MM-DD HH:MM:SS` UTC from SMB `last_write_time`, when available.
    pub mtime: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmbEntryKind {
    File,
    Directory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmbMetadata {
    pub kind: SmbEntryKind,
    pub size: u64,
    /// RFC3339-style UTC timestamp `YYYY-MM-DD HH:MM:SS`, matching local library scan.
    pub mtime: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmbWatchAction {
    Created,
    Removed,
    Modified,
    RenamedOld,
    RenamedNew,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmbWatchEvent {
    pub path: String,
    pub action: SmbWatchAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmbWatchStatus {
    pub connected: bool,
    pub degraded_reason: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum SmbStorageError {
    #[error("invalid SMB location: {0}")]
    InvalidLocation(String),
    #[error("SMB resource type mismatch")]
    ResourceType,
    #[error("SMB client error: {0}")]
    Client(String),
    #[error("SMB IO error: {0}")]
    Io(String),
}

pub type Result<T> = std::result::Result<T, SmbStorageError>;
pub type SmbByteStream =
    Pin<Box<dyn Stream<Item = std::result::Result<Bytes, std::io::Error>> + Send + 'static>>;
const SMB_WRITE_CHUNK: usize = 64 * 1024;

/// SMB share + credential identity for session reuse: one `share_connect` per burst on this tuple.
#[derive(Clone, PartialEq, Eq, Hash)]
struct ConnectKey {
    host: String,
    port: u16,
    share: String,
    username: String,
    password: String,
}

impl std::fmt::Debug for ConnectKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectKey")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("share", &self.share)
            .field("username", &self.username)
            .field("password", &"<redacted>")
            .finish()
    }
}

impl ConnectKey {
    fn new(location: &SmbShareLocation, credentials: &SmbCredentials) -> Self {
        Self {
            host: location.host.clone(),
            port: location.port,
            share: location.share.clone(),
            username: credentials.username.clone(),
            password: credentials.password.clone(),
        }
    }
}

/// Reusable SMB connection to a single share.
///
/// Thread safety: `Arc<smb::Client>` is shared across tasks; `tokio::sync::Mutex` serializes
/// `share_connect` and tracks the active [`ConnectKey`]. After connect, concurrent file/dir
/// operations use the same underlying client without additional `share_connect` calls until
/// the share, username, or password changes.
pub struct SmbSession {
    client: Arc<smb::Client>,
    connected: Mutex<Option<ConnectKey>>,
    /// Serializes `share_connect` / `ipc_connect` (smb `Connection::authenticate` is not re-entrant).
    connect_gate: Mutex<()>,
    /// Serializes tree/file SMB ops on one share (Samba + parallel scan safety).
    op_serial: Mutex<()>,
    #[cfg(any(test, feature = "test-hooks"))]
    share_connect_calls: Option<Arc<AtomicUsize>>,
    #[cfg(any(test, feature = "test-hooks"))]
    open_resource_calls: Option<Arc<AtomicUsize>>,
    #[cfg(any(test, feature = "test-hooks"))]
    close_resource_calls: Option<Arc<AtomicUsize>>,
    #[cfg(any(test, feature = "test-hooks"))]
    write_block_calls: Option<Arc<AtomicUsize>>,
    #[cfg(any(test, feature = "test-hooks"))]
    write_block_sizes: Option<Arc<StdMutex<Vec<usize>>>>,
    #[cfg(any(test, feature = "test-hooks"))]
    dry_run: bool,
}

fn smb_client_config() -> smb::ClientConfig {
    let mut config = smb::ClientConfig::default();
    let secs = std::env::var("EUTERPE_SMB_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(30)
        .clamp(5, 300);
    config.connection.timeout = Some(Duration::from_secs(secs));
    config
}

impl SmbSession {
    pub fn new() -> Self {
        Self {
            client: Arc::new(smb::Client::new(smb_client_config())),
            connected: Mutex::new(None),
            connect_gate: Mutex::new(()),
            op_serial: Mutex::new(()),
            #[cfg(any(test, feature = "test-hooks"))]
            share_connect_calls: None,
            #[cfg(any(test, feature = "test-hooks"))]
            open_resource_calls: None,
            #[cfg(any(test, feature = "test-hooks"))]
            close_resource_calls: None,
            #[cfg(any(test, feature = "test-hooks"))]
            write_block_calls: None,
            #[cfg(any(test, feature = "test-hooks"))]
            write_block_sizes: None,
            #[cfg(any(test, feature = "test-hooks"))]
            dry_run: false,
        }
    }

    #[cfg(any(test, feature = "test-hooks"))]
    pub fn new_for_connect_tests() -> (Self, Arc<AtomicUsize>) {
        let share_connect_calls = Arc::new(AtomicUsize::new(0));
        let open_resource_calls = Arc::new(AtomicUsize::new(0));
        let close_resource_calls = Arc::new(AtomicUsize::new(0));
        let write_block_calls = Arc::new(AtomicUsize::new(0));
        let write_block_sizes = Arc::new(StdMutex::new(Vec::new()));
        (
            Self {
                client: Arc::new(smb::Client::new(smb_client_config())),
                connected: Mutex::new(None),
                connect_gate: Mutex::new(()),
                op_serial: Mutex::new(()),
                share_connect_calls: Some(share_connect_calls.clone()),
                open_resource_calls: Some(open_resource_calls),
                close_resource_calls: Some(close_resource_calls),
                write_block_calls: Some(write_block_calls),
                write_block_sizes: Some(write_block_sizes),
                dry_run: true,
            },
            share_connect_calls,
        )
    }

    #[cfg(any(test, feature = "test-hooks"))]
    pub fn share_connect_count(&self) -> usize {
        self.share_connect_calls
            .as_ref()
            .map(|c| c.load(Ordering::SeqCst))
            .unwrap_or(0)
    }

    #[cfg(any(test, feature = "test-hooks"))]
    pub fn open_resource_count(&self) -> usize {
        self.open_resource_calls
            .as_ref()
            .map(|c| c.load(Ordering::SeqCst))
            .unwrap_or(0)
    }

    #[cfg(any(test, feature = "test-hooks"))]
    pub fn close_resource_count(&self) -> usize {
        self.close_resource_calls
            .as_ref()
            .map(|c| c.load(Ordering::SeqCst))
            .unwrap_or(0)
    }

    #[cfg(any(test, feature = "test-hooks"))]
    pub fn write_block_count(&self) -> usize {
        self.write_block_calls
            .as_ref()
            .map(|c| c.load(Ordering::SeqCst))
            .unwrap_or(0)
    }

    #[cfg(any(test, feature = "test-hooks"))]
    pub fn write_block_sizes(&self) -> Vec<usize> {
        self.write_block_sizes
            .as_ref()
            .map(|sizes| sizes.lock().unwrap().clone())
            .unwrap_or_default()
    }

    #[cfg(any(test, feature = "test-hooks"))]
    fn record_open_resource(&self) {
        if let Some(counter) = &self.open_resource_calls {
            counter.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[cfg(any(test, feature = "test-hooks"))]
    fn record_close_resource(&self) {
        if let Some(counter) = &self.close_resource_calls {
            counter.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[cfg(any(test, feature = "test-hooks"))]
    fn record_write_block(&self, size: usize) {
        if let Some(counter) = &self.write_block_calls {
            counter.fetch_add(1, Ordering::SeqCst);
        }
        if let Some(sizes) = &self.write_block_sizes {
            sizes.lock().unwrap().push(size);
        }
    }

    #[cfg(any(test, feature = "test-hooks"))]
    pub fn is_dry_run(&self) -> bool {
        self.dry_run
    }

    pub fn client(&self) -> &smb::Client {
        &self.client
    }

    async fn connect_share(
        &self,
        location: &SmbShareLocation,
        credentials: &SmbCredentials,
    ) -> Result<UncPath> {
        let _connect_gate = self.connect_gate.lock().await;
        let key = ConnectKey::new(location, credentials);
        if self.connected.lock().await.as_ref() == Some(&key) {
            return unc_for_share(location);
        }
        let unc = unc_for_share(location)?;
        #[cfg(any(test, feature = "test-hooks"))]
        {
            if !self.dry_run {
                self.client
                    .share_connect(&unc, &credentials.username, credentials.password.clone())
                    .await
                    .map_err(map_smb_error)?;
            }
            if let Some(counter) = &self.share_connect_calls {
                counter.fetch_add(1, Ordering::SeqCst);
            }
        }
        #[cfg(not(any(test, feature = "test-hooks")))]
        {
            self.client
                .share_connect(&unc, &credentials.username, credentials.password.clone())
                .await
                .map_err(map_smb_error)?;
        }
        *self.connected.lock().await = Some(key);
        Ok(unc)
    }
}

impl Default for SmbSession {
    fn default() -> Self {
        Self::new()
    }
}

pub struct SmbStorageClient {
    session: Arc<SmbSession>,
}

impl Clone for SmbStorageClient {
    fn clone(&self) -> Self {
        Self {
            session: Arc::clone(&self.session),
        }
    }
}

impl SmbStorageClient {
    pub fn new() -> Self {
        Self {
            session: Arc::new(SmbSession::new()),
        }
    }

    pub fn with_session(session: Arc<SmbSession>) -> Self {
        Self { session }
    }

    #[cfg(any(test, feature = "test-hooks"))]
    pub fn new_for_connect_tests() -> (Self, Arc<AtomicUsize>) {
        let (session, counter) = SmbSession::new_for_connect_tests();
        (Self::with_session(Arc::new(session)), counter)
    }

    #[cfg(any(test, feature = "test-hooks"))]
    pub fn share_connect_count(&self) -> usize {
        self.session.share_connect_count()
    }

    #[cfg(any(test, feature = "test-hooks"))]
    pub fn open_resource_count(&self) -> usize {
        self.session.open_resource_count()
    }

    #[cfg(any(test, feature = "test-hooks"))]
    pub fn close_resource_count(&self) -> usize {
        self.session.close_resource_count()
    }

    #[cfg(any(test, feature = "test-hooks"))]
    pub fn write_block_count(&self) -> usize {
        self.session.write_block_count()
    }

    #[cfg(any(test, feature = "test-hooks"))]
    pub fn write_block_sizes(&self) -> Vec<usize> {
        self.session.write_block_sizes()
    }

    pub async fn list_shares(
        &self,
        host: &str,
        port: u16,
        credentials: &SmbCredentials,
    ) -> Result<Vec<String>> {
        let _op = self.session.op_serial.lock().await;
        let server = format_server(host, port);
        {
            let _connect_gate = self.session.connect_gate.lock().await;
            self.session
                .client()
                .ipc_connect(&server, &credentials.username, credentials.password.clone())
                .await
                .map_err(map_smb_error)?;
        }
        let client = self.session.client();
        match client.list_shares(&server).await {
            Ok(shares) => Ok(shares
                .into_iter()
                .filter_map(|share| share.netname.as_ref().map(|name| name.value.to_string()))
                .collect()),
            Err(smb_err) if srvsvc_ndr::is_ndr64_bind_rejection(&smb_err) => {
                let pipe = client
                    .open_pipe(&server, "srvsvc")
                    .await
                    .map_err(map_smb_error)?;
                let rpc_server = srvsvc_ndr::rpc_server_name(&server);
                srvsvc_ndr::list_shares(pipe, &rpc_server)
                    .await
                    .map_err(map_smb_error)
            }
            Err(smb_err) => Err(map_smb_error(smb_err)),
        }
    }

    pub async fn list_directory(
        &self,
        location: &SmbShareLocation,
        credentials: &SmbCredentials,
    ) -> Result<Vec<SmbDirectoryEntry>> {
        let _op = self.session.op_serial.lock().await;
        let unc = self.session.connect_share(location, credentials).await?;
        #[cfg(any(test, feature = "test-hooks"))]
        if self.session.is_dry_run() {
            #[cfg(test)]
            if location.path.contains("__query_setup_error__") {
                self.session.record_open_resource();
                self.session.record_close_resource();
                return Err(SmbStorageError::Client("query setup failed".into()));
            }
            #[cfg(test)]
            if location.path.contains("__query_item_error__") {
                self.session.record_open_resource();
                self.session.record_close_resource();
                return Err(SmbStorageError::Client("query item failed".into()));
            }
            return Ok(Vec::new());
        }
        let dir_path = normalize_remote_path(&location.path);
        let resource_path = unc.with_path(&dir_path);
        let resource = self
            .session
            .client()
            .create_file(
                &resource_path,
                &smb::FileCreateArgs::make_open_existing(
                    smb::FileAccessMask::new().with_generic_read(true),
                ),
            )
            .await
            .map_err(map_smb_error)?;
        let smb::Resource::Directory(dir) = resource else {
            close_resource_with_session(self.session.as_ref(), resource).await?;
            return Err(SmbStorageError::ResourceType);
        };
        let dir = std::sync::Arc::new(dir);
        let mut close_guard = SmbDirectoryCloseGuard::new(self.session.clone(), dir.clone());
        let query_result = smb::Directory::query::<smb::FileDirectoryInformation>(&dir, "*").await;
        let mut stream = match query_result {
            Ok(stream) => stream,
            Err(e) => {
                return Err(map_smb_error(e));
            }
        };
        let mut entries = Vec::new();
        use futures_util::StreamExt;
        while let Some(item) = stream.next().await {
            let item = match item {
                Ok(item) => item,
                Err(e) => {
                    drop(stream);
                    let _ = close_guard.close().await;
                    return Err(map_smb_error(e));
                }
            };
            let name = item.file_name.to_string();
            if name == "." || name == ".." {
                continue;
            }
            let path = if dir_path.is_empty() {
                name.clone()
            } else {
                format!("{dir_path}/{name}")
            };
            entries.push(SmbDirectoryEntry {
                name,
                path,
                is_dir: item.file_attributes.directory(),
                size: if item.file_attributes.directory() {
                    None
                } else {
                    Some(item.end_of_file)
                },
                mtime: format_file_mtime(&item.last_write_time),
            });
        }
        drop(stream);
        close_guard.close().await?;
        entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.cmp(&b.name)));
        Ok(entries)
    }

    pub async fn metadata(
        &self,
        location: &SmbShareLocation,
        credentials: &SmbCredentials,
    ) -> Result<SmbMetadata> {
        let _op = self.session.op_serial.lock().await;
        #[cfg(any(test, feature = "test-hooks"))]
        if self.session.is_dry_run() {
            self.session.connect_share(location, credentials).await?;
            self.session.record_open_resource();
            let kind = dry_run_resource_kind(&location.path);
            self.session.record_close_resource();
            return Ok(SmbMetadata {
                kind: match kind {
                    DryRunResourceKind::File => SmbEntryKind::File,
                    DryRunResourceKind::Directory => SmbEntryKind::Directory,
                },
                size: 0,
                mtime: None,
            });
        }
        let resource = self
            .open_resource_inner(
                location,
                credentials,
                FileAccessMask::new().with_generic_read(true),
            )
            .await?;
        match resource {
            Resource::File(file) => {
                let metadata = file_metadata(&file).await;
                let close =
                    close_resource_with_session(self.session.as_ref(), Resource::File(file)).await;
                match (metadata, close) {
                    (Ok(metadata), Ok(())) => Ok(metadata),
                    (Err(err), _) => Err(err),
                    (Ok(_), Err(err)) => Err(err),
                }
            }
            Resource::Directory(dir) => {
                let metadata = dir_metadata(&dir).await;
                let close =
                    close_resource_with_session(self.session.as_ref(), Resource::Directory(dir))
                        .await;
                match (metadata, close) {
                    (Ok(metadata), Ok(())) => Ok(metadata),
                    (Err(err), _) => Err(err),
                    (Ok(_), Err(err)) => Err(err),
                }
            }
            resource => {
                close_resource_with_session(self.session.as_ref(), resource).await?;
                Err(SmbStorageError::ResourceType)
            }
        }
    }

    pub async fn delete_tree(
        &self,
        location: &SmbShareLocation,
        credentials: &SmbCredentials,
    ) -> Result<()> {
        delete_tree_recursive(self, location, credentials).await
    }

    pub async fn read_at(
        &self,
        location: &SmbShareLocation,
        credentials: &SmbCredentials,
        offset: u64,
        len: usize,
    ) -> Result<Bytes> {
        let file = self.open_file_for_read(location, credentials).await?;
        let read = file.read_block(offset, len).await;
        let close = file.close().await;
        match (read, close) {
            (Ok(bytes), Ok(())) => Ok(bytes),
            (Err(err), _) => Err(err),
            (Ok(_), Err(err)) => Err(err),
        }
    }

    /// Open one SMB file handle for sequential [`SmbReadFile::read_block`] / streaming reads.
    pub async fn open_file_for_read(
        &self,
        location: &SmbShareLocation,
        credentials: &SmbCredentials,
    ) -> Result<SmbReadFile> {
        let _op = self.session.op_serial.lock().await;
        self.session.connect_share(location, credentials).await?;
        #[cfg(any(test, feature = "test-hooks"))]
        if self.session.is_dry_run() {
            self.session.record_open_resource();
            return Ok(SmbReadFile::dry_run(self.session.clone()));
        }
        let resource = self
            .open_resource_inner(
                location,
                credentials,
                FileAccessMask::new().with_generic_read(true),
            )
            .await?;
        Ok(SmbReadFile::live(
            self.session.clone(),
            resource_into_file(resource)?,
        ))
    }

    pub async fn read_all(
        &self,
        location: &SmbShareLocation,
        credentials: &SmbCredentials,
    ) -> Result<Bytes> {
        let meta = self.metadata(location, credentials).await?;
        if meta.kind != SmbEntryKind::File {
            return Err(SmbStorageError::ResourceType);
        }
        self.read_at(location, credentials, 0, meta.size as usize)
            .await
    }

    pub async fn write_all(
        &self,
        location: &SmbShareLocation,
        credentials: &SmbCredentials,
        bytes: Bytes,
    ) -> Result<()> {
        self.write_stream_all(
            location,
            credentials,
            Box::pin(futures_util::stream::once(async move { Ok(bytes) })),
        )
        .await
    }

    async fn write_stream_all(
        &self,
        location: &SmbShareLocation,
        credentials: &SmbCredentials,
        mut stream: SmbByteStream,
    ) -> Result<()> {
        let _op = self.session.op_serial.lock().await;
        #[cfg(any(test, feature = "test-hooks"))]
        if self.session.is_dry_run() {
            self.session.connect_share(location, credentials).await?;
            self.session.record_open_resource();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(map_io_error)?;
                for block in chunk.chunks(SMB_WRITE_CHUNK) {
                    self.session.record_write_block(block.len());
                }
            }
            self.session.record_close_resource();
            return Ok(());
        }
        let resource = self
            .create_resource_inner(
                location,
                credentials,
                &FileCreateArgs::make_overwrite(FileAttributes::new(), CreateOptions::new()),
            )
            .await?;
        let file = resource_into_file(resource)?;
        let mut offset = 0u64;
        while let Some(chunk) = stream.next().await {
            let chunk = match chunk {
                Ok(chunk) => chunk,
                Err(e) => {
                    let _ = file.close().await;
                    return Err(map_io_error(e));
                }
            };
            let mut chunk_offset = 0usize;
            while chunk_offset < chunk.len() {
                let end = (chunk_offset + SMB_WRITE_CHUNK).min(chunk.len());
                let block = &chunk[chunk_offset..end];
                let written = match file.write_block(block, offset, None).await {
                    Ok(written) => written,
                    Err(e) => {
                        let _ = file.close().await;
                        return Err(map_io_error(e));
                    }
                };
                if written == 0 {
                    let _ = file.close().await;
                    return Err(SmbStorageError::Io("zero-byte SMB write".into()));
                }
                offset += written as u64;
                chunk_offset += written;
            }
        }
        file.flush().await.map_err(map_io_error)?;
        file.close().await.map_err(map_smb_error)?;
        Ok(())
    }

    pub async fn atomic_write(
        &self,
        location: &SmbShareLocation,
        credentials: &SmbCredentials,
        bytes: Bytes,
    ) -> Result<()> {
        self.atomic_write_stream(
            location,
            credentials,
            Box::pin(futures_util::stream::once(async move { Ok(bytes) })),
        )
        .await
    }

    pub async fn atomic_write_stream(
        &self,
        location: &SmbShareLocation,
        credentials: &SmbCredentials,
        stream: SmbByteStream,
    ) -> Result<()> {
        let tmp = temporary_sibling(&location.path);
        let tmp_location = SmbShareLocation {
            path: tmp.clone(),
            ..location.clone()
        };
        if let Err(err) = self
            .write_stream_all(&tmp_location, credentials, stream)
            .await
        {
            let _ = self.delete(&tmp_location, credentials).await;
            return Err(err);
        }
        match self
            .rename(&tmp_location, location, credentials, true)
            .await
        {
            Ok(()) => Ok(()),
            Err(err) => {
                let _ = self.delete(&tmp_location, credentials).await;
                Err(err)
            }
        }
    }

    pub async fn create_dir_all(
        &self,
        location: &SmbShareLocation,
        credentials: &SmbCredentials,
    ) -> Result<()> {
        let _op = self.session.op_serial.lock().await;
        let mut cursor = String::new();
        for part in normalize_remote_path(&location.path).split('/') {
            if part.is_empty() {
                continue;
            }
            if !cursor.is_empty() {
                cursor.push('/');
            }
            cursor.push_str(part);
            let step = SmbShareLocation {
                path: cursor.clone(),
                ..location.clone()
            };
            let args = FileCreateArgs {
                disposition: CreateDisposition::OpenIf,
                options: CreateOptions::new().with_directory_file(true),
                attributes: FileAttributes::new().with_directory(true),
                desired_access: FileAccessMask::new().with_generic_all(true),
            };
            #[cfg(any(test, feature = "test-hooks"))]
            if self.session.is_dry_run() {
                self.session.connect_share(&step, credentials).await?;
                self.session.record_open_resource();
                let is_dir = dry_run_resource_kind(&step.path) == DryRunResourceKind::Directory;
                self.session.record_close_resource();
                if !is_dir {
                    return Err(SmbStorageError::ResourceType);
                }
                continue;
            }
            let resource = self
                .create_resource_inner(&step, credentials, &args)
                .await?;
            match resource {
                Resource::Directory(dir) => {
                    close_resource_with_session(self.session.as_ref(), Resource::Directory(dir))
                        .await?;
                }
                resource => {
                    close_resource_with_session(self.session.as_ref(), resource).await?;
                    return Err(SmbStorageError::ResourceType);
                }
            }
        }
        Ok(())
    }

    pub async fn delete(
        &self,
        location: &SmbShareLocation,
        credentials: &SmbCredentials,
    ) -> Result<()> {
        let _op = self.session.op_serial.lock().await;
        let resource = self
            .open_resource_inner(
                location,
                credentials,
                FileAccessMask::new()
                    .with_delete(true)
                    .with_generic_read(true),
            )
            .await?;
        match resource {
            Resource::File(file) => {
                file.set_info(FileDispositionInformation::default())
                    .await
                    .map_err(map_smb_error)?;
                file.close().await.map_err(map_smb_error)?;
            }
            Resource::Directory(dir) => {
                dir.set_info(FileDispositionInformation::default())
                    .await
                    .map_err(map_smb_error)?;
                dir.close().await.map_err(map_smb_error)?;
            }
            _ => return Err(SmbStorageError::ResourceType),
        }
        Ok(())
    }

    pub async fn rename(
        &self,
        from: &SmbShareLocation,
        to: &SmbShareLocation,
        credentials: &SmbCredentials,
        replace: bool,
    ) -> Result<()> {
        if from.host != to.host || from.port != to.port || from.share != to.share {
            return Err(SmbStorageError::InvalidLocation(
                "SMB rename must stay within one share".into(),
            ));
        }
        let _op = self.session.op_serial.lock().await;
        #[cfg(any(test, feature = "test-hooks"))]
        if self.session.is_dry_run() {
            self.session.connect_share(from, credentials).await?;
            return Ok(());
        }
        let resource = self
            .open_resource_inner(
                from,
                credentials,
                FileAccessMask::new()
                    .with_delete(true)
                    .with_generic_read(true)
                    .with_generic_write(true),
            )
            .await?;
        let info = FileRenameInformation {
            replace_if_exists: replace.into(),
            root_directory: 0,
            file_name: SizedWideString::from(format!(r"\{}", normalize_remote_path(&to.path))),
        };
        match resource {
            Resource::File(file) => {
                file.set_info(info).await.map_err(map_smb_error)?;
                file.close().await.map_err(map_smb_error)?;
            }
            Resource::Directory(dir) => {
                dir.set_info(info).await.map_err(map_smb_error)?;
                dir.close().await.map_err(map_smb_error)?;
            }
            _ => return Err(SmbStorageError::ResourceType),
        }
        Ok(())
    }

    pub async fn watch_directory(
        &self,
        location: &SmbShareLocation,
        credentials: &SmbCredentials,
        recursive: bool,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<SmbWatchEvent>> + Send + 'static>>> {
        let _op = self.session.op_serial.lock().await;
        #[cfg(any(test, feature = "test-hooks"))]
        if self.session.is_dry_run() {
            if location.path.contains("__watch_type_mismatch__") {
                self.session.record_open_resource();
                self.session.record_close_resource();
                return Err(SmbStorageError::ResourceType);
            }
            if location.path.contains("__watch_setup_error__") {
                self.session.record_open_resource();
                self.session.record_close_resource();
                return Err(SmbStorageError::Client("watch setup failed".into()));
            }
        }
        let resource = self
            .open_resource_inner(
                location,
                credentials,
                FileAccessMask::new().with_generic_read(true),
            )
            .await?;
        let Resource::Directory(dir) = resource else {
            close_resource_with_session(self.session.as_ref(), resource).await?;
            return Err(SmbStorageError::ResourceType);
        };
        let dir = Arc::new(dir);
        let mut close_guard = SmbDirectoryCloseGuard::new(self.session.clone(), dir.clone());
        let base_path = normalize_remote_path(&location.path);
        let watch_result = smb::Directory::watch_stream(&dir, smb::NotifyFilter::all(), recursive);
        let stream = match watch_result {
            Ok(stream) => stream.map(move |item| {
                item.map(|notify| map_watch_event(&base_path, notify))
                    .map_err(map_smb_error)
            }),
            Err(e) => {
                return Err(map_smb_error(e));
            }
        };
        close_guard.disarm();
        Ok(detach_watch_stream_lifetime(Box::pin(stream)))
    }

    async fn open_resource_inner(
        &self,
        location: &SmbShareLocation,
        credentials: &SmbCredentials,
        access: FileAccessMask,
    ) -> Result<Resource> {
        self.create_resource_inner(
            location,
            credentials,
            &FileCreateArgs::make_open_existing(access),
        )
        .await
    }

    async fn create_resource_inner(
        &self,
        location: &SmbShareLocation,
        credentials: &SmbCredentials,
        args: &FileCreateArgs,
    ) -> Result<Resource> {
        let unc = self.session.connect_share(location, credentials).await?;
        #[cfg(any(test, feature = "test-hooks"))]
        {
            self.session.record_open_resource();
            if self.session.is_dry_run() {
                return Err(SmbStorageError::ResourceType);
            }
        }
        let resource_path = unc.with_path(&normalize_remote_path(&location.path));
        self.session
            .client()
            .create_file(&resource_path, args)
            .await
            .map_err(map_smb_error)
    }
}

fn resource_into_file(resource: Resource) -> Result<smb::File> {
    match resource {
        Resource::File(file) => Ok(file),
        _ => Err(SmbStorageError::ResourceType),
    }
}

struct SmbDirectoryCloseGuard {
    session: Arc<SmbSession>,
    dir: Option<Arc<smb::Directory>>,
}

impl SmbDirectoryCloseGuard {
    fn new(session: Arc<SmbSession>, dir: Arc<smb::Directory>) -> Self {
        Self {
            session,
            dir: Some(dir),
        }
    }

    fn disarm(&mut self) {
        self.dir = None;
    }

    async fn close(&mut self) -> Result<()> {
        let Some(dir) = self.dir.take() else {
            return Ok(());
        };
        if let Ok(dir) = Arc::try_unwrap(dir) {
            close_resource_with_session(self.session.as_ref(), Resource::Directory(dir)).await?;
        }
        Ok(())
    }
}

impl Drop for SmbDirectoryCloseGuard {
    fn drop(&mut self) {
        let Some(dir) = self.dir.take() else {
            return;
        };
        let session = self.session.clone();
        if let Ok(dir) = Arc::try_unwrap(dir) {
            tokio::spawn(async move {
                let _ =
                    close_resource_with_session(session.as_ref(), Resource::Directory(dir)).await;
            });
        }
    }
}

async fn close_resource_with_session(_session: &SmbSession, resource: Resource) -> Result<()> {
    let result = match resource {
        Resource::File(file) => file.close().await.map_err(map_smb_error),
        Resource::Directory(dir) => dir.close().await.map_err(map_smb_error),
        Resource::Pipe(pipe) => pipe.close().await.map_err(map_smb_error),
    };
    if result.is_ok() {
        #[cfg(any(test, feature = "test-hooks"))]
        _session.record_close_resource();
    }
    result
}

#[cfg(any(test, feature = "test-hooks"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DryRunResourceKind {
    File,
    Directory,
}

#[cfg(any(test, feature = "test-hooks"))]
fn dry_run_resource_kind(path: &str) -> DryRunResourceKind {
    let path = normalize_remote_path(path);
    let leaf = path.rsplit('/').next().unwrap_or(path.as_str());
    if leaf.contains('.') {
        DryRunResourceKind::File
    } else {
        DryRunResourceKind::Directory
    }
}

impl Default for SmbStorageClient {
    fn default() -> Self {
        Self::new()
    }
}

/// One SMB file handle reused for sequential `read_block` calls (streaming).
pub struct SmbReadFile {
    session: Arc<SmbSession>,
    inner: Option<SmbReadFileInner>,
}

enum SmbReadFileInner {
    Live(smb::File),
    #[cfg(any(test, feature = "test-hooks"))]
    DryRun,
}

impl SmbReadFile {
    fn live(session: Arc<SmbSession>, file: smb::File) -> Self {
        Self {
            session,
            inner: Some(SmbReadFileInner::Live(file)),
        }
    }

    #[cfg(any(test, feature = "test-hooks"))]
    fn dry_run(session: Arc<SmbSession>) -> Self {
        Self {
            session,
            inner: Some(SmbReadFileInner::DryRun),
        }
    }

    pub async fn read_block(&self, offset: u64, len: usize) -> Result<Bytes> {
        let _op = self.session.op_serial.lock().await;
        match self.inner.as_ref() {
            Some(SmbReadFileInner::Live(file)) => {
                let mut buf = vec![0; len];
                let n = file
                    .read_block(&mut buf, offset, None, false)
                    .await
                    .map_err(map_io_error)?;
                buf.truncate(n);
                Ok(Bytes::from(buf))
            }
            #[cfg(any(test, feature = "test-hooks"))]
            Some(SmbReadFileInner::DryRun) => Ok(Bytes::from(vec![0u8; len])),
            None => Err(SmbStorageError::Io("SMB file already closed".into())),
        }
    }

    pub async fn close(mut self) -> Result<()> {
        let Some(inner) = self.inner.take() else {
            return Ok(());
        };
        match inner {
            SmbReadFileInner::Live(file) => {
                file.close().await.map_err(map_smb_error)?;
                #[cfg(any(test, feature = "test-hooks"))]
                self.session.record_close_resource();
                Ok(())
            }
            #[cfg(any(test, feature = "test-hooks"))]
            SmbReadFileInner::DryRun => {
                self.session.record_close_resource();
                Ok(())
            }
        }
    }

    /// Stream file bytes in 64 KiB chunks from `offset`, optionally capped by `len`.
    pub fn byte_stream(
        self,
        offset: u64,
        len: Option<u64>,
    ) -> Pin<Box<dyn Stream<Item = Result<Bytes>> + Send + 'static>> {
        const CHUNK: usize = 64 * 1024;
        Box::pin(futures_util::stream::unfold(
            (Some(self), offset, len),
            |(file, mut cursor, mut remaining)| async move {
                let file = file?;
                if remaining == Some(0) {
                    return match file.close().await {
                        Ok(()) => None,
                        Err(e) => Some((Err(e), (None, cursor, remaining))),
                    };
                }
                let want = remaining
                    .map(|v| v.min(CHUNK as u64) as usize)
                    .unwrap_or(CHUNK);
                match file.read_block(cursor, want).await {
                    Ok(bytes) if bytes.is_empty() => match file.close().await {
                        Ok(()) => None,
                        Err(e) => Some((Err(e), (None, cursor, remaining))),
                    },
                    Ok(bytes) => {
                        cursor += bytes.len() as u64;
                        remaining = remaining.map(|v| v.saturating_sub(bytes.len() as u64));
                        Some((Ok(bytes), (Some(file), cursor, remaining)))
                    }
                    Err(e) => {
                        let _ = file.close().await;
                        Some((Err(e), (None, cursor, remaining)))
                    }
                }
            },
        ))
    }
}

impl Drop for SmbReadFile {
    fn drop(&mut self) {
        let Some(inner) = self.inner.take() else {
            return;
        };
        let session = self.session.clone();
        match inner {
            SmbReadFileInner::Live(file) => {
                tokio::spawn(async move {
                    if file.close().await.is_ok() {
                        #[cfg(any(test, feature = "test-hooks"))]
                        session.record_close_resource();
                    }
                });
            }
            #[cfg(any(test, feature = "test-hooks"))]
            SmbReadFileInner::DryRun => {
                session.record_close_resource();
            }
        }
    }
}

pub fn parse_share_location(input: &str) -> Result<SmbShareLocation> {
    let trimmed = input.trim();
    if trimmed.starts_with(r"\\") {
        return parse_unc(trimmed);
    }
    if trimmed.to_ascii_lowercase().starts_with("smb://") {
        return parse_smb_url(trimmed);
    }
    Err(SmbStorageError::InvalidLocation(
        "expected UNC path or smb:// URL".into(),
    ))
}

pub fn normalize_remote_path(path: &str) -> String {
    path.replace('\\', "/")
        .split('/')
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>()
        .join("/")
}

fn parse_unc(input: &str) -> Result<SmbShareLocation> {
    let parts = input
        .trim_start_matches('\\')
        .split('\\')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.len() < 2 {
        return Err(SmbStorageError::InvalidLocation(
            "UNC path must include host and share".into(),
        ));
    }
    let (host, port) = parse_host_port(parts[0], 445)?;
    Ok(SmbShareLocation {
        host,
        port,
        share: parts[1].to_string(),
        path: normalize_remote_path(&parts[2..].join("/")),
    })
}

fn parse_smb_url(input: &str) -> Result<SmbShareLocation> {
    let url = url::Url::parse(input)
        .map_err(|e| SmbStorageError::InvalidLocation(format!("invalid smb URL: {e}")))?;
    let host = url
        .host_str()
        .ok_or_else(|| SmbStorageError::InvalidLocation("SMB URL must include host".into()))?
        .to_string();
    let port = url.port().unwrap_or(445);
    let parts = url
        .path_segments()
        .map(|segments| segments.filter(|s| !s.is_empty()).collect::<Vec<_>>())
        .unwrap_or_default();
    let Some(share) = parts.first() else {
        return Err(SmbStorageError::InvalidLocation(
            "SMB URL must include share".into(),
        ));
    };
    Ok(SmbShareLocation {
        host,
        port,
        share: (*share).to_string(),
        path: normalize_remote_path(&parts[1..].join("/")),
    })
}

fn parse_host_port(value: &str, default_port: u16) -> Result<(String, u16)> {
    if let Some((host, port)) = value.rsplit_once(':') {
        let port = port
            .parse::<u16>()
            .map_err(|e| SmbStorageError::InvalidLocation(format!("invalid SMB port: {e}")))?;
        return Ok((host.to_string(), port));
    }
    Ok((value.to_string(), default_port))
}

/// Format SMB `FileTime` like local `LibraryStorage` mtime (`YYYY-MM-DD HH:MM:SS` UTC).
pub fn format_file_mtime(ft: &smb::binrw_util::prelude::FileTime) -> Option<String> {
    if ft.is_zero() {
        return None;
    }
    let dt = ft.date_time();
    Some(format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        dt.year(),
        u8::from(dt.month()),
        dt.day(),
        dt.hour(),
        dt.minute(),
        dt.second()
    ))
}

/// Build username for NTLM: `DOMAIN\user` when workgroup set and username has no domain.
pub fn format_smb_username(workgroup: Option<&str>, username: &str) -> String {
    let username = username.trim();
    if username.is_empty() {
        return String::new();
    }
    if username.contains('\\') || username.contains('@') {
        return username.to_string();
    }
    let Some(workgroup) = workgroup.map(str::trim).filter(|w| !w.is_empty()) else {
        return username.to_string();
    };
    format!("{workgroup}\\{username}")
}

async fn file_metadata(file: &smb::File) -> Result<SmbMetadata> {
    let standard = file
        .query_info::<FileStandardInformation>()
        .await
        .map_err(map_smb_error)?;
    let basic = file
        .query_info::<FileBasicInformation>()
        .await
        .map_err(map_smb_error)?;
    Ok(SmbMetadata {
        kind: SmbEntryKind::File,
        size: standard.end_of_file,
        mtime: format_file_mtime(&basic.last_write_time),
    })
}

async fn dir_metadata(dir: &smb::Directory) -> Result<SmbMetadata> {
    let standard = dir
        .query_info::<FileStandardInformation>()
        .await
        .map_err(map_smb_error)?;
    let basic = dir
        .query_info::<FileBasicInformation>()
        .await
        .map_err(map_smb_error)?;
    Ok(SmbMetadata {
        kind: SmbEntryKind::Directory,
        size: standard.end_of_file,
        mtime: format_file_mtime(&basic.last_write_time),
    })
}

fn unc_for_share(location: &SmbShareLocation) -> Result<UncPath> {
    let server = format_server(&location.host, location.port);
    smb::UncPath::new(&server)
        .map_err(map_smb_error)?
        .with_share(&location.share)
        .map_err(map_smb_error)
}

fn format_server(host: &str, port: u16) -> String {
    if port == 445 {
        host.to_string()
    } else {
        format!("{host}:{port}")
    }
}

/// Extract NTSTATUS hex from smb crate error text (`0xc000006d`, `0xC000006D`, etc.).
pub fn extract_ntstatus(message: &str) -> Option<u32> {
    let lower = message.to_ascii_lowercase();
    let mut search_from = 0;
    while let Some(rel) = lower[search_from..].find("0x") {
        let start = search_from + rel + 2;
        let hex: String = lower[start..]
            .chars()
            .take_while(|c| c.is_ascii_hexdigit())
            .collect();
        if hex.len() >= 4
            && let Ok(value) = u32::from_str_radix(&hex, 16)
        {
            return Some(value);
        }
        search_from = start.saturating_add(1);
    }
    None
}

/// Map NTSTATUS to API error code and a short user-facing hint.
pub fn classify_smb_client_error(message: &str) -> (&'static str, String) {
    if let Some(status) = extract_ntstatus(message) {
        return ntstatus_user_error(status, message);
    }
    let lower = message.to_ascii_lowercase();
    if lower.contains("logon failure") {
        return (
            "SMB_AUTH_FAILED",
            "Logon failed — check username, password, and workgroup".into(),
        );
    }
    if lower.contains("access denied") {
        return (
            "SMB_ACCESS_DENIED",
            "Access denied — check share permissions for this user".into(),
        );
    }
    if lower.contains("timed out")
        || lower.contains("timeout")
        || lower.contains("receivenextmessage")
    {
        return (
            "SMB_TIMEOUT",
            "SMB server did not respond in time — check host, port 445, firewall, and VPN".into(),
        );
    }
    if lower.contains("bindack")
        && (lower.contains("proposedtransfersyntaxesnotsupported")
            || lower.contains("providerrejection")
            || lower.contains("71710533-beba-4937-8319-b5dbef9ccc36"))
    {
        return (
            "SMB_SHARE_ENUM_UNSUPPORTED",
            "This server does not support automatic share listing (common on Samba). \
             Enter the share name manually, then use Test connection."
                .into(),
        );
    }
    if lower.contains("dce/rpc fault") || lower.contains("0x6f7") || lower.contains("0x000006f7") {
        return (
            "SMB_SHARE_ENUM_UNSUPPORTED",
            "Share listing RPC was rejected by the server — enter the share name manually, \
             then use Test connection."
                .into(),
        );
    }
    ("SMB_CONNECTION_FAILED", message.trim().to_string())
}

fn ntstatus_user_error(status: u32, raw: &str) -> (&'static str, String) {
    let (code, name, hint) = match status {
        0x0000_06F7 => (
            "SMB_SHARE_ENUM_UNSUPPORTED",
            "RPC_BAD_STUB_DATA",
            "Share listing RPC rejected the request (Samba NDR) — enter the share name manually, \
             then use Test connection",
        ),
        0xC000_006D => (
            "SMB_AUTH_FAILED",
            "STATUS_LOGON_FAILURE",
            "Logon failed — check username, password, and workgroup",
        ),
        0xC000_006E..=0xC000_0070 => (
            "SMB_AUTH_FAILED",
            "STATUS_ACCOUNT_RESTRICTION",
            "Account cannot log on from this host or at this time",
        ),
        0xC000_0071 | 0xC000_0072 => (
            "SMB_AUTH_FAILED",
            "STATUS_PASSWORD_EXPIRED",
            "Password expired or must be changed on the server",
        ),
        0xC000_0022 => (
            "SMB_ACCESS_DENIED",
            "STATUS_ACCESS_DENIED",
            "Access denied — check share permissions for this user",
        ),
        0xC000_0034 | 0xC000_003A | 0xC000_003B => (
            "SMB_NOT_FOUND",
            "STATUS_OBJECT_NOT_FOUND",
            "Share or path not found on the server",
        ),
        0xC000_009A => (
            "SMB_UNAVAILABLE",
            "STATUS_INSUFFICIENT_RESOURCES",
            "SMB server is overloaded or out of resources — retry or check the NAS",
        ),
        _ => ("SMB_CONNECTION_FAILED", "SMB_STATUS", raw.trim()),
    };
    let detail = if hint == raw.trim() {
        format!("{name} ({status:#010x})")
    } else {
        format!("{hint} [{name} {status:#010x}]")
    };
    (code, detail)
}

/// Classify any [`SmbStorageError`] for HTTP API responses.
pub fn user_facing_error(err: &SmbStorageError) -> (&'static str, String) {
    match err {
        SmbStorageError::InvalidLocation(msg) => ("BAD_REQUEST", msg.clone()),
        SmbStorageError::ResourceType => {
            ("SMB_CONNECTION_FAILED", "SMB resource type mismatch".into())
        }
        SmbStorageError::Client(msg) => classify_smb_client_error(msg),
        SmbStorageError::Io(msg) => ("SMB_CONNECTION_FAILED", msg.clone()),
    }
}

fn map_smb_error(error: smb::Error) -> SmbStorageError {
    let raw = error.to_string();
    let (_, detail) = classify_smb_client_error(&raw);
    SmbStorageError::Client(detail)
}

fn map_io_error(error: std::io::Error) -> SmbStorageError {
    SmbStorageError::Io(error.to_string())
}

fn detach_watch_stream_lifetime<'a>(
    stream: Pin<Box<dyn Stream<Item = Result<SmbWatchEvent>> + Send + 'a>>,
) -> Pin<Box<dyn Stream<Item = Result<SmbWatchEvent>> + Send + 'static>> {
    // smb 0.11.2's watch_stream returns a receiver stream and moves cloned directory handles into
    // its spawned tasks, but its Rust 2024 impl Trait signature overcaptures the input Arc borrow.
    // This narrows that accidental lifetime so callers can own the returned stream.
    unsafe { std::mem::transmute(stream) }
}

fn map_watch_event(base_path: &str, notify: smb::FileNotifyInformation) -> SmbWatchEvent {
    SmbWatchEvent {
        path: join_watch_path(base_path, &notify.file_name.to_string()),
        action: map_watch_action(notify.action),
    }
}

fn map_watch_action(action: smb::NotifyAction) -> SmbWatchAction {
    match action {
        smb::NotifyAction::Added => SmbWatchAction::Created,
        smb::NotifyAction::Removed => SmbWatchAction::Removed,
        smb::NotifyAction::Modified => SmbWatchAction::Modified,
        smb::NotifyAction::RenamedOldName => SmbWatchAction::RenamedOld,
        smb::NotifyAction::RenamedNewName => SmbWatchAction::RenamedNew,
        smb::NotifyAction::AddedStream
        | smb::NotifyAction::RemovedStream
        | smb::NotifyAction::ModifiedStream
        | smb::NotifyAction::RemovedByDelete
        | smb::NotifyAction::IdNotTunnelled
        | smb::NotifyAction::TunnelledIdCollision => SmbWatchAction::Modified,
    }
}

fn join_watch_path(base_path: &str, file_name: &str) -> String {
    let file_name = normalize_remote_path(file_name);
    if base_path.is_empty() {
        file_name
    } else if file_name.is_empty() {
        base_path.to_string()
    } else {
        format!("{base_path}/{file_name}")
    }
}

fn temporary_sibling(path: &str) -> String {
    let path = normalize_remote_path(path);
    match path.rsplit_once('/') {
        Some((parent, name)) => format!("{parent}/.{name}.euterpe-part"),
        None => format!(".{path}.euterpe-part"),
    }
}

trait DeleteTreeBackend {
    async fn tree_metadata(
        &self,
        location: &SmbShareLocation,
        credentials: &SmbCredentials,
    ) -> Result<SmbMetadata>;
    async fn tree_list_directory(
        &self,
        location: &SmbShareLocation,
        credentials: &SmbCredentials,
    ) -> Result<Vec<SmbDirectoryEntry>>;
    async fn tree_delete(
        &self,
        location: &SmbShareLocation,
        credentials: &SmbCredentials,
    ) -> Result<()>;
}

async fn delete_tree_recursive<B: DeleteTreeBackend>(
    backend: &B,
    location: &SmbShareLocation,
    credentials: &SmbCredentials,
) -> Result<()> {
    let meta = backend.tree_metadata(location, credentials).await?;
    match meta.kind {
        SmbEntryKind::File => backend.tree_delete(location, credentials).await,
        SmbEntryKind::Directory => {
            let entries = backend.tree_list_directory(location, credentials).await?;
            for entry in entries {
                let child = SmbShareLocation {
                    path: entry.path,
                    ..location.clone()
                };
                Box::pin(delete_tree_recursive(backend, &child, credentials)).await?;
            }
            backend.tree_delete(location, credentials).await
        }
    }
}

impl DeleteTreeBackend for SmbStorageClient {
    async fn tree_metadata(
        &self,
        location: &SmbShareLocation,
        credentials: &SmbCredentials,
    ) -> Result<SmbMetadata> {
        self.metadata(location, credentials).await
    }

    async fn tree_list_directory(
        &self,
        location: &SmbShareLocation,
        credentials: &SmbCredentials,
    ) -> Result<Vec<SmbDirectoryEntry>> {
        self.list_directory(location, credentials).await
    }

    async fn tree_delete(
        &self,
        location: &SmbShareLocation,
        credentials: &SmbCredentials,
    ) -> Result<()> {
        self.delete(location, credentials).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_unc_share_with_path() {
        let loc = parse_share_location(r"\\192.168.0.124\dietpi\Musik").unwrap();
        assert_eq!(
            loc,
            SmbShareLocation {
                host: "192.168.0.124".into(),
                port: 445,
                share: "dietpi".into(),
                path: "Musik".into(),
            }
        );
    }

    #[test]
    fn smb_credentials_debug_redacts_password() {
        let rendered = format!(
            "{:?}",
            SmbCredentials {
                username: "user".into(),
                password: "super-secret".into(),
            }
        );
        assert!(rendered.contains("user"));
        assert!(rendered.contains("<redacted>"));
        assert!(!rendered.contains("super-secret"));
    }

    #[test]
    fn parses_smb_url_with_port_and_path() {
        let loc = parse_share_location("smb://nas.local:1445/music/Jazz/Classics").unwrap();
        assert_eq!(loc.host, "nas.local");
        assert_eq!(loc.port, 1445);
        assert_eq!(loc.share, "music");
        assert_eq!(loc.path, "Jazz/Classics");
    }

    #[test]
    fn rejects_missing_share() {
        assert!(parse_share_location("smb://nas.local").is_err());
    }

    #[test]
    fn normalizes_backslash_path() {
        assert_eq!(normalize_remote_path(r"\Jazz\A"), "Jazz/A");
        assert_eq!(normalize_remote_path("Jazz//A/"), "Jazz/A");
    }

    #[test]
    fn temporary_sibling_stays_in_parent() {
        assert_eq!(
            temporary_sibling("Artist/Album/01.flac"),
            "Artist/Album/.01.flac.euterpe-part"
        );
        assert_eq!(temporary_sibling("01.flac"), ".01.flac.euterpe-part");
    }

    #[test]
    fn maps_notify_actions_to_stable_watch_actions() {
        let cases = [
            (smb::NotifyAction::Added, SmbWatchAction::Created),
            (smb::NotifyAction::Removed, SmbWatchAction::Removed),
            (smb::NotifyAction::Modified, SmbWatchAction::Modified),
            (
                smb::NotifyAction::RenamedOldName,
                SmbWatchAction::RenamedOld,
            ),
            (
                smb::NotifyAction::RenamedNewName,
                SmbWatchAction::RenamedNew,
            ),
            (smb::NotifyAction::AddedStream, SmbWatchAction::Modified),
            (smb::NotifyAction::RemovedStream, SmbWatchAction::Modified),
            (smb::NotifyAction::ModifiedStream, SmbWatchAction::Modified),
            (smb::NotifyAction::RemovedByDelete, SmbWatchAction::Modified),
            (smb::NotifyAction::IdNotTunnelled, SmbWatchAction::Modified),
            (
                smb::NotifyAction::TunnelledIdCollision,
                SmbWatchAction::Modified,
            ),
        ];

        for (input, expected) in cases {
            assert_eq!(map_watch_action(input), expected);
        }
    }

    #[test]
    fn format_file_mtime_zero_is_none() {
        use smb::binrw_util::prelude::FileTime;
        assert_eq!(format_file_mtime(&FileTime::ZERO), None);
    }

    #[test]
    fn format_file_mtime_formats_utc_wall_clock() {
        use smb::binrw_util::prelude::FileTime;
        use time::macros::datetime;
        let ft = FileTime::from(datetime!(2025-01-20 15:36:20));
        assert_eq!(
            format_file_mtime(&ft).as_deref(),
            Some("2025-01-20 15:36:20")
        );
    }

    #[test]
    fn format_smb_username_adds_workgroup_prefix() {
        assert_eq!(
            format_smb_username(Some("WORKGROUP"), "roon"),
            "WORKGROUP\\roon"
        );
        assert_eq!(
            format_smb_username(Some("WORKGROUP"), r"NAS\roon"),
            r"NAS\roon"
        );
        assert_eq!(
            format_smb_username(Some("WORKGROUP"), "roon@nas.local"),
            "roon@nas.local"
        );
        assert_eq!(format_smb_username(None, "roon"), "roon");
    }

    #[test]
    fn joins_watch_paths_with_normalization() {
        assert_eq!(
            join_watch_path("Music", r"Artist\Album"),
            "Music/Artist/Album"
        );
        assert_eq!(join_watch_path("", r"Artist\Album"), "Artist/Album");
        assert_eq!(join_watch_path("Music", ""), "Music");
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum MockNodeKind {
        File,
        Dir,
    }

    struct MockDeleteTree {
        nodes: std::collections::BTreeMap<String, MockNodeKind>,
        deleted: std::sync::Mutex<Vec<String>>,
    }

    impl MockDeleteTree {
        fn new(nodes: &[(&str, MockNodeKind)]) -> Self {
            Self {
                nodes: nodes
                    .iter()
                    .map(|(path, kind)| (normalize_remote_path(path), *kind))
                    .collect(),
                deleted: std::sync::Mutex::new(Vec::new()),
            }
        }

        fn deleted_paths(&self) -> Vec<String> {
            self.deleted.lock().unwrap().clone()
        }

        fn child_entries(&self, dir_path: &str) -> Vec<SmbDirectoryEntry> {
            let mut children = Vec::new();
            for (path, kind) in &self.nodes {
                if path == dir_path {
                    continue;
                }
                let (parent, name) = match path.rsplit_once('/') {
                    Some((parent, name)) => (parent, name),
                    None => ("", path.as_str()),
                };
                if parent != dir_path {
                    continue;
                }
                children.push(SmbDirectoryEntry {
                    name: name.to_string(),
                    path: path.clone(),
                    is_dir: matches!(kind, MockNodeKind::Dir),
                    size: if matches!(kind, MockNodeKind::Dir) {
                        None
                    } else {
                        Some(0)
                    },
                    mtime: None,
                });
            }
            children.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.cmp(&b.name)));
            children
        }
    }

    impl DeleteTreeBackend for MockDeleteTree {
        async fn tree_metadata(
            &self,
            location: &SmbShareLocation,
            _credentials: &SmbCredentials,
        ) -> Result<SmbMetadata> {
            let path = normalize_remote_path(&location.path);
            let kind = self
                .nodes
                .get(&path)
                .copied()
                .ok_or_else(|| SmbStorageError::Client(format!("missing mock node: {path}")))?;
            Ok(SmbMetadata {
                kind: match kind {
                    MockNodeKind::File => SmbEntryKind::File,
                    MockNodeKind::Dir => SmbEntryKind::Directory,
                },
                size: 0,
                mtime: None,
            })
        }

        async fn tree_list_directory(
            &self,
            location: &SmbShareLocation,
            _credentials: &SmbCredentials,
        ) -> Result<Vec<SmbDirectoryEntry>> {
            Ok(self.child_entries(&normalize_remote_path(&location.path)))
        }

        async fn tree_delete(
            &self,
            location: &SmbShareLocation,
            _credentials: &SmbCredentials,
        ) -> Result<()> {
            self.deleted
                .lock()
                .unwrap()
                .push(normalize_remote_path(&location.path));
            Ok(())
        }
    }

    fn mock_location(path: &str) -> SmbShareLocation {
        SmbShareLocation {
            host: "mock".into(),
            port: 445,
            share: "music".into(),
            path: path.into(),
        }
    }

    fn mock_credentials() -> SmbCredentials {
        SmbCredentials {
            username: "user".into(),
            password: "pass".into(),
        }
    }

    #[tokio::test]
    async fn stream_chunks_open_file_and_share_connect_once() {
        use futures_util::StreamExt;
        let (client, connect_counter) = SmbStorageClient::new_for_connect_tests();
        let location = mock_location("track.flac");
        let credentials = mock_credentials();
        let file = client
            .open_file_for_read(&location, &credentials)
            .await
            .unwrap();
        let mut stream = file.byte_stream(0, Some(256 * 1024));
        let mut chunks = 0usize;
        while let Some(chunk) = stream.next().await {
            chunk.unwrap();
            chunks += 1;
        }
        assert_eq!(chunks, 4);
        assert_eq!(connect_counter.load(Ordering::SeqCst), 1);
        assert_eq!(client.share_connect_count(), 1);
        assert_eq!(client.open_resource_count(), 1);
        assert_eq!(client.close_resource_count(), 1);
    }

    #[tokio::test]
    async fn dropping_byte_stream_before_eof_closes_opened_file_resource() {
        use futures_util::StreamExt;
        let (client, _) = SmbStorageClient::new_for_connect_tests();
        let location = mock_location("track.flac");
        let credentials = mock_credentials();
        let file = client
            .open_file_for_read(&location, &credentials)
            .await
            .unwrap();
        let mut stream = file.byte_stream(0, Some(256 * 1024));

        let first = stream.next().await.unwrap().unwrap();
        assert_eq!(first.len(), 64 * 1024);
        drop(stream);
        tokio::task::yield_now().await;

        assert_eq!(client.open_resource_count(), 1);
        assert_eq!(client.close_resource_count(), 1);
    }

    #[tokio::test]
    async fn read_at_closes_opened_file_resource() {
        let (client, _) = SmbStorageClient::new_for_connect_tests();
        let location = mock_location("album/track.flac");
        let credentials = mock_credentials();

        let bytes = client
            .read_at(&location, &credentials, 64, 128)
            .await
            .unwrap();

        assert_eq!(bytes.len(), 128);
        assert_eq!(client.open_resource_count(), 1);
        assert_eq!(client.close_resource_count(), 1);
    }

    #[tokio::test]
    async fn list_directory_query_setup_error_closes_opened_directory() {
        let (client, _) = SmbStorageClient::new_for_connect_tests();
        let location = mock_location("__query_setup_error__");
        let credentials = mock_credentials();

        let err = client
            .list_directory(&location, &credentials)
            .await
            .unwrap_err();

        assert!(matches!(err, SmbStorageError::Client(_)));
        assert_eq!(client.open_resource_count(), 1);
        assert_eq!(client.close_resource_count(), 1);
    }

    #[tokio::test]
    async fn list_directory_stream_item_error_closes_opened_directory() {
        let (client, _) = SmbStorageClient::new_for_connect_tests();
        let location = mock_location("__query_item_error__");
        let credentials = mock_credentials();

        let err = client
            .list_directory(&location, &credentials)
            .await
            .unwrap_err();

        assert!(matches!(err, SmbStorageError::Client(_)));
        assert_eq!(client.open_resource_count(), 1);
        assert_eq!(client.close_resource_count(), 1);
    }

    #[tokio::test]
    async fn watch_directory_type_mismatch_closes_opened_resource() {
        let (client, _) = SmbStorageClient::new_for_connect_tests();
        let location = mock_location("__watch_type_mismatch__");
        let credentials = mock_credentials();

        let err = match client.watch_directory(&location, &credentials, true).await {
            Ok(_) => panic!("watch_directory unexpectedly succeeded"),
            Err(err) => err,
        };

        assert!(matches!(err, SmbStorageError::ResourceType));
        assert_eq!(client.open_resource_count(), 1);
        assert_eq!(client.close_resource_count(), 1);
    }

    #[tokio::test]
    async fn watch_directory_setup_error_closes_opened_directory() {
        let (client, _) = SmbStorageClient::new_for_connect_tests();
        let location = mock_location("__watch_setup_error__");
        let credentials = mock_credentials();

        let err = match client.watch_directory(&location, &credentials, true).await {
            Ok(_) => panic!("watch_directory unexpectedly succeeded"),
            Err(err) => err,
        };

        assert!(matches!(err, SmbStorageError::Client(_)));
        assert_eq!(client.open_resource_count(), 1);
        assert_eq!(client.close_resource_count(), 1);
    }

    #[tokio::test]
    async fn streaming_atomic_write_dry_run_records_multiple_bounded_chunks() {
        let (client, connect_counter) = SmbStorageClient::new_for_connect_tests();
        let location = mock_location("album/large.flac");
        let credentials = mock_credentials();
        let bytes = Bytes::from(vec![7u8; 130 * 1024]);
        let stream = Box::pin(futures_util::stream::iter(vec![Ok(bytes)]));

        client
            .atomic_write_stream(&location, &credentials, stream)
            .await
            .unwrap();

        assert_eq!(connect_counter.load(Ordering::SeqCst), 1);
        assert_eq!(client.write_block_count(), 3);
        assert_eq!(
            client.write_block_sizes(),
            vec![64 * 1024, 64 * 1024, 2 * 1024]
        );
    }

    #[tokio::test]
    async fn burst_list_and_reads_share_connect_once() {
        let (client, counter) = SmbStorageClient::new_for_connect_tests();
        let location = mock_location("album");
        let credentials = mock_credentials();
        client
            .list_directory(&location, &credentials)
            .await
            .unwrap();
        client
            .read_at(&location, &credentials, 0, 64)
            .await
            .unwrap();
        client
            .read_at(&location, &credentials, 64, 64)
            .await
            .unwrap();
        assert_eq!(counter.load(Ordering::SeqCst), 1);
        assert_eq!(client.share_connect_count(), 1);
        assert_eq!(client.open_resource_count(), 2);
        assert_eq!(client.close_resource_count(), 2);
    }

    #[tokio::test]
    async fn share_connect_runs_again_when_username_changes() {
        let (client, counter) = SmbStorageClient::new_for_connect_tests();
        let location = mock_location("album");
        let mut other = mock_credentials();
        client
            .list_directory(&location, &mock_credentials())
            .await
            .unwrap();
        other.username = "other".into();
        client.list_directory(&location, &other).await.unwrap();
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn share_connect_runs_again_when_password_changes() {
        let (client, counter) = SmbStorageClient::new_for_connect_tests();
        let location = mock_location("album");
        let mut other = mock_credentials();
        client
            .list_directory(&location, &mock_credentials())
            .await
            .unwrap();
        other.password = "rotated".into();
        client.list_directory(&location, &other).await.unwrap();
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn metadata_file_closes_opened_resource() {
        let (client, _) = SmbStorageClient::new_for_connect_tests();
        let location = mock_location("album/track.flac");

        let meta = client
            .metadata(&location, &mock_credentials())
            .await
            .unwrap();

        assert_eq!(meta.kind, SmbEntryKind::File);
        assert_eq!(client.open_resource_count(), 1);
        assert_eq!(client.close_resource_count(), 1);
    }

    #[tokio::test]
    async fn metadata_directory_closes_opened_resource() {
        let (client, _) = SmbStorageClient::new_for_connect_tests();
        let location = mock_location("album");

        let meta = client
            .metadata(&location, &mock_credentials())
            .await
            .unwrap();

        assert_eq!(meta.kind, SmbEntryKind::Directory);
        assert_eq!(client.open_resource_count(), 1);
        assert_eq!(client.close_resource_count(), 1);
    }

    #[tokio::test]
    async fn create_dir_all_closes_each_opened_directory_resource() {
        let (client, _) = SmbStorageClient::new_for_connect_tests();
        let location = mock_location("artist/album/disc");

        client
            .create_dir_all(&location, &mock_credentials())
            .await
            .unwrap();

        assert_eq!(client.open_resource_count(), 3);
        assert_eq!(client.close_resource_count(), 3);
    }

    #[tokio::test]
    async fn create_dir_all_closes_mismatched_resource_before_error() {
        let (client, _) = SmbStorageClient::new_for_connect_tests();
        let location = mock_location("artist/album.flac");

        let err = client
            .create_dir_all(&location, &mock_credentials())
            .await
            .unwrap_err();

        assert!(matches!(err, SmbStorageError::ResourceType));
        assert_eq!(client.open_resource_count(), 2);
        assert_eq!(client.close_resource_count(), 2);
    }

    #[tokio::test]
    async fn delete_tree_deletes_single_file() {
        let mock = MockDeleteTree::new(&[("track.flac", MockNodeKind::File)]);
        let location = mock_location("track.flac");
        delete_tree_recursive(&mock, &location, &mock_credentials())
            .await
            .unwrap();
        assert_eq!(mock.deleted_paths(), vec!["track.flac"]);
    }

    #[tokio::test]
    async fn delete_tree_deletes_empty_directory() {
        let mock = MockDeleteTree::new(&[("empty", MockNodeKind::Dir)]);
        let location = mock_location("empty");
        delete_tree_recursive(&mock, &location, &mock_credentials())
            .await
            .unwrap();
        assert_eq!(mock.deleted_paths(), vec!["empty"]);
    }

    #[tokio::test]
    async fn delete_tree_post_order_deletes_files_before_parent_dir() {
        let mock = MockDeleteTree::new(&[
            ("album", MockNodeKind::Dir),
            ("album/01.flac", MockNodeKind::File),
            ("album/cover.jpg", MockNodeKind::File),
        ]);
        let location = mock_location("album");
        delete_tree_recursive(&mock, &location, &mock_credentials())
            .await
            .unwrap();
        assert_eq!(
            mock.deleted_paths(),
            vec!["album/01.flac", "album/cover.jpg", "album"]
        );
    }

    #[tokio::test]
    async fn delete_tree_post_order_deletes_nested_dirs_bottom_up() {
        let mock = MockDeleteTree::new(&[
            ("music", MockNodeKind::Dir),
            ("music/artist", MockNodeKind::Dir),
            ("music/artist/album", MockNodeKind::Dir),
            ("music/artist/album/01.flac", MockNodeKind::File),
            ("music/artist/album/02.flac", MockNodeKind::File),
        ]);
        let location = mock_location("music");
        delete_tree_recursive(&mock, &location, &mock_credentials())
            .await
            .unwrap();
        assert_eq!(
            mock.deleted_paths(),
            vec![
                "music/artist/album/01.flac",
                "music/artist/album/02.flac",
                "music/artist/album",
                "music/artist",
                "music",
            ]
        );
    }

    #[test]
    fn classify_logon_failure_ntstatus() {
        let raw = "Unexpected message status: Logon Failure (0xc000006d).";
        let (code, detail) = classify_smb_client_error(raw);
        assert_eq!(code, "SMB_AUTH_FAILED");
        assert!(detail.contains("Logon failed"));
        assert!(detail.contains("0xc000006d") || detail.contains("C000006D"));
    }

    #[test]
    fn classify_insufficient_resources_ntstatus() {
        let raw = "Server returned an error message with status: 0xc000009a.";
        let (code, detail) = classify_smb_client_error(raw);
        assert_eq!(code, "SMB_UNAVAILABLE");
        assert!(detail.contains("overloaded") || detail.contains("resources"));
    }

    #[test]
    fn classify_operation_timeout() {
        let raw = "Operation timed out: ReceiveNextMessage, took >10s";
        let (code, detail) = classify_smb_client_error(raw);
        assert_eq!(code, "SMB_TIMEOUT");
        assert!(detail.contains("445") || detail.contains("respond"));
    }

    #[test]
    fn classify_ndr64_bind_rejection() {
        let raw = "BindAck result for syntax (71710533-beba-4937-8319-b5dbef9ccc36/1) was not acceptance: ProviderRejection ProposedTransferSyntaxesNotSupported";
        let (code, detail) = classify_smb_client_error(raw);
        assert_eq!(code, "SMB_SHARE_ENUM_UNSUPPORTED");
        assert!(detail.contains("manually"));
    }

    #[test]
    #[ignore = "requires a reachable SMB share configured with EUTERPE_TEST_SMB_*"]
    fn smb_watch_integration_is_env_gated() {
        let required = [
            "EUTERPE_TEST_SMB_LOCATION",
            "EUTERPE_TEST_SMB_USERNAME",
            "EUTERPE_TEST_SMB_PASSWORD",
        ];
        if required
            .iter()
            .any(|key| std::env::var_os(key).is_none_or(|value| value.is_empty()))
        {
            eprintln!("skipping SMB watch integration test; EUTERPE_TEST_SMB_* is incomplete");
            return;
        }

        eprintln!(
            "EUTERPE_TEST_SMB_* is configured; run a runtime-backed SMB watch smoke test from an integration harness"
        );
    }
}
