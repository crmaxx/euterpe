use chrono::{Duration, Utc};
use euterpe_data::repositories::qobuz;
use euterpe_data::{connect_database, migrations};
use std::sync::{Arc, Barrier};

async fn migrated_handle() -> euterpe_data::DataHandle {
    let handle = connect_database("sqlite::memory:").await.unwrap();
    migrations::migrate(&handle).await.unwrap();
    handle
}

async fn migrated_file_handle() -> (tempfile::TempDir, euterpe_data::DataHandle) {
    let tempdir = tempfile::tempdir().unwrap();
    let db_path = tempdir.path().join("qobuz.sqlite");
    let database_url = format!("sqlite:{}", db_path.display());
    let handle = connect_database(&database_url).await.unwrap();
    migrations::migrate(&handle).await.unwrap();
    (tempdir, handle)
}

#[tokio::test]
async fn accounts_upsert_lookup_list_and_delete_preserve_secret_boundary() {
    let handle = migrated_handle().await;
    let obtained = Utc::now();

    let first = qobuz::upsert_after_oauth(
        &handle,
        1001,
        "encrypted-token-v1",
        Some("Alice"),
        Some("Studio"),
        obtained,
        Some(obtained + Duration::days(30)),
    )
    .await
    .unwrap();
    let second = qobuz::upsert_after_oauth(
        &handle,
        1002,
        "encrypted-token-other",
        None,
        None,
        obtained,
        None,
    )
    .await
    .unwrap();
    let updated = qobuz::upsert_after_oauth(
        &handle,
        1001,
        "encrypted-token-v2",
        Some("Alice Updated"),
        Some("Sublime"),
        obtained + Duration::hours(1),
        None,
    )
    .await
    .unwrap();

    assert_eq!(updated, first);
    let record = qobuz::find_by_qobuz_user_id(&handle, 1001)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(record.id, first);
    assert_eq!(record.uat_encrypted, "encrypted-token-v2");
    assert_eq!(record.display_name.as_deref(), Some("Alice Updated"));
    assert!(record.uat_expires_at.is_none());

    let listed = qobuz::list_without_uat(&handle).await.unwrap();
    assert_eq!(
        listed.iter().map(|account| account.id).collect::<Vec<_>>(),
        [first, second]
    );
    assert_eq!(listed[0].qobuz_user_id, 1001);
    assert_eq!(listed[0].display_name.as_deref(), Some("Alice Updated"));

    assert!(qobuz::delete_by_id(&handle, first).await.unwrap());
    assert!(qobuz::get_by_id(&handle, first).await.unwrap().is_none());
    assert!(!qobuz::delete_by_id(&handle, first).await.unwrap());
}

#[tokio::test]
async fn oauth_states_purge_and_consume_only_valid_pending_rows_once() {
    let handle = migrated_handle().await;
    let now = Utc::now();

    qobuz::insert_oauth_state(&handle, "expired", now - Duration::minutes(5))
        .await
        .unwrap();
    qobuz::insert_oauth_state(&handle, "only", now + Duration::minutes(5))
        .await
        .unwrap();
    qobuz::purge_expired_oauth_states(&handle).await.unwrap();
    assert_eq!(
        qobuz::consume_sole_pending_oauth_state(&handle)
            .await
            .unwrap()
            .as_deref(),
        Some("only")
    );
    assert!(!qobuz::consume_oauth_state(&handle, "only").await.unwrap());

    qobuz::insert_oauth_state(&handle, "first", now + Duration::minutes(5))
        .await
        .unwrap();
    qobuz::insert_oauth_state(&handle, "second", now + Duration::minutes(5))
        .await
        .unwrap();
    assert!(
        qobuz::consume_sole_pending_oauth_state(&handle)
            .await
            .unwrap()
            .is_none()
    );
    assert!(qobuz::consume_oauth_state(&handle, "first").await.unwrap());
    assert!(!qobuz::consume_oauth_state(&handle, "first").await.unwrap());
    assert!(qobuz::consume_oauth_state(&handle, "second").await.unwrap());
    assert!(
        !qobuz::consume_oauth_state(&handle, "expired")
            .await
            .unwrap()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn consume_oauth_state_allows_only_one_concurrent_success() {
    let (_tempdir, handle) = migrated_file_handle().await;
    let state = "concurrent-oauth-state";

    qobuz::insert_oauth_state(&handle, state, Utc::now() + Duration::minutes(5))
        .await
        .unwrap();

    let barrier = Arc::new(Barrier::new(8));
    let tasks = (0..8)
        .map(|_| {
            let barrier = Arc::clone(&barrier);
            let handle = handle.clone();
            tokio::spawn(async move {
                barrier.wait();
                qobuz::consume_oauth_state(&handle, state).await.unwrap()
            })
        })
        .collect::<Vec<_>>();

    let mut successes = 0;
    for task in tasks {
        if task.await.unwrap() {
            successes += 1;
        }
    }

    assert_eq!(successes, 1);
}

#[tokio::test]
async fn sync_run_lifecycle_reports_latest_status_and_counters() {
    let handle = migrated_handle().await;

    let first = qobuz::start_sync_run(&handle).await.unwrap();
    let second = qobuz::start_sync_run(&handle).await.unwrap();
    let running = qobuz::sync_latest(&handle).await.unwrap().unwrap();
    assert_eq!(running.id, second);
    assert_eq!(running.status, "running");

    qobuz::finish_sync_success(&handle, first, 10, 7, 3)
        .await
        .unwrap();
    let first_done = qobuz::get_sync_run_by_id(&handle, first)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first_done.status, "success");
    assert_eq!(first_done.albums_total, Some(10));
    assert_eq!(first_done.albums_added, Some(7));
    assert_eq!(first_done.albums_removed, Some(3));
    assert!(first_done.finished_at.is_some());

    qobuz::finish_sync_failed(&handle, second, "network")
        .await
        .unwrap();
    let latest = qobuz::sync_latest(&handle).await.unwrap().unwrap();
    assert_eq!(latest.id, second);
    assert_eq!(latest.status, "failed");
    assert_eq!(latest.error_message.as_deref(), Some("network"));
}
