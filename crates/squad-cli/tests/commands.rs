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
    assert!(!temp.path().join(".local/share/squad/main.lock").exists());
    assert!(!temp.path().join(".local/share/squad/main.db").exists());
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
