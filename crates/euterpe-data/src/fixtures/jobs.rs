use crate::connection::DataHandle;
use crate::error::Result;
use crate::repositories::catalog::{self, AlbumUpsert};

pub async fn seed_album(handle: &DataHandle) -> Result<i64> {
    let artist_id = catalog::upsert_artist_by_name(handle, "Artist", None).await?;
    catalog::upsert_album(
        handle,
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
}
