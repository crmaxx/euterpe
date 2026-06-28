use chrono::{DateTime, Utc};
use welds::WeldsModel;

use crate::connection::DataHandle;
use crate::error::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QobuzAccountListItem {
    pub id: i64,
    pub label: Option<String>,
    pub qobuz_user_id: i64,
    pub display_name: Option<String>,
    pub membership_label: Option<String>,
    pub uat_obtained_at: String,
    pub uat_expires_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QobuzAccountRecord {
    pub id: i64,
    pub qobuz_user_id: i64,
    pub uat_encrypted: String,
    pub display_name: Option<String>,
    pub membership_label: Option<String>,
    pub uat_obtained_at: String,
    pub uat_expires_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QobuzSyncRunSummary {
    pub id: i64,
    pub status: String,
    pub trigger: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub albums_total: Option<i64>,
    pub albums_added: Option<i64>,
    pub albums_removed: Option<i64>,
    pub error_message: Option<String>,
    pub skip_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QobuzSyncTrigger {
    Manual,
    Scheduled,
    SettingsRunNow,
}

impl QobuzSyncTrigger {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Scheduled => "scheduled",
            Self::SettingsRunNow => "settings_run_now",
        }
    }
}

#[derive(Debug, WeldsModel)]
#[welds(table = "qobuz_accounts")]
struct QobuzAccount {
    #[welds(primary_key)]
    id: i64,
    label: Option<String>,
    qobuz_user_id: i64,
    uat_encrypted: String,
    display_name: Option<String>,
    membership_label: Option<String>,
    uat_obtained_at: String,
    uat_expires_at: Option<String>,
    oauth_refresh_encrypted: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, WeldsModel)]
#[welds(table = "qobuz_oauth_states")]
struct QobuzOauthState {
    #[welds(primary_key)]
    id: i64,
    state: String,
    created_at: String,
    expires_at: String,
}

#[derive(Debug, WeldsModel)]
#[welds(table = "qobuz_sync_runs")]
struct QobuzSyncRun {
    #[welds(primary_key)]
    id: i64,
    started_at: String,
    finished_at: Option<String>,
    status: String,
    albums_total: Option<i64>,
    albums_added: Option<i64>,
    albums_removed: Option<i64>,
    error_message: Option<String>,
    trigger: Option<String>,
    skip_reason: Option<String>,
}

pub async fn get_by_id(handle: &DataHandle, id: i64) -> Result<Option<QobuzAccountRecord>> {
    Ok(QobuzAccount::find_by_id(handle.client(), id)
        .await?
        .map(account_record_from_model))
}

pub async fn find_by_qobuz_user_id(
    handle: &DataHandle,
    qobuz_user_id: i64,
) -> Result<Option<QobuzAccountRecord>> {
    Ok(QobuzAccount::all()
        .run(handle.client())
        .await?
        .into_iter()
        .find(|account| account.qobuz_user_id == qobuz_user_id)
        .map(account_record_from_model))
}

pub async fn list_without_uat(handle: &DataHandle) -> Result<Vec<QobuzAccountListItem>> {
    let mut rows = QobuzAccount::all()
        .run(handle.client())
        .await?
        .into_iter()
        .map(account_list_item_from_model)
        .collect::<Vec<_>>();
    rows.sort_by_key(|account| account.id);
    Ok(rows)
}

