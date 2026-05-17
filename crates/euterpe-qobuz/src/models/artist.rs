use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct ArtistRef {
    pub id: u64,
    pub name: String,
}
