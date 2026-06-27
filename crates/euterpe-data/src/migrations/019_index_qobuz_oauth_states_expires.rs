use super::*;

pub(super) fn index_qobuz_oauth_states_expires(
    _: &TableState,
) -> welds::errors::Result<MigrationStep> {
    Ok(MigrationStep::new(
        "019_index_qobuz_oauth_states_expires",
        create_index()
            .table("qobuz_oauth_states")
            .column("expires_at"),
    ))
}
