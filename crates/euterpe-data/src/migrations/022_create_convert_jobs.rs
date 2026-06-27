use super::*;

pub(super) fn create_convert_jobs(_: &TableState) -> welds::errors::Result<MigrationStep> {
    let migration = create_table("convert_jobs")
        .id(|c| c("id", Type::IntBig))
        .column(|c| {
            c("album_id", Type::IntBig).create_foreign_key("albums", "id", OnDelete::Cascade)
        })
        .column(|c| c("status", Type::String))
        .column(|c| c("trigger", Type::String))
        .column(|c| c("files_total", Type::IntBig))
        .column(|c| c("files_done", Type::IntBig))
        .column(|c| c("progress_pct", Type::FloatBig))
        .column(|c| c("error_message", Type::Text).is_null())
        .column(|c| c("payload_json", Type::Text).is_null())
        .column(|c| c("created_at", Type::String))
        .column(|c| c("updated_at", Type::String));
    Ok(MigrationStep::new("022_create_convert_jobs", migration))
}
