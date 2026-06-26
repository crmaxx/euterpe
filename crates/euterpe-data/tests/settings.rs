use euterpe_data::repositories::settings;
use euterpe_data::{connect_database, migrations};

#[tokio::test]
async fn settings_round_trip_insert_update_and_delete() {
    let handle = connect_database("sqlite::memory:").await.unwrap();
    migrations::migrate(&handle).await.unwrap();

    assert_eq!(settings::get(&handle, "custom.key").await.unwrap(), None);

    settings::set(&handle, "custom.key", "first").await.unwrap();
    assert_eq!(
        settings::get(&handle, "custom.key").await.unwrap(),
        Some("first".to_string())
    );

    settings::set(&handle, "custom.key", "second").await.unwrap();
    assert_eq!(
        settings::get(&handle, "custom.key").await.unwrap(),
        Some("second".to_string())
    );

    settings::delete(&handle, "custom.key").await.unwrap();
    assert_eq!(settings::get(&handle, "custom.key").await.unwrap(), None);
}

#[tokio::test]
async fn settings_preserve_seeded_values_until_overwritten() {
    let handle = connect_database("sqlite::memory:").await.unwrap();
    migrations::migrate(&handle).await.unwrap();

    let seeded = settings::get(&handle, "downloads.settings")
        .await
        .unwrap()
        .unwrap();

    assert!(seeded.contains("\"concurrency\":3"));

    settings::set(&handle, "downloads.settings", "{\"concurrency\":1}")
        .await
        .unwrap();

    assert_eq!(
        settings::get(&handle, "downloads.settings").await.unwrap(),
        Some("{\"concurrency\":1}".to_string())
    );
}
