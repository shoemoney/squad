pub use squad_core::room::{check_writable, create_private_dir};
#[cfg(test)]
use std::fs;

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
