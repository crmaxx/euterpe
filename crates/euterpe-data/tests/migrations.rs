use std::collections::{BTreeMap, BTreeSet};

use euterpe_data::repositories::{
    catalog, convert_jobs, cue_jobs, download_jobs, integrations, library_scan_runs, qobuz,
    settings,
};
use euterpe_data::{connect_database, migrations};
use welds::WeldsModel;
use welds::detect::{self, TableDef};

#[derive(Debug, WeldsModel)]
#[welds(table = "settings")]
struct Setting {
    #[welds(primary_key)]
    key: String,
    value: String,
    updated_at: String,
}

#[tokio::test]
async fn fresh_database_migrates_to_current_schema_shape() {
    let handle = connect_database("sqlite::memory:").await.unwrap();

    migrations::migrate(&handle).await.unwrap();

    let tables = detect::find_all_tables(handle.client()).await.unwrap();
    let table_names = table_names(&tables);
    for expected in [
        "albums",
        "artists",
        "convert_jobs",
        "cue_jobs",
        "download_jobs",
        "integrations",
        "library_scan_runs",
        "qobuz_accounts",
        "qobuz_favorites",
        "qobuz_oauth_states",
        "qobuz_sync_runs",
        "settings",
        "tracks",
    ] {
        assert!(table_names.contains(expected), "missing table {expected}");
    }

    let columns = columns_by_table(&tables);
    assert_columns(
        &columns,
        "tracks",
        &[
            "album_id",
            "created_at",
            "disc_number",
            "duration_sec",
            "file_hash",
            "file_mtime",
            "file_size",
            "genre",
            "id",
            "path",
            "qobuz_track_id",
            "title",
            "track_number",
            "updated_at",
            "year",
        ],
    );
    assert_columns(
        &columns,
        "qobuz_sync_runs",
        &[
            "albums_added",
            "albums_removed",
            "albums_total",
            "error_message",
            "finished_at",
            "id",
            "skip_reason",
            "started_at",
            "status",
            "trigger",
        ],
    );
    assert_columns(
        &columns,
        "download_jobs",
        &[
            "created_at",
            "download_speed_bps",
            "error_message",
            "id",
            "job_type",
            "payload_json",
            "progress_pct",
            "qobuz_id",
            "quality",
            "queue_position",
            "status",
            "updated_at",
        ],
    );
    assert_columns(
        &columns,
        "library_scan_runs",
        &[
            "error_message",
            "files_indexed",
            "files_processed",
            "files_seen",
            "files_total",
            "finished_at",
            "id",
            "started_at",
            "status",
        ],
    );
    assert_columns(
        &columns,
        "convert_jobs",
        &[
            "album_id",
            "created_at",
            "error_message",
            "files_done",
            "files_total",
            "id",
            "payload_json",
            "progress_pct",
            "status",
            "trigger",
            "updated_at",
        ],
    );
    assert_columns(
        &columns,
        "cue_jobs",
        &[
            "album_id",
            "created_at",
            "error_message",
            "id",
            "payload_json",
            "progress_pct",
            "status",
            "tracks_done",
            "tracks_total",
            "updated_at",
        ],
    );
}

#[tokio::test]
async fn migrations_seed_default_settings_through_typed_reads() {
    let handle = connect_database("sqlite::memory:").await.unwrap();

    migrations::migrate(&handle).await.unwrap();

    for key in [
        "ui.preferences",
        "converter.settings",
        "library.scan.settings",
        "downloads.settings",
        "qobuz.scheduled_sync.settings",
    ] {
        let setting = Setting::find_by_id(handle.client(), key.to_string())
            .await
            .unwrap();
        assert!(setting.is_some(), "missing seeded setting {key}");
    }
}

#[tokio::test]
async fn migrate_can_run_more_than_once() {
    let handle = connect_database("sqlite::memory:").await.unwrap();

    migrations::migrate(&handle).await.unwrap();
    migrations::migrate(&handle).await.unwrap();

    let setting = Setting::find_by_id(handle.client(), "downloads.settings".to_string())
        .await
        .unwrap();
    assert!(setting.is_some());
}

#[tokio::test]
async fn migrations_preserve_existing_user_settings() {
    let handle = connect_database("sqlite::memory:").await.unwrap();
    migrations::migrate(&handle).await.unwrap();
    let mut setting = Setting::find_by_id(handle.client(), "downloads.settings".to_string())
        .await
        .unwrap()
        .unwrap();
    setting.value = r#"{"concurrency":9}"#.to_string();
    setting.save(handle.client()).await.unwrap();

    migrations::migrate(&handle).await.unwrap();

    let setting = Setting::find_by_id(handle.client(), "downloads.settings".to_string())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(setting.value, r#"{"concurrency":9}"#);
}

#[tokio::test]
async fn existing_sqlx_migrated_database_fixture_is_adopted_without_reset() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("legacy/library.db");
    std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
    std::fs::copy("tests/fixtures/legacy-sqlx-v18.sqlite", &db_path).unwrap();
    let database_url = format!("sqlite:{}?mode=rwc", db_path.display());
    let handle = connect_database(&database_url).await.unwrap();

    migrations::migrate(&handle).await.unwrap();

    assert_eq!(
        settings::get(&handle, "downloads.settings").await.unwrap(),
        Some(r#"{"concurrency":7}"#.to_string())
    );
    assert_eq!(
        catalog::get_track_by_id(&handle, 1)
            .await
            .unwrap()
            .unwrap()
            .path,
        "Legacy Artist/Legacy Album/01.flac"
    );
    assert_eq!(
        download_jobs::get_by_id(&handle, 1)
            .await
            .unwrap()
            .unwrap()
            .queue_position,
        1
    );
    assert!(convert_jobs::get_by_id(&handle, 1).await.unwrap().is_some());
    assert!(cue_jobs::get_by_id(&handle, 1).await.unwrap().is_some());
    assert_eq!(
        integrations::list(&handle, Some("tag_source"))
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(qobuz::get_by_id(&handle, 1).await.unwrap().is_some());
    assert!(library_scan_runs::latest(&handle).await.unwrap().is_some());
}

fn table_names(tables: &[TableDef]) -> BTreeSet<String> {
    tables
        .iter()
        .map(|table| table.ident().name().to_string())
        .collect()
}

fn columns_by_table(tables: &[TableDef]) -> BTreeMap<String, BTreeSet<String>> {
    tables
        .iter()
        .map(|table| {
            let columns = table
                .columns()
                .iter()
                .map(|column| column.name().to_string())
                .collect();
            (table.ident().name().to_string(), columns)
        })
        .collect()
}

fn assert_columns(columns: &BTreeMap<String, BTreeSet<String>>, table: &str, expected: &[&str]) {
    let actual = columns
        .get(table)
        .unwrap_or_else(|| panic!("missing columns for table {table}"));
    for column in expected {
        assert!(
            actual.contains(*column),
            "missing column {table}.{column}; actual columns: {actual:?}"
        );
    }
}
