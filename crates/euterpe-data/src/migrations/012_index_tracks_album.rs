use super::*;

pub(super) fn index_tracks_album(_: &TableState) -> welds::errors::Result<MigrationStep> {
    Ok(MigrationStep::new(
        "012_index_tracks_album",
        create_index().table("tracks").column("album_id"),
    ))
}
