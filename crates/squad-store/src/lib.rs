//! Squad SQLite store: schema, migrations and the transactional commit API.
pub mod accounting;
pub mod models;
pub mod schema;

/// Open every writer/reader connection with the same durability and integrity policy.
pub fn open(path: impl AsRef<std::path::Path>) -> rusqlite::Result<rusqlite::Connection> {
    let path = path.as_ref();
    #[cfg(unix)]
    if path != std::path::Path::new(":memory:") {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .mode(0o600)
            .open(path)
            .map_err(|_| rusqlite::Error::InvalidPath(path.to_owned()))?;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(|_| rusqlite::Error::InvalidPath(path.to_owned()))?;
    }
    let connection = rusqlite::Connection::open(path)?;
    schema::configure(&connection)?;
    Ok(connection)
}

pub mod admission;
pub mod cases;
pub mod clear;
pub mod commit;
pub mod contests;
pub mod error;
pub mod events;
pub mod export;
pub mod history;
pub mod judge_repairs;
pub mod readers;
pub mod room_lock;
pub mod round_scores;
pub mod work;
pub mod writer;
pub use error::{Result, StoreError};
pub use writer::Store;
pub mod contest_steps;
