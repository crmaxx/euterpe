use super::*;

pub(super) fn index_library_scan_runs_status(
    _: &TableState,
) -> welds::errors::Result<MigrationStep> {
    Ok(MigrationStep::new(
        "015_index_library_scan_runs_status",
        create_index().table("library_scan_runs").column("status"),
    ))
}
