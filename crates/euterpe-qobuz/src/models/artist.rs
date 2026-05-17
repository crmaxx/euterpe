use serde::Deserialize;

use super::deser::{deserialize_null_string, deserialize_qobuz_id};

#[derive(Debug, Clone, Deserialize)]
pub struct ArtistRef {
    #[serde(deserialize_with = "deserialize_qobuz_id")]
    pub id: u64,
    #[serde(default, deserialize_with = "deserialize_null_string")]
    pub name: String,
}
