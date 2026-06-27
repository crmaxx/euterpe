use super::*;

pub(super) fn index_convert_jobs_status(_: &TableState) -> welds::errors::Result<MigrationStep> {
    Ok(MigrationStep::new(
        "024_index_convert_jobs_status",
        create_index().table("convert_jobs").column("status"),
    ))
}
