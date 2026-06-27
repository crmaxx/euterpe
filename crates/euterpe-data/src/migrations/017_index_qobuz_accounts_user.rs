use super::*;

pub(super) fn index_qobuz_accounts_user(_: &TableState) -> welds::errors::Result<MigrationStep> {
    Ok(MigrationStep::new(
        "017_index_qobuz_accounts_user",
        create_index()
            .table("qobuz_accounts")
            .column("qobuz_user_id"),
    ))
}
