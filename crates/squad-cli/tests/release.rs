#![allow(dead_code)]
use std::{
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

#[path = "../../squad-agents/tests/support/fake_openrouter.rs"]
mod fake_openrouter;
use fake_openrouter::{FakeOpenRouter, MODEL};
mod support;
use squad_protocol::*;

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

/// The optimized binary users actually receive, built once per test thread.
fn release_binary() -> PathBuf {
    thread_local! {
        // A thread-local owner drops the snapshot when the test thread exits;
        // a static OnceLock<TempDir> would leak a binary per Nextest process.
        static BINARY: std::cell::OnceCell<(tempfile::TempDir, PathBuf)> = const {
            std::cell::OnceCell::new()
        };
    }
    BINARY.with(|binary| {
        binary
            .get_or_init(|| {
                let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
                // Ask Cargo so config-file and relative target directories work too.
                let metadata = Command::new(&cargo)
                    .args([
                        "metadata",
                        "--no-deps",
                        "--format-version",
                        "1",
                        "--offline",
                    ])
                    .current_dir(workspace())
                    .output()
                    .unwrap();
                assert!(metadata.status.success(), "{}", stderr(&metadata));
                let metadata: serde_json::Value = serde_json::from_slice(&metadata.stdout).unwrap();
                let target = PathBuf::from(metadata["target_directory"].as_str().unwrap());
                std::fs::create_dir_all(&target).unwrap();

                // Nextest runs each test in its own process: a local cache does not
                // serialize these builds. On macOS Cargo unlinks and copies even a
                // fresh target/release/squad (rather than hard-linking). Lock the copy,
                // then execute only the private snapshot, never Cargo's mutable path.
                let lock = std::fs::File::options()
                    .create(true)
                    .truncate(false)
                    .read(true)
                    .write(true)
                    .open(target.join(".squad-release-test.lock"))
                    .unwrap();
                lock.lock().unwrap();
                let status = Command::new(&cargo)
                    .args(["build", "--release", "--locked", "-p", "squad-cli"])
                    .current_dir(workspace())
                    .status()
                    .unwrap();
                assert!(status.success(), "release build failed");
                snapshot_binary(&target.join("release/squad"))
            })
            .1
            .clone()
    })
}

fn snapshot_binary(source: &Path) -> (tempfile::TempDir, PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let binary = directory.path().join("squad");
    // Copy, rather than hard-link, so later writes cannot change our executable.
    std::fs::copy(source, &binary).unwrap();
    (directory, binary)
}

#[test]
fn release_binary_snapshot_survives_source_replacement_and_removal() {
    let source = tempfile::tempdir().unwrap();
    let executable = source.path().join("squad");
    std::fs::copy(release_binary(), &executable).unwrap();
    let (_snapshot, binary) = snapshot_binary(&executable);
    std::fs::write(&executable, b"replaced by another build").unwrap();
    source.close().unwrap();

    let root = tempfile::tempdir().unwrap();
    let output = Command::new(&binary)
        .arg("--version")
        .env_clear()
        .env("HOME", root.path())
        .current_dir(root.path())
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        text(&output).trim(),
        format!("squad {}", env!("CARGO_PKG_VERSION"))
    );
}

fn squad(root: &Path) -> Command {
    let mut command = Command::new(release_binary());
    command
        .env_clear()
        .env("HOME", root)
        .env("HTTPS_PROXY", "http://127.0.0.1:9")
        .env("HTTP_PROXY", "http://127.0.0.1:9")
        .current_dir(root)
        .stdin(Stdio::null());
    command
}

fn big_catalog() -> serde_json::Value {
    serde_json::json!({"data": [{"id": MODEL, "name": "Fixture reasoner", "context_length": 2000000,
        "top_provider": {"max_completion_tokens": 100000},
        "supported_parameters": ["reasoning", "response_format"],
        "pricing": {"prompt": "0.000001", "completion": "0.000002"}}]})
}

async fn provider() -> FakeOpenRouter {
    provider_with(vec![fake_openrouter::streaming("x", "stop"); 400]).await
}

async fn provider_with(responses: Vec<wiremock::ResponseTemplate>) -> FakeOpenRouter {
    let server = FakeOpenRouter::start().await;
    server.catalog(big_catalog()).await;
    server.completions(responses).await;
    server
        .generation(
            fake_openrouter::GENERATION,
            serde_json::json!({"total_cost": 0.000045}),
        )
        .await;
    server
}

/// Runs the release binary against the loopback provider without blocking the
/// executor that serves it.
async fn run(root: &Path, args: &[&str], server: &FakeOpenRouter, roster: usize) -> Output {
    run_env(root, args, server, roster, &[], 20).await.0
}

async fn run_env(
    root: &Path,
    args: &[&str],
    server: &FakeOpenRouter,
    roster: usize,
    env: &[(&str, &str)],
    seconds: u64,
) -> (Output, bool) {
    let mut command = squad(root);
    command.envs(env.iter().copied());
    command
        .env("OPENROUTER_API_KEY", "fixture")
        .env("OPENROUTER_BASE_URL", server.base_url())
        .env("SQUAD_JUDGE_MODEL", MODEL)
        .env_remove("HTTPS_PROXY")
        .env_remove("HTTP_PROXY");
    for nick in ["a", "b", "c", "d", "e", "f"].iter().take(roster) {
        command.env(format!("SQUAD_MODEL_{}", nick.to_uppercase()), MODEL);
    }
    let (output, timed_out) = spawn_and_collect(command.args(args), seconds).await;
    (output, timed_out)
}