pub async fn upsert_after_oauth(
    handle: &DataHandle,
    qobuz_user_id: i64,
    uat_encrypted: &str,
    display_name: Option<&str>,
    membership_label: Option<&str>,
    uat_obtained_at: DateTime<Utc>,
    uat_expires_at: Option<DateTime<Utc>>,
) -> Result<i64> {
    let obtained = uat_obtained_at.to_rfc3339();
    let expires = uat_expires_at.map(|t| t.to_rfc3339());
    if let Some(mut account) = QobuzAccount::all()
        .run(handle.client())
        .await?
        .into_iter()
        .find(|account| account.qobuz_user_id == qobuz_user_id)
    {
        account.uat_encrypted = uat_encrypted.to_string();
        account.display_name = display_name.map(ToString::to_string);
        account.membership_label = membership_label.map(ToString::to_string);
        account.uat_obtained_at = obtained;
        account.uat_expires_at = expires;
        account.updated_at = sqlite_timestamp();
        account.save(handle.client()).await?;
        return Ok(account.id);
    }

    let now = sqlite_timestamp();
    let mut account = QobuzAccount::new();
    account.label = None;
    account.qobuz_user_id = qobuz_user_id;
    account.uat_encrypted = uat_encrypted.to_string();
    account.display_name = display_name.map(ToString::to_string);
    account.membership_label = membership_label.map(ToString::to_string);
    account.uat_obtained_at = obtained;
    account.uat_expires_at = expires;
    account.oauth_refresh_encrypted = None;
    account.created_at = now.clone();
    account.updated_at = now;
    account.save(handle.client()).await?;
    Ok(account.id)
}

pub async fn delete_by_id(handle: &DataHandle, id: i64) -> Result<bool> {
    let Some(mut account) = QobuzAccount::find_by_id(handle.client(), id).await? else {
        return Ok(false);
    };
    account.delete(handle.client()).await?;
    Ok(true)
}

pub async fn purge_expired_oauth_states(handle: &DataHandle) -> Result<()> {
    let now = Utc::now();
    for mut row in QobuzOauthState::all()
        .run(handle.client())
        .await?
        .into_iter()
        .filter(|row| oauth_state_is_expired(&row.expires_at, now))
    {
        row.delete(handle.client()).await?;
    }
    Ok(())
}

pub async fn insert_oauth_state(
    handle: &DataHandle,
    state: &str,
    expires_at: DateTime<Utc>,
) -> Result<()> {
    let mut row = QobuzOauthState::new();
    row.state = state.to_string();
    row.created_at = sqlite_timestamp();
    row.expires_at = expires_at.to_rfc3339();
    row.save(handle.client()).await?;
    Ok(())
}

pub async fn consume_sole_pending_oauth_state(handle: &DataHandle) -> Result<Option<String>> {
    let now = Utc::now();
    let rows = QobuzOauthState::all()
        .run(handle.client())
        .await?
        .into_iter()
        .filter(|row| !oauth_state_is_expired(&row.expires_at, now))
        .collect::<Vec<_>>();

    if rows.len() != 1 {
        return Ok(None);
    }
    let state = rows[0].state.clone();
    if consume_oauth_state(handle, &state).await? {
        Ok(Some(state))
    } else {
        Ok(None)
    }
}

pub async fn consume_oauth_state(handle: &DataHandle, state: &str) -> Result<bool> {
    let affected = QobuzOauthState::all()
        .where_col(|row| row.state.equal(state))
        .where_manual(|row| row.expires_at, " >= ?", (Utc::now().to_rfc3339(),))
        .delete(handle.client())
        .await?;
    Ok(affected > 0)
}

pub async fn get_sync_run_by_id(
    handle: &DataHandle,
    id: i64,
) -> Result<Option<QobuzSyncRunSummary>> {
    Ok(QobuzSyncRun::find_by_id(handle.client(), id)
        .await?
        .map(sync_summary_from_model))
}

pub async fn sync_latest(handle: &DataHandle) -> Result<Option<QobuzSyncRunSummary>> {
    Ok(QobuzSyncRun::all()
        .run(handle.client())
        .await?
        .into_iter()
        .max_by_key(|run| run.id)
        .map(sync_summary_from_model))
}

pub async fn start_sync_run(handle: &DataHandle) -> Result<i64> {
    start_sync_run_with_trigger(handle, QobuzSyncTrigger::Manual).await
}

