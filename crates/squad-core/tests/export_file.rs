use squad_core::export::{export_dir, write_export_file};
#[test]
fn writes_replace_atomically_and_restart_uses_the_same_destination() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path();
    let first = write_export_file(data, "prd-test.md", b"first\n").unwrap();
    assert_eq!(first, data.join("exports/prd-test.md"));
    assert_eq!(std::fs::read(&first).unwrap(), b"first\n");
    let repeated = write_export_file(data, "prd-test.md", b"second\n").unwrap();
    assert_eq!(first, repeated);
    assert_eq!(std::fs::read(&first).unwrap(), b"second\n");
    assert_eq!(
        std::fs::read_dir(export_dir(data).unwrap())
            .unwrap()
            .count(),
        1
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&first).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(first.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
    }
}
#[test]
fn unwritable_destination_and_invalid_names_preserve_existing_files() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("exports"), b"not a directory").unwrap();
    assert!(write_export_file(root.path(), "prd.md", b"data").is_err());
    assert_eq!(
        std::fs::read(root.path().join("exports")).unwrap(),
        b"not a directory"
    );
    for filename in ["../outside.md", "/outside.md", ".", ""] {
        assert!(write_export_file(root.path(), filename, b"data").is_err());
    }
}
