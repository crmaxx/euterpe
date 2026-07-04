use crate::connection::DataHandle;
use crate::error::Result;
use crate::repositories::settings;
use chrono::Utc;
use std::collections::BTreeSet;
use welds::WeldsModel;
use welds::detect;
use welds::migrations::create_index;
use welds::migrations::prelude::*;
use welds::migrations::types::OnDelete;
use welds::state::DbState;

#[path = "001_create_settings.rs"]
mod m001_create_settings;
#[path = "002_create_qobuz_favorites.rs"]
mod m002_create_qobuz_favorites;
#[path = "003_create_qobuz_sync_runs.rs"]
mod m003_create_qobuz_sync_runs;
#[path = "004_create_download_jobs.rs"]
mod m004_create_download_jobs;
#[path = "005_index_download_jobs_status.rs"]
mod m005_index_download_jobs_status;
#[path = "006_index_download_jobs_queue.rs"]
mod m006_index_download_jobs_queue;
#[path = "007_create_artists.rs"]
mod m007_create_artists;
#[path = "008_create_albums.rs"]
mod m008_create_albums;
#[path = "009_index_albums_qobuz.rs"]
mod m009_index_albums_qobuz;
#[path = "010_index_albums_artist.rs"]
mod m010_index_albums_artist;
#[path = "011_create_tracks.rs"]
mod m011_create_tracks;
#[path = "012_index_tracks_album.rs"]
mod m012_index_tracks_album;
#[path = "013_index_tracks_path.rs"]
mod m013_index_tracks_path;
#[path = "014_create_library_scan_runs.rs"]
mod m014_create_library_scan_runs;
#[path = "015_index_library_scan_runs_status.rs"]
mod m015_index_library_scan_runs_status;
#[path = "016_create_qobuz_accounts.rs"]
mod m016_create_qobuz_accounts;
#[path = "017_index_qobuz_accounts_user.rs"]
mod m017_index_qobuz_accounts_user;
#[path = "018_create_qobuz_oauth_states.rs"]
mod m018_create_qobuz_oauth_states;
#[path = "019_index_qobuz_oauth_states_expires.rs"]
mod m019_index_qobuz_oauth_states_expires;
#[path = "020_create_integrations.rs"]
mod m020_create_integrations;
#[path = "021_index_integrations_type_enabled.rs"]
mod m021_index_integrations_type_enabled;
#[path = "022_create_convert_jobs.rs"]
mod m022_create_convert_jobs;
#[path = "023_index_convert_jobs_album_status.rs"]
mod m023_index_convert_jobs_album_status;
#[path = "024_index_convert_jobs_status.rs"]
mod m024_index_convert_jobs_status;
#[path = "025_create_cue_jobs.rs"]
mod m025_create_cue_jobs;
#[path = "026_index_cue_jobs_album_status.rs"]
mod m026_index_cue_jobs_album_status;
#[path = "027_index_cue_jobs_status.rs"]
mod m027_index_cue_jobs_status;
#[path = "028_create_scan_keep_paths.rs"]
mod m028_create_scan_keep_paths;
#[path = "029_extend_qobuz_sync_runs_for_scheduler.rs"]
mod m029_extend_qobuz_sync_runs_for_scheduler;

use m001_create_settings::create_settings;
use m002_create_qobuz_favorites::create_qobuz_favorites;
use m003_create_qobuz_sync_runs::create_qobuz_sync_runs;
use m004_create_download_jobs::create_download_jobs;
use m005_index_download_jobs_status::index_download_jobs_status;
use m006_index_download_jobs_queue::index_download_jobs_queue;
use m007_create_artists::create_artists;
use m008_create_albums::create_albums;
use m009_index_albums_qobuz::index_albums_qobuz;
use m010_index_albums_artist::index_albums_artist;
use m011_create_tracks::create_tracks;
use m012_index_tracks_album::index_tracks_album;
use m013_index_tracks_path::index_tracks_path;
use m014_create_library_scan_runs::create_library_scan_runs;
use m015_index_library_scan_runs_status::index_library_scan_runs_status;
use m016_create_qobuz_accounts::create_qobuz_accounts;
use m017_index_qobuz_accounts_user::index_qobuz_accounts_user;
use m018_create_qobuz_oauth_states::create_qobuz_oauth_states;
use m019_index_qobuz_oauth_states_expires::index_qobuz_oauth_states_expires;
use m020_create_integrations::create_integrations;
use m021_index_integrations_type_enabled::index_integrations_type_enabled;
use m022_create_convert_jobs::create_convert_jobs;
use m023_index_convert_jobs_album_status::index_convert_jobs_album_status;
use m024_index_convert_jobs_status::index_convert_jobs_status;
use m025_create_cue_jobs::create_cue_jobs;
use m026_index_cue_jobs_album_status::index_cue_jobs_album_status;
use m027_index_cue_jobs_status::index_cue_jobs_status;
use m028_create_scan_keep_paths::create_scan_keep_paths;
use m029_extend_qobuz_sync_runs_for_scheduler::extend_qobuz_sync_runs_for_scheduler;

