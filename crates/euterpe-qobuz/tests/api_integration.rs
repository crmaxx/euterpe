use euterpe_qobuz::{FavoritesAlbumsResponse, QobuzClient, QobuzConfig, QobuzError, Quality};

#[test]
fn deserialize_favorites_albums_page0() {
    let json = include_str!("fixtures/favorites_albums_page0.json");
    let resp: FavoritesAlbumsResponse = serde_json::from_str(json).unwrap();
    assert_eq!(resp.albums.items.len(), 1);
    assert_eq!(resp.albums.items[0].title, "Test Album");
}

#[tokio::test]
async fn favorites_album_api_id_for_catalog_mock() {
    let body = r#"{
        "albums": {
            "total": 1,
            "limit": 500,
            "offset": 0,
            "items": [{
                "id": "zg7pv28g4mldg",
                "qobuz_id": 393908828,
                "title": "Lutosławski",
                "slug": "lutosawski-concertos"
            }]
        }
    }"#;
    let mut server = mockito::Server::new_async().await;
    let _m = server
        .mock(
            "GET",
            mockito::Matcher::Regex(r".*/favorite/getUserFavorites.*".into()),
        )
        .with_status(200)
        .with_body(body)
        .create_async()
        .await;

    let mut cfg = QobuzConfig::session_token(1, "uat");
    cfg.api_base = format!("{}/api.json/0.2", server.url());
    let client = QobuzClient::new_for_test(cfg, "123456789".into(), vec!["secret".into()]);

    let api_id = client
        .favorites_album_api_id_for_catalog(393908828)
        .await
        .unwrap();
    assert_eq!(api_id.as_deref(), Some("zg7pv28g4mldg"));
}

#[tokio::test]
async fn favorites_list_mock() {
    let mut server = mockito::Server::new_async().await;
    let body = include_str!("fixtures/favorites_albums_page0.json");
    let _m = server
        .mock(
            "GET",
            mockito::Matcher::Regex(r".*/favorite/getUserFavorites.*".into()),
        )
        .with_status(200)
        .with_body(body)
        .create_async()
        .await;

    let mut cfg = QobuzConfig::session_token(1, "uat");
    cfg.api_base = format!("{}/api.json/0.2", server.url());
    let client = QobuzClient::new_for_test(cfg, "123456789".into(), vec!["secret".into()]);

    let page = client
        .favorites_albums(euterpe_qobuz::PageRequest::default())
        .await
        .unwrap();
    assert_eq!(page.items.len(), 1);
}

#[tokio::test]
async fn favorites_fetch_all_two_pages() {
    let mut server = mockito::Server::new_async().await;
    let page0 = include_str!("fixtures/favorites_albums_page0_p2.json");
    let page1 = include_str!("fixtures/favorites_albums_page1.json");

    let _m = server
        .mock(
            "GET",
            mockito::Matcher::Regex(r".*getUserFavorites.*offset=0.*".into()),
        )
        .with_status(200)
        .with_body(page0)
        .create_async()
        .await;
    let _m2 = server
        .mock(
            "GET",
            mockito::Matcher::Regex(r".*getUserFavorites.*offset=2.*".into()),
        )
        .with_status(200)
        .with_body(page1)
        .create_async()
        .await;

    let mut cfg = QobuzConfig::session_token(1, "uat");
    cfg.api_base = format!("{}/api.json/0.2", server.url());
    let client = QobuzClient::new_for_test(cfg, "123456789".into(), vec!["secret".into()]);

    let all = client.favorites_all_albums().await.unwrap();
    assert_eq!(all.len(), 3);
}

#[tokio::test]
async fn favorite_create_delete_mock() {
    let mut server = mockito::Server::new_async().await;

    let _create = server
        .mock(
            "GET",
            mockito::Matcher::Regex(r".*favorite/create.*album_ids=1%2C2.*".into()),
        )
        .with_status(200)
        .with_body("{}")
        .create_async()
        .await;

    let _delete = server
        .mock(
            "GET",
            mockito::Matcher::Regex(r".*favorite/delete.*album_ids=42.*".into()),
        )
        .with_status(200)
        .with_body("{}")
        .create_async()
        .await;

    let mut cfg = QobuzConfig::session_token(1, "uat");
    cfg.api_base = format!("{}/api.json/0.2", server.url());
    let client = QobuzClient::new_for_test(cfg, "123456789".into(), vec!["secret".into()]);

    client.favorite_add_albums(&[1, 2]).await.unwrap();
    client.favorite_remove_albums(&[42]).await.unwrap();
}

