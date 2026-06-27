use super::*;

pub(super) fn create_artists(_: &TableState) -> welds::errors::Result<MigrationStep> {
    let migration = create_table("artists")
        .id(|c| c("id", Type::IntBig))
        .column(|c| c("name", Type::Text))
        .column(|c| {
            c("qobuz_artist_id", Type::IntBig)
                .is_null()
                .create_unique_index()
        })
        .column(|c| c("created_at", Type::String));
    Ok(MigrationStep::new("007_create_artists", migration))
}
