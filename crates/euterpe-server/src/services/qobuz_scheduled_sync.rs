use std::str::FromStr as _;
use std::sync::Arc;

use chrono::{DateTime, Local};
use croner::Cron;
use euterpe_data::repositories::{favorites, qobuz};
use euterpe_data::{DataHandle, repositories::qobuz::QobuzSyncTrigger};
use euterpe_qobuz::QobuzApi;
use tokio::sync::{Mutex, RwLock, mpsc};
use tokio_util::sync::CancellationToken;

use crate::error::ApiError;
use crate::services::app_settings::RuntimeSettings;
use crate::services::{download, qobuz_sync};

type QobuzClient = Arc<Mutex<Box<dyn QobuzApi + Send + Sync>>>;
type RuntimeSettingsHandle = Arc<RwLock<RuntimeSettings>>;

#[derive(Debug, Clone)]
pub struct CronSchedule {
    cron: Cron,
}

impl CronSchedule {
    pub fn parse(expression: &str) -> Result<Self, ApiError> {
        let cron = Cron::from_str(expression)
            .map_err(|e| ApiError::bad_request(format!("invalid cron expression: {e}")))?;
        Ok(Self { cron })
    }

    pub fn next_after(&self, after: DateTime<Local>) -> Result<DateTime<Local>, ApiError> {
        self.cron
            .find_next_occurrence(&after, false)
            .map_err(|e| ApiError::bad_request(format!("invalid cron expression: {e}")))
    }

    pub fn next_from_now(&self) -> Result<DateTime<Local>, ApiError> {
        self.next_after(Local::now())
    }
}

pub fn server_timezone_label() -> String {
    format!("server-local ({})", Local::now().offset())
}

#[derive(Clone)]
pub struct QobuzScheduledSyncDeps {
    pub data: DataHandle,
    pub qobuz: QobuzClient,
    pub runtime: RuntimeSettingsHandle,
    pub job_tx: mpsc::Sender<i64>,
}

#[derive(Clone)]
pub struct QobuzScheduledSyncHandle {
    deps: QobuzScheduledSyncDeps,
    active: Arc<Mutex<()>>,
    task: Arc<Mutex<Option<ScheduledTask>>>,
}

struct ScheduledTask {
    cancel: CancellationToken,
    join: tokio::task::JoinHandle<()>,
}

impl QobuzScheduledSyncHandle {
    pub fn new(deps: QobuzScheduledSyncDeps) -> Self {
        Self {
            deps,
            active: Arc::new(Mutex::new(())),
            task: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn restart(&self) -> Result<(), ApiError> {
        self.cancel_current().await;
        let settings = self.deps.runtime.read().await.qobuz_scheduled_sync.clone();
        if !settings.enabled {
            return Ok(());
        }
        let schedule = CronSchedule::parse(&settings.cron_expression)?;
        let cancel = CancellationToken::new();
        let loop_handle = self.clone();
        let loop_cancel = cancel.clone();
        let join = tokio::spawn(async move {
            loop_handle.run_loop(schedule, loop_cancel).await;
        });
        *self.task.lock().await = Some(ScheduledTask { cancel, join });
        Ok(())
    }

    pub async fn trigger_once(&self, trigger: QobuzSyncTrigger) -> Result<(), ApiError> {
        let Ok(_guard) = self.active.try_lock() else {
            qobuz::insert_sync_skipped(&self.deps.data, trigger, "already_running").await?;
            return Ok(());
        };
        let runtime = self.deps.runtime.read().await;
        let settings = runtime.qobuz_scheduled_sync.clone();
        let default_quality = runtime.ui.default_quality;
        drop(runtime);
        let sync = qobuz_sync::run_with_details_and_trigger(
            &self.deps.data,
            Arc::clone(&self.deps.qobuz),
            trigger,
        )
        .await?;
        if settings.auto_download_new_favorites {
            queue_new_favorites(&self.deps, default_quality, sync.newly_added_albums).await?;
        }
        Ok(())
    }

    async fn run_loop(self, schedule: CronSchedule, cancel: CancellationToken) {
        loop {
            let Ok(next) = schedule.next_from_now() else {
                return;
            };
            let sleep_for = (next - Local::now())
                .to_std()
                .unwrap_or_else(|_| std::time::Duration::from_secs(0));
            tokio::select! {
                _ = cancel.cancelled() => return,
                _ = tokio::time::sleep(sleep_for) => {
                    if let Err(error) = self.trigger_once(QobuzSyncTrigger::Scheduled).await {
                        tracing::error!(error = %error, "scheduled Qobuz favorites sync failed");
                    }
                }
            }
        }
    }

    async fn cancel_current(&self) {
        if let Some(task) = self.task.lock().await.take() {
            task.cancel.cancel();
            task.join.abort();
        }
    }
}

async fn queue_new_favorites(
    deps: &QobuzScheduledSyncDeps,
    quality: u8,
    albums: Vec<qobuz_sync::NewlyAddedFavoriteAlbum>,
) -> Result<(), ApiError> {
    for album in albums {
        if favorites::album_is_in_library(&deps.data, album.qobuz_id).await? {
            continue;
        }
        download::queue_album_download_if_missing(
            &deps.data,
            &deps.job_tx,
            &album.album_api_id,
            quality,
            Some(album.qobuz_id),
            Some(album.display_title),
        )
        .await?;
    }
    Ok(())
}

#[cfg(test)]
async fn record_overlap_skip_for_test(data: &DataHandle) -> Result<(), ApiError> {
    qobuz::insert_sync_skipped(data, QobuzSyncTrigger::Scheduled, "already_running").await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use euterpe_data::{connect_database, migrations, repositories::qobuz};

    use super::*;

    #[test]
    fn cron_schedule_rejects_invalid_expression() {
        assert!(CronSchedule::parse("not a cron").is_err());
    }

    #[test]
    fn cron_schedule_computes_next_server_local_run() {
        let schedule = CronSchedule::parse("0 3 * * *").unwrap();
        let next = schedule.next_from_now().unwrap();
        assert!(next > Local::now());
    }

    #[tokio::test]
    async fn record_overlap_skip_writes_scheduled_skipped_run() {
        let data = connect_database("sqlite::memory:").await.unwrap();
        migrations::migrate(&data).await.unwrap();

        record_overlap_skip_for_test(&data).await.unwrap();

        let latest = qobuz::sync_latest(&data).await.unwrap().unwrap();
        assert_eq!(latest.status, "skipped");
        assert_eq!(latest.trigger, "scheduled");
        assert_eq!(latest.skip_reason.as_deref(), Some("already_running"));
    }
}
