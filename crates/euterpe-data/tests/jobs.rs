use euterpe_data::fixtures::jobs::seed_album;
use euterpe_data::repositories::{convert_jobs, cue_jobs, download_jobs};
use euterpe_data::{DataHandle, connect_database, migrations};
use serde::{Deserialize, Serialize};
use serde_json::json;

async fn migrated_handle() -> DataHandle {
    let handle = connect_database("sqlite::memory:").await.unwrap();
    migrations::migrate(&handle).await.unwrap();
    handle
}

async fn migrated_file_handle() -> (tempfile::TempDir, DataHandle) {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("jobs.sqlite");
    let handle = connect_database(&format!("sqlite:{}", db_path.display()))
        .await
        .unwrap();
    migrations::migrate(&handle).await.unwrap();
    (temp, handle)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn download_claim_running_has_one_winner_for_concurrent_claims() {
    let (_temp, handle) = migrated_file_handle().await;
    let id = download_jobs::insert_queued(
        &handle,
        download_jobs::DownloadJobType::Album,
        Some(1),
        6,
        None::<&serde_json::Value>,
    )
    .await
    .unwrap();
    let contender = handle.clone();

    let (first, second) = tokio::join!(
        download_jobs::claim_running(&handle, id),
        download_jobs::claim_running(&contender, id)
    );

    let winners = [first.unwrap(), second.unwrap()]
        .into_iter()
        .filter(|claimed| *claimed)
        .count();
    assert_eq!(winners, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn convert_claim_running_has_one_winner_for_concurrent_claims() {
    let (_temp, handle) = migrated_file_handle().await;
    let album_id = seed_album(&handle).await.unwrap();
    let id = convert_jobs::create(&handle, album_id, convert_jobs::ConvertTrigger::Manual, 1)
        .await
        .unwrap();
    let contender = handle.clone();

    let (first, second) = tokio::join!(
        convert_jobs::claim_running(&handle, id),
        convert_jobs::claim_running(&contender, id)
    );

    let winners = [first.unwrap(), second.unwrap()]
        .into_iter()
        .filter(|claimed| *claimed)
        .count();
    assert_eq!(winners, 1);
}

#[tokio::test]
async fn download_queue_priority_swaps_within_job_type() {
    let handle = migrated_handle().await;
    let first = download_jobs::insert_queued(
        &handle,
        download_jobs::DownloadJobType::Album,
        Some(1),
        6,
        None::<&serde_json::Value>,
    )
    .await
    .unwrap();
    let second = download_jobs::insert_queued(
        &handle,
        download_jobs::DownloadJobType::Album,
        Some(2),
        6,
        None::<&serde_json::Value>,
    )
    .await
    .unwrap();
    let torrent = download_jobs::insert_queued(
        &handle,
        download_jobs::DownloadJobType::Torrent,
        None,
        0,
        None::<&serde_json::Value>,
    )
    .await
    .unwrap();

    let first_before = download_jobs::get_by_id(&handle, first)
        .await
        .unwrap()
        .unwrap()
        .queue_position;
    let second_before = download_jobs::get_by_id(&handle, second)
        .await
        .unwrap()
        .unwrap()
        .queue_position;
    assert!(first_before < second_before);

    download_jobs::adjust_queue_priority(&handle, second, download_jobs::PriorityDirection::Up)
        .await
        .unwrap();

    let first_after = download_jobs::get_by_id(&handle, first)
        .await
        .unwrap()
        .unwrap()
        .queue_position;
    let second_after = download_jobs::get_by_id(&handle, second)
        .await
        .unwrap()
        .unwrap()
        .queue_position;
    assert_eq!(first_after, second_before);
    assert_eq!(second_after, first_before);
    assert_eq!(
        download_jobs::next_queued_id(&handle, download_jobs::DownloadJobType::Album)
            .await
            .unwrap(),
        Some(second)
    );
    assert_eq!(
        download_jobs::next_queued_id(&handle, download_jobs::DownloadJobType::Torrent)
            .await
            .unwrap(),
        Some(torrent)
    );

    let mut album_positions = Vec::new();
    for id in [first, second] {
        album_positions.push(
            download_jobs::get_by_id(&handle, id)
                .await
                .unwrap()
                .unwrap()
                .queue_position,
        );
    }
    album_positions.sort_unstable();
    album_positions.dedup();
    assert_eq!(album_positions.len(), 2);
}

#[tokio::test]
async fn download_lifecycle_validation_errors_are_not_configuration_errors() {
    let handle = migrated_handle().await;
    let queued = download_jobs::insert_queued(
        &handle,
        download_jobs::DownloadJobType::Album,
        Some(1),
        6,
        None::<&serde_json::Value>,
    )
    .await
    .unwrap();

    let resume_error = download_jobs::resume_paused(&handle, queued)
        .await
        .unwrap_err();
    assert!(
        !resume_error.to_string().contains("configuration"),
        "{resume_error}"
    );

    let retry_error = download_jobs::retry_failed(&handle, queued)
        .await
        .unwrap_err();
    assert!(
        !retry_error.to_string().contains("configuration"),
        "{retry_error}"
    );

    assert!(download_jobs::claim_running(&handle, queued).await.unwrap());
    let reorder_error =
        download_jobs::adjust_queue_priority(&handle, queued, download_jobs::PriorityDirection::Up)
            .await
            .unwrap_err();
    assert!(
        !reorder_error.to_string().contains("configuration"),
        "{reorder_error}"
    );

    download_jobs::finish_success(&handle, queued)
        .await
        .unwrap();
    let pause_error = download_jobs::pause(&handle, queued).await.unwrap_err();
    assert!(
        !pause_error.to_string().contains("configuration"),
        "{pause_error}"
    );
}

#[tokio::test]
async fn download_lifecycle_transitions_preserve_current_rules() {
    use download_jobs::DownloadJobStatus::{Cancelled, Completed, Failed, Paused, Queued, Running};

    assert!(download_jobs::can_transition(Paused, Queued));
    assert!(download_jobs::can_transition(Paused, Cancelled));
    assert!(!download_jobs::can_transition(Cancelled, Queued));
    assert!(!download_jobs::can_transition(Failed, Queued));
    assert!(!download_jobs::can_transition(Completed, Running));

    let handle = migrated_handle().await;
    let paused = download_jobs::insert_queued(
        &handle,
        download_jobs::DownloadJobType::Album,
        Some(1),
        6,
        None::<&serde_json::Value>,
    )
    .await
    .unwrap();
    download_jobs::pause(&handle, paused).await.unwrap();
    assert!(download_jobs::is_paused(&handle, paused).await.unwrap());
    download_jobs::resume_paused(&handle, paused).await.unwrap();
    assert_eq!(
        download_jobs::get_by_id(&handle, paused)
            .await
            .unwrap()
            .unwrap()
            .status,
        Queued
    );

    let cancelled = download_jobs::insert_queued(
        &handle,
        download_jobs::DownloadJobType::Album,
        Some(2),
        6,
        None::<&serde_json::Value>,
    )
    .await
    .unwrap();
    assert!(download_jobs::cancel(&handle, cancelled).await.unwrap());
    assert!(
        download_jobs::is_cancelled(&handle, cancelled)
            .await
            .unwrap()
    );

    let failed = download_jobs::insert_queued(
        &handle,
        download_jobs::DownloadJobType::Album,
        Some(3),
        6,
        None::<&serde_json::Value>,
    )
    .await
    .unwrap();
    assert!(download_jobs::claim_running(&handle, failed).await.unwrap());
    download_jobs::finish_failed(&handle, failed, "network")
        .await
        .unwrap();
    assert_eq!(
        download_jobs::get_by_id(&handle, failed)
            .await
            .unwrap()
            .unwrap()
            .status,
        Failed
    );

    let completed = download_jobs::insert_queued(
        &handle,
        download_jobs::DownloadJobType::Album,
        Some(4),
        6,
        None::<&serde_json::Value>,
    )
    .await
    .unwrap();
    assert!(
        download_jobs::claim_running(&handle, completed)
            .await
            .unwrap()
    );
    download_jobs::finish_success(&handle, completed)
        .await
        .unwrap();
    assert_eq!(
        download_jobs::get_by_id(&handle, completed)
            .await
            .unwrap()
            .unwrap()
            .status,
        Completed
    );
}

#[tokio::test]
async fn active_album_job_check_matches_queued_running_and_paused_work() {
    let handle = migrated_handle().await;
    let payload = json!({"album_api_id":"album-api-1"});
    let queued = download_jobs::insert_queued(
        &handle,
        download_jobs::DownloadJobType::Album,
        Some(42),
        6,
        Some(&payload),
    )
    .await
    .unwrap();
    assert!(
        download_jobs::has_active_album(&handle, "album-api-1", Some(42), 6)
            .await
            .unwrap()
    );

    assert!(download_jobs::claim_running(&handle, queued).await.unwrap());
    assert!(
        download_jobs::has_active_album(&handle, "album-api-1", Some(42), 6)
            .await
            .unwrap()
    );

    download_jobs::pause(&handle, queued).await.unwrap();
    assert!(
        download_jobs::has_active_album(&handle, "album-api-1", Some(42), 6)
            .await
            .unwrap()
    );

    assert!(
        !download_jobs::has_active_album(&handle, "album-api-1", Some(42), 7)
            .await
            .unwrap()
    );

    download_jobs::cancel(&handle, queued).await.unwrap();
    assert!(
        !download_jobs::has_active_album(&handle, "album-api-1", Some(42), 6)
            .await
            .unwrap()
    );
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
struct TorrentPayload {
    display_title: Option<String>,
    torrent: TorrentPayloadDetail,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
struct TorrentPayloadDetail {
    display_name: String,
    info_hash: String,
    selected_file_indices: Vec<usize>,
    runtime: serde_json::Value,
}

#[tokio::test]
async fn download_payload_round_trips_without_json_shape_changes() {
    let handle = migrated_handle().await;
    let payload = TorrentPayload {
        display_title: Some("Torrent Album".to_string()),
        torrent: TorrentPayloadDetail {
            display_name: "Torrent Album".to_string(),
            info_hash: "abc123".to_string(),
            selected_file_indices: vec![0, 2],
            runtime: json!({
                "librqbit_state": "live",
                "progress_bytes": 42,
                "total_bytes": 100,
                "nested": { "kept": true }
            }),
        },
    };
    let id = download_jobs::insert_queued(
        &handle,
        download_jobs::DownloadJobType::Torrent,
        None,
        0,
        Some(&payload),
    )
    .await
    .unwrap();

    let stored: TorrentPayload = download_jobs::get_payload(&handle, id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored, payload);

    let updated = json!({
        "display_title": "Torrent Album",
        "torrent": {
            "display_name": "Torrent Album",
            "info_hash": "abc123",
            "selected_file_indices": [0, 2],
            "runtime": {
                "librqbit_state": "paused",
                "progress_bytes": 64,
                "total_bytes": 100,
                "nested": { "kept": true }
            }
        }
    });
    download_jobs::set_payload(&handle, id, &updated)
        .await
        .unwrap();
    let stored_value: serde_json::Value = download_jobs::get_payload(&handle, id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored_value, updated);
}

#[tokio::test]
async fn active_convert_and_cue_jobs_are_unique_per_album() {
    let handle = migrated_handle().await;
    let album_id = seed_album(&handle).await.unwrap();

    let convert_id =
        convert_jobs::create(&handle, album_id, convert_jobs::ConvertTrigger::Manual, 1)
            .await
            .unwrap();
    assert!(
        convert_jobs::album_has_active_job(&handle, album_id)
            .await
            .unwrap()
    );
    assert_eq!(
        convert_jobs::enqueue_album_if_needed(&handle, album_id, 1)
            .await
            .unwrap(),
        None
    );
    assert!(
        convert_jobs::create(&handle, album_id, convert_jobs::ConvertTrigger::Auto, 1)
            .await
            .is_err(),
        "second active convert job should be rejected"
    );
    convert_jobs::finish(
        &handle,
        convert_id,
        convert_jobs::ConvertJobStatus::Success,
        None,
        None,
    )
    .await
    .unwrap();

    let cue_id = cue_jobs::create_queued(
        &handle,
        album_id,
        2,
        Some(&cue_jobs::CueJobPayload {
            cue_path: "Artist/Album/album.cue".to_string(),
            audio_path: "album.flac".to_string(),
            source_file_policy: "keep".to_string(),
        }),
    )
    .await
    .unwrap();
    assert!(
        cue_jobs::album_has_active_job(&handle, album_id)
            .await
            .unwrap()
    );
    assert!(
        cue_jobs::create_queued(
            &handle,
            album_id,
            2,
            Some(&cue_jobs::CueJobPayload {
                cue_path: "Artist/Album/other.cue".to_string(),
                audio_path: "other.flac".to_string(),
                source_file_policy: "keep".to_string(),
            }),
        )
        .await
        .is_err(),
        "second active CUE job should be rejected"
    );
    cue_jobs::finish_failed(&handle, cue_id, "bad cue")
        .await
        .unwrap();
    assert!(
        !cue_jobs::album_has_active_job(&handle, album_id)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn progress_and_terminal_updates_match_current_semantics() {
    let handle = migrated_handle().await;
    let album_id = seed_album(&handle).await.unwrap();

    let download_id = download_jobs::insert_queued(
        &handle,
        download_jobs::DownloadJobType::Album,
        Some(1),
        6,
        None::<&serde_json::Value>,
    )
    .await
    .unwrap();
    assert!(
        download_jobs::claim_running(&handle, download_id)
            .await
            .unwrap()
    );
    download_jobs::update_progress_and_speed(&handle, download_id, 37.5, Some(2048))
        .await
        .unwrap();
    download_jobs::finish_success(&handle, download_id)
        .await
        .unwrap();
    let download = download_jobs::get_by_id(&handle, download_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(download.status, download_jobs::DownloadJobStatus::Completed);
    assert_eq!(download.progress_pct, 100.0);
    assert_eq!(download.download_speed_bps, 2048);

    let convert_id =
        convert_jobs::create(&handle, album_id, convert_jobs::ConvertTrigger::Manual, 2)
            .await
            .unwrap();
    assert!(
        convert_jobs::claim_running(&handle, convert_id)
            .await
            .unwrap()
    );
    assert!(
        convert_jobs::update_progress(
            &handle,
            convert_id,
            1,
            2,
            50.0,
            Some(r#"[{"path":"one.flac","status":"running"}]"#),
        )
        .await
        .unwrap()
    );
    convert_jobs::finish(
        &handle,
        convert_id,
        convert_jobs::ConvertJobStatus::Success,
        None,
        Some(r#"[]"#),
    )
    .await
    .unwrap();
    assert!(
        !convert_jobs::update_progress(
            &handle,
            convert_id,
            1,
            2,
            50.0,
            Some(r#"[{"path":"late"}]"#),
        )
        .await
        .unwrap()
    );
    let convert = convert_jobs::get_by_id(&handle, convert_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(convert.status, convert_jobs::ConvertJobStatus::Success);
    assert_eq!(convert.progress_pct, 100.0);
    assert_eq!(convert.payload_json.as_deref(), Some(r#"[]"#));

    let cue_id = cue_jobs::create_queued(
        &handle,
        album_id,
        2,
        Some(&cue_jobs::CueJobPayload {
            cue_path: "Artist/Album/album.cue".to_string(),
            audio_path: "album.flac".to_string(),
            source_file_policy: "keep".to_string(),
        }),
    )
    .await
    .unwrap();
    cue_jobs::mark_running(&handle, cue_id).await.unwrap();
    cue_jobs::update_progress(&handle, cue_id, 1, 2)
        .await
        .unwrap();
    cue_jobs::finish_success(&handle, cue_id, 2).await.unwrap();
    let cue = cue_jobs::get_by_id(&handle, cue_id).await.unwrap().unwrap();
    assert_eq!(cue.status, cue_jobs::CueJobStatus::Success);
    assert_eq!(cue.tracks_done, 2);
    assert_eq!(cue.progress_pct, 100.0);
    assert!(cue.error_message.is_none());
}
