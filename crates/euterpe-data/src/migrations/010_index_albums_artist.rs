use super::*;

pub(super) fn index_albums_artist(_: &TableState) -> welds::errors::Result<MigrationStep> {
    Ok(MigrationStep::new(
        "010_index_albums_artist",
        create_index().table("albums").column("artist_id"),
    ))
}
