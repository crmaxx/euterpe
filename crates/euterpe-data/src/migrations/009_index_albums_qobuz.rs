use super::*;

pub(super) fn index_albums_qobuz(_: &TableState) -> welds::errors::Result<MigrationStep> {
    Ok(MigrationStep::new(
        "009_index_albums_qobuz",
        create_index().table("albums").column("qobuz_album_id"),
    ))
}
