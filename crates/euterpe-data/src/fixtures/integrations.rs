use crate::connection::DataHandle;
use crate::error::Result;
use crate::repositories::integrations::{self, IntegrationInsert};

#[derive(Debug, Clone)]
pub struct IntegrationFixture {
    pub type_: String,
    pub provider: String,
    pub display_name: String,
    pub enabled: bool,
    pub config_json: String,
    pub config_secrets_enc: Option<String>,
    pub sort_order: i32,
}

impl Default for IntegrationFixture {
    fn default() -> Self {
        Self {
            type_: "tag_source".to_string(),
            provider: "musicbrainz".to_string(),
            display_name: "MusicBrainz".to_string(),
            enabled: true,
            config_json: "{}".to_string(),
            config_secrets_enc: None,
            sort_order: 0,
        }
    }
}

pub async fn seed_integration(handle: &DataHandle, fixture: IntegrationFixture) -> Result<i64> {
    integrations::insert(
        handle,
        IntegrationInsert {
            type_: &fixture.type_,
            provider: &fixture.provider,
            display_name: &fixture.display_name,
            enabled: fixture.enabled,
            config_json: &fixture.config_json,
            config_secrets_enc: fixture.config_secrets_enc.as_deref(),
            sort_order: fixture.sort_order,
        },
    )
    .await
}
