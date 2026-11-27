use std::{
    path::Path,
    process::{Command, Output, Stdio},
};

#[path = "../../squad-agents/tests/support/fake_openrouter.rs"]
mod fake_openrouter;
use fake_openrouter::{FakeOpenRouter, MODEL};

async fn provider(responses: Vec<wiremock::ResponseTemplate>) -> FakeOpenRouter {
    let server = FakeOpenRouter::start().await;
    server.catalog(fake_openrouter::catalog()).await;
    server.completions(responses).await;
    server
        .generation(
            fake_openrouter::GENERATION,
            serde_json::json!({"total_cost": 0.000045}),
        )
        .await;
    server
}

async fn invoke_provider(root: &Path, args: &[&str], server: &FakeOpenRouter) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_squad"));
    command
        .env_clear()
        .env("HOME", root)
        .current_dir(root)
        .env("OPENROUTER_API_KEY", "fixture")
        .env("OPENROUTER_BASE_URL", server.base_url())
        .env("SQUAD_MODEL_A", MODEL)
        .env("SQUAD_MODEL_B", MODEL)
        .env("SQUAD_JUDGE_MODEL", MODEL)
        .stdin(Stdio::null())
        .args(args);
    // Keep the async executor free to serve wiremock while the executable runs.
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        if child.try_wait().unwrap().is_some() {
            return child.wait_with_output().unwrap();
        }
        if std::time::Instant::now() >= deadline {
            child.kill().unwrap();
            panic!("CLI timed out: {:?}", child.wait_with_output().unwrap());
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}

fn user_messages(data: &Path) -> Vec<(i64, serde_json::Value)> {
    let db = squad_store::open(data.join("squad.db")).unwrap();
    let mut query = db
        .prepare("SELECT seq, payload FROM events WHERE kind = 'user.message' ORDER BY seq")
        .unwrap();
    query
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                serde_json::from_str(&row.get::<_, String>(1)?).unwrap(),
            ))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}

fn invoke(root: &Path, args: &[&str], key: Option<&str>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_squad"));
    command
        .env_clear()
        .env("HOME", root)
        .env("HTTPS_PROXY", "http://127.0.0.1:9")
        .env("HTTP_PROXY", "http://127.0.0.1:9")
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

#[tokio::test]
async fn bound_commands_only_report_real_provider_output() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("explicit-data");
    let server = provider(vec![fake_openrouter::streaming(
        "Provider supplied answer",
        "stop",
    )])
    .await;
    let output = invoke_provider(
        temp.path(),
        &["--data-dir", data.to_str().unwrap()],
        &server,
    )
    .await;
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr).trim(),
        "Terminal application failed."
    );
    assert!(server.captured_bodies().await.is_empty());
    // Terminal setup now precedes room startup, so a piped interactive
    // invocation fails before opening SQLite or touching the provider.
    assert!(!data.join("squad.db").exists());
    let output = invoke_provider(
        temp.path(),
        &["send", "@a hello", "--data-dir", data.to_str().unwrap()],
        &server,
    )
    .await;
    assert!(output.status.success(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stdout).contains("Provider supplied answer"));
    assert_eq!(server.captured_bodies().await.len(), 1);
    assert_eq!(user_messages(&data)[0].1["text"], "@a hello");

    let temp = tempfile::tempdir().unwrap();
    let server = provider(vec![wiremock::ResponseTemplate::new(401)]).await;
    let output = invoke_provider(
        temp.path(),
        &["send", "@a hello", "--data-dir", "data", "--json"],
        &server,
    )
    .await;
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    let records: Vec<serde_json::Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(
        records.iter().any(|record| record["type"] == "ack"),
        "{records:?}"
    );
    assert!(
        !records
            .iter()
            .any(|record| record.to_string().contains("message.final")),
        "{records:?}"
    );
    assert_eq!(server.captured_bodies().await.len(), 1);
    let db = squad_store::open(temp.path().join("data/squad.db")).unwrap();
    let finals: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM events WHERE kind = 'message.final'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(finals, 0);
}

