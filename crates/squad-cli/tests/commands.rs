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
fn missing_credentials_fail_before_opening_the_room() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("explicit-data");
    for args in [
        vec![],
        vec!["send", "hello"],
        vec!["send", "hello", "--data-dir", data.to_str().unwrap()],
    ] {
        let output = invoke(temp.path(), &args, None);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("OPENROUTER_API_KEY is required"));
    }
    assert!(!data.exists());
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
    assert!(text.contains("Model identity: no recorded attempts"));
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
        assert!(!String::from_utf8_lossy(&output.stdout).contains("key is required"));
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
fn doctor_and_send_preserve_existing_directory_mode_and_accept_symlinks() {
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
        let output = invoke(temp.path(), &["send", "hello", "--data-dir", path], None);
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(
            String::from_utf8_lossy(&output.stderr).trim(),
            "OPENROUTER_API_KEY is required. Add it to .env or the process environment."
        );
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
    use clap::Parser;
    for args in [
        vec!["squad", "send", "--quote", "1", "--quote", "2", "hello"],
        vec!["squad", "send", "--quote", "1,2", "hello"],
    ] {
        let cli = squad_cli::args::Cli::try_parse_from(args).unwrap();
        let Some(squad_cli::args::Command::Send(send)) = cli.command else {
            panic!("send expected")
        };
        assert_eq!(send.message, "hello");
        assert_eq!(send.quote, [1, 2]);
    }
}

#[test]
fn all_export_scopes_are_visible_typed_and_preserve_renderer_errors() {
    let temp = tempfile::tempdir().unwrap();
    let help = invoke(temp.path(), &["export", "--help"], None);
    let help = String::from_utf8(help.stdout).unwrap();
    for scope in [
        "room",
        "debate",
        "grill",
        "tournament",
        "census",
        "crosscheck",
        "survey",
        "probe",
    ] {
        assert!(help.contains(scope), "{help}");
        let mut args = vec!["export", scope];
        if scope != "room" {
            let missing = invoke(temp.path(), &args, None);
            assert_eq!(missing.status.code(), Some(2));
            args.push("invalid-id");
            assert_eq!(invoke(temp.path(), &args, None).status.code(), Some(2));
            args.pop();
            args.push("01900000-0000-7000-8000-000000000001");
        }
        args.extend(["-o", "out.md"]);
        let output = invoke(temp.path(), &args, None);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert_eq!(
            String::from_utf8_lossy(&output.stderr).trim(),
            if matches!(scope, "census" | "crosscheck" | "survey" | "probe") {
                "export scope is not implemented"
            } else {
                "could not read export snapshot"
            }
        );
    }
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
}

#[cfg(unix)]
#[test]
fn invalid_utf8_key_has_only_the_encoding_diagnostic_for_every_command() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let temp = tempfile::tempdir().unwrap();
    for args in [vec![], vec!["doctor"], vec!["send", "hello"]] {
        let output = Command::new(env!("CARGO_BIN_EXE_squad"))
            .env_clear()
            .env("HOME", temp.path())
            .env(
                "OPENROUTER_API_KEY",
                OsString::from_vec(b"secret-never-print\xff".to_vec()),
            )
            .current_dir(temp.path())
            .args(&args)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(text.matches("OPENROUTER_API_KEY").count(), 1, "{text}");
        assert!(text.contains("OPENROUTER_API_KEY must contain valid UTF-8."));
        assert!(!text.contains("is required"));
        assert!(!text.contains("secret-never-print"));
    }
}

#[test]
fn models_missing_snapshot_is_read_only_and_requires_no_credential() {
    let temp = tempfile::tempdir().unwrap();
    let output = invoke(temp.path(), &["models"], None);
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("Open the room to resolve models"));
    assert!(output.stderr.is_empty());
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
}

#[test]
fn offline_export_ignores_invalid_generation_config_and_a_live_room_lock() {
    let temp = tempfile::tempdir().unwrap();
    let mut db = squad_store::open(temp.path().join("squad.db")).unwrap();
    squad_store::schema::migrate(&mut db).unwrap();
    db.execute("INSERT INTO room_meta VALUES ('main',1,1,1,1,NULL)", [])
        .unwrap();
    let _lock = squad_store::room_lock::RoomLock::acquire(&temp.path().join("main.lock")).unwrap();
    std::fs::write(
        temp.path().join(".env"),
        "SQUAD_ROUNDS=invalid\nOPENROUTER_API_KEY=\n",
    )
    .unwrap();
    let output = invoke(
        temp.path(),
        &[
            "export",
            "room",
            "--data-dir",
            temp.path().to_str().unwrap(),
        ],
        None,
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn json_startup_failures_have_error_records_without_fabricated_request_ids() {
    let temp = tempfile::tempdir().unwrap();
    let output = invoke(temp.path(), &["send", "hello", "--json"], None);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["type"], "error");
    assert_eq!(value["error"]["code"], "CONFIG_INVALID");
    assert!(
        value["error"]["message"]
            .as_str()
            .unwrap()
            .contains("OPENROUTER_API_KEY is required")
    );
    assert!(value.get("requestId").is_none());
    assert!(value.get("cursor").is_none());
}
