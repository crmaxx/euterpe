use super::*;

pub(super) fn create_settings(_: &TableState) -> welds::errors::Result<MigrationStep> {
    let migration = create_table("settings")
        .id(|c| c("key", Type::String))
        .column(|c| c("value", Type::Text))
        .column(|c| c("updated_at", Type::String));
    Ok(MigrationStep::new("001_create_settings", migration))
}
