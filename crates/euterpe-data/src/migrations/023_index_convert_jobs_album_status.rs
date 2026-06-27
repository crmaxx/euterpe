use super::*;

pub(super) fn index_convert_jobs_album_status(
    _: &TableState,
) -> welds::errors::Result<MigrationStep> {
    Ok(MigrationStep::new(
        "023_index_convert_jobs_album_status",
        create_index()
            .table("convert_jobs")
            .column("album_id")
            .column("status"),
    ))
}
