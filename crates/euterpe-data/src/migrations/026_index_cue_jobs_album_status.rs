use super::*;

pub(super) fn index_cue_jobs_album_status(_: &TableState) -> welds::errors::Result<MigrationStep> {
    Ok(MigrationStep::new(
        "026_index_cue_jobs_album_status",
        create_index()
            .table("cue_jobs")
            .column("album_id")
            .column("status"),
    ))
}
