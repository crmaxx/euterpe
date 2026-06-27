use super::*;

pub(super) fn create_scan_keep_paths(_: &TableState) -> welds::errors::Result<MigrationStep> {
    let migration = create_table("scan_keep_paths")
        .id(|c| c("id", Type::IntBig))
        .column(|c| c("scan_id", Type::IntBig))
        .column(|c| c("path", Type::Text));
    Ok(MigrationStep::new("028_create_scan_keep_paths", migration))
}
