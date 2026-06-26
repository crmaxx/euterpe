use euterpe_data::repositories::library_scan_runs;
use euterpe_data::{connect_database, migrations};

#[tokio::test]
async fn scan_run_lifecycle_tracks_progress_and_terminal_states() {
    let handle = connect_database("sqlite::memory:").await.unwrap();
    migrations::migrate(&handle).await.unwrap();

    assert!(!library_scan_runs::has_running(&handle).await.unwrap());

    let id = library_scan_runs::start(&handle).await.unwrap();
    assert!(library_scan_runs::has_running(&handle).await.unwrap());

    library_scan_runs::update_progress(&handle, id, 10, 7, 5, 12)
        .await
        .unwrap();
    let running = library_scan_runs::get_by_id(&handle, id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(running.status, "running");
    assert_eq!(running.files_seen, 10);
    assert_eq!(running.files_processed, 7);
    assert_eq!(running.files_indexed, 5);
    assert_eq!(running.files_total, 12);

    library_scan_runs::finish_success(&handle, id)
        .await
        .unwrap();
    assert!(!library_scan_runs::has_running(&handle).await.unwrap());
    let finished = library_scan_runs::latest(&handle).await.unwrap().unwrap();
    assert_eq!(finished.id, id);
    assert_eq!(finished.status, "success");
    assert!(finished.finished_at.is_some());
}

#[tokio::test]
async fn scan_run_cancel_only_changes_running_rows() {
    let handle = connect_database("sqlite::memory:").await.unwrap();
    migrations::migrate(&handle).await.unwrap();

    let id = library_scan_runs::start(&handle).await.unwrap();
    assert!(library_scan_runs::cancel(&handle, id).await.unwrap());
    assert!(library_scan_runs::is_cancelled(&handle, id).await.unwrap());
    assert!(!library_scan_runs::cancel(&handle, id).await.unwrap());
}

#[tokio::test]
async fn scan_run_failure_stores_error_message() {
    let handle = connect_database("sqlite::memory:").await.unwrap();
    migrations::migrate(&handle).await.unwrap();

    let id = library_scan_runs::start(&handle).await.unwrap();
    library_scan_runs::finish_failed(&handle, id, "boom")
        .await
        .unwrap();
    let failed = library_scan_runs::get_by_id(&handle, id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(failed.status, "failed");
    assert_eq!(failed.error_message.as_deref(), Some("boom"));
    assert!(failed.finished_at.is_some());
}
