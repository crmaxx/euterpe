use super::*;

pub(super) fn create_albums(_: &TableState) -> welds::errors::Result<MigrationStep> {
    let migration = create_table("albums")
        .id(|c| c("id", Type::IntBig))
        .column(|c| {
            c("artist_id", Type::IntBig).is_null().create_foreign_key(
                "artists",
                "id",
                OnDelete::SetNull,
            )
        })
        .column(|c| c("title", Type::Text))
        .column(|c| c("year", Type::Int).is_null())
        .column(|c| {
            c("qobuz_album_id", Type::IntBig)
                .is_null()
                .create_unique_index()
        })
        .column(|c| c("path", Type::Text).is_null().create_unique_index())
        .column(|c| c("cover_path", Type::Text).is_null())
        .column(|c| c("created_at", Type::String))
        .column(|c| c("updated_at", Type::String));
    Ok(MigrationStep::new("008_create_albums", migration))
}
