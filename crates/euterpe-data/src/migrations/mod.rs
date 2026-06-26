use crate::connection::DataHandle;
use crate::error::Result;
use welds::migrations::prelude::*;

const MIGRATIONS: &[MigrationFn] = &[
    adopt_legacy_sqlx_schema,
    phase2_qobuz,
    phase3_download_jobs,
    qobuz_favorites_slug,
    phase5_library,
    track_tag_fields,
    qobuz_accounts_oauth,
    qobuz_favorites_cover_url,
    integrations,
    library_scan_progress_columns,
    tracks_file_size,
    download_job_torrent,
    download_queue_position,
    download_job_paused_status,
    app_settings_seeds,
    convert_jobs,
    convert_jobs_active_album_index,
    cue_jobs,
];

const APPLIED_MIGRATION_NAMES: &[&str] = &[
    "001_phase2_qobuz",
    "002_phase3_download_jobs",
    "003_qobuz_favorites_slug",
    "004_phase5_library",
    "005_track_tag_fields",
    "006_qobuz_accounts_oauth",
    "007_qobuz_favorites_cover_url",
    "008_integrations",
    "009_library_scan_progress_columns",
    "010_tracks_file_size",
    "011_download_job_torrent",
    "012_download_queue_position",
    "013_download_job_paused_status",
    "014_app_settings_seeds",
    "015_convert_jobs",
    "016_convert_jobs_active_album_index",
    "017_cue_jobs",
];

pub async fn migrate(handle: &DataHandle) -> Result<()> {
    up(handle.client(), MIGRATIONS).await?;
    Ok(())
}

fn manual_step(name: &'static str, sql: impl Into<String>) -> welds::errors::Result<MigrationStep> {
    Ok(MigrationStep::new(name, Manual::up(sql)))
}

fn adopt_legacy_sqlx_schema(_: &TableState) -> welds::errors::Result<MigrationStep> {
    manual_step("000_adopt_legacy_sqlx_schema", legacy_adoption_sql())
}

fn legacy_adoption_sql() -> String {
    APPLIED_MIGRATION_NAMES
        .iter()
        .map(|name| {
            format!(
                "INSERT INTO _welds_migrations (name, when_applied, rollback_sql) \
                 SELECT '{name}', CAST(strftime('%s', 'now') AS INTEGER) * 1000, '' \
                 WHERE EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'settings')"
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn phase2_qobuz(_: &TableState) -> welds::errors::Result<MigrationStep> {
    manual_step(
        "001_phase2_qobuz",
        include_str!("../../../../migrations/001_phase2_qobuz.sql"),
    )
}

fn phase3_download_jobs(_: &TableState) -> welds::errors::Result<MigrationStep> {
    manual_step(
        "002_phase3_download_jobs",
        include_str!("../../../../migrations/002_phase3_download_jobs.sql"),
    )
}

fn qobuz_favorites_slug(_: &TableState) -> welds::errors::Result<MigrationStep> {
    manual_step(
        "003_qobuz_favorites_slug",
        include_str!("../../../../migrations/003_qobuz_favorites_slug.sql"),
    )
}

fn phase5_library(_: &TableState) -> welds::errors::Result<MigrationStep> {
    manual_step(
        "004_phase5_library",
        include_str!("../../../../migrations/004_phase5_library.sql"),
    )
}

fn track_tag_fields(_: &TableState) -> welds::errors::Result<MigrationStep> {
    manual_step(
        "005_track_tag_fields",
        include_str!("../../../../migrations/005_track_tag_fields.sql"),
    )
}

fn qobuz_accounts_oauth(_: &TableState) -> welds::errors::Result<MigrationStep> {
    manual_step(
        "006_qobuz_accounts_oauth",
        include_str!("../../../../migrations/006_qobuz_accounts_oauth.sql"),
    )
}

fn qobuz_favorites_cover_url(_: &TableState) -> welds::errors::Result<MigrationStep> {
    manual_step(
        "007_qobuz_favorites_cover_url",
        include_str!("../../../../migrations/007_qobuz_favorites_cover_url.sql"),
    )
}

fn integrations(_: &TableState) -> welds::errors::Result<MigrationStep> {
    manual_step(
        "008_integrations",
        include_str!("../../../../migrations/008_integrations.sql"),
    )
}

fn library_scan_progress_columns(_: &TableState) -> welds::errors::Result<MigrationStep> {
    manual_step(
        "009_library_scan_progress_columns",
        include_str!("../../../../migrations/009_library_scan_progress_columns.sql"),
    )
}

fn tracks_file_size(_: &TableState) -> welds::errors::Result<MigrationStep> {
    manual_step(
        "010_tracks_file_size",
        include_str!("../../../../migrations/010_tracks_file_size.sql"),
    )
}

fn download_job_torrent(_: &TableState) -> welds::errors::Result<MigrationStep> {
    manual_step(
        "011_download_job_torrent",
        include_str!("../../../../migrations/011_download_job_torrent.sql"),
    )
}

fn download_queue_position(_: &TableState) -> welds::errors::Result<MigrationStep> {
    manual_step(
        "012_download_queue_position",
        include_str!("../../../../migrations/012_download_queue_position.sql"),
    )
}

fn download_job_paused_status(_: &TableState) -> welds::errors::Result<MigrationStep> {
    manual_step(
        "013_download_job_paused_status",
        include_str!("../../../../migrations/013_download_job_paused_status.sql"),
    )
}

fn app_settings_seeds(_: &TableState) -> welds::errors::Result<MigrationStep> {
    manual_step(
        "014_app_settings_seeds",
        include_str!("../../../../migrations/014_app_settings_seeds.sql"),
    )
}

fn convert_jobs(_: &TableState) -> welds::errors::Result<MigrationStep> {
    manual_step(
        "015_convert_jobs",
        include_str!("../../../../migrations/015_convert_jobs.sql"),
    )
}

fn convert_jobs_active_album_index(_: &TableState) -> welds::errors::Result<MigrationStep> {
    manual_step(
        "016_convert_jobs_active_album_index",
        include_str!("../../../../migrations/016_convert_jobs_active_album_index.sql"),
    )
}

fn cue_jobs(_: &TableState) -> welds::errors::Result<MigrationStep> {
    manual_step(
        "017_cue_jobs",
        include_str!("../../../../migrations/017_cue_jobs.sql"),
    )
}
