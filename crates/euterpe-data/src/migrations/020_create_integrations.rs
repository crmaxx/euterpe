use super::*;

pub(super) fn create_integrations(_: &TableState) -> welds::errors::Result<MigrationStep> {
    let migration = create_table("integrations")
        .id(|c| c("id", Type::IntBig))
        .column(|c| c("type", Type::String))
        .column(|c| c("provider", Type::String))
        .column(|c| c("display_name", Type::Text))
        .column(|c| c("enabled", Type::Bool))
        .column(|c| c("config_json", Type::Text))
        .column(|c| c("config_secrets_enc", Type::Text).is_null())
        .column(|c| c("sort_order", Type::Int))
        .column(|c| c("created_at", Type::String))
        .column(|c| c("updated_at", Type::String));
    Ok(MigrationStep::new("020_create_integrations", migration))
}
