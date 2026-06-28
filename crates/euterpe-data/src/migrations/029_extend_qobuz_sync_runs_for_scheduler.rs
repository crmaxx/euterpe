use super::*;

pub(super) fn extend_qobuz_sync_runs_for_scheduler(
    state: &TableState,
) -> welds::errors::Result<MigrationStep> {
    let trigger = change_table(state, "qobuz_sync_runs")?
        .add_column("trigger", Type::String)
        .null();
    let skip_reason = change_table(state, "qobuz_sync_runs")?
        .add_column("skip_reason", Type::Text)
        .null();
    let migration = Steps::new().add(trigger).add(skip_reason);
    Ok(MigrationStep::new(
        "029_extend_qobuz_sync_runs_for_scheduler",
        migration,
    ))
}
