//! Integration-test helpers split by concern. Import only what a test binary needs:
//!
//! ```ignore
//! #[path = "support/schema.rs"]
//! mod schema;
//!
//! #[path = "support/qobuz_mock.rs"]
//! mod qobuz_mock;
//! ```
//!
//! - [`schema`](schema.rs) — OpenAPI / JSON Schema validation
//! - [`qobuz_account`](qobuz_account.rs) — seed Qobuz OAuth credentials in DB
//! - [`qobuz_mock`](qobuz_mock.rs) — favorites/sync mock + `state_with_mock`
//! - [`download_mock`](download_mock.rs) — download worker mock + `state_with_download_mock`
//!
//! For default app state use `euterpe_server::app::test_support::test_state`.

pub mod download_mock;
pub mod qobuz_account;
pub mod qobuz_mock;
pub mod schema;

pub use euterpe_server::app::test_support::test_state;
