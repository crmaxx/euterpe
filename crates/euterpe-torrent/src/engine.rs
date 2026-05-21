use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};

use librqbit::api::TorrentIdOrHash;
use librqbit::{
    AddTorrent, AddTorrentOptions, AddTorrentResponse, ListenerMode, ListenerOptions, Session,
    SessionOptions, TorrentStats,
};
use tokio::sync::Mutex as AsyncMutex;

use crate::TorrentEngine;
use crate::error::TorrentError;
use crate::types::{
    InspectFile, InspectResult, JobHandle, JobStats, LibrqbitState, SessionSettings,
    StartJobRequest,
};

pub struct TorrentEngineConfig {
    pub incoming_dir: PathBuf,
    pub session_settings: SessionSettings,
}

pub struct LibrqbitEngine {
    incoming_dir: PathBuf,
    session: RwLock<Option<Arc<Session>>>,
    session_settings: Mutex<SessionSettings>,
    session_mutex: AsyncMutex<()>,
}

impl LibrqbitEngine {
    pub async fn new(config: TorrentEngineConfig) -> Result<Self, TorrentError> {
        tokio::fs::create_dir_all(&config.incoming_dir)
            .await
            .map_err(|e| TorrentError::msg(format!("create incoming dir: {e}")))?;

        let opts = session_options(config.session_settings);
        let session = Session::new_with_opts(config.incoming_dir.clone(), opts)
            .await
            .map_err(TorrentError::Other)?;

        Ok(Self {
            incoming_dir: config.incoming_dir,
            session: RwLock::new(Some(session)),
            session_settings: Mutex::new(config.session_settings),
            session_mutex: AsyncMutex::new(()),
        })
    }

    fn session(&self) -> Arc<Session> {
        self.session
            .read()
            .expect("session lock poisoned")
            .clone()
            .expect("torrent session not initialized")
    }

    fn dht_routing_nodes(session: &Session) -> u32 {
        session.get_dht().map_or(0, |dht| {
            let stats = dht.stats();
            stats
                .routing_table_size
                .saturating_add(stats.routing_table_size_v6) as u32
        })
    }

    fn active_torrent_count(session: &Session) -> usize {
        let count = std::cell::Cell::new(0);
        session.with_torrents(|torrents| count.set(torrents.count()));
        count.get()
    }

    fn add_torrent_options(
        list_only: bool,
        only_files: Option<Vec<usize>>,
        output_folder: PathBuf,
        ratelimits: librqbit::limits::LimitsConfig,
    ) -> AddTorrentOptions {
        AddTorrentOptions {
            list_only,
            only_files,
            output_folder: Some(output_folder.display().to_string()),
            overwrite: true,
            ratelimits,
            ..Default::default()
        }
    }

    async fn add_torrent_inner(
        &self,
        add: AddTorrent<'_>,
        opts: AddTorrentOptions,
    ) -> Result<AddTorrentResponse, TorrentError> {
        let _guard = self.session_mutex.lock().await;
        self.session()
            .add_torrent(add, Some(opts))
            .await
            .map_err(TorrentError::Other)
    }

