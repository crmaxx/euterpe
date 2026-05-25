use bytes::Bytes;
use futures_util::{Stream, StreamExt};
use smb::FileStandardInformation;
use smb::binrw_util::prelude::SizedWideString;
use smb::{CreateDisposition, FileAccessMask, FileAttributes, FileCreateArgs, Resource, UncPath};
use smb::{CreateOptions, FileDispositionInformation, FileRenameInformation};
use std::pin::Pin;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmbShareLocation {
    pub host: String,
    pub port: u16,
    pub share: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmbCredentials {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmbDirectoryEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: Option<u64>,
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

pub struct SmbStorageClient {
    inner: smb::Client,
}

impl SmbStorageClient {
    pub fn new() -> Self {
        Self {
            inner: smb::Client::new(smb::ClientConfig::default()),
        }
    }

    pub async fn list_shares(
        &self,
        host: &str,
        port: u16,
        credentials: &SmbCredentials,
    ) -> Result<Vec<String>> {
        let server = format_server(host, port);
        self.inner
            .ipc_connect(&server, &credentials.username, credentials.password.clone())
            .await
            .map_err(map_smb_error)?;
        let shares = self
            .inner
            .list_shares(&server)
            .await
            .map_err(map_smb_error)?;
        Ok(shares
            .into_iter()
            .filter_map(|share| share.netname.as_ref().map(|name| name.value.to_string()))
            .collect())
    }

    pub async fn list_directory(
        &self,
        location: &SmbShareLocation,
        credentials: &SmbCredentials,
    ) -> Result<Vec<SmbDirectoryEntry>> {
        let server = format_server(&location.host, location.port);
        let unc = smb::UncPath::new(&server)
            .map_err(map_smb_error)?
            .with_share(&location.share)
            .map_err(map_smb_error)?;
        self.inner
            .share_connect(&unc, &credentials.username, credentials.password.clone())
            .await
            .map_err(map_smb_error)?;

        let dir_path = normalize_remote_path(&location.path);
        let resource_path = unc.with_path(&dir_path);
        let resource = self
            .inner
            .create_file(
                &resource_path,
                &smb::FileCreateArgs::make_open_existing(
                    smb::FileAccessMask::new().with_generic_read(true),
                ),
            )
            .await
            .map_err(map_smb_error)?;
        let dir = std::sync::Arc::new(resource.unwrap_dir());
        let mut stream = smb::Directory::query::<smb::FileDirectoryInformation>(&dir, "*")
            .await
            .map_err(map_smb_error)?;
        let mut entries = Vec::new();
        use futures_util::StreamExt;
        while let Some(item) = stream.next().await {
            let item = item.map_err(map_smb_error)?;
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
            });
        }
        entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.cmp(&b.name)));
        Ok(entries)
    }

    pub async fn metadata(
        &self,
        location: &SmbShareLocation,
        credentials: &SmbCredentials,
    ) -> Result<SmbMetadata> {
        let resource = self
            .open_resource(
                location,
                credentials,
                FileAccessMask::new().with_generic_read(true),
            )
            .await?;
        match resource {
            Resource::File(file) => {
                let info = file
                    .query_info::<FileStandardInformation>()
                    .await
                    .map_err(map_smb_error)?;
                Ok(SmbMetadata {
                    kind: SmbEntryKind::File,
                    size: info.end_of_file,
                })
            }
            Resource::Directory(dir) => {
                let info = dir
                    .query_info::<FileStandardInformation>()
                    .await
                    .map_err(map_smb_error)?;
                Ok(SmbMetadata {
                    kind: SmbEntryKind::Directory,
                    size: info.end_of_file,
                })
            }
            _ => Err(SmbStorageError::ResourceType),
        }
    }

    pub async fn read_at(
        &self,
        location: &SmbShareLocation,
        credentials: &SmbCredentials,
        offset: u64,
        len: usize,
    ) -> Result<Bytes> {
        let file = self
            .open_resource(
                location,
                credentials,
                FileAccessMask::new().with_generic_read(true),
            )
            .await?
            .unwrap_file();
        let mut buf = vec![0; len];
        let n = file
            .read_block(&mut buf, offset, None, false)
            .await
            .map_err(map_io_error)?;
        buf.truncate(n);
        Ok(Bytes::from(buf))
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
        let resource = self
            .create_resource(
                location,
                credentials,
                &FileCreateArgs::make_overwrite(FileAttributes::new(), CreateOptions::new()),
            )
            .await?;
        let file = resource.unwrap_file();
        let mut offset = 0u64;
        while offset < bytes.len() as u64 {
            let written = file
                .write_block(&bytes[offset as usize..], offset, None)
                .await
                .map_err(map_io_error)?;
            if written == 0 {
                return Err(SmbStorageError::Io("zero-byte SMB write".into()));
            }
            offset += written as u64;
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
        let tmp = temporary_sibling(&location.path);
        let tmp_location = SmbShareLocation {
            path: tmp.clone(),
            ..location.clone()
        };
        self.write_all(&tmp_location, credentials, bytes).await?;
        self.rename(&tmp_location, location, credentials, true)
            .await
    }

    pub async fn create_dir_all(
        &self,
        location: &SmbShareLocation,
        credentials: &SmbCredentials,
    ) -> Result<()> {
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
            let resource = self.create_resource(&step, credentials, &args).await?;
            if !resource.is_dir() {
                return Err(SmbStorageError::ResourceType);
            }
        }
        Ok(())
    }

    pub async fn delete(
        &self,
        location: &SmbShareLocation,
        credentials: &SmbCredentials,
    ) -> Result<()> {
        let resource = self
            .open_resource(
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
        let resource = self
            .open_resource(
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
        let resource = self
            .open_resource(
                location,
                credentials,
                FileAccessMask::new().with_generic_read(true),
            )
            .await?;
        let Resource::Directory(dir) = resource else {
            return Err(SmbStorageError::ResourceType);
        };
        let dir = Arc::new(dir);
        let base_path = normalize_remote_path(&location.path);
        let stream = smb::Directory::watch_stream(&dir, smb::NotifyFilter::all(), recursive)
            .map_err(map_smb_error)?
            .map(move |item| {
                item.map(|notify| map_watch_event(&base_path, notify))
                    .map_err(map_smb_error)
            });
        Ok(detach_watch_stream_lifetime(Box::pin(stream)))
    }

    async fn open_resource(
        &self,
        location: &SmbShareLocation,
        credentials: &SmbCredentials,
        access: FileAccessMask,
    ) -> Result<Resource> {
        self.create_resource(
            location,
            credentials,
            &FileCreateArgs::make_open_existing(access),
        )
        .await
    }

    async fn create_resource(
        &self,
        location: &SmbShareLocation,
        credentials: &SmbCredentials,
        args: &FileCreateArgs,
    ) -> Result<Resource> {
        let unc = self.connect_share(location, credentials).await?;
        let resource_path = unc.with_path(&normalize_remote_path(&location.path));
        self.inner
            .create_file(&resource_path, args)
            .await
            .map_err(map_smb_error)
    }

    async fn connect_share(
        &self,
        location: &SmbShareLocation,
        credentials: &SmbCredentials,
    ) -> Result<UncPath> {
        let server = format_server(&location.host, location.port);
        let unc = smb::UncPath::new(&server)
            .map_err(map_smb_error)?
            .with_share(&location.share)
            .map_err(map_smb_error)?;
        self.inner
            .share_connect(&unc, &credentials.username, credentials.password.clone())
            .await
            .map_err(map_smb_error)?;
        Ok(unc)
    }
}

impl Default for SmbStorageClient {
    fn default() -> Self {
        Self::new()
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

fn format_server(host: &str, port: u16) -> String {
    if port == 445 {
        host.to_string()
    } else {
        format!("{host}:{port}")
    }
}

fn map_smb_error(error: smb::Error) -> SmbStorageError {
    SmbStorageError::Client(error.to_string())
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
    fn joins_watch_paths_with_normalization() {
        assert_eq!(
            join_watch_path("Music", r"Artist\Album"),
            "Music/Artist/Album"
        );
        assert_eq!(join_watch_path("", r"Artist\Album"), "Artist/Album");
        assert_eq!(join_watch_path("Music", ""), "Music");
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
