use super::*;

pub(super) fn create_qobuz_accounts(_: &TableState) -> welds::errors::Result<MigrationStep> {
    let migration = create_table("qobuz_accounts")
        .id(|c| c("id", Type::IntBig))
        .column(|c| c("label", Type::Text).is_null())
        .column(|c| c("qobuz_user_id", Type::IntBig).create_unique_index())
        .column(|c| c("uat_encrypted", Type::Text))
        .column(|c| c("display_name", Type::Text).is_null())
        .column(|c| c("membership_label", Type::Text).is_null())
        .column(|c| c("uat_obtained_at", Type::String))
        .column(|c| c("uat_expires_at", Type::String).is_null())
        .column(|c| c("oauth_refresh_encrypted", Type::Text).is_null())
        .column(|c| c("created_at", Type::String))
        .column(|c| c("updated_at", Type::String));
    Ok(MigrationStep::new("016_create_qobuz_accounts", migration))
}
