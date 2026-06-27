use super::*;

pub(super) fn create_download_jobs(_: &TableState) -> welds::errors::Result<MigrationStep> {
    let migration = create_table("download_jobs")
        .id(|c| c("id", Type::IntBig))
        .column(|c| c("status", Type::String))
        .column(|c| c("job_type", Type::String))
        .column(|c| c("qobuz_id", Type::IntBig).is_null())
        .column(|c| c("quality", Type::Int))
        .column(|c| c("progress_pct", Type::FloatBig).is_null())
        .column(|c| c("download_speed_bps", Type::IntBig))
        .column(|c| c("queue_position", Type::IntBig))
        .column(|c| c("payload_json", Type::Text).is_null())
        .column(|c| c("error_message", Type::Text).is_null())
        .column(|c| c("created_at", Type::String))
        .column(|c| c("updated_at", Type::String));
    Ok(MigrationStep::new("004_create_download_jobs", migration))
}