/// Drains both pipes on threads: contest records are large and a child blocked on
/// a full stdout pipe would look like a hang.
async fn spawn_and_collect(command: &mut Command, seconds: u64) -> (Output, bool) {
    use std::io::Read;
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut out = child.stdout.take().unwrap();
    let mut err = child.stderr.take().unwrap();
    let out = std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = out.read_to_end(&mut buffer);
        buffer
    });
    let err = std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = err.read_to_end(&mut buffer);
        buffer
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(seconds);
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if std::time::Instant::now() >= deadline {
            child.kill().unwrap();
            timed_out = true;
            break child.wait().unwrap();
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    };
    (
        Output {
            status,
            stdout: out.join().unwrap(),
            stderr: err.join().unwrap(),
        },
        timed_out,
    )
}

const CONFIRM_ALWAYS: &[(&str, &str)] = &[("SQUAD_DEBATE_CONFIRM_USD", "0")];

fn text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn json_records(output: &Output) -> Vec<serde_json::Value> {
    text(output)
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

fn event_kinds(output: &Output) -> Vec<String> {
    json_records(output)
        .iter()
        .filter(|record| record["type"] == "event")
        .map(|record| record["event"]["kind"].as_str().unwrap().to_owned())
        .collect()
}

/// The contest identifier printed by the preflight record (or debate record).
fn contest_id(output: &Output) -> String {
    json_records(output)
        .iter()
        .find_map(|record| {
            let payload = &record["event"]["payload"];
            [
                &record["contestId"],
                &record["debateId"],
                &payload["contestId"],
                &payload["debateId"],
                &payload["base"]["contestId"],
            ]
            .into_iter()
            .find_map(|value| value.as_str())
            .map(str::to_owned)
        })
        .unwrap_or_else(|| panic!("no contest id in {}", text(output)))
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn db(root: &Path) -> rusqlite::Connection {
    squad_store::open(root.join("data/squad.db")).unwrap()
}

fn count(root: &Path, kind: &str) -> i64 {
    db(root)
        .query_row("SELECT COUNT(*) FROM events WHERE kind = ?1", [kind], |r| {
            r.get(0)
        })
        .unwrap()
}

// ---------------------------------------------------------------------------
// M7.P2.L1.T3: reported versions and packaged crate compatibility
// ---------------------------------------------------------------------------

#[test]
fn release_binary_reports_the_workspace_version_and_help_has_no_side_effects() {
    let root = tempfile::tempdir().unwrap();
    let version = squad(root.path()).arg("--version").output().unwrap();
    assert!(version.status.success());
    assert_eq!(text(&version), "squad 0.1.3\n");
    assert_eq!(
        text(&version).trim(),
        format!("squad {}", env!("CARGO_PKG_VERSION"))
    );
    let mut helps: Vec<Vec<&str>> = vec![
        vec!["--help"],
        vec!["send", "--help"],
        vec!["export", "--help"],
        vec!["models", "--help"],
        vec!["doctor", "--help"],
    ];
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
        helps.push(vec!["export", scope, "--help"]);
    }
    for args in helps {
        let output = squad(root.path()).args(&args).output().unwrap();
        assert!(output.status.success(), "{args:?}: {output:?}");
        assert!(output.stderr.is_empty(), "{args:?}");
    }
    let overview = squad(root.path()).arg("--help").output().unwrap();
    for command in ["send", "export", "models", "doctor"] {
        assert!(text(&overview).contains(command), "{}", text(&overview));
    }
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}

#[test]
fn six_workspace_crates_carry_release_metadata_and_versioned_internal_dependencies() {
    let output = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args([
            "metadata",
            "--no-deps",
            "--format-version",
            "1",
            "--offline",
        ])
        .current_dir(workspace())
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let mut names = Vec::new();
    for package in metadata["packages"].as_array().unwrap() {
        let name = package["name"].as_str().unwrap();
        names.push(name.to_owned());
        assert_eq!(package["version"], "0.1.3", "{name}");
        assert_eq!(package["license"], "Apache-2.0", "{name}");
        assert_eq!(
            package["repository"], "https://github.com/shoemoney/squad",
            "{name}"
        );
        assert!(
            !package["description"].as_str().unwrap_or("").is_empty(),
            "{name}"
        );
        assert!(package["rust_version"].is_string(), "{name}");
        for dependency in package["dependencies"].as_array().unwrap() {
            if dependency["name"].as_str().unwrap().starts_with("squad-") {
                assert_ne!(dependency["req"], "*", "{name} -> {}", dependency["name"]);
                assert_eq!(
                    dependency["req"], "^0.1.3",
                    "{name} -> {}",
                    dependency["name"]
                );
            }
        }
    }
    names.sort();
    assert_eq!(
        names,
        [
            "squad-agents",
            "squad-cli",
            "squad-core",
            "squad-protocol",
            "squad-store",
            "squad-tui"
        ]
    );
}

#[test]
fn license_and_notice_records_are_complete() {
    let license = std::fs::read_to_string(workspace().join("LICENSE")).unwrap();
    for needle in [
        "Apache License",
        "Version 2.0, January 2004",
        "TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION",
        "END OF TERMS AND CONDITIONS",
        "APPENDIX: How to apply the Apache License to your work.",
    ] {
        assert!(license.contains(needle), "LICENSE lacks {needle:?}");
    }
    let notice = std::fs::read_to_string(workspace().join("NOTICE")).unwrap();
    for needle in [
        "Squad",
        "Apache License, Version 2.0",
        "FIGlet",
        "Glenn Chappell",
        "BSD-3-Clause",
    ] {
        assert!(notice.contains(needle), "NOTICE lacks {needle:?}");
    }
    let fonts = workspace().join("crates/squad-core/src/banner/fonts");
    for file in ["standard.flf", "small.flf", "LICENSE-FIGlet.txt"] {
        assert!(fonts.join(file).is_file(), "{file}");
    }
}

#[test]
fn every_crate_packages_without_secrets_and_the_cli_ships_its_sources() {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    for krate in [
        "squad-protocol",
        "squad-store",
        "squad-agents",
        "squad-core",
        "squad-tui",
        "squad-cli",
    ] {
        let output = Command::new(&cargo)
            .args([
                "package",
                "-p",
                krate,
                "--list",
                "--allow-dirty",
                "--offline",
            ])
            .current_dir(workspace())
            .output()
            .unwrap();
        assert!(output.status.success(), "{krate}: {}", stderr(&output));
        let files = text(&output);
        assert!(files.lines().any(|line| line == "Cargo.toml"), "{krate}");
        assert!(
            files.lines().any(|line| line.starts_with("src/")),
            "{krate}"
        );
        for line in files.lines() {
            assert!(
                !line.starts_with(".env") && !line.contains("/.env") && !line.ends_with(".db"),
                "{krate} packages {line}"
            );
        }
        if krate == "squad-cli" {
            for file in [
                "src/main.rs",
                "src/args.rs",
                "src/export.rs",
                "src/doctor.rs",
                "README.md",
            ] {
                assert!(files.lines().any(|line| line == file), "missing {file}");
            }
        }
    }
}

/// Regression: the embedded prompt pack must live inside the squad-core crate
/// (`crates/squad-core/prompts/`), or a packaged squad-core and a crates.io
/// `cargo install squad-cli` cannot compile.
#[test]
fn packaged_squad_core_contains_every_embedded_prompt() {
    let output = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args([
            "package",
            "-p",
            "squad-core",
            "--list",
            "--allow-dirty",
            "--offline",
        ])
        .current_dir(workspace())
        .output()
        .unwrap();
    let files = text(&output);
    let embedded =
        std::fs::read_to_string(workspace().join("crates/squad-core/src/prompts/embedded.rs"))
            .unwrap();
    for line in embedded
        .lines()
        .filter(|line| line.contains("include_str!(\"../../prompts/"))
    {
        let name = line
            .split("../../prompts/")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap();
        assert!(
            files.lines().any(|file| file.ends_with(name)),
            "{name} is embedded but not packaged"
        );
    }
}

// ---------------------------------------------------------------------------
// M7.P1.L2.T4: commands, JSON output, exit codes, pre-confirmation, export
// ---------------------------------------------------------------------------

#[tokio::test]
async fn send_text_and_json_output_and_exit_codes() {
    let server = provider_with(vec![
        fake_openrouter::streaming("Release answer", "stop"),
        fake_openrouter::streaming("Release answer", "stop"),
        wiremock::ResponseTemplate::new(401),
    ])
    .await;
    let root = tempfile::tempdir().unwrap();
    let human = run(
        root.path(),
        &["send", "@a hello", "--data-dir", "data"],
        &server,
        2,
    )
    .await;
    assert_eq!(human.status.code(), Some(0), "{human:?}");
    assert!(
        text(&human).starts_with("Accepted (cursor "),
        "{}",
        text(&human)
    );
    assert!(text(&human).contains("a: Release answer"));
    assert!(stderr(&human).is_empty());

    let json = run(
        root.path(),
        &["send", "@a hello", "--json", "--data-dir", "data"],
        &server,
        2,
    )
    .await;
    assert_eq!(json.status.code(), Some(0), "{json:?}");
    let records = json_records(&json);
    assert_eq!(records[0]["type"], "ack");
    assert!(
        records
            .iter()
            .any(|r| r["event"]["kind"] == "message.final")
    );
    let total = records.last().unwrap();
    assert_eq!(total["type"], "usage.total");
    assert!(
        total.to_string().contains("unknown"),
        "usage.total must state the unknown-cost call count: {total}"
    );

    let failed = run(
        root.path(),
        &["send", "@a hello", "--json", "--data-dir", "data"],
        &server,
        2,
    )
    .await;
    assert_eq!(failed.status.code(), Some(1), "{failed:?}");
    assert!(json_records(&failed).iter().any(|r| r["type"] == "ack"));

    let invalid = run(
        root.path(),
        &["send", "no recipient", "--data-dir", "data"],
        &server,
        2,
    )
    .await;
    assert_eq!(invalid.status.code(), Some(1));
    assert_eq!(
        stderr(&invalid).trim(),
        "Add at least one recipient: @nick or @all."
    );
    let json_invalid = run(
        root.path(),
        &["send", "no recipient", "--json", "--data-dir", "data"],
        &server,
        2,
    )
    .await;
    assert_eq!(json_invalid.status.code(), Some(1));
    assert_eq!(
        json_records(&json_invalid)[0]["error"]["code"],
        "RECIPIENT_REQUIRED"
    );

    let usage = run(root.path(), &["send", "--bogus"], &server, 2).await;
    assert_eq!(usage.status.code(), Some(2));
    assert_eq!(server.captured_bodies().await.len(), 3);
}

#[test]
fn bus_log_bodies_flag_requires_bus_log_and_is_documented() {
    let root = tempfile::tempdir().unwrap();
    let output = squad(root.path())
        .args(["send", "@a hi", "--bus-log-bodies"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("--bus-log"));
    let help = squad(root.path()).arg("--help").output().unwrap();
    assert!(text(&help).contains("--bus-log <file>") && text(&help).contains("--bus-log-bodies"));
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}

/// PRODUCT BUG (reported, not fixed here): `--bus-log` is accepted by `send` but
/// nothing consumes it (no file is written), and `doctor --bus-log` still fails
/// with "not implemented yet (milestone M3)" at crates/squad-cli/src/main.rs:148.
#[tokio::test]
async fn bus_log_is_written_and_bodies_require_the_explicit_flag() {
    let server = provider_with(vec![fake_openrouter::streaming("Reply", "stop"); 2]).await;
    let root = tempfile::tempdir().unwrap();
    for (name, flags) in [
        ("plain.jsonl", vec![]),
        ("bodies.jsonl", vec!["--bus-log-bodies"]),
    ] {
        let log = root.path().join(name);
        let mut args = vec![
            "send",
            "@a SECRET-BODY-TEXT",
            "--data-dir",
            "data",
            "--bus-log",
            log.to_str().unwrap(),
        ];
        args.extend(flags.iter().copied());
        let output = run(root.path(), &args, &server, 2).await;
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        let log = std::fs::read_to_string(log).unwrap();
        assert!(!log.is_empty());
        for line in log.lines() {
            serde_json::from_str::<serde_json::Value>(line).unwrap();
        }
        assert_eq!(
            log.contains("SECRET-BODY-TEXT"),
            name == "bodies.jsonl",
            "{name}"
        );
    }
}

/// PRODUCT BUG (reported, not fixed here): a noninteractive refusal prints
/// "Re-run with --yes" but leaves the pending preflight stored
/// (crates/squad-cli/src/send.rs:224-232 returns without ContestCancel), so the
/// re-run is rejected with CONTEST_BUSY / DEBATE_BUSY until the plan expires.
#[tokio::test]
async fn rerun_with_yes_after_a_refused_preflight_is_admitted() {
    let server = provider().await;
    for message in [
        "@all $census Release plan: ship on Friday",
        "@all $survey Which database?",
        "@a @b $debate Which database?",
    ] {
        let root = tempfile::tempdir().unwrap();
        let refused = run_env(
            root.path(),
            &["send", message, "--json", "--data-dir", "data"],
            &server,
            6,
            CONFIRM_ALWAYS,
            20,
        )
        .await
        .0;
        assert_eq!(refused.status.code(), Some(1));
        let rerun = run_env(
            root.path(),
            &[
                "send",
                message,
                "--json",
                "--yes",
                "--no-wait",
                "--data-dir",
                "data",
            ],
            &server,
            6,
            CONFIRM_ALWAYS,
            20,
        )
        .await
        .0;
        assert_eq!(rerun.status.code(), Some(0), "{message}: {}", text(&rerun));
    }
}

#[tokio::test]
async fn every_contest_requires_pre_confirmation_without_yes_and_admits_with_yes() {
    let server = provider().await;
    let cases = [
        ("census", "@all $census Release plan: ship on Friday"),
        (
            "crosscheck",
            "@all $crosscheck Release plan: ship on Friday",
        ),
        ("survey", "@all $survey Which database?"),
        (
            "probe",
            "@all $probe Noise when charging, fan ruled out, inspect the indicator only",
        ),
        ("debate", "@a @b $debate Which database?"),
        ("tournament", "@all $tournament Name a database"),
    ];
    for (kind, message) in cases {
        let root = tempfile::tempdir().unwrap();
        let before = server.captured_bodies().await.len();
        let refused = run_env(
            root.path(),
            &["send", message, "--json", "--data-dir", "data"],
            &server,
            6,
            CONFIRM_ALWAYS,
            20,
        )
        .await
        .0;
        assert_eq!(refused.status.code(), Some(1), "{kind}: {refused:?}");
        let records = json_records(&refused);
        assert_eq!(records[0]["type"], format!("{kind}.preflight"), "{kind}");
        assert_eq!(records[0]["confirmationRequired"], true, "{kind}");
        assert!(
            records[0]["estimate"]["modelCalls"].as_u64().unwrap() > 0,
            "{kind}"
        );
        assert!(
            records[0]["estimate"]["estimatedUsd"].as_f64().unwrap() > 0.0,
            "{kind}"
        );
        assert_eq!(
            records.last().unwrap()["error"]["message"],
            "Confirmation required. Re-run with --yes in noninteractive mode.",
            "{kind}"
        );
        assert!(
            !records.iter().any(|r| r["type"] == "ack"),
            "{kind} admitted without --yes"
        );
        assert_eq!(
            server.captured_bodies().await.len(),
            before,
            "{kind} called the provider"
        );

        // --yes with --no-wait commits the admission and leaves the work unrun.
        let root = tempfile::tempdir().unwrap();
        let admitted = run_env(
            root.path(),
            &[
                "send",
                message,
                "--json",
                "--yes",
                "--no-wait",
                "--data-dir",
                "data",
            ],
            &server,
            6,
            CONFIRM_ALWAYS,
            20,
        )
        .await
        .0;
        assert_eq!(admitted.status.code(), Some(0), "{kind}: {admitted:?}");
        let records = json_records(&admitted);
        assert!(records.iter().any(|r| r["type"] == "ack"), "{kind}");
        assert!(
            records.last().unwrap()["cursor"].is_string()
                || records.iter().any(|r| r["cursor"].is_string()),
            "{kind}"
        );
        assert_eq!(
            server.captured_bodies().await.len(),
            before,
            "{kind} --no-wait dispatched work"
        );
    }
}

#[tokio::test]
async fn human_preflight_is_printed_and_not_confirmed_when_noninteractive() {
    let server = provider().await;
    let root = tempfile::tempdir().unwrap();
    let output = run_env(
        root.path(),
        &[
            "send",
            "@all $census Release plan: ship on Friday",
            "--data-dir",
            "data",
        ],
        &server,
        6,
        CONFIRM_ALWAYS,
        20,
    )
    .await
    .0;
    assert_eq!(output.status.code(), Some(1));
    let out = text(&output);
    assert!(
        stderr(&output).contains("Confirmation required. Re-run with --yes"),
        "{}",
        stderr(&output)
    );
    assert!(out.starts_with("census preflight:"), "{out}");
    assert!(
        out.contains("28 model calls") && out.contains("reserved $2.66"),
        "{out}"
    );
    assert!(!out.contains("Accepted"));
    assert!(server.captured_bodies().await.is_empty());
}

// ---------------------------------------------------------------------------
// M7.P1.L2.T7: export scopes, locked-room offline export, --no-wait, doctor
// ---------------------------------------------------------------------------

fn lock(root: &Path) -> squad_store::room_lock::RoomLock {
    squad_store::room_lock::RoomLock::acquire(&root.join("data/main.lock")).unwrap()
}

#[tokio::test]
async fn room_export_works_offline_while_the_room_is_locked_and_other_commands_respect_the_lock() {
    let server = provider_with(vec![fake_openrouter::streaming("Exported answer", "stop")]).await;
    let root = tempfile::tempdir().unwrap();
    let sent = run(
        root.path(),
        &["send", "@a hello export", "--data-dir", "data"],
        &server,
        2,
    )
    .await;
    assert_eq!(sent.status.code(), Some(0), "{sent:?}");
    let _guard = lock(root.path());
    let posts = server.captured_bodies().await.len();

    let exported = squad(root.path())
        .args(["export", "room", "--data-dir", "data"])
        .output()
        .unwrap();
    assert_eq!(exported.status.code(), Some(0), "{exported:?}");
    let markdown = text(&exported);
    assert!(
        markdown.contains("hello export") && markdown.contains("Exported answer"),
        "{markdown}"
    );

    let file = root.path().join("room.md");
    let to_file = squad(root.path())
        .args([
            "export",
            "room",
            "--data-dir",
            "data",
            "-o",
            file.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(to_file.status.code(), Some(0));
    assert_eq!(text(&to_file).trim(), file.to_str().unwrap());
    assert!(
        std::fs::read_to_string(&file)
            .unwrap()
            .contains("Exported answer")
    );

    // models and doctor never open the room, so the lock does not matter.
    for command in ["models", "doctor"] {
        let output = squad(root.path())
            .env("OPENROUTER_API_KEY", "fixture")
            .env("SQUAD_MODEL_A", MODEL)
            .args([command, "--data-dir", "data"])
            .output()
            .unwrap();
        assert!(output.status.success(), "{command}: {output:?}");
    }
    // Opening commands respect the same-room lock and never reach the provider.
    let blocked = run(
        root.path(),
        &["send", "@a again", "--data-dir", "data"],
        &server,
        2,
    )
    .await;
    assert_eq!(blocked.status.code(), Some(1), "{blocked:?}");
    assert!(!stderr(&blocked).is_empty());
    assert_eq!(server.captured_bodies().await.len(), posts);
}

/// Real runs against the loopback provider; contests whose scripted answers can
/// never satisfy their structured steps are killed after a few seconds, the way
/// a crash would end them. Committed content must still export offline.
#[tokio::test]
async fn every_contest_scope_exports_from_the_release_binary_while_locked() {
    let server = provider().await;
    let cases = [
        ("survey", "@all $survey Which database?", 10),
        (
            "crosscheck",
            "@all $crosscheck Release plan: ship on Friday",
            10,
        ),
        ("census", "@all $census Release plan: ship on Friday", 4),
        (
            "probe",
            "@all $probe Noise when charging, fan ruled out, inspect the indicator only",
            4,
        ),
        ("debate", "@a @b $debate Which database?", 4),
        ("tournament", "@all $tournament Name a database", 4),
    ];
    for (scope, message, seconds) in cases {
        let root = tempfile::tempdir().unwrap();
        let (output, _) = run_env(
            root.path(),
            &["send", message, "--json", "--yes", "--data-dir", "data"],
            &server,
            6,
            CONFIRM_ALWAYS,
            seconds,
        )
        .await;
        let id = contest_id(&output);
        assert!(
            !event_kinds(&output).is_empty(),
            "{scope}: {}",
            text(&output)
        );
        let _guard = lock(root.path());
        let exported = squad(root.path())
            .args(["export", scope, &id, "--data-dir", "data"])
            .output()
            .unwrap();
        assert_eq!(
            exported.status.code(),
            Some(0),
            "{scope}: {} {}",
            text(&exported),
            stderr(&exported)
        );
        assert!(
            text(&exported).starts_with('#'),
            "{scope}: {}",
            text(&exported)
        );
        let file = root.path().join(format!("{scope}.md"));
        let to_file = squad(root.path())
            .args([
                "export",
                scope,
                &id,
                "-o",
                file.to_str().unwrap(),
                "--data-dir",
                "data",
            ])
            .output()
            .unwrap();
        assert_eq!(text(&to_file).trim(), file.to_str().unwrap(), "{scope}");
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            text(&exported).replace("\r", ""),
            "{scope}"
        );
        // A scope that does not match the contest is rejected without output.
        let other = if scope == "survey" {
            "census"
        } else {
            "survey"
        };
        let mismatch = squad(root.path())
            .args(["export", other, &id, "--data-dir", "data"])
            .output()
            .unwrap();
        assert_eq!(mismatch.status.code(), Some(1), "{scope} as {other}");
        assert!(mismatch.stdout.is_empty());
    }
}

#[tokio::test]
async fn grill_scope_exports_after_a_plan_is_grilled_while_locked() {
    let server = provider().await;
    let root = tempfile::tempdir().unwrap();
    let plan = run(
        root.path(),
        &["send", "@a Draft a release plan", "--data-dir", "data"],
        &server,
        3,
    )
    .await;
    assert_eq!(plan.status.code(), Some(0), "{plan:?}");
    let (grill, _) = run_env(
        root.path(),
        &[
            "send",
            "@a @b @c $grill",
            "--json",
            "--yes",
            "--data-dir",
            "data",
        ],
        &server,
        3,
        CONFIRM_ALWAYS,
        20,
    )
    .await;
    let id = contest_id(&grill);
    let _guard = lock(root.path());
    let exported = squad(root.path())
        .args(["export", "grill", &id, "--data-dir", "data"])
        .output()
        .unwrap();
    assert_eq!(
        exported.status.code(),
        Some(0),
        "{} {}",
        text(&exported),
        stderr(&exported)
    );
    assert!(text(&exported).starts_with('#'));
    let mismatch = squad(root.path())
        .args(["export", "tournament", &id, "--data-dir", "data"])
        .output()
        .unwrap();
    assert_eq!(mismatch.status.code(), Some(1));
}

#[tokio::test]
async fn question_first_grill_collects_answers_and_the_room_stays_exportable() {
    let server = provider().await;
    let root = tempfile::tempdir().unwrap();
    let collected = run(
        root.path(),
        &[
            "send",
            "@a @b @c $grill Which database?",
            "--json",
            "--data-dir",
            "data",
        ],
        &server,
        3,
    )
    .await;
    assert_eq!(collected.status.code(), Some(0), "{collected:?}");
    assert!(event_kinds(&collected).contains(&"contest.collecting".to_owned()));
    assert_eq!(server.captured_bodies().await.len(), 3);
    // Normal room use continues while the pick is pending, and the state survives.
    assert!(count(root.path(), "contest.pick_requested") <= 1);
    let room = squad(root.path())
        .args(["export", "room", "--data-dir", "data"])
        .output()
        .unwrap();
    assert_eq!(room.status.code(), Some(0));
    assert!(text(&room).contains("Which database?"));
}

#[test]
fn doctor_reports_unknown_cost_calls_and_never_rounds_them_to_zero() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    {
        let mut db = squad_store::open(data.join("squad.db")).unwrap();
        squad_store::schema::migrate(&mut db).unwrap();
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        db.execute(
            "INSERT INTO spend_daily VALUES (?1,1234567,2500000,2)",
            [&today],
        )
        .unwrap();
    }
    let output = squad(root.path())
        .env("OPENROUTER_API_KEY", "fixture")
        .args(["doctor", "--data-dir", "data"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let out = text(&output);
    assert!(out.contains("actual $1.23; reserved $2.50"), "{out}");
    assert!(
        out.contains("unknown-cost calls: 2; unreconciled charges (~)"),
        "{out}"
    );
    assert!(!out.contains("fixture\n"));
}

// ---------------------------------------------------------------------------
// M7.P1.L2.T8: new-mode persistent variables and stop paths
// ---------------------------------------------------------------------------

#[tokio::test]
async fn census_passes_is_a_persistent_room_variable_that_overrides_the_environment_seed() {
    let server = provider().await;
    let root = tempfile::tempdir().unwrap();
    let doctor = |seed: &str| {
        let output = squad(root.path())
            .env("OPENROUTER_API_KEY", "fixture")
            .env("SQUAD_CENSUS_PASSES", seed)
            .args(["doctor", "--data-dir", "data"])
            .output()
            .unwrap();
        text(&output)
    };
    assert!(doctor("5").contains("CENSUS_PASSES: effective 5; source process environment"));
    let set = run_env(
        root.path(),
        &["send", "/set CENSUS_PASSES 3", "--data-dir", "data"],
        &server,
        3,
        &[("SQUAD_CENSUS_PASSES", "5")],
        20,
    )
    .await
    .0;
    assert_eq!(set.status.code(), Some(0), "{set:?} {}", stderr(&set));
    let report = doctor("5");
    assert!(
        report.contains("CENSUS_PASSES: effective 3; source persisted room value"),
        "{report}"
    );
    assert!(report.contains("SQUAD_CENSUS_PASSES seed 5"), "{report}");
    // Out-of-range values are refused with the exact message and nothing changes.
    let bad = run(
        root.path(),
        &["send", "/set CENSUS_PASSES 9", "--data-dir", "data"],
        &server,
        3,
    )
    .await;
    assert_eq!(bad.status.code(), Some(1));
    assert!(doctor("5").contains("effective 3"));
    assert!(server.captured_bodies().await.is_empty());
    // The persisted value is what the census preflight reports, not the seed.
    let preflight = run_env(
        root.path(),
        &[
            "send",
            "@all $census Release plan: ship on Friday",
            "--json",
            "--data-dir",
            "data",
        ],
        &server,
        6,
        &[
            ("SQUAD_DEBATE_CONFIRM_USD", "0"),
            ("SQUAD_CENSUS_PASSES", "5"),
        ],
        20,
    )
    .await
    .0;
    assert_eq!(
        json_records(&preflight)[0]["settings"]["passes"],
        3,
        "{}",
        text(&preflight)
    );
}

#[tokio::test]
async fn interrupting_a_running_contest_cleans_up_and_leaves_exportable_state() {
    let slow = fake_openrouter::streaming("x", "stop").set_delay(std::time::Duration::from_secs(8));
    let server = provider_with(vec![slow; 60]).await;
    let root = tempfile::tempdir().unwrap();
    let database = root.path().join("data/squad.db");
    let watcher = {
        let database = database.clone();
        tokio::spawn(async move {
            loop {
                let admitted = database.exists()
                    && rusqlite::Connection::open_with_flags(
                        &database,
                        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
                    )
                    .and_then(|db| {
                        db.query_row(
                            "SELECT COUNT(*) FROM events WHERE kind='probe.setup'",
                            [],
                            |r| r.get::<_, i64>(0),
                        )
                    })
                    .is_ok_and(|n| n > 0);
                if admitted {
                    return;
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        })
    };
    let mut command = squad(root.path());
    command
        .env("OPENROUTER_API_KEY", "fixture")
        .env("OPENROUTER_BASE_URL", server.base_url())
        .env("SQUAD_JUDGE_MODEL", MODEL)
        .env_remove("HTTPS_PROXY")
        .env_remove("HTTP_PROXY");
    for nick in ["A", "B", "C", "D", "E", "F"] {
        command.env(format!("SQUAD_MODEL_{nick}"), MODEL);
    }
    command.args([
        "send",
        // Unique topic: sibling tests run similar probes in parallel, and the
        // interrupter must signal only this process.
        "@all $probe Noise when charging, fan ruled out, inspect the indicator only interrupt-test",
        "--yes",
        "--json",
        "--data-dir",
        "data",
    ]);
    let interrupter = tokio::spawn(async move {
        watcher.await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let pid = std::process::Command::new("pgrep")
            .args(["-n", "-f", "indicator only interrupt-test"])
            .output()
            .unwrap();
        let pid = String::from_utf8(pid.stdout).unwrap().trim().to_owned();
        assert!(!pid.is_empty(), "contest process not found");
        assert!(
            std::process::Command::new("kill")
                .args(["-INT", &pid])
                .status()
                .unwrap()
                .success()
        );
    });
    let (output, timed_out) = spawn_and_collect(&mut command, 40).await;
    interrupter.await.unwrap();
    assert!(!timed_out);
    assert_eq!(output.status.code(), Some(130), "{}", stderr(&output));
    // The lock is released: a read-only export works and the stop caveat is committed.
    let id: String = db(root.path())
        .query_row(
            "SELECT json_extract(payload,'$.contestId') FROM events WHERE kind='probe.setup'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let exported = squad(root.path())
        .args(["export", "probe", &id, "--data-dir", "data"])
        .output()
        .unwrap();
    assert_eq!(exported.status.code(), Some(0), "{}", stderr(&exported));
    // A fresh one-shot open can take the lock again (the interrupted process cleaned up).
    let again = run(
        root.path(),
        &["send", "/who", "--data-dir", "data"],
        &server,
        6,
    )
    .await;
    assert_eq!(again.status.code(), Some(0), "{}", stderr(&again));
}

// ---------------------------------------------------------------------------
// Seeded committed records: completed debate, census report and stop caveat
// ---------------------------------------------------------------------------

fn debate_state(n: u32, status: &str, round: u32) -> CommittedEnvelope {
    support::event(
        n,
        "debate.state",
        serde_json::json!({"debateId":support::ID,"state":status,"debaters":["alpha","beta"],"round":round,"totalRounds":3,"judgeModel":"judge/model","judgeRequested":false,"stateVersion":n,"terminalReason":null,"caseId":null,"caseIds":[]}),
    )
}

#[test]
fn seeded_debate_exports_fixed_banners_while_the_room_is_locked() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    let _db = support::database(
        &data,
        vec![
            debate_state(1, "ROUND_RUNNING", 0),
            debate_state(2, "ROUND_RUNNING", 1),
            debate_state(3, "ROUND_RUNNING", 3),
            debate_state(4, "JUDGING", 3),
        ],
    );
    let _guard = lock(root.path());
    let output = squad(root.path())
        .args(["export", "debate", support::ID, "--data-dir", "data"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    for title in ["OPENING", "ROUND 1", "FINAL ROUND", "VERDICT"] {
        assert!(
            text(&output).contains(&squad_core::banner::render_default(title)),
            "{title}"
        );
    }
}

fn census_fixture(status: CensusStatus, reason: Option<&str>) -> (ContestId, Vec<EnvelopePayload>) {
    let id: ContestId = support::ID.parse().unwrap();
    let text = |s: &str| Text::new(s).unwrap();
    let gap = CensusGapOutcome {
        gap: CensusGap {
            id: text("g1"),
            omission: text("No rollback plan"),
            consequence: text("A bad release may cause an outage"),
            lenses: vec![text("failure modes")],
            source_find_ids: vec![text("find1")],
        },
        disposition: CensusDisposition::Confirmed,
        votes: 3,
        covered: 1,
        trivial: 0,
        finders: vec!["member0".parse().unwrap()],
        finder_count: 1,
        priority: Some(text("Prevent a prolonged outage")),
        citations: vec![CensusCitation {
            location: text("Recovery section"),
            quote: text("Restore from backup"),
        }],
    };
    let report = squad_core::orchestrator::census::report(id, 1, 1, vec![gap], vec![], vec![], &[]);
    report.validate().unwrap();
    let state = CensusState {
        contest_id: id,
        state_version: 3,
        status,
        pass: 1,
        plan_version: 1,
        step_id: None,
        artifact_event_ids: vec![],
        unfinished_work_ids: vec![],
        terminal_reason: reason.map(text),
    };
    (
        id,
        vec![
            EnvelopePayload::CensusState(state),
            EnvelopePayload::CensusReport(report),
        ],
    )
}

fn seed_census(root: &Path, payloads: Vec<EnvelopePayload>) {
    let data = root.join("data");
    std::fs::create_dir_all(&data).unwrap();
    let store = squad_store::Store::open(data.join("squad.db")).unwrap();
    let request = EventId::new();
    store
        .commit(
            request,
            "fixture".into(),
            "census-release".into(),
            move |tx, group| {
                for payload in payloads {
                    group.push(
                        tx,
                        squad_core::router::envelope(
                            request,
                            Actor::System,
                            vec![Recipient::Me],
                            payload,
                        ),
                    )?;
                }
                Ok(group.accepted(request))
            },
        )
        .unwrap();
}

#[test]
fn seeded_census_report_exports_offline_and_a_stopped_census_carries_its_caveat() {
    for (status, reason) in [
        (CensusStatus::Complete, None),
        (CensusStatus::Stopped, Some("STOPPED_BY_USER")),
    ] {
        let root = tempfile::tempdir().unwrap();
        let (id, payloads) = census_fixture(status, reason);
        seed_census(root.path(), payloads);
        let _guard = lock(root.path());
        let output = squad(root.path())
            .args(["export", "census", &id.to_string(), "--data-dir", "data"])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
        let markdown = text(&output);
        assert!(markdown.contains("No rollback plan"), "{markdown}");
        assert!(markdown.contains(
            "Models share blind spots; the true number still unfound is at least this estimate."
        ));
        assert!(markdown.contains("Singleton: the other members missed this gap."));
        assert!(markdown.contains("Discovery curve"));
        match reason {
            None => assert!(
                markdown.contains("Pass 1 · plan 1 · Complete\n"),
                "{markdown}"
            ),
            Some(reason) => assert!(
                markdown.contains(&format!("· Stopped · {reason}")),
                "{markdown}"
            ),
        }
        let wrong = squad(root.path())
            .args(["export", "debate", &id.to_string(), "--data-dir", "data"])
            .output()
            .unwrap();
        assert_eq!(wrong.status.code(), Some(1));
    }
}