    fn map_list_only(resp: AddTorrentResponse) -> Result<InspectResult, TorrentError> {
        match resp {
            AddTorrentResponse::ListOnly(lo) => {
                let name = lo
                    .info
                    .name()
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| hex::encode(lo.info_hash.0));
                let files: Vec<InspectFile> = lo
                    .info
                    .iter_file_details()
                    .enumerate()
                    .map(|(index, fd)| InspectFile {
                        index,
                        path: fd.filename.to_string(),
                        size_bytes: fd.len,
                        selected: true,
                    })
                    .collect();
                let total_size_bytes = files.iter().map(|f| f.size_bytes).sum();
                Ok(InspectResult {
                    name,
                    info_hash_v1: hex::encode(lo.info_hash.0),
                    total_size_bytes,
                    comment: None,
                    files,
                    torrent_bytes: Some(lo.torrent_bytes),
                })
            }
            AddTorrentResponse::Added(_, _) => {
                Err(TorrentError::msg("expected list_only response, got Added"))
            }
            AddTorrentResponse::AlreadyManaged(_, _) => Err(TorrentError::msg(
                "torrent already in session during inspect",
            )),
        }
    }

    fn map_librqbit_state(stats: &TorrentStats) -> LibrqbitState {
        match stats.state.to_string().as_str() {
            "live" => LibrqbitState::Live,
            "paused" => LibrqbitState::Paused,
            "error" => LibrqbitState::Error,
            _ => LibrqbitState::Initializing,
        }
    }

    fn eta_secs_from_live(stats: &TorrentStats) -> Option<u64> {
        let live = stats.live.as_ref()?;
        let value = serde_json::to_value(live).ok()?;
        let duration = value.get("time_remaining")?.get("duration")?;
        if let Some(secs) = duration.get("secs").and_then(|s| s.as_u64()) {
            return Some(secs);
        }
        duration.as_u64()
    }

    fn stats_from_torrent(stats: &TorrentStats) -> JobStats {
        let progress_pct = if stats.total_bytes == 0 {
            0.0
        } else {
            (stats.progress_bytes as f64 / stats.total_bytes as f64) * 100.0
        };
        let (download_speed_bps, upload_speed_bps, peers_live, peers_connecting) = stats
            .live
            .as_ref()
            .map(|l| {
                let ps = &l.snapshot.peer_stats;
                (
                    l.download_speed.as_bytes(),
                    l.upload_speed.as_bytes(),
                    ps.live,
                    ps.connecting,
                )
            })
            .unwrap_or((0, 0, 0, 0));
        JobStats {
            progress_pct,
            download_speed_bps,
            upload_speed_bps,
            progress_bytes: stats.progress_bytes,
            total_bytes: stats.total_bytes,
            finished: stats.finished,
            librqbit_state: Self::map_librqbit_state(stats),
            peers_live,
            peers_connecting,
            dht_routing_nodes: 0,
            eta_secs: Self::eta_secs_from_live(stats),
            error: stats.error.clone(),
        }
    }
}

fn session_options(settings: SessionSettings) -> SessionOptions {
    let listen = ListenerOptions {
        mode: ListenerMode::TcpAndUtp,
        enable_upnp_port_forwarding: settings.enable_upnp_port_forwarding,
        ..Default::default()
    };
    SessionOptions {
        disable_upload: settings.disable_upload,
        // Ephemeral DHT avoids lock/port conflicts when the session is recreated on settings save.
        disable_dht_persistence: true,
        ratelimits: settings.limits_config(),
        listen: Some(listen),
        ..Default::default()
    }
}

#[async_trait::async_trait]
impl TorrentEngine for LibrqbitEngine {
    async fn inspect_magnet(
        &self,
        magnet: &str,
        staging_output: PathBuf,
    ) -> Result<InspectResult, TorrentError> {
        tokio::fs::create_dir_all(&staging_output)
            .await
            .map_err(|e| TorrentError::msg(format!("staging dir: {e}")))?;

        let ratelimits = self
            .session_settings
            .lock()
            .expect("settings lock")
            .limits_config();
        let opts = Self::add_torrent_options(true, None, staging_output, ratelimits);

        let resp = self
            .add_torrent_inner(AddTorrent::from_url(magnet), opts)
            .await?;
        Self::map_list_only(resp)
    }

    async fn inspect_bytes(
        &self,
        torrent_file: &[u8],
        staging_output: PathBuf,
    ) -> Result<InspectResult, TorrentError> {
        tokio::fs::create_dir_all(&staging_output)
            .await
            .map_err(|e| TorrentError::msg(format!("staging dir: {e}")))?;

        let ratelimits = self
            .session_settings
            .lock()
            .expect("settings lock")
            .limits_config();
        let opts = Self::add_torrent_options(true, None, staging_output, ratelimits);

        let resp = self
            .add_torrent_inner(
                AddTorrent::from_bytes(bytes::Bytes::copy_from_slice(torrent_file)),
                opts,
            )
            .await?;
        Self::map_list_only(resp)
    }

