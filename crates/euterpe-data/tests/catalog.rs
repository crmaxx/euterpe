use euterpe_data::repositories::catalog::{self, AlbumUpsert, TrackUpsert};
use euterpe_data::{connect_database, migrations};

#[tokio::test]
async fn artist_album_and_track_upserts_return_stable_ids() {
    let handle = connect_database("sqlite::memory:").await.unwrap();
    migrations::migrate(&handle).await.unwrap();

    let artist_id = catalog::upsert_artist_by_name(&handle, "Artist A", None)
        .await
        .unwrap();
    assert_eq!(
        catalog::get_artist_name_by_id(&handle, artist_id)
            .await
            .unwrap()
            .as_deref(),
        Some("Artist A")
    );
    let artist_again = catalog::upsert_artist_by_name(&handle, "artist a", None)
        .await
        .unwrap();
    assert_eq!(artist_id, artist_again);

    let album_id = catalog::upsert_album(
        &handle,
        AlbumUpsert {
            artist_id: Some(artist_id),
            title: "Album",
            year: Some(2020),
            qobuz_album_id: Some(10),
            path: Some("Artist A/Album"),
            cover_path: Some("cover.jpg"),
        },
    )
    .await
    .unwrap();
    let album_again = catalog::upsert_album(
        &handle,
        AlbumUpsert {
            artist_id: Some(artist_id),
            title: "Album Updated",
            year: Some(2021),
            qobuz_album_id: None,
            path: Some("Artist A/Album"),
            cover_path: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(album_id, album_again);

    let album = catalog::get_album_by_id(&handle, album_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(album.title, "Album Updated");
    assert_eq!(album.qobuz_album_id, Some(10));
    assert_eq!(album.cover_path.as_deref(), Some("cover.jpg"));

    let track_id = catalog::upsert_track(
        &handle,
        TrackUpsert {
            album_id,
            title: "Track",
            track_number: Some(1),
            year: Some(2021),
            disc_number: Some(1),
            genre: Some("Rock"),
            qobuz_track_id: Some(99),
            path: "Artist A/Album/01.flac",
            duration_sec: Some(200),
            file_mtime: Some("mtime"),
            file_hash: Some("hash"),
            file_size: Some(123),
        },
    )
    .await
    .unwrap();
    let track_again = catalog::upsert_track(
        &handle,
        TrackUpsert {
            album_id,
            title: "Track Updated",
            track_number: Some(1),
            year: Some(2022),
            disc_number: Some(1),
            genre: Some("Jazz"),
            qobuz_track_id: None,
            path: "Artist A/Album/01.flac",
            duration_sec: Some(201),
            file_mtime: Some("mtime2"),
            file_hash: Some("hash2"),
            file_size: Some(456),
        },
    )
    .await
    .unwrap();
    assert_eq!(track_id, track_again);

    let track = catalog::get_track_by_id(&handle, track_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(track.title, "Track Updated");
    assert_eq!(track.qobuz_track_id, Some(99));
    assert_eq!(track.file_size, Some(456));
}

#[tokio::test]
async fn concurrent_album_upserts_by_path_return_existing_id() {
    let dir = tempfile::TempDir::new().unwrap();
    let db_url = format!("sqlite:{}", dir.path().join("catalog.sqlite").display());
    let handle = connect_database(&db_url).await.unwrap();
    migrations::migrate(&handle).await.unwrap();

    let artist_id = catalog::upsert_artist_by_name(&handle, "Artist", None)
        .await
        .unwrap();
    let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(2));
    let left_handle = handle.clone();
    let right_handle = handle.clone();
    let left_barrier = barrier.clone();
    let right_barrier = barrier.clone();

    let left = tokio::spawn(async move {
        left_barrier.wait().await;
        catalog::upsert_album(
            &left_handle,
            AlbumUpsert {
                artist_id: Some(artist_id),
                title: "Album",
                year: Some(2024),
                qobuz_album_id: None,
                path: Some("Artist/Album"),
                cover_path: None,
            },
        )
        .await
    });
    let right = tokio::spawn(async move {
        right_barrier.wait().await;
        catalog::upsert_album(
            &right_handle,
            AlbumUpsert {
                artist_id: Some(artist_id),
                title: "Album",
                year: Some(2024),
                qobuz_album_id: None,
                path: Some("Artist/Album"),
                cover_path: None,
            },
        )
        .await
    });

    let left_id = left.await.unwrap().unwrap();
    let right_id = right.await.unwrap().unwrap();

    assert_eq!(left_id, right_id);
    let albums = catalog::list_albums_keyset(
        &handle,
        catalog::AlbumListParams {
            sort: catalog::AlbumListSort::Title,
            order: catalog::AlbumListOrder::Asc,
            limit: 10,
            q: Some("Album".to_string()),
            after: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(albums.items.len(), 1);
}

#[tokio::test]
async fn tracks_list_by_album_sorts_by_filename_and_prefix_delete_keeps_siblings() {
    let handle = connect_database("sqlite::memory:").await.unwrap();
    migrations::migrate(&handle).await.unwrap();
    let artist_id = catalog::upsert_artist_by_name(&handle, "Artist", None)
        .await
        .unwrap();
    let album_id = catalog::upsert_album(
        &handle,
        AlbumUpsert {
            artist_id: Some(artist_id),
            title: "Album",
            year: None,
            qobuz_album_id: None,
            path: Some("Artist/Album"),
            cover_path: None,
        },
    )
    .await
    .unwrap();

    for (path, title) in [
        ("Artist/Album/10.flac", "Ten"),
        ("Artist/Album/02.flac", "Two"),
        ("Artist/Album/01.flac", "One"),
        ("Artist/AlbumX/01.flac", "Sibling"),
    ] {
        catalog::upsert_track(
            &handle,
            TrackUpsert {
                album_id,
                title,
                track_number: None,
                year: None,
                disc_number: None,
                genre: None,
                qobuz_track_id: None,
                path,
                duration_sec: None,
                file_mtime: None,
                file_hash: None,
                file_size: None,
            },
        )
        .await
        .unwrap();
    }

    let listed = catalog::list_tracks_by_album(&handle, album_id)
        .await
        .unwrap();
    let paths: Vec<_> = listed.iter().map(|track| track.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "Artist/Album/01.flac",
            "Artist/AlbumX/01.flac",
            "Artist/Album/02.flac",
            "Artist/Album/10.flac",
        ]
    );

    let deleted = catalog::delete_tracks_by_path_or_prefix(&handle, "Artist/Album")
        .await
        .unwrap();
    assert_eq!(deleted, 3);

    let remaining = catalog::list_tracks_by_album(&handle, album_id)
        .await
        .unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].path, "Artist/AlbumX/01.flac");
}

#[tokio::test]
async fn album_helpers_find_cover_and_delete_empty_storage_albums_in_scope() {
    let handle = connect_database("sqlite::memory:").await.unwrap();
    migrations::migrate(&handle).await.unwrap();
    let artist_id = catalog::upsert_artist_by_name(&handle, "Artist", None)
        .await
        .unwrap();
    let empty_in_scope = catalog::upsert_album(
        &handle,
        AlbumUpsert {
            artist_id: Some(artist_id),
            title: "Empty",
            year: None,
            qobuz_album_id: Some(42),
            path: Some("Artist/Empty"),
            cover_path: None,
        },
    )
    .await
    .unwrap();
    let non_empty = catalog::upsert_album(
        &handle,
        AlbumUpsert {
            artist_id: Some(artist_id),
            title: "Full",
            year: None,
            qobuz_album_id: None,
            path: Some("Artist/Full"),
            cover_path: None,
        },
    )
    .await
    .unwrap();
    let empty_outside_scope = catalog::upsert_album(
        &handle,
        AlbumUpsert {
            artist_id: Some(artist_id),
            title: "Outside",
            year: None,
            qobuz_album_id: None,
            path: Some("Other/Outside"),
            cover_path: None,
        },
    )
    .await
    .unwrap();
    let metadata_only = catalog::upsert_album(
        &handle,
        AlbumUpsert {
            artist_id: Some(artist_id),
            title: "Metadata",
            year: None,
            qobuz_album_id: Some(99),
            path: None,
            cover_path: None,
        },
    )
    .await
    .unwrap();

    catalog::upsert_track(
        &handle,
        TrackUpsert {
            album_id: non_empty,
            title: "Track",
            track_number: None,
            year: None,
            disc_number: None,
            genre: None,
            qobuz_track_id: None,
            path: "Artist/Full/01.flac",
            duration_sec: None,
            file_mtime: None,
            file_hash: None,
            file_size: None,
        },
    )
    .await
    .unwrap();

    assert_eq!(
        catalog::album_id_by_path(&handle, "Artist/Empty")
            .await
            .unwrap(),
        Some(empty_in_scope)
    );
    assert_eq!(
        catalog::find_album_id_by_qobuz_album_id(&handle, 42)
            .await
            .unwrap(),
        Some(empty_in_scope)
    );

    assert!(
        catalog::set_album_cover_path(&handle, empty_in_scope, "cover.jpg")
            .await
            .unwrap()
    );
    assert_eq!(
        catalog::get_album_by_id(&handle, empty_in_scope)
            .await
            .unwrap()
            .unwrap()
            .cover_path
            .as_deref(),
        Some("cover.jpg")
    );

    let deleted = catalog::delete_empty_storage_albums_in_scope(&handle, Some("Artist"))
        .await
        .unwrap();
    assert_eq!(deleted, 1);
    assert!(
        catalog::get_album_by_id(&handle, empty_in_scope)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        catalog::get_album_by_id(&handle, non_empty)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        catalog::get_album_by_id(&handle, empty_outside_scope)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        catalog::get_album_by_id(&handle, metadata_only)
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn track_helpers_update_fingerprint_metadata_path_and_hash_batches() {
    let handle = connect_database("sqlite::memory:").await.unwrap();
    migrations::migrate(&handle).await.unwrap();
    let artist_id = catalog::upsert_artist_by_name(&handle, "Artist", None)
        .await
        .unwrap();
    let album_id = catalog::upsert_album(
        &handle,
        AlbumUpsert {
            artist_id: Some(artist_id),
            title: "Album",
            year: None,
            qobuz_album_id: None,
            path: Some("Artist/Album"),
            cover_path: None,
        },
    )
    .await
    .unwrap();
    let track_id = catalog::upsert_track(
        &handle,
        TrackUpsert {
            album_id,
            title: "Original",
            track_number: Some(1),
            year: Some(2000),
            disc_number: Some(1),
            genre: Some("Rock"),
            qobuz_track_id: None,
            path: "Artist/Album/01.flac",
            duration_sec: Some(180),
            file_mtime: Some("old-mtime"),
            file_hash: None,
            file_size: Some(100),
        },
    )
    .await
    .unwrap();

    assert_eq!(
        catalog::get_track_fingerprint_by_path(&handle, "Artist/Album/01.flac")
            .await
            .unwrap(),
        Some((Some("old-mtime".to_string()), Some(100)))
    );

    assert!(
        catalog::update_track_metadata(
            &handle,
            track_id,
            catalog::TrackMetadataUpdate {
                title: "Updated",
                track_number: Some(2),
                year: Some(2001),
                disc_number: Some(1),
                genre: Some("Jazz"),
                file_mtime: Some("tag-mtime"),
            },
        )
        .await
        .unwrap()
    );
    let updated = catalog::get_track_by_id(&handle, track_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated.title, "Updated");
    assert_eq!(updated.track_number, Some(2));
    assert_eq!(updated.genre.as_deref(), Some("Jazz"));
    assert_eq!(updated.file_mtime.as_deref(), Some("tag-mtime"));

    assert!(
        catalog::update_track_path_fingerprint(
            &handle,
            track_id,
            "Artist/Album/01-renamed.flac",
            Some(200),
            Some("hash"),
            Some("new-mtime"),
        )
        .await
        .unwrap()
    );
    assert_eq!(
        catalog::get_track_fingerprint_by_path(&handle, "Artist/Album/01-renamed.flac")
            .await
            .unwrap(),
        Some((Some("new-mtime".to_string()), Some(200)))
    );

    let needing = catalog::list_tracks_needing_file_hash_batch(&handle, 0, 10)
        .await
        .unwrap();
    assert!(needing.is_empty());

    assert!(
        catalog::set_track_file_hash(&handle, track_id, "hash2")
            .await
            .unwrap()
    );
    assert!(
        catalog::set_track_file_size(&handle, track_id, 300)
            .await
            .unwrap()
    );
    let hashed = catalog::get_track_by_id(&handle, track_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(hashed.file_hash.as_deref(), Some("hash2"));
    assert_eq!(hashed.file_size, Some(300));
    assert_eq!(catalog::count_tracks(&handle).await.unwrap(), 1);
    assert_eq!(
        catalog::count_distinct_track_paths(&handle).await.unwrap(),
        1
    );

    assert_eq!(
        catalog::delete_track_by_path(&handle, "Artist/Album/01-renamed.flac")
            .await
            .unwrap(),
        1
    );
    assert!(
        catalog::get_track_by_id(&handle, track_id)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn track_album_or_path_prefix_listing_includes_moved_album_tracks() {
    let handle = connect_database("sqlite::memory:").await.unwrap();
    migrations::migrate(&handle).await.unwrap();
    let artist_id = catalog::upsert_artist_by_name(&handle, "Artist", None)
        .await
        .unwrap();
    let album_id = catalog::upsert_album(
        &handle,
        AlbumUpsert {
            artist_id: Some(artist_id),
            title: "Album",
            year: None,
            qobuz_album_id: None,
            path: Some("Artist/Album"),
            cover_path: None,
        },
    )
    .await
    .unwrap();
    let other_album_id = catalog::upsert_album(
        &handle,
        AlbumUpsert {
            artist_id: Some(artist_id),
            title: "Other",
            year: None,
            qobuz_album_id: None,
            path: Some("Artist/Other"),
            cover_path: None,
        },
    )
    .await
    .unwrap();

    for (album_id, path) in [
        (album_id, "Artist/Album/02.flac"),
        (other_album_id, "Artist/Album/01.flac"),
        (other_album_id, "Artist/AlbumX/01.flac"),
    ] {
        catalog::upsert_track(
            &handle,
            TrackUpsert {
                album_id,
                title: path,
                track_number: None,
                year: None,
                disc_number: None,
                genre: None,
                qobuz_track_id: None,
                path,
                duration_sec: None,
                file_mtime: None,
                file_hash: None,
                file_size: None,
            },
        )
        .await
        .unwrap();
    }

    let listed =
        catalog::list_tracks_by_album_or_path_prefix(&handle, album_id, Some("Artist/Album"))
            .await
            .unwrap();
    let paths = listed
        .iter()
        .map(|track| track.path.as_str())
        .collect::<Vec<_>>();
    assert_eq!(paths, ["Artist/Album/01.flac", "Artist/Album/02.flac"]);
}

#[tokio::test]
async fn album_keyset_listing_filters_sorts_and_counts_tracks() {
    let handle = connect_database("sqlite::memory:").await.unwrap();
    migrations::migrate(&handle).await.unwrap();
    let alpha_artist = catalog::upsert_artist_by_name(&handle, "Alpha Artist", None)
        .await
        .unwrap();
    let beta_artist = catalog::upsert_artist_by_name(&handle, "Beta Artist", None)
        .await
        .unwrap();

    let alpha = catalog::upsert_album(
        &handle,
        AlbumUpsert {
            artist_id: Some(alpha_artist),
            title: "Zeta",
            year: Some(2020),
            qobuz_album_id: None,
            path: Some("Alpha/Zeta"),
            cover_path: Some("zeta.jpg"),
        },
    )
    .await
    .unwrap();
    let beta = catalog::upsert_album(
        &handle,
        AlbumUpsert {
            artist_id: Some(beta_artist),
            title: "Beta",
            year: Some(2021),
            qobuz_album_id: None,
            path: Some("Beta/Beta"),
            cover_path: None,
        },
    )
    .await
    .unwrap();
    let gamma = catalog::upsert_album(
        &handle,
        AlbumUpsert {
            artist_id: Some(alpha_artist),
            title: "Gamma",
            year: None,
            qobuz_album_id: None,
            path: Some("Alpha/Gamma"),
            cover_path: None,
        },
    )
    .await
    .unwrap();

    for (album_id, path) in [(alpha, "Alpha/Zeta/01.flac"), (beta, "Beta/Beta/01.flac")] {
        catalog::upsert_track(
            &handle,
            TrackUpsert {
                album_id,
                title: path,
                track_number: None,
                year: None,
                disc_number: None,
                genre: None,
                qobuz_track_id: None,
                path,
                duration_sec: None,
                file_mtime: None,
                file_hash: None,
                file_size: None,
            },
        )
        .await
        .unwrap();
    }

    let first = catalog::list_albums_keyset(
        &handle,
        catalog::AlbumListParams {
            sort: catalog::AlbumListSort::Title,
            order: catalog::AlbumListOrder::Asc,
            limit: 2,
            q: None,
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
        ["Beta", "Gamma"]
    );

    let next = catalog::list_albums_keyset(
        &handle,
        catalog::AlbumListParams {
            sort: catalog::AlbumListSort::Title,
            order: catalog::AlbumListOrder::Asc,
            limit: 2,
            q: None,
            after: first.next_after,
        },
    )
    .await
    .unwrap();
    assert!(!next.has_more);
    assert_eq!(next.items[0].id, alpha);
    assert_eq!(next.items[0].track_count, 1);
    assert_eq!(next.items[0].cover_path.as_deref(), Some("zeta.jpg"));

    let artist_filtered = catalog::list_albums_keyset(
        &handle,
        catalog::AlbumListParams {
            sort: catalog::AlbumListSort::AlbumDate,
            order: catalog::AlbumListOrder::Desc,
            limit: 10,
            q: Some("alpha artist".to_string()),
            after: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        artist_filtered
            .items
            .iter()
            .map(|album| album.id)
            .collect::<Vec<_>>(),
        [alpha, gamma]
    );
}

#[tokio::test]
async fn album_keyset_listing_sorts_album_date_with_unknowns_last() {
    let handle = connect_database("sqlite::memory:").await.unwrap();
    migrations::migrate(&handle).await.unwrap();
    let artist_id = catalog::upsert_artist_by_name(&handle, "Artist", None)
        .await
        .unwrap();

    let unknown = catalog::upsert_album(
        &handle,
        AlbumUpsert {
            artist_id: Some(artist_id),
            title: "Unknown",
            year: None,
            qobuz_album_id: None,
            path: Some("Artist/Unknown"),
            cover_path: None,
        },
    )
    .await
    .unwrap();
    let old = catalog::upsert_album(
        &handle,
        AlbumUpsert {
            artist_id: Some(artist_id),
            title: "Old",
            year: Some(2020),
            qobuz_album_id: None,
            path: Some("Artist/Old"),
            cover_path: None,
        },
    )
    .await
    .unwrap();
    let new = catalog::upsert_album(
        &handle,
        AlbumUpsert {
            artist_id: Some(artist_id),
            title: "New",
            year: Some(2024),
            qobuz_album_id: None,
            path: Some("Artist/New"),
            cover_path: None,
        },
    )
    .await
    .unwrap();

    let asc_first = catalog::list_albums_keyset(
        &handle,
        catalog::AlbumListParams {
            sort: catalog::AlbumListSort::AlbumDate,
            order: catalog::AlbumListOrder::Asc,
            limit: 2,
            q: None,
            after: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        asc_first
            .items
            .iter()
            .map(|album| album.id)
            .collect::<Vec<_>>(),
        [old, new]
    );

    let asc_next = catalog::list_albums_keyset(
        &handle,
        catalog::AlbumListParams {
            sort: catalog::AlbumListSort::AlbumDate,
            order: catalog::AlbumListOrder::Asc,
            limit: 2,
            q: None,
            after: asc_first.next_after,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        asc_next
            .items
            .iter()
            .map(|album| album.id)
            .collect::<Vec<_>>(),
        [unknown]
    );

    let desc = catalog::list_albums_keyset(
        &handle,
        catalog::AlbumListParams {
            sort: catalog::AlbumListSort::AlbumDate,
            order: catalog::AlbumListOrder::Desc,
            limit: 3,
            q: None,
            after: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        desc.items.iter().map(|album| album.id).collect::<Vec<_>>(),
        [new, old, unknown]
    );
}

#[tokio::test]
async fn album_keyset_listing_sorts_by_date_added() {
    let handle = connect_database("sqlite::memory:").await.unwrap();
    migrations::migrate(&handle).await.unwrap();
    let artist_id = catalog::upsert_artist_by_name(&handle, "Artist", None)
        .await
        .unwrap();

    let first = catalog::upsert_album(
        &handle,
        AlbumUpsert {
            artist_id: Some(artist_id),
            title: "First",
            year: None,
            qobuz_album_id: None,
            path: Some("Artist/First"),
            cover_path: None,
        },
    )
    .await
    .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    let second = catalog::upsert_album(
        &handle,
        AlbumUpsert {
            artist_id: Some(artist_id),
            title: "Second",
            year: None,
            qobuz_album_id: None,
            path: Some("Artist/Second"),
            cover_path: None,
        },
    )
    .await
    .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    let third = catalog::upsert_album(
        &handle,
        AlbumUpsert {
            artist_id: Some(artist_id),
            title: "Third",
            year: None,
            qobuz_album_id: None,
            path: Some("Artist/Third"),
            cover_path: None,
        },
    )
    .await
    .unwrap();

    let asc_first = catalog::list_albums_keyset(
        &handle,
        catalog::AlbumListParams {
            sort: catalog::AlbumListSort::DateAdded,
            order: catalog::AlbumListOrder::Asc,
            limit: 1,
            q: Some("i".to_string()),
            after: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(asc_first.items[0].id, first);

    let asc_next = catalog::list_albums_keyset(
        &handle,
        catalog::AlbumListParams {
            sort: catalog::AlbumListSort::DateAdded,
            order: catalog::AlbumListOrder::Asc,
            limit: 2,
            q: Some("i".to_string()),
            after: asc_first.next_after,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        asc_next
            .items
            .iter()
            .map(|album| album.id)
            .collect::<Vec<_>>(),
        [second, third]
    );

    let desc = catalog::list_albums_keyset(
        &handle,
        catalog::AlbumListParams {
            sort: catalog::AlbumListSort::DateAdded,
            order: catalog::AlbumListOrder::Desc,
            limit: 3,
            q: None,
            after: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        desc.items.iter().map(|album| album.id).collect::<Vec<_>>(),
        [third, second, first]
    );
}

#[tokio::test]
async fn scan_keep_paths_prune_absent_tracks_and_cleanup_records() {
    let handle = connect_database("sqlite::memory:").await.unwrap();
    migrations::migrate(&handle).await.unwrap();
    let artist_id = catalog::upsert_artist_by_name(&handle, "Artist", None)
        .await
        .unwrap();
    let album_id = catalog::upsert_album(
        &handle,
        AlbumUpsert {
            artist_id: Some(artist_id),
            title: "Album",
            year: None,
            qobuz_album_id: None,
            path: Some("Artist/Album"),
            cover_path: None,
        },
    )
    .await
    .unwrap();

    for path in [
        "Artist/Album/01.flac",
        "Artist/Album/stale.flac",
        "Artist/AlbumX/stale.flac",
    ] {
        catalog::upsert_track(
            &handle,
            TrackUpsert {
                album_id,
                title: path,
                track_number: None,
                year: None,
                disc_number: None,
                genre: None,
                qobuz_track_id: None,
                path,
                duration_sec: None,
                file_mtime: None,
                file_hash: None,
                file_size: None,
            },
        )
        .await
        .unwrap();
    }

    catalog::reset_scan_keep_paths(&handle, 7).await.unwrap();
    catalog::record_scan_keep_path(&handle, 7, "Artist/Album/01.flac")
        .await
        .unwrap();
    catalog::record_scan_keep_path(&handle, 7, "Artist/Album/02.flac")
        .await
        .unwrap();
    catalog::record_scan_keep_path(&handle, 7, "Artist/Album/01.flac")
        .await
        .unwrap();
    assert_eq!(catalog::scan_keep_path_count(&handle, 7).await.unwrap(), 2);

    let deleted = catalog::delete_absent_in_scope_for_scan(&handle, Some("Artist/Album"), 7)
        .await
        .unwrap();
    assert_eq!(deleted, 1);
    assert_eq!(catalog::scan_keep_path_count(&handle, 7).await.unwrap(), 2);

    catalog::cleanup_scan_keep_paths(&handle, 7).await.unwrap();
    assert_eq!(catalog::scan_keep_path_count(&handle, 7).await.unwrap(), 0);

    let remaining = catalog::list_tracks_by_album(&handle, album_id)
        .await
        .unwrap();
    assert_eq!(
        remaining
            .iter()
            .map(|track| track.path.as_str())
            .collect::<Vec<_>>(),
        ["Artist/Album/01.flac", "Artist/AlbumX/stale.flac"]
    );
}
