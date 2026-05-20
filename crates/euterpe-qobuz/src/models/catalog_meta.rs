use serde::Deserialize;

use super::deser::deserialize_null_string;

#[derive(Debug, Clone, Deserialize)]
pub struct GenreRef {
    #[serde(default, deserialize_with = "deserialize_qobuz_id_optional")]
    pub id: Option<u64>,
    #[serde(default, deserialize_with = "deserialize_null_string")]
    pub name: String,
}

impl GenreRef {
    pub fn display_name(&self) -> Option<&str> {
        let t = self.name.trim();
        if t.is_empty() { None } else { Some(t) }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct LabelRef {
    #[serde(default, deserialize_with = "deserialize_qobuz_id_optional")]
    pub id: Option<u64>,
    #[serde(default, deserialize_with = "deserialize_null_string")]
    pub name: String,
}

impl LabelRef {
    pub fn display_name(&self) -> Option<&str> {
        let t = self.name.trim();
        if t.is_empty() { None } else { Some(t) }
    }
}

fn deserialize_qobuz_id_optional<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value: Option<serde_json::Value> = Option::deserialize(deserializer)?;
    match value {
        None => Ok(None),
        Some(v) => super::deser::parse_id_value(&v)
            .map(Some)
            .map_err(serde::de::Error::custom),
    }
}
