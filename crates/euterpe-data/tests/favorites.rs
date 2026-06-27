use euterpe_data::repositories::catalog::{self, AlbumUpsert};
use euterpe_data::repositories::favorites::{self, FavoritesListParams, FavoritesSort, SortOrder};
use euterpe_data::{connect_database, migrations};

async fn migrated_handle() -> euterpe_data::DataHandle {
    let handle = connect_database("sqlite::memory:").await.unwrap();
    migrations::migrate(&handle).await.unwrap();
    handle
}

#[tokio::test]
async fn album_upsert_reactivates_and_preserves_existing_cover_when_new_cover_missing() {
    let handle = migrated_handle().await;

    assert!(
        favorites::upsert_album(
            &handle,
            10,
            "Original",
            "Artist",
            Some("album-slug"),
            Some("https://example.test/cover.jpg"),
        )
        .await
        .unwrap()
    );
    assert_eq!(
        favorites::mark_removed_except(&handle, &[]).await.unwrap(),
        1
    );
    assert!(favorites::album_meta(&handle, 10).await.unwrap().is_none());

    assert!(
        favorites::upsert_album(&handle, 10, "Updated", "Artist", Some(""), None)
            .await
            .unwrap()
    );
    let meta = favorites::album_meta(&handle, 10).await.unwrap().unwrap();
    assert!(meta.slug.is_none());
    assert_eq!(meta.title, "Updated");

    let page = favorites::list_albums_keyset(
        &handle,
        FavoritesListParams {
            sort: FavoritesSort::Title,
            order: SortOrder::Asc,
            limit: 10,
            q: None,
            in_library: None,
            after: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        page.items[0].cover_url.as_deref(),
        Some("https://example.test/cover.jpg")
    );
}

#[tokio::test]
async fn list_albums_keyset_filters_sorts_and_joins_local_album_rows() {
    let handle = migrated_handle().await;
    let artist_id = catalog::upsert_artist_by_name(&handle, "Local Artist", None)
        .await
        .unwrap();
    let local_album = catalog::upsert_album(
        &handle,
        AlbumUpsert {
            artist_id: Some(artist_id),
            title: "Local Alpha",
            year: Some(2024),
            qobuz_album_id: Some(20),
            path: Some("Local/Alpha"),
            cover_path: Some("covers/alpha.jpg"),
        },
    )
    .await
    .unwrap();

    for (id, title, artist, slug) in [
        (30, "Beta", "Remote Artist", Some("beta-slug")),
        (20, "Alpha", "Local Artist", Some("alpha-slug")),
        (10, "Zeta", "Remote Artist", None),
        (40, "Removed", "Remote Artist", None),
    ] {
        favorites::upsert_album(&handle, id, title, artist, slug, None)
            .await
            .unwrap();
    }
    favorites::mark_albums_removed(&handle, &[40])
        .await
        .unwrap();

    let first = favorites::list_albums_keyset(
        &handle,
        FavoritesListParams {
            sort: FavoritesSort::Title,
            order: SortOrder::Asc,
            limit: 2,
            q: None,
            in_library: None,
            after: None,
        },
    )
    .await
    .unwrap();
    assert!(first.has_more);
    assert_eq!(
        first
            .items
            .iter()
            .map(|album| album.title.as_str())
            .collect::<Vec<_>>(),
        ["Alpha", "Beta"]
    );
    assert_eq!(first.items[0].local_album_id, Some(local_album));
    assert_eq!(
        first.items[0].local_cover_path.as_deref(),
        Some("covers/alpha.jpg")
    );

    let second = favorites::list_albums_keyset(
        &handle,
        FavoritesListParams {
            sort: FavoritesSort::Title,
            order: SortOrder::Asc,
            limit: 2,
            q: None,
            in_library: None,
            after: first.next_after,
        },
    )
    .await
    .unwrap();
    assert!(!second.has_more);
    assert_eq!(second.items[0].qobuz_id, 10);
    assert_eq!(second.items[0].album_api_id, "10");

    let local_only = favorites::list_albums_keyset(
        &handle,
        FavoritesListParams {
            sort: FavoritesSort::InLibrary,
            order: SortOrder::Desc,
            limit: 10,
            q: Some("local".to_string()),
            in_library: Some(true),
            after: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(local_only.items.len(), 1);
    assert_eq!(local_only.items[0].qobuz_id, 20);
}

#[tokio::test]
async fn removed_markers_and_active_album_ids_track_sync_deletions() {
    let handle = migrated_handle().await;

    for id in [1, 2, 3] {
        favorites::upsert_album(&handle, id, &format!("Album {id}"), "Artist", None, None)
            .await
            .unwrap();
    }
    assert_eq!(
        favorites::mark_removed_except(&handle, &[1, 3])
            .await
            .unwrap(),
        1
    );
    assert_eq!(favorites::active_album_ids(&handle).await.unwrap(), [1, 3]);

    favorites::mark_albums_removed(&handle, &[3]).await.unwrap();
    assert_eq!(favorites::active_album_ids(&handle).await.unwrap(), [1]);

    favorites::upsert_album(&handle, 2, "Album 2", "Artist", None, None)
        .await
        .unwrap();
    assert_eq!(favorites::active_album_ids(&handle).await.unwrap(), [1, 2]);
    assert!(FavoritesSort::parse("invalid").is_err());
}
