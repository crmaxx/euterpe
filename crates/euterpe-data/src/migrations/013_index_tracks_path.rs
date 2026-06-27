use super::*;

pub(super) fn index_tracks_path(_: &TableState) -> welds::errors::Result<MigrationStep> {
    Ok(MigrationStep::new(
        "013_index_tracks_path",
        create_index().table("tracks").column("path"),
    ))
}
