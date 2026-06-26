pub mod connection;
pub mod error;

pub use connection::{DataHandle, connect_database};
pub use error::{DataError, Result};
