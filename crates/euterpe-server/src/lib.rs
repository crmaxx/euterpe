//! Euterpe HTTP server (Axum).

pub mod api;
pub mod app;
pub mod config;
pub mod credentials;
pub mod crypto;
pub mod error;
pub mod integrations;
pub mod library;
pub mod middleware;
pub mod openapi;
pub mod routes;
pub mod services;
pub mod state;
mod static_files;
#[cfg(test)]
pub(crate) mod test_db;

pub use app::{app, serve, test_support};
pub use config::AppConfig;
pub use state::{AppChannels, AppState};
