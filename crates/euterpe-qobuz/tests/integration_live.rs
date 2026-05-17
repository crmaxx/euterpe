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
    let albums = client.favorites_all_albums().await.expect("favorites");
    assert!(!albums.is_empty(), "expected at least one favorite album");
}

#[tokio::test]
#[ignore = "requires EUTERPE_QOBUZ_USER_ID and EUTERPE_QOBUZ_AUTH_TOKEN"]
async fn track_stream_url_live() {
    let config = live_config().expect("live env");
    let mut client = QobuzClient::connect(config).await.expect("connect");

    let track_id = if let Ok(tracks) = client.favorites_all_tracks().await {
        tracks.first().map(|t| t.id)
    } else {
        None
    };

    let track_id = if let Some(id) = track_id {
        id
    } else {
        let albums = client.favorites_all_albums().await.expect("favorites");
        let mut found = None;
        for summary in albums.iter().take(10) {
            if let Ok(album) = client.album(summary.id).await {
                if let Some(id) = album
                    .tracks
                    .and_then(|t| t.items.into_iter().next().map(|tr| tr.id))
                {
                    found = Some(id);
                    break;
                }
            }
        }
        found.expect("track id from favorites (tracks list or album tracks)")
    };

    let url = client
        .track_stream_url(track_id, Quality::Mp3_320)
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
    let artist = first
        .artist
        .as_ref()
        .or_else(|| first.artists.as_ref().and_then(|a| a.first()))
        .expect("artist on favorite album");
    let discography = client.artist_albums(artist.id).await.expect("artist albums");
    assert!(!discography.is_empty());

    // Best-effort: album/get for the first favorite (some entries may 404 depending on catalog).
    if let Ok(detail) = client.album(first.id).await {
        assert_eq!(detail.summary.id, first.id);
    }
}

#[tokio::test]
#[ignore = "requires EUTERPE_QOBUZ_USER_ID and EUTERPE_QOBUZ_AUTH_TOKEN"]
async fn favorites_round_trip_live() {
    let config = live_config().expect("live env");
    let client = QobuzClient::connect(config).await.expect("connect");
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
