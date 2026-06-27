use super::*;

pub(super) fn index_download_jobs_queue(_: &TableState) -> welds::errors::Result<MigrationStep> {
    Ok(MigrationStep::new(
        "006_index_download_jobs_queue",
        create_index()
            .table("download_jobs")
            .column("job_type")
            .column("status")
            .column("queue_position"),
    ))
}
