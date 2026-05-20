//! Hawk.so error catcher — sends events in the [Codex Hawk format](https://docs.hawk.so/event-format).

mod backtrace;
mod catcher;
mod config;
mod contexts;
mod error_chain;
mod event;
mod filter;
mod http_addons;
mod panic_flag;
mod scope;
mod sender;
mod source;
mod token;
mod trim;

#[cfg(feature = "axum")]
pub mod axum;

#[cfg(feature = "tracing-layer")]
pub mod tracing_layer;

pub use catcher::{CatchOpts, Hawk};
pub use config::HawkConfig;
pub use event::{AffectedUser, ErrorReport, EventLevel};
pub use sender::HawkGuard;
pub use token::{InvalidHawkToken, collector_endpoint_from_token};

#[cfg(feature = "tracing-layer")]
pub use tracing_layer::HawkLayer;

pub const CATCHER_TYPE: &str = "errors/rust";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
