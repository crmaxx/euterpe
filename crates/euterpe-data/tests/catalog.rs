use euterpe_data::repositories::catalog::{self, AlbumUpsert, TrackUpsert};
use euterpe_data::{connect_database, migrations};

#[tokio::test]
async fn artist_album_and_track_upserts_return_stable_ids() {
    let handle = connect_database("sqlite::memory:").await.unwrap();
    migrations::migrate(&handle).await.unwrap();

    let artist_id = catalog::upsert_artist_by_name(&handle, "Artist A", None)
        .await
        .unwrap();
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
    assert_eq!(
        catalog::get_track_by_id(&handle, track_id)
            .await
            .unwrap()
            .unwrap()
            .file_hash
            .as_deref(),
        Some("hash2")
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
