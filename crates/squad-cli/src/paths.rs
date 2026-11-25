use std::{fs, io, path::Path};

/// Create missing components privately; preserve permissions of existing directories.
/// Existing parent directories (such as HOME or XDG_DATA_HOME) are not changed.
pub fn create_private_dir(path: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)?;
    let metadata = fs::metadata(path)?;
    if !metadata.is_dir() {
        return Err(io::Error::other("application path must be a directory"));
    }
    Ok(())
}

pub fn check_writable(path: &Path) -> io::Result<()> {
    use std::io::Write;
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_dir() => {}
        Ok(_) => return Err(io::Error::other("data path is not a directory")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => create_private_dir(path)?,
        Err(error) => return Err(error),
    }
    // Exclusive, private probe; never touches room rows or an existing file.
    let probe = path.join(format!(".squad-write-check-{}", std::process::id()));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&probe)?;
    let result = file
        .write_all(b"squad doctor\n")
        .and_then(|()| file.sync_all());
    drop(file);
    let cleanup = fs::remove_file(probe);
    result.and(cleanup)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn private_creation_and_writable_probe() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("nested/squad");
        check_writable(&dir).unwrap();
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
                0o700
            );
            assert_eq!(
                fs::metadata(dir.parent().unwrap())
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
        }
    }
}
