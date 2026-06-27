use super::*;

pub(super) fn index_download_jobs_status(_: &TableState) -> welds::errors::Result<MigrationStep> {
    Ok(MigrationStep::new(
        "005_index_download_jobs_status",
        create_index().table("download_jobs").column("status"),
    ))
}
