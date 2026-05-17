use serde::Deserialize;

use super::album::AlbumSummary;
use super::artist::ArtistRef;
use super::track::TrackSummary;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FavoriteType {
    Albums,
    Tracks,
    Artists,
}

impl FavoriteType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Albums => "albums",
            Self::Tracks => "tracks",
            Self::Artists => "artists",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct FavoritesAlbumsResponse {
    pub albums: FavoritesPage<AlbumSummary>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct FavoritesTracksResponse {
    pub tracks: FavoritesPage<TrackSummary>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct FavoritesArtistsResponse {
    pub artists: FavoritesPage<ArtistRef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FavoritesPage<T> {
    pub total: u32,
    pub limit: u32,
    pub offset: u32,
    pub items: Vec<T>,
}
