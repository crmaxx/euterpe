use super::*;

pub(super) fn index_integrations_type_enabled(
    _: &TableState,
) -> welds::errors::Result<MigrationStep> {
    Ok(MigrationStep::new(
        "021_index_integrations_type_enabled",
        create_index()
            .table("integrations")
            .column("type")
            .column("enabled"),
    ))
}
