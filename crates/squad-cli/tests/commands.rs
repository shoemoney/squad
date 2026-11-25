use std::{
    path::Path,
    process::{Command, Output, Stdio},
};

fn invoke(root: &Path, args: &[&str], key: Option<&str>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_squad"));
    command
        .env_clear()
        .env("HOME", root)
        .current_dir(root)
        .stdin(Stdio::null())
        .args(args);
    if let Some(key) = key {
        command.env("OPENROUTER_API_KEY", key);
    }
    command.output().unwrap()
}

#[test]
fn help_and_version_have_no_side_effects() {
    let temp = tempfile::tempdir().unwrap();
    for args in [
        vec!["--help"],
        vec!["--version"],
        vec!["send", "--help"],
        vec!["export", "--help"],
        vec!["models", "--help"],
        vec!["doctor", "--help"],
    ] {
        let output = invoke(temp.path(), &args, None);
        assert!(output.status.success(), "{output:?}");
        assert!(output.stderr.is_empty());
    }
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
}

#[test]
fn deferred_commands_fail_explicitly_without_fake_output() {
    let temp = tempfile::tempdir().unwrap();
    for (args, expected) in [
        (vec![], "TUI: not implemented yet (milestone M4)"),
        (
            vec!["send", "hello", "--json"],
            "send: not implemented yet (milestone M3)",
        ),
        (
            vec!["export", "room"],
            "export: not implemented yet (milestone M3)",
        ),
        (vec!["models"], "models: not implemented yet (milestone M3)"),
    ] {
        let output = invoke(temp.path(), &args, Some("sk-test-not-a-real-key"));
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert_eq!(String::from_utf8(output.stderr).unwrap().trim(), expected);
    }
    // Inspect the actual --data-dir independent of platform defaults.
    let data = temp.path().join("explicit-data");
    let output = invoke(
        temp.path(),
        &["send", "hello", "--data-dir", data.to_str().unwrap()],
        Some("fixture"),
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(data.is_dir());
    let entries: Vec<_> = std::fs::read_dir(&data)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(entries, [std::ffi::OsString::from("logs")]);
}

#[test]
fn quote_limit_is_enforced_across_repeated_flags() {
    let temp = tempfile::tempdir().unwrap();
    let mut args = vec!["send", "hello"];
    for _ in 0..13 {
        args.extend(["--quote", "1"]);
    }
    let output = invoke(temp.path(), &args, None);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("at most 12")
    );
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
}

#[test]
fn doctor_exit_codes_and_secret_exclusion() {
    let temp = tempfile::tempdir().unwrap();
    let output = invoke(temp.path(), &["doctor"], Some("test-secret-never-print"));
    assert_eq!(output.status.code(), Some(0));
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("Credential presence: present"));
    assert!(text.contains("Data directory: writable"));
    assert!(text.contains("unsuitable for TUI"));
    assert!(text.contains("Overall: PASS"));
    assert!(!text.contains("test-secret-never-print"));
    assert!(output.stderr.is_empty());
    let output = invoke(temp.path(), &["doctor"], None);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("Credential presence: missing")
    );
    let output = invoke(temp.path(), &["doctor"], Some(""));
    assert_eq!(output.status.code(), Some(1));
    let output = invoke(
        temp.path(),
        &["doctor", "--env-file", "missing-file"],
        Some("test-secret-never-print"),
    );
    assert_eq!(output.status.code(), Some(1));
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("Environment file not found: missing-file."));
    assert!(text.contains("Data directory: writable"));
}

#[test]
fn doctor_continues_after_unwritable_directory() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("file"), b"fixture").unwrap();
    let output = invoke(temp.path(), &["doctor", "--data-dir", "file"], None);
    assert_eq!(output.status.code(), Some(1));
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("Data directory is not writable:"));
    assert!(text.contains("Credential presence: missing"));
    assert!(text.contains("Terminal capabilities:"));
    assert!(text.contains("Model identity: skipped until M3"));
}

#[cfg(unix)]
#[test]
fn non_utf8_environment_never_panics_or_prints_values() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    for (variable, expected) in [
        ("OPENROUTER_API_KEY", 1),
        ("SQUAD_USER_NAME", 1),
        ("UNRELATED_SECRET", 0),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_squad"))
            .env_clear()
            .env("HOME", temp.path())
            .env("OPENROUTER_API_KEY", "fixture")
            .env(
                variable,
                OsString::from_vec(b"never-print-this-secret\xff".to_vec()),
            )
            .current_dir(temp.path())
            .args(["doctor"])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(expected));
        for bytes in [&output.stdout, &output.stderr] {
            let text = String::from_utf8_lossy(bytes);
            assert!(!text.contains("never-print-this-secret"));
            assert!(!text.contains("panicked"));
        }
        if expected == 1 {
            assert!(String::from_utf8_lossy(&output.stdout).contains(variable));
        }
    }
}

#[test]
fn doctor_reports_all_errors_and_uses_file_data_directory() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(
        temp.path().join(".env"),
        "OPENROUTER_API_KEY=\nSQUAD_DATA_DIR=mydata\nSQUAD_ROUNDS=99\nSQUAD_MODEL_ME=x/y\n",
    )
    .unwrap();
    let output = invoke(temp.path(), &["doctor"], None);
    assert_eq!(output.status.code(), Some(1));
    let text = String::from_utf8(output.stdout).unwrap();
    for expected in [
        "OPENROUTER_API_KEY is required",
        "SQUAD_ROUNDS must",
        "Reserved member nick",
        "Credential presence: missing",
    ] {
        assert!(text.contains(expected), "{text}");
    }
    assert!(temp.path().join("mydata").is_dir());
    assert!(!temp.path().join(".local").exists());
}

#[cfg(unix)]
#[test]
fn doctor_preserves_existing_directory_mode_and_accepts_symlinks() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    std::fs::create_dir(&data).unwrap();
    std::fs::set_permissions(&data, std::fs::Permissions::from_mode(0o755)).unwrap();
    symlink(&data, temp.path().join("link")).unwrap();
    for path in ["data", "link"] {
        let output = invoke(temp.path(), &["doctor", "--data-dir", path], None);
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.contains("Data directory: writable"), "{text}");
        assert!(text.contains("Data directory mode: 0755"));
        assert_eq!(
            std::fs::metadata(&data).unwrap().permissions().mode() & 0o777,
            0o755
        );
    }
}

#[test]
fn no_home_requires_explicit_data_directory() {
    let temp = tempfile::tempdir().unwrap();
    let invoke = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_squad"))
            .env_clear()
            .env("OPENROUTER_API_KEY", "fixture")
            .current_dir(temp.path())
            .args(args)
            .output()
            .unwrap()
    };
    let output = invoke(&["doctor"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stdout).contains("without HOME"));
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
    assert!(invoke(&["doctor", "--data-dir", "data"]).status.success());
}

#[test]
fn quoted_references_do_not_consume_the_message() {
    let temp = tempfile::tempdir().unwrap();
    for args in [
        vec!["send", "--quote", "1", "--quote", "2", "hello"],
        vec!["send", "--quote", "1,2", "hello"],
    ] {
        let output = invoke(temp.path(), &args, Some("fixture"));
        assert_eq!(output.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&output.stderr).contains("send: not implemented"));
    }
}
