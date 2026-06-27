use super::*;

pub(super) fn index_cue_jobs_status(_: &TableState) -> welds::errors::Result<MigrationStep> {
    Ok(MigrationStep::new(
        "027_index_cue_jobs_status",
        create_index().table("cue_jobs").column("status"),
    ))
}
