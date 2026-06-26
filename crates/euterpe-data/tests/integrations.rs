use euterpe_data::repositories::integrations::{self, IntegrationInsert, IntegrationUpdate};
use euterpe_data::{connect_database, migrations};

async fn migrated_handle() -> euterpe_data::DataHandle {
    let handle = connect_database("sqlite::memory:").await.unwrap();
    migrations::migrate(&handle).await.unwrap();
    handle
}

#[tokio::test]
async fn integrations_list_orders_by_sort_order_then_id_and_filters_by_type() {
    let handle = migrated_handle().await;

    let later = integrations::insert(
        &handle,
        IntegrationInsert {
            type_: "tag_source",
            provider: "musicbrainz",
            display_name: "MusicBrainz",
            enabled: true,
            config_json: r#"{"contact":"a@example.test"}"#,
            config_secrets_enc: None,
            sort_order: 10,
        },
    )
    .await
    .unwrap();
    let earlier = integrations::insert(
        &handle,
        IntegrationInsert {
            type_: "tag_source",
            provider: "discogs",
            display_name: "Discogs",
            enabled: false,
            config_json: "{}",
            config_secrets_enc: Some("encrypted-secret"),
            sort_order: 0,
        },
    )
    .await
    .unwrap();

    let rows = integrations::list(&handle, None).await.unwrap();
    assert_eq!(
        rows.iter().map(|row| row.id).collect::<Vec<_>>(),
        [earlier, later]
    );
    assert_eq!(
        integrations::list(&handle, Some("tag_source"))
            .await
            .unwrap()
            .len(),
        2
    );
    assert!(
        integrations::list(&handle, Some("unknown"))
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        integrations::max_sort_order(&handle, "tag_source")
            .await
            .unwrap(),
        11
    );
    assert_eq!(
        integrations::max_sort_order(&handle, "unknown")
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn integrations_update_preserves_clears_and_replaces_secrets() {
    let handle = migrated_handle().await;
    let id = integrations::insert(
        &handle,
        IntegrationInsert {
            type_: "tag_source",
            provider: "tracktype",
            display_name: "TrackType",
            enabled: true,
            config_json: "{}",
            config_secrets_enc: Some("encrypted-v1"),
            sort_order: 1,
        },
    )
    .await
    .unwrap();

    integrations::update(
        &handle,
        id,
        IntegrationUpdate {
            display_name: Some("TrackType Updated"),
            enabled: Some(false),
            config_json: Some(r#"{"api_base":"https://tracktype.test"}"#),
            config_secrets_enc: None,
            sort_order: Some(4),
        },
    )
    .await
    .unwrap();
    let preserved = integrations::get_by_id(&handle, id).await.unwrap().unwrap();
    assert_eq!(preserved.display_name, "TrackType Updated");
    assert_eq!(preserved.enabled, 0);
    assert_eq!(
        preserved.config_secrets_enc.as_deref(),
        Some("encrypted-v1")
    );
    assert_eq!(preserved.sort_order, 4);

    integrations::update(
        &handle,
        id,
        IntegrationUpdate {
            display_name: None,
            enabled: None,
            config_json: None,
            config_secrets_enc: Some(None),
            sort_order: None,
        },
    )
    .await
    .unwrap();
    assert!(
        integrations::get_by_id(&handle, id)
            .await
            .unwrap()
            .unwrap()
            .config_secrets_enc
            .is_none()
    );

    integrations::update(
        &handle,
        id,
        IntegrationUpdate {
            display_name: None,
            enabled: None,
            config_json: None,
            config_secrets_enc: Some(Some("encrypted-v2".to_string())),
            sort_order: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        integrations::get_by_id(&handle, id)
            .await
            .unwrap()
            .unwrap()
            .config_secrets_enc
            .as_deref(),
        Some("encrypted-v2")
    );
}

#[tokio::test]
async fn integrations_delete_reports_whether_a_row_was_removed() {
    let handle = migrated_handle().await;
    let id = integrations::insert(
        &handle,
        IntegrationInsert {
            type_: "tag_source",
            provider: "gnudb",
            display_name: "GnuDB",
            enabled: true,
            config_json: "{}",
            config_secrets_enc: None,
            sort_order: 0,
        },
    )
    .await
    .unwrap();

    assert!(integrations::delete(&handle, id).await.unwrap());
    assert!(
        integrations::get_by_id(&handle, id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(!integrations::delete(&handle, id).await.unwrap());
}
