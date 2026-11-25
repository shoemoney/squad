//! Squad SQLite store: schema, migrations and the transactional commit API.
pub mod accounting;
pub mod models;
pub mod schema;

/// Open every writer/reader connection with the same durability and integrity policy.
pub fn open(path: impl AsRef<std::path::Path>) -> rusqlite::Result<rusqlite::Connection> {
    let connection = rusqlite::Connection::open(path)?;
    schema::configure(&connection)?;
    Ok(connection)
}