const MIGRATIONS: &[MigrationFn] = &[
    create_settings,
    create_qobuz_favorites,
    create_qobuz_sync_runs,
    create_download_jobs,
    index_download_jobs_status,
    index_download_jobs_queue,
    create_artists,
    create_albums,
    index_albums_qobuz,
    index_albums_artist,
    create_tracks,
    index_tracks_album,
    index_tracks_path,
    create_library_scan_runs,
    index_library_scan_runs_status,
    create_qobuz_accounts,
    index_qobuz_accounts_user,
    create_qobuz_oauth_states,
    index_qobuz_oauth_states_expires,
    create_integrations,
    index_integrations_type_enabled,
    create_convert_jobs,
    index_convert_jobs_album_status,
    index_convert_jobs_status,
    create_cue_jobs,
    index_cue_jobs_album_status,
    index_cue_jobs_status,
    create_scan_keep_paths,
    extend_qobuz_sync_runs_for_scheduler,
];

const MIGRATION_NAMES: &[&str] = &[
    "001_create_settings",
    "002_create_qobuz_favorites",
    "003_create_qobuz_sync_runs",
    "004_create_download_jobs",
    "005_index_download_jobs_status",
    "006_index_download_jobs_queue",
    "007_create_artists",
    "008_create_albums",
    "009_index_albums_qobuz",
    "010_index_albums_artist",
    "011_create_tracks",
    "012_index_tracks_album",
    "013_index_tracks_path",
    "014_create_library_scan_runs",
    "015_index_library_scan_runs_status",
    "016_create_qobuz_accounts",
    "017_index_qobuz_accounts_user",
    "018_create_qobuz_oauth_states",
    "019_index_qobuz_oauth_states_expires",
    "020_create_integrations",
    "021_index_integrations_type_enabled",
    "022_create_convert_jobs",
    "023_index_convert_jobs_album_status",
    "024_index_convert_jobs_status",
    "025_create_cue_jobs",
    "026_index_cue_jobs_album_status",
    "027_index_cue_jobs_status",
    "028_create_scan_keep_paths",
    "029_extend_qobuz_sync_runs_for_scheduler",
];

#[derive(Debug, WeldsModel)]
#[welds(table = "_welds_migrations")]
struct MigrationLog {
    #[welds(primary_key)]
    id: i64,
    name: String,
    when_applied: i64,
    rollback_sql: String,
}

