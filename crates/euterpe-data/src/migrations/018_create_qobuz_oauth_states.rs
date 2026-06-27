use super::*;

pub(super) fn create_qobuz_oauth_states(_: &TableState) -> welds::errors::Result<MigrationStep> {
    let migration = create_table("qobuz_oauth_states")
        .id(|c| c("id", Type::IntBig))
        .column(|c| c("state", Type::Text).create_unique_index())
        .column(|c| c("created_at", Type::String))
        .column(|c| c("expires_at", Type::String));
    Ok(MigrationStep::new(
        "018_create_qobuz_oauth_states",
        migration,
    ))
}
