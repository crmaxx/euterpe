pub mod deser;
mod album;
mod artist;
mod catalog_meta;
mod favorites;
mod track;
mod user;

pub use album::{AlbumDetail, AlbumSummary, AlbumTracks, Image};
pub use artist::ArtistRef;
pub use catalog_meta::{GenreRef, LabelRef};
pub use favorites::{FavoriteType, FavoritesAlbumsResponse, FavoritesTracksResponse};
pub use user::LoginResponse;
pub use track::{StreamUrl, TrackSummary};
pub use user::UserProfile;
