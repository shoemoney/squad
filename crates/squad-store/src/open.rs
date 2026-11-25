//! Private on-disk files with the same durability policy on every connection.
use std::path::Path;

pub fn private_dir(path: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

pub fn open(path: impl AsRef<Path>) -> rusqlite::Result<rusqlite::Connection> {
    let path = path.as_ref();
    if path != Path::new(":memory:") {
        #[cfg(unix)]
        {
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
            // SQLite inherits the database mode for new auxiliaries. Correct existing
            // files before opening as well, including a crash's rollback journal.
            for suffix in ["-wal", "-shm", "-journal"] {
                let mut name = path.as_os_str().to_owned();
                name.push(suffix);
                match std::fs::set_permissions(
                    Path::new(&name),
                    std::fs::Permissions::from_mode(0o600),
                ) {
                    Ok(()) => (),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                    Err(_) => return Err(rusqlite::Error::InvalidPath(path.to_owned())),
                }
            }
        }
    }
    let connection = rusqlite::Connection::open(path)?;
    crate::schema::configure(&connection)?;
    Ok(connection)
}
