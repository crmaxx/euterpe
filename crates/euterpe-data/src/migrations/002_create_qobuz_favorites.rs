use super::*;

pub(super) fn create_qobuz_favorites(_: &TableState) -> welds::errors::Result<MigrationStep> {
    let migration = create_table("qobuz_favorites")
        .id(|c| c("id", Type::IntBig))
        .column(|c| c("entity_type", Type::String).create_index())
        .column(|c| c("qobuz_id", Type::IntBig))
        .column(|c| c("title", Type::Text).is_null())
        .column(|c| c("artist_name", Type::Text).is_null())
        .column(|c| c("synced_at", Type::String))
        .column(|c| c("removed", Type::Bool))
        .column(|c| c("slug", Type::Text).is_null())
        .column(|c| c("cover_url", Type::Text).is_null());
    Ok(MigrationStep::new("002_create_qobuz_favorites", migration))
}
