use super::*;

pub(super) fn create_tracks(_: &TableState) -> welds::errors::Result<MigrationStep> {
    let migration = create_table("tracks")
        .id(|c| c("id", Type::IntBig))
        .column(|c| {
            c("album_id", Type::IntBig).create_foreign_key("albums", "id", OnDelete::Cascade)
        })
        .column(|c| c("title", Type::Text))
        .column(|c| c("track_number", Type::Int).is_null())
        .column(|c| c("year", Type::Int).is_null())
        .column(|c| c("disc_number", Type::Int).is_null())
        .column(|c| c("genre", Type::Text).is_null())
        .column(|c| {
            c("qobuz_track_id", Type::IntBig)
                .is_null()
                .create_unique_index()
        })
        .column(|c| c("path", Type::Text).create_unique_index())
        .column(|c| c("duration_sec", Type::Int).is_null())
        .column(|c| c("file_mtime", Type::Text).is_null())
        .column(|c| c("file_hash", Type::Text).is_null())
        .column(|c| c("file_size", Type::IntBig).is_null())
        .column(|c| c("created_at", Type::String))
        .column(|c| c("updated_at", Type::String));
    Ok(MigrationStep::new("011_create_tracks", migration))
}
