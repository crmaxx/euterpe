//! SQLite-only facade for the workspace.
//!
//! The upstream `sqlx` meta-crate currently enables optional dependency edges
//! that conflict with the vendored SMB/SSPI stack when resolving `sqlx = 0.9`.
//! Euterpe uses SQLite only, so this crate exposes the SQLx core/SQLite API
//! surface needed by Welds and by the local code without pulling MySQL,
//! Postgres, or macro dependencies into the graph.

pub use sqlx_core::column::Column;
pub use sqlx_core::database::Database;
pub use sqlx_core::decode::Decode;
pub use sqlx_core::executor::Executor;
pub use sqlx_core::from_row::FromRow;
pub use sqlx_core::query::{Query, query};
pub use sqlx_core::query_as::{QueryAs, query_as};
pub use sqlx_core::query_scalar::{QueryScalar, query_scalar};
pub use sqlx_core::row::Row;
pub use sqlx_core::sql_str::AssertSqlSafe;
pub use sqlx_core::transaction::Transaction;
pub use sqlx_core::types::Type;
pub use sqlx_core::*;
pub use sqlx_sqlite::{
    Sqlite, SqliteArguments, SqliteColumn, SqliteConnectOptions, SqliteConnection, SqliteError,
    SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteQueryResult, SqliteRow,
    SqliteStatement, SqliteTransaction, SqliteTypeInfo, SqliteValue, SqliteValueRef,
};

pub mod sqlite {
    pub use sqlx_sqlite::{
        Sqlite, SqliteArguments, SqliteColumn, SqliteConnectOptions, SqliteConnection, SqliteError,
        SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteQueryResult, SqliteRow,
        SqliteStatement, SqliteTransaction, SqliteTypeInfo, SqliteValue, SqliteValueRef,
    };
}
