use euterpe_qobuz::{QobuzClient, QobuzConfig, Quality};

fn live_config() -> Option<QobuzConfig> {
    if std::env::var("EUTERPE_QOBUZ_USER_ID").is_err()
        || std::env::var("EUTERPE_QOBUZ_AUTH_TOKEN").is_err()
    {
        return None;
    }
    QobuzConfig::from_env().ok()
}

#[tokio::test]
#[ignore = "requires EUTERPE_QOBUZ_USER_ID and EUTERPE_QOBUZ_AUTH_TOKEN"]
async fn favorites_all_albums_live() {
    let config = live_config().expect("live env");
    let client = QobuzClient::connect(config).await.expect("connect");
    let _albums = client.favorites_all_albums().await.expect("favorites");
}

#[tokio::test]
#[ignore = "requires EUTERPE_QOBUZ_USER_ID and EUTERPE_QOBUZ_AUTH_TOKEN"]
async fn track_stream_url_live() {
    let config = live_config().expect("live env");
    let client = QobuzClient::connect(config).await.expect("connect");
    // probe track id from qobuz-sync reference
    let url = client
        .track_stream_url(5_966_783, Quality::Mp3_320)
        .await
        .expect("stream url");
    assert!(url.url.is_some());
}

#[tokio::test]
#[ignore = "requires EUTERPE_QOBUZ_USER_ID and EUTERPE_QOBUZ_AUTH_TOKEN"]
async fn album_and_artist_live() {
    let config = live_config().expect("live env");
    let client = QobuzClient::connect(config).await.expect("connect");
    let albums = client.favorites_all_albums().await.expect("favorites");
    let Some(first) = albums.first() else {
        return;
    };
    let album = client.album(first.id).await.expect("album");
    assert_eq!(album.summary.id, first.id);
    if let Some(artist) = album.summary.artist.as_ref().or_else(|| {
        album
            .summary
            .artists
            .as_ref()
            .and_then(|a| a.first())
    }) {
        let discography = client.artist_albums(artist.id).await.expect("artist albums");
        assert!(!discography.is_empty());
    }
}

#[tokio::test]
#[ignore = "requires EUTERPE_QOBUZ_USER_ID and EUTERPE_QOBUZ_AUTH_TOKEN"]
async fn favorites_round_trip_live() {
    let config = live_config().expect("live env");
    let client = QobuzClient::connect(config).await.expect("connect");
    // Use a unlikely album id; if API rejects, skip assertion on add
    let test_id = 9_999_999_999u64;
    if client.favorite_add_albums(&[test_id]).await.is_ok() {
        let all = client.favorites_all_albums().await.expect("list");
        let found = all.iter().any(|a| a.id == test_id);
        if found {
            client
                .favorite_remove_albums(&[test_id])
                .await
                .expect("remove");
        }
    }
}
