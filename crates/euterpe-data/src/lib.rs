pub mod connection;
pub mod error;
pub mod migrations;
pub mod repositories;

pub use connection::{DataHandle, connect_database};
pub use error::{DataError, Result};