#[tokio::test]
async fn track_stream_url_mock() {
    let mut server = mockito::Server::new_async().await;
    let body = include_str!("fixtures/get_file_url_flac.json");
    let _m = server
        .mock(
            "GET",
            mockito::Matcher::Regex(r".*/track/getFileUrl.*".into()),
        )
        .with_status(200)
        .with_body(body)
        .create_async()
        .await;

    let mut cfg = QobuzConfig::session_token(1, "uat");
    cfg.api_base = format!("{}/api.json/0.2", server.url());
    let mut client = QobuzClient::new_for_test(cfg, "123456789".into(), vec!["secret".into()]);

    let stream = client.track_stream_url(123, Quality::FlacCd).await.unwrap();
    assert_eq!(
        stream.url.as_deref(),
        Some("https://stream.qobuz.com/sample.flac")
    );
}

#[tokio::test]
async fn track_stream_url_restrictions() {
    let mut server = mockito::Server::new_async().await;
    let body = include_str!("fixtures/get_file_url_restricted.json");
    let _m = server
        .mock(
            "GET",
            mockito::Matcher::Regex(r".*/track/getFileUrl.*".into()),
        )
        .with_status(200)
        .with_body(body)
        .create_async()
        .await;

    let mut cfg = QobuzConfig::session_token(1, "uat");
    cfg.api_base = format!("{}/api.json/0.2", server.url());
    let mut client = QobuzClient::new_for_test(cfg, "123456789".into(), vec!["secret".into()]);

    let err = client
        .track_stream_url(1, Quality::Mp3_320)
        .await
        .unwrap_err();
    assert!(matches!(err, QobuzError::NonStreamable(_)));
}

#[tokio::test]
async fn album_get_fixture() {
    let json = include_str!("fixtures/album_get.json");
    let album: euterpe_qobuz::AlbumDetail = serde_json::from_str(json).unwrap();
    assert_eq!(album.summary.title, "Catalog Album");
    assert_eq!(album.tracks.as_ref().unwrap().items.len(), 1);
}

#[test]
fn album_get_string_id_fixture() {
    let json = r#"{
        "id": "zg7pv28g4mldg",
        "qobuz_id": 393908828,
        "title": "Lutosławski",
        "tracks": { "items": [{ "id": 1, "title": "T", "track_number": 1, "duration": 1 }] }
    }"#;
    let album: euterpe_qobuz::AlbumDetail = serde_json::from_str(json).unwrap();
    assert_eq!(album.summary.id, 393908828);
    assert_eq!(album.summary.album_ref.as_deref(), Some("zg7pv28g4mldg"));
    assert_eq!(album.summary.api_album_id(), "zg7pv28g4mldg");
}

#[tokio::test]
async fn album_get_mock() {
    let mut server = mockito::Server::new_async().await;
    let body = include_str!("fixtures/album_get.json");
    let _m = server
        .mock("GET", mockito::Matcher::Regex(r".*/album/get.*".into()))
        .with_status(200)
        .with_body(body)
        .create_async()
        .await;

    let mut cfg = QobuzConfig::session_token(1, "uat");
    cfg.api_base = format!("{}/api.json/0.2", server.url());
    let client = QobuzClient::new_for_test(cfg, "123456789".into(), vec!["secret".into()]);

    let album = client.album(5001).await.unwrap();
    assert_eq!(album.summary.id, 5001);
}

#[tokio::test]
async fn artist_albums_two_pages() {
    let mut server = mockito::Server::new_async().await;
    let p0 = include_str!("fixtures/artist_get_page0.json");
    let p1 = include_str!("fixtures/artist_get_page1.json");

    let _m0 = server
        .mock(
            "GET",
            mockito::Matcher::Regex(r".*artist/get.*offset=0.*".into()),
        )
        .with_status(200)
        .with_body(p0)
        .create_async()
        .await;
    let _m1 = server
        .mock(
            "GET",
            mockito::Matcher::Regex(r".*artist/get.*offset=2.*".into()),
        )
        .with_status(200)
        .with_body(p1)
        .create_async()
        .await;

    let mut cfg = QobuzConfig::session_token(1, "uat");
    cfg.api_base = format!("{}/api.json/0.2", server.url());
    let client = QobuzClient::new_for_test(cfg, "123456789".into(), vec!["secret".into()]);

    let albums = client.artist_albums(77).await.unwrap();
    assert_eq!(albums.len(), 3);
}
