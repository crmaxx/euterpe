use super::*;

pub(super) fn create_library_scan_runs(_: &TableState) -> welds::errors::Result<MigrationStep> {
    let migration = create_table("library_scan_runs")
        .id(|c| c("id", Type::IntBig))
        .column(|c| c("status", Type::String))
        .column(|c| c("files_seen", Type::IntBig))
        .column(|c| c("files_indexed", Type::IntBig))
        .column(|c| c("files_total", Type::IntBig))
        .column(|c| c("files_processed", Type::IntBig))
        .column(|c| c("started_at", Type::String))
        .column(|c| c("finished_at", Type::String).is_null())
        .column(|c| c("error_message", Type::Text).is_null());
    Ok(MigrationStep::new(
        "014_create_library_scan_runs",
        migration,
    ))
}