#[derive(Debug, WeldsModel)]
#[welds(table = "qobuz_sync_runs")]
struct LegacyQobuzSyncRun {
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

fn add_legacy_qobuz_sync_runs_trigger(state: &TableState) -> welds::errors::Result<MigrationStep> {
    Ok(MigrationStep::new(
        "legacy_add_qobuz_sync_runs_trigger",
        change_table(state, "qobuz_sync_runs")?
            .add_column("trigger", Type::String)
            .null(),
    ))
}

fn add_legacy_qobuz_sync_runs_skip_reason(
    state: &TableState,
) -> welds::errors::Result<MigrationStep> {
    Ok(MigrationStep::new(
        "legacy_add_qobuz_sync_runs_skip_reason",
        change_table(state, "qobuz_sync_runs")?
            .add_column("skip_reason", Type::Text)
            .null(),
    ))
}

fn remove_legacy_qobuz_sync_runs_status_check(
    state: &TableState,
) -> welds::errors::Result<MigrationStep> {
    Ok(MigrationStep::new(
        "legacy_remove_qobuz_sync_runs_status_check",
        change_table(state, "qobuz_sync_runs")?
            .change("status")
            .to_type(Type::Raw("VARCHAR(255)".to_string())),
    ))
}

pub async fn migrate(handle: &DataHandle) -> Result<()> {
    if has_legacy_sqlx_schema(handle).await? {
        upgrade_legacy_sqlx_schema(handle).await?;
        seed_default_settings(handle).await?;
        return Ok(());
    }
    up(handle.client(), MIGRATIONS).await?;
    seed_default_settings(handle).await?;
    Ok(())
}

pub async fn applied_migration_count(handle: &DataHandle) -> Result<u64> {
    Ok(MigrationLog::all().count(handle.client()).await?)
}

async fn has_legacy_sqlx_schema(handle: &DataHandle) -> Result<bool> {
    let tables = detect::find_all_tables(handle.client()).await?;
    let table_names: BTreeSet<&str> = tables.iter().map(|table| table.ident().name()).collect();
    let required_tables = [
        "settings",
        "qobuz_favorites",
        "qobuz_sync_runs",
        "download_jobs",
        "artists",
        "albums",
        "tracks",
        "library_scan_runs",
        "qobuz_accounts",
        "qobuz_oauth_states",
        "integrations",
        "convert_jobs",
        "cue_jobs",
    ];
    let has_welds_metadata = table_names.contains("_welds_migrations");
    let has_legacy_tables = required_tables
        .iter()
        .all(|table| table_names.contains(table));
    if !has_legacy_tables {
        return Ok(false);
    }
    if !has_welds_metadata {
        return Ok(true);
    }
    Ok(welds_migration_count(handle).await? < MIGRATION_NAMES.len() as i64)
}

async fn welds_migration_count(handle: &DataHandle) -> Result<i64> {
    Ok(MigrationLog::all().count(handle.client()).await? as i64)
}

async fn upgrade_legacy_sqlx_schema(handle: &DataHandle) -> Result<()> {
    let tables = detect::find_all_tables(handle.client()).await?;
    let table_names: BTreeSet<&str> = tables.iter().map(|table| table.ident().name()).collect();
    if !table_names.contains("scan_keep_paths") {
        up(handle.client(), &[create_scan_keep_paths]).await?;
    }
    upgrade_legacy_qobuz_sync_runs(handle).await?;
    mark_current_migrations_applied(handle).await?;
    Ok(())
}

async fn upgrade_legacy_qobuz_sync_runs(handle: &DataHandle) -> Result<()> {
    let columns = table_columns(handle, "qobuz_sync_runs").await?;
    if !columns.contains("trigger") {
        up(handle.client(), &[add_legacy_qobuz_sync_runs_trigger]).await?;
    }
    if !columns.contains("skip_reason") {
        up(handle.client(), &[add_legacy_qobuz_sync_runs_skip_reason]).await?;
    }
    up(
        handle.client(),
        &[remove_legacy_qobuz_sync_runs_status_check],
    )
    .await?;
    for mut run in LegacyQobuzSyncRun::all()
        .run(handle.client())
        .await?
        .into_iter()
        .filter(|run| run.trigger.is_none())
    {
        run.trigger = Some("manual".to_string());
        run.save(handle.client()).await?;
    }
    Ok(())
}

async fn table_columns(handle: &DataHandle, table: &str) -> Result<BTreeSet<String>> {
    Ok(detect::find_all_tables(handle.client())
        .await?
        .into_iter()
        .find(|table_def| table_def.ident().name() == table)
        .map(|table_def| {
            table_def
                .columns()
                .iter()
                .map(|column| column.name().to_string())
                .collect()
        })
        .unwrap_or_default())
}

async fn mark_current_migrations_applied(handle: &DataHandle) -> Result<()> {
    up(handle.client(), &[]).await?;
    let existing = MigrationLog::all()
        .run(handle.client())
        .await?
        .into_iter()
        .map(|row| row.name.clone())
        .collect::<BTreeSet<_>>();
    for name in MIGRATION_NAMES {
        if existing.contains(*name) {
            continue;
        }
        let mut row = DbState::new_uncreated(MigrationLog {
            id: 0,
            name: (*name).to_string(),
            when_applied: Utc::now().timestamp_millis(),
            rollback_sql: String::new(),
        });
        row.save(handle.client()).await?;
    }
    Ok(())
}

async fn seed_default_settings(handle: &DataHandle) -> Result<()> {
    seed_setting(
        handle,
        "ui.preferences",
        r#"{"theme":"system","locale":"en","default_quality":6}"#,
    )
    .await?;
    seed_setting(
        handle,
        "converter.settings",
        r#"{"auto_enabled":false,"file_policy":"sibling_then_delete","parallelism":5,"formats":["wav","m4a","ape"],"flac_encode":{"preset":"balanced","block_size":null,"multithread":false}}"#,
    )
    .await?;
    seed_setting(
        handle,
        "library.scan.settings",
        r#"{"worker_total":10,"enum_workers":5,"process_workers":5,"seed_depth":1,"index_queue_capacity":512,"path_queue_capacity":2048}"#,
    )
    .await?;
    seed_setting(handle, "downloads.settings", r#"{"concurrency":3}"#).await?;
    seed_setting(
        handle,
        "qobuz.scheduled_sync.settings",
        r#"{"enabled":false,"cron_expression":"0 3 * * *","auto_download_new_favorites":false}"#,
    )
    .await?;
    Ok(())
}

async fn seed_setting(handle: &DataHandle, key: &str, value: &str) -> Result<()> {
    if settings::get(handle, key).await?.is_none() {
        settings::set(handle, key, value).await?;
    }
    Ok(())
}