#[tokio::test]
async fn bound_commands_validate_before_provider_calls() {
    let temp = tempfile::tempdir().unwrap();
    let server = provider(vec![]).await;
    let data = temp.path().join("explicit-data");
    let output = invoke_provider(
        temp.path(),
        &["send", "hello", "--data-dir", data.to_str().unwrap()],
        &server,
    )
    .await;
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert_eq!(
        String::from_utf8(output.stderr).unwrap().trim(),
        "Add at least one recipient: @nick or @all."
    );
    assert!(data.is_dir());
    let entries = std::fs::read_dir(&data)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        entries,
        ["logs", "main.lock", "squad.db"]
            .into_iter()
            .map(std::ffi::OsString::from)
            .collect()
    );
    assert!(server.captured_bodies().await.is_empty());
    assert!(user_messages(&data).is_empty());
}

#[tokio::test]
async fn unavailable_quoted_references_preserve_the_exact_error() {
    let temp = tempfile::tempdir().unwrap();
    let server = provider(vec![]).await;
    for mut args in [
        vec!["send", "--quote", "1", "--quote", "2", "hello"],
        vec!["send", "--quote", "1,2", "hello"],
    ] {
        args.extend(["--data-dir", "data"]);
        let output = invoke_provider(temp.path(), &args, &server).await;
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        assert!(output.stdout.is_empty(), "{output:?}");
        assert_eq!(
            String::from_utf8(output.stderr).unwrap().trim(),
            "Quoted message 1 is unavailable."
        );
    }
    assert!(server.captured_bodies().await.is_empty());
    assert!(user_messages(&temp.path().join("data")).is_empty());
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
#[tokio::test]
async fn doctor_warns_about_existing_public_directory_and_send_accepts_symlinks() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data's folder");
    std::fs::create_dir(&data).unwrap();
    std::fs::set_permissions(&data, std::fs::Permissions::from_mode(0o755)).unwrap();
    symlink(&data, temp.path().join("link")).unwrap();
    let server = provider(vec![
        fake_openrouter::streaming("Directory answer", "stop");
        2
    ])
    .await;
    for path in ["data's folder", "link"] {
        let output = invoke(temp.path(), &["doctor", "--data-dir", path], None);
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.contains("Data directory: writable"), "{text}");
        assert!(text.contains("Data directory mode: 0755"));
        assert!(text.contains("Data directory permissions: WARN"), "{text}");
        // The warning names the same directory the "writable" line reports (macOS temp
        // dirs canonicalize under /private), and that directory is the one under test.
        let reported = text
            .lines()
            .find_map(|l| l.strip_prefix("Data directory: writable - "))
            .expect("writable line");
        assert!(
            text.contains(&format!("chmod 700 '{}'", reported.replace("'", "'\\''"))),
            "{text}"
        );
        assert_eq!(
            std::fs::canonicalize(reported).unwrap(),
            std::fs::canonicalize(&data).unwrap(),
            "{text}"
        );
        let output = invoke_provider(
            temp.path(),
            &["send", "@a hello", "--data-dir", path],
            &server,
        )
        .await;
        assert!(output.status.success(), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
        assert!(String::from_utf8_lossy(&output.stdout).contains("Directory answer"));
        assert_eq!(
            std::fs::metadata(&data).unwrap().permissions().mode() & 0o777,
            0o755
        );
    }
    assert!(
        std::fs::symlink_metadata(temp.path().join("link"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    std::fs::set_permissions(&data, std::fs::Permissions::from_mode(0o700)).unwrap();
    let output = invoke(temp.path(), &["doctor", "--data-dir", "link"], None);
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("Data directory mode: 0700"), "{text}");
    assert!(!text.contains("Data directory permissions: WARN"), "{text}");
    assert_eq!(server.captured_bodies().await.len(), 2);
    assert_eq!(user_messages(&data).len(), 2);
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

#[tokio::test]
async fn quoted_references_do_not_consume_the_message() {
    let temp = tempfile::tempdir().unwrap();
    let server = provider(vec![
        fake_openrouter::streaming(
            "Quoted fixture answer",
            "stop"
        );
        4
    ])
    .await;
    for message in ["@a First source", "@a Second source"] {
        let output = invoke_provider(
            temp.path(),
            &["send", message, "--data-dir", "data"],
            &server,
        )
        .await;
        assert!(output.status.success(), "{output:?}");
    }
    let sources = user_messages(&temp.path().join("data"));
    let first = sources[0].0.to_string();
    let second = sources[1].0.to_string();
    let combined = format!("{first},{second}");
    for args in [
        vec!["send", "--quote", &first, "--quote", &second, "@a hello"],
        vec!["send", "--quote", &combined, "@a hello"],
    ] {
        let mut args = args;
        args.extend(["--data-dir", "data"]);
        let output = invoke_provider(temp.path(), &args, &server).await;
        assert!(output.status.success(), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
        let messages = user_messages(&temp.path().join("data"));
        let last = &messages.last().unwrap().1;
        assert_eq!(last["text"], "@a hello");
        assert_eq!(last["resolvedQuotes"].as_array().unwrap().len(), 2);
        assert_eq!(last["resolvedQuotes"][0]["text"], "First source");
        assert_eq!(last["resolvedQuotes"][1]["text"], "Second source");
    }
    let bodies = server.captured_bodies().await;
    assert_eq!(bodies.len(), 4);
    for body in &bodies[2..] {
        let prompt = body["messages"].to_string();
        for text in ["hello", "First source", "Second source"] {
            assert!(prompt.contains(text), "{prompt}");
        }
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
            "could not read export snapshot"
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
fn quoted_reference_flags_preserve_message_arguments() {
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
fn invalid_census_seed_reports_exact_error_and_continues_independent_doctor_checks() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(
        temp.path().join(".env"),
        "OPENROUTER_API_KEY=fixture\nSQUAD_CENSUS_PASSES=6\n",
    )
    .unwrap();
    let output = invoke(temp.path(), &["doctor"], None);
    assert_eq!(output.status.code(), Some(1));
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("SQUAD_CENSUS_PASSES must be an integer from 1 to 5."));
    assert!(text.contains("Data directory: writable"));
    assert!(text.contains("Terminal capabilities:"));
    assert!(text.contains("Daily accounting:"));
}

#[tokio::test]
async fn doctor_online_uses_only_free_get_endpoints_and_checks_key() {
    use wiremock::{
        Mock, ResponseTemplate,
        matchers::{method, path},
    };
    for status in [200, 401] {
        let temp = tempfile::tempdir().unwrap();
        let server = provider(vec![]).await;
        Mock::given(method("GET"))
            .and(path("/api/v1/key"))
            .respond_with(
                ResponseTemplate::new(status)
                    .set_body_json(serde_json::json!({"data":{"label":"fixture"}})),
            )
            .mount(&server.server)
            .await;
        let output = invoke_provider(
            temp.path(),
            &["--env-file", "/dev/null", "doctor", "--online"],
            &server,
        )
        .await;
        let text = String::from_utf8(output.stdout).unwrap();
        assert_eq!(output.status.success(), status == 200, "{text}");
        assert!(text.contains("OpenRouter reachability: PASS"), "{text}");
        assert!(
            text.contains(if status == 200 {
                "OpenRouter key validity: PASS"
            } else {
                "OpenRouter key validity: FAIL - HTTP 401"
            }),
            "{text}"
        );
        let requests = server.server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 2);
        assert!(requests.iter().all(|r| r.method == "GET"));
        let offline =
            invoke_provider(temp.path(), &["--env-file", "/dev/null", "doctor"], &server).await;
        assert!(
            String::from_utf8(offline.stdout)
                .unwrap()
                .contains("Online checks not run (use --online)")
        );
        assert_eq!(server.server.received_requests().await.unwrap().len(), 2);
    }
}
