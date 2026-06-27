use super::*;

pub(super) fn create_qobuz_sync_runs(_: &TableState) -> welds::errors::Result<MigrationStep> {
    let migration = create_table("qobuz_sync_runs")
        .id(|c| c("id", Type::IntBig))
        .column(|c| c("started_at", Type::String))
        .column(|c| c("finished_at", Type::String).is_null())
        .column(|c| c("status", Type::String))
        .column(|c| c("albums_total", Type::IntBig).is_null())
        .column(|c| c("albums_added", Type::IntBig).is_null())
        .column(|c| c("albums_removed", Type::IntBig).is_null())
        .column(|c| c("error_message", Type::Text).is_null());
    Ok(MigrationStep::new("003_create_qobuz_sync_runs", migration))
}
