//! Async Qobuz API client for [Euterpe](https://github.com/crmaxx/euterpe).
//!
//! Inspired by community tools (see `docs/references/`, gitignored: qobuz-dl, streamrip, qobuz-sync, qobuz-qt; for browser OAuth also qobuz-dl-go). See `docs/05-qobuz/oauth-and-tokens.ru.md`.

mod album_url;
mod api;
pub mod bundle;
mod client;
mod config;
mod error;
mod models;
pub mod oauth;
mod pagination;
pub mod signing;
mod traits;

pub use album_url::{parse_album_url, AlbumUrlError};
pub use api::streaming::Quality;
pub use client::QobuzClient;
pub use config::{AuthConfig, QobuzConfig};
pub use error::QobuzError;
pub use oauth::{
    authorize_url, fetch_oauth_bootstrap, login_with_oauth_code, redirect_uri_with_state,
    OAuthBootstrap, OAuthLoginResult,
};
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
