//! Euterpe HTTP server (Axum).

pub mod api;
pub mod app;
pub mod config;
pub mod crypto;
pub mod credentials;
pub mod db;
pub mod error;
pub mod middleware;
pub mod openapi;
pub mod services;
pub mod state;

pub use app::{app, serve, test_support};
pub use config::AppConfig;
pub use state::AppState;