    async fn start_job(&self, req: StartJobRequest) -> Result<JobHandle, TorrentError> {
        tokio::fs::create_dir_all(&req.output_folder)
            .await
            .map_err(|e| TorrentError::msg(format!("create output dir: {e}")))?;

        let only_files = if req.only_files.is_empty() {
            None
        } else {
            Some(req.only_files)
        };

        let opts = Self::add_torrent_options(false, only_files, req.output_folder, req.ratelimits);

        let add = match (&req.magnet, &req.torrent_bytes) {
            (Some(m), _) => AddTorrent::from_url(m.as_str()),
            (None, Some(b)) => AddTorrent::from_bytes(b.clone()),
            _ => return Err(TorrentError::msg("magnet or torrent_bytes required")),
        };

        let resp = self.add_torrent_inner(add, opts).await?;
        match resp {
            AddTorrentResponse::Added(id, handle) => Ok(JobHandle {
                librqbit_id: id,
                info_hash: hex::encode(handle.info_hash().0),
            }),
            AddTorrentResponse::ListOnly(_) => {
                Err(TorrentError::msg("expected Added, got ListOnly"))
            }
            AddTorrentResponse::AlreadyManaged(id, handle) => Ok(JobHandle {
                librqbit_id: id,
                info_hash: hex::encode(handle.info_hash().0),
            }),
        }
    }

    async fn job_stats(&self, handle: &JobHandle) -> Result<JobStats, TorrentError> {
        let session = self.session();
        let dht_routing_nodes = Self::dht_routing_nodes(&session);
        let managed = session
            .get(TorrentIdOrHash::Id(handle.librqbit_id))
            .ok_or_else(|| TorrentError::msg("torrent not in session"))?;
        let mut stats = Self::stats_from_torrent(&managed.stats());
        stats.dht_routing_nodes = dht_routing_nodes;
        Ok(stats)
    }

    async fn wait_until_completed(&self, handle: &JobHandle) -> Result<(), TorrentError> {
        let managed = self
            .session()
            .get(TorrentIdOrHash::Id(handle.librqbit_id))
            .ok_or_else(|| TorrentError::msg("torrent not in session"))?;
        managed
            .wait_until_completed()
            .await
            .map_err(TorrentError::Other)
    }

    async fn cancel(&self, handle: &JobHandle) -> Result<(), TorrentError> {
        self.remove_from_session(handle).await
    }

    async fn remove_from_session(&self, handle: &JobHandle) -> Result<(), TorrentError> {
        let _guard = self.session_mutex.lock().await;
        self.session()
            .delete(TorrentIdOrHash::Id(handle.librqbit_id), false)
            .await
            .map_err(TorrentError::Other)
    }

    async fn apply_session_settings(&self, settings: SessionSettings) -> Result<(), TorrentError> {
        let prev = *self
            .session_settings
            .lock()
            .expect("settings lock");

        if prev.disable_upload != settings.disable_upload {
            let session = self.session();
            if Self::active_torrent_count(&session) > 0 {
                return Err(TorrentError::msg(
                    "TORRENT_SESSION_BUSY: cannot change disable_upload while torrent downloads are active",
                ));
            }
            let _guard = self.session_mutex.lock().await;
            *self.session.write().expect("session lock poisoned") = None;
            let opts = session_options(settings);
            let new_session = Session::new_with_opts(self.incoming_dir.clone(), opts)
                .await
                .map_err(TorrentError::Other)?;
            *self.session.write().expect("session lock poisoned") = Some(new_session);
        } else {
            let session = self.session();
            session.ratelimits.set_upload_bps(settings.upload_bps);
            session
                .ratelimits
                .set_download_bps(settings.download_bps);
        }

        *self
            .session_settings
            .lock()
            .expect("settings lock") = settings;
        Ok(())
    }
}
