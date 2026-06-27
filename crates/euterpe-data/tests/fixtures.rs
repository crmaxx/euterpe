use euterpe_data::fixtures::{catalog, integrations, settings};
use euterpe_data::{connect_database, migrations};

#[tokio::test]
async fn catalog_fixtures_seed_album_and_track() {
    let data = connect_database("sqlite::memory:").await.unwrap();
    migrations::migrate(&data).await.unwrap();

    let (album_id, track_id) = catalog::seed_album_with_track(
        &data,
        catalog::AlbumFixture {
            title: "Fixture Album".to_string(),
            path: Some("Fixture Artist/Fixture Album".to_string()),
            ..Default::default()
        },
        |album_id| catalog::TrackFixture {
            title: "Fixture Track".to_string(),
            track_number: Some(1),
            path: "Fixture Artist/Fixture Album/01 - Fixture Track.flac".to_string(),
            ..catalog::TrackFixture::for_album(album_id, "")
        },
    )
    .await
    .unwrap();

    let album = catalog::album(&data, album_id).await.unwrap().unwrap();
    let track = catalog::track(&data, track_id).await.unwrap().unwrap();

    assert_eq!(album.title, "Fixture Album");
    assert_eq!(track.title, "Fixture Track");
    assert_eq!(track.track_number, Some(1));
}

#[tokio::test]
async fn settings_fixture_round_trips_values() {
    let data = connect_database("sqlite::memory:").await.unwrap();
    migrations::migrate(&data).await.unwrap();

    settings::set(&data, "storage.settings", "{\"library\":null}")
        .await
        .unwrap();

    assert_eq!(
        settings::get(&data, "storage.settings")
            .await
            .unwrap()
            .as_deref(),
        Some("{\"library\":null}")
    );
}

#[tokio::test]
async fn integration_fixture_seeds_row() {
    let data = connect_database("sqlite::memory:").await.unwrap();
    migrations::migrate(&data).await.unwrap();

    let id = integrations::seed_integration(&data, integrations::IntegrationFixture::default())
        .await
        .unwrap();
    let row = euterpe_data::repositories::integrations::get_by_id(&data, id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(row.provider, "musicbrainz");
    assert_eq!(row.enabled, 1);
}
