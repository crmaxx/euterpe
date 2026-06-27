use euterpe_data::{DataError, connect_database};
use welds::Syntax;
use welds::connections::Client;

#[tokio::test]
async fn connects_to_in_memory_sqlite_database() {
    let handle = connect_database("sqlite::memory:").await.unwrap();

    assert_eq!(handle.client().syntax(), Syntax::Sqlite);
}

#[tokio::test]
async fn creates_parent_directory_for_file_backed_sqlite_url() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("nested/library.db");
    let database_url = format!("sqlite:{}?mode=rwc", db_path.display());

    let handle = connect_database(&database_url).await.unwrap();

    assert_eq!(handle.client().syntax(), Syntax::Sqlite);
    assert!(temp.path().join("nested").is_dir());
}

#[tokio::test]
async fn malformed_sqlite_url_returns_configuration_error() {
    let error = match connect_database("sqlite:").await {
        Ok(_) => panic!("malformed sqlite URL unexpectedly connected"),
        Err(error) => error,
    };

    assert!(
        matches!(error, DataError::Config(_) | DataError::Database(_)),
        "unexpected error: {error:?}"
    );
}
