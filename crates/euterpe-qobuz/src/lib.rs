//! Async Qobuz API client for [Euterpe](https://github.com/crmaxx/euterpe).
//!
//! Inspired by community tools (see `docs/references/`): qobuz-dl, streamrip, qobuz-sync.

mod api;
pub mod bundle;
mod client;
mod config;
mod error;
mod models;
mod pagination;
pub mod signing;
mod traits;

pub use api::streaming::Quality;
pub use client::QobuzClient;
pub use config::{AuthConfig, QobuzConfig};
pub use error::QobuzError;
pub use models::{
    AlbumDetail, AlbumSummary, AlbumTracks, ArtistRef, FavoriteType, FavoritesAlbumsResponse,
    Image,
    LoginResponse, StreamUrl, TrackSummary, UserProfile,
};
pub use signing::{sign_favorites, sign_track_file_url};
pub use pagination::{Page, PageRequest};
pub use signing::FavoritesSignMode;
pub use traits::QobuzApi;

pub mod prelude {
    pub use crate::{
        AuthConfig, Page, PageRequest, QobuzApi, QobuzClient, QobuzConfig, QobuzError, Quality,
    };
}