pub async fn start_sync_run_with_trigger(
    handle: &DataHandle,
    trigger: QobuzSyncTrigger,
) -> Result<i64> {
    let mut row = QobuzSyncRun::new();
    row.started_at = sqlite_timestamp();
    row.finished_at = None;
    row.status = "running".to_string();
    row.trigger = Some(trigger.as_str().to_string());
    row.albums_total = Some(0);
    row.albums_added = Some(0);
    row.albums_removed = Some(0);
    row.error_message = None;
    row.skip_reason = None;
    row.save(handle.client()).await?;
    Ok(row.id)
}

pub async fn finish_sync_success(
    handle: &DataHandle,
    run_id: i64,
    albums_total: i64,
    added: i64,
    removed: i64,
) -> Result<()> {
    if let Some(mut run) = QobuzSyncRun::find_by_id(handle.client(), run_id).await? {
        run.finished_at = Some(sqlite_timestamp());
        run.status = "success".to_string();
        run.albums_total = Some(albums_total);
        run.albums_added = Some(added);
        run.albums_removed = Some(removed);
        run.save(handle.client()).await?;
    }
    Ok(())
}

pub async fn finish_sync_failed(handle: &DataHandle, run_id: i64, error: &str) -> Result<()> {
    if let Some(mut run) = QobuzSyncRun::find_by_id(handle.client(), run_id).await? {
        run.finished_at = Some(sqlite_timestamp());
        run.status = "failed".to_string();
        run.error_message = Some(error.to_string());
        run.save(handle.client()).await?;
    }
    Ok(())
}

pub async fn insert_sync_skipped(
    handle: &DataHandle,
    trigger: QobuzSyncTrigger,
    reason: &str,
) -> Result<i64> {
    let now = sqlite_timestamp();
    let mut row = QobuzSyncRun::new();
    row.started_at = now.clone();
    row.finished_at = Some(now);
    row.status = "skipped".to_string();
    row.trigger = Some(trigger.as_str().to_string());
    row.albums_total = Some(0);
    row.albums_added = Some(0);
    row.albums_removed = Some(0);
    row.error_message = None;
    row.skip_reason = Some(reason.to_string());
    row.save(handle.client()).await?;
    Ok(row.id)
}

fn account_record_from_model(account: welds::state::DbState<QobuzAccount>) -> QobuzAccountRecord {
    QobuzAccountRecord {
        id: account.id,
        qobuz_user_id: account.qobuz_user_id,
        uat_encrypted: account.uat_encrypted.clone(),
        display_name: account.display_name.clone(),
        membership_label: account.membership_label.clone(),
        uat_obtained_at: account.uat_obtained_at.clone(),
        uat_expires_at: account.uat_expires_at.clone(),
    }
}

fn account_list_item_from_model(
    account: welds::state::DbState<QobuzAccount>,
) -> QobuzAccountListItem {
    QobuzAccountListItem {
        id: account.id,
        label: account.label.clone(),
        qobuz_user_id: account.qobuz_user_id,
        display_name: account.display_name.clone(),
        membership_label: account.membership_label.clone(),
        uat_obtained_at: account.uat_obtained_at.clone(),
        uat_expires_at: account.uat_expires_at.clone(),
        created_at: account.created_at.clone(),
        updated_at: account.updated_at.clone(),
    }
}

fn sync_summary_from_model(run: welds::state::DbState<QobuzSyncRun>) -> QobuzSyncRunSummary {
    QobuzSyncRunSummary {
        id: run.id,
        status: run.status.clone(),
        trigger: run.trigger.clone().unwrap_or_else(|| "manual".to_string()),
        started_at: run.started_at.clone(),
        finished_at: run.finished_at.clone(),
        albums_total: run.albums_total,
        albums_added: run.albums_added,
        albums_removed: run.albums_removed,
        error_message: run.error_message.clone(),
        skip_reason: run.skip_reason.clone(),
    }
}

fn oauth_state_is_expired(expires_at: &str, now: DateTime<Utc>) -> bool {
    DateTime::parse_from_rfc3339(expires_at)
        .map(|expires| expires.with_timezone(&Utc) < now)
        .unwrap_or(false)
}

fn sqlite_timestamp() -> String {
    Utc::now()
        .naive_utc()
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}
