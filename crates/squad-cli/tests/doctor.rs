mod support;
use support::*;

#[test]
fn unknown_charges_keep_reservations_and_are_never_reported_as_zero_cost() {
    let temp = tempfile::tempdir().unwrap();
    let db = database(temp.path(), vec![]);
    db.execute(
        "INSERT INTO spend_daily VALUES ('2026-11-24',1234567,2500000,2)",
        [],
    )
    .unwrap();
    let output = squad_cli::diagnostics::accounting(temp.path(), "2026-11-24", 20.).unwrap();
    insta::assert_snapshot!(output, @"
    Daily accounting: 2026-11-24; actual $1.23; reserved $2.50; effective max $20.00; unknown-cost calls: 2; unreconciled charges (~)
    ");
    let before = db.total_changes();
    squad_cli::diagnostics::accounting(temp.path(), "2026-11-25", 35.).unwrap();
    assert_eq!(db.total_changes(), before);
}

#[test]
fn absent_database_is_read_only_and_reports_no_ledger_yet() {
    let temp = tempfile::tempdir().unwrap();
    assert!(
        squad_cli::diagnostics::accounting(temp.path(), "2026-11-24", 20.)
            .unwrap()
            .contains("no ledger yet")
    );
    assert!(
        squad_cli::diagnostics::identities(temp.path())
            .unwrap()
            .contains("no recorded attempts")
    );
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
}

#[test]
fn response_model_mismatches_keep_work_attempt_and_concrete_slugs() {
    let temp = tempfile::tempdir().unwrap();
    let db = database(temp.path(), vec![]);
    db.execute("INSERT INTO attempts(work_id,attempt,epoch,target,context_id,model,response_model,day,started_at,outcome,estimated_cost_usd,unresolved) VALUES (?1,1,1,'alpha','member:alpha','p/model-20261101','p/other-20261102','2026-11-24','2026-11-24T00:00:00Z','unknown',0.5,1)", [ID]).unwrap();
    let output = squad_cli::diagnostics::identities(temp.path()).unwrap();
    assert!(output.contains("attempt 1"));
    assert!(output.contains(ID));
    assert!(output.contains("selected p/model-20261101; response p/other-20261102"));
}
#[test]
fn census_diagnostics_read_persisted_value_without_writing_or_replacing_seed() {
    let temp = tempfile::tempdir().unwrap();
    let absent =
        squad_cli::diagnostics::census_passes(temp.path(), Some(5), "process environment").unwrap();
    assert!(absent.contains("effective 5; source process environment"));
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
    let db = database(temp.path(), vec![]);
    db.execute("INSERT INTO vars(name,value) VALUES ('CENSUS_PASSES',3) ON CONFLICT(name) DO UPDATE SET value=3",[]).unwrap();
    let before = db.total_changes();
    let stored =
        squad_cli::diagnostics::census_passes(temp.path(), Some(5), "process environment").unwrap();
    assert!(stored.contains("effective 3; source persisted room value"));
    assert!(stored.contains("seed 5; default 2; range 1..=5"));
    assert_eq!(db.total_changes(), before);
}

#[test]
fn storage_reports_used_of_capacity_and_warns_near_and_at_the_ceiling() {
    let temp = tempfile::tempdir().unwrap();
    let absent = squad_cli::diagnostics::storage(temp.path(), 1 << 30).unwrap();
    assert!(absent.contains("Storage: 0.0 MiB used of 1024.0 MiB (0.0%)"));
    assert!(!absent.contains("WARNING"));
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
    let db = database(temp.path(), vec![]);
    let used = squad_store::storage::used_bytes(&db).unwrap();
    assert!(used > 0);
    let before = db.total_changes();
    let roomy = squad_cli::diagnostics::storage(temp.path(), used * 100).unwrap();
    assert!(roomy.starts_with("Storage: "));
    assert!(!roomy.contains("WARNING"));
    let near = squad_cli::diagnostics::storage(temp.path(), used * 105 / 100).unwrap();
    assert!(near.contains("WARNING - near the ceiling"));
    let full = squad_cli::diagnostics::storage(temp.path(), used).unwrap();
    assert!(full.contains("at the ceiling") && full.contains("STORAGE_LIMIT"));
    assert_eq!(db.total_changes(), before);
}

// ---- M3.P3.L4.T5 / T8 / T9 / T11: effective values, sources, invalid diagnostics and trace safety ----
use std::process::Command;

fn doctor(root: &std::path::Path, extra: &[(&str, &str)], args: &[&str]) -> (bool, String) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_squad"));
    command
        .env_clear()
        .env("HOME", root)
        .current_dir(root)
        .env("OPENROUTER_API_KEY", "DOCTOR-SENTINEL-KEY")
        .env("SQUAD_MODEL_A", "m/x")
        .env("SQUAD_MODEL_B", "m/y")
        .env("SQUAD_JUDGE_MODEL", "m/j")
        .arg("doctor")
        .args(args);
    for (name, value) in extra {
        command.env(name, value);
    }
    let output = command.output().unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    (output.status.success(), text)
}

fn line<'a>(text: &'a str, name: &str) -> &'a str {
    text.lines()
        .find(|l| l.starts_with(&format!("{name}:")))
        .unwrap_or_else(|| panic!("{name} missing from:\n{text}"))
}

#[test]
fn defaults_effective_values_and_sources_are_reported_for_every_documented_variable() {
    let temp = tempfile::tempdir().unwrap();
    let (ok, text) = doctor(temp.path(), &[], &["--env"]);
    assert!(ok, "{text}");
    for (name, effective, default) in [
        ("SQUAD_BANNERS", "on", "on"),
        ("SQUAD_FX", "full", "full"),
        ("SQUAD_DAILY_MAX_USD", "150", "150"),
        ("SQUAD_GRILL_ROUNDS", "2", "2"),
        ("SQUAD_PLAN_WORDS", "1200", "1200"),
        ("SQUAD_FX_DEBUG", "unset", "unset"),
    ] {
        let row = line(&text, name);
        assert!(row.contains(&format!("effective {effective};")), "{row}");
        assert!(row.contains("source built-in default;"), "{row}");
        assert!(row.contains(&format!("default {default};")), "{row}");
    }
    assert!(
        line(&text, "SQUAD_REASONING_EFFORT_A")
            .contains("effective unset; source built-in default")
    );
    // The static table documents all of them too.
    for name in [
        "SQUAD_BANNERS",
        "SQUAD_FX",
        "SQUAD_DAILY_MAX_USD",
        "SQUAD_GRILL_ROUNDS",
        "SQUAD_PLAN_WORDS",
        "SQUAD_REASONING_EFFORT_<NICK>",
        "SQUAD_FX_DEBUG",
        "SQUAD_CENSUS_PASSES",
    ] {
        assert!(
            text.contains(&format!("`{name}`")) || text.contains(name),
            "{name}"
        );
    }
    assert!(!text.contains("DOCTOR-SENTINEL-KEY"));
}

#[test]
fn process_environment_and_file_sources_are_distinguished_and_values_take_effect() {
    let temp = tempfile::tempdir().unwrap();
    let env_file = temp.path().join("custom.env");
    std::fs::write(
        &env_file,
        "SQUAD_GRILL_ROUNDS=4\nSQUAD_PLAN_WORDS=800\nSQUAD_BANNERS=off\n",
    )
    .unwrap();
    let (ok, text) = doctor(
        temp.path(),
        &[
            ("SQUAD_BANNERS", "compact"),
            ("SQUAD_FX", "lite"),
            ("SQUAD_DAILY_MAX_USD", "5"),
            ("SQUAD_FX_DEBUG", "1"),
            ("SQUAD_REASONING_EFFORT_A", "low"),
        ],
        &["--env-file", env_file.to_str().unwrap()],
    );
    assert!(ok, "{text}");
    // Process values beat file values; file values beat defaults.
    assert!(line(&text, "SQUAD_BANNERS").contains("effective compact; source process environment"));
    assert!(line(&text, "SQUAD_GRILL_ROUNDS").contains("effective 4; source environment file"));
    assert!(line(&text, "SQUAD_PLAN_WORDS").contains("effective 800; source environment file"));
    assert!(line(&text, "SQUAD_FX").contains("effective lite; source process environment"));
    assert!(line(&text, "SQUAD_DAILY_MAX_USD").contains("effective 5; source process environment"));
    assert!(line(&text, "SQUAD_FX_DEBUG").contains("effective 1; source process environment"));
    assert!(
        line(&text, "SQUAD_REASONING_EFFORT_A")
            .contains("effective low; source process environment")
    );
    assert!(
        text.contains("effective max $5.00"),
        "accounting uses the effective max:\n{text}"
    );
}

#[test]
fn invalid_values_report_exact_diagnostics_and_independent_checks_continue() {
    let temp = tempfile::tempdir().unwrap();
    let (ok, text) = doctor(
        temp.path(),
        &[
            ("SQUAD_BANNERS", "loud"),
            ("SQUAD_FX", "wild"),
            ("SQUAD_DAILY_MAX_USD", "-1"),
            ("SQUAD_GRILL_ROUNDS", "99"),
            ("SQUAD_PLAN_WORDS", "0"),
            ("SQUAD_REASONING_EFFORT_A", "extreme"),
        ],
        &["--env"],
    );
    assert!(!ok, "invalid configuration fails doctor");
    for expected in [
        "Configuration: FAIL - SQUAD_BANNERS must be on, compact, or off.",
        "Configuration: FAIL - SQUAD_FX must be full, lite, or off.",
        "Configuration: FAIL - SQUAD_DAILY_MAX_USD must be a positive finite number.",
        "Configuration: FAIL - SQUAD_GRILL_ROUNDS must be an integer from 1 to 10.",
        "Configuration: FAIL - SQUAD_PLAN_WORDS must be a positive integer.",
    ] {
        assert!(text.contains(expected), "{expected} missing from:\n{text}");
    }
    assert!(text.contains("SQUAD_REASONING_EFFORT_A"), "{text}");
    // Independent checks still ran and the credential stayed hidden.
    for check in [
        "Credential presence: present",
        "Data directory: writable",
        "Storage:",
        "Daily accounting:",
        "CENSUS_PASSES:",
    ] {
        assert!(text.contains(check), "{check} missing from:\n{text}");
    }
    assert!(!text.contains("DOCTOR-SENTINEL-KEY"));
    assert!(text.trim_end().ends_with("Overall: FAIL"));
}

#[tokio::test]
async fn contest_diagnostics_keep_trace_identities_and_exclude_private_context() {
    use squad_agents::openrouter::OpenRouterClient;
    use squad_core::room::RoomHandle;
    use squad_protocol::{config::LoadOptions, *};
    use tracing_subscriber::layer::SubscriberExt;
    let dir = tempfile::tempdir().unwrap();
    let log_path = dir.path().join("bus.jsonl");
    let log =
        squad_cli::buslog::BusLog::open(&log_path, false, vec!["DIAG-SECRET-KEY".into()]).unwrap();
    let subscriber = tracing_subscriber::registry().with(log.layer());
    // The bus runs on its own threads, so only a process-wide subscriber sees its diagnostics.
    tracing::subscriber::set_global_default(subscriber).unwrap();
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/models"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({"data":[{"id":"m/x","context_length":1000000,"pricing":{"prompt":"0.000001","completion":"0.000002"}}]})))
        .mount(&server)
        .await;
    let mut config = SquadConfig::load_with_env(
        &LoadOptions {
            env_file: None,
            data_dir: Some(dir.path().join("data")),
            working_dir: dir.path().into(),
            config_dir: None,
            default_data_dir: dir.path().join("data"),
        },
        &std::collections::BTreeMap::from([(
            "OPENROUTER_API_KEY".into(),
            "DIAG-SECRET-KEY".into(),
        )]),
    )
    .unwrap();
    config.agents = (0..4)
        .map(|i| squad_protocol::config::AgentConfig {
            nick: format!("m{i}").parse().unwrap(),
            model: "m/x".into(),
            persona: format!("PRIVATE_PERSONA_{i}"),
            reasoning_effort: None,
        })
        .collect();
    config.judge.model = "m/x".into();
    config.budget.debate_confirm_usd = 0.;
    config.budget.daily_max_usd = 100.;
    let room = RoomHandle::open_with_client(
        config.clone(),
        "main",
        OpenMode::OneShot,
        OpenRouterClient::new(&format!("{}/", server.uri()), "DIAG-SECRET-KEY").unwrap(),
    )
    .await
    .unwrap();
    let context = parser::ParseContext {
        roster: config.agents.iter().map(|a| a.nick.clone()).collect(),
        ..Default::default()
    };
    let meta = parser::SubmissionMeta::default();
    let mut request_ids = Vec::new();
    for text in [
        "@all $survey PRIVATE_CONTEXT_CANARY which checks help?",
        "@all $tournament PRIVATE_CONTEXT_CANARY design a bridge",
    ] {
        let id = EventId::new();
        request_ids.push(id);
        let request =
            Request::from_parsed(id, parse(text, &meta, &context).unwrap(), &meta, false).unwrap();
        if let RequestResult::PendingConfirmation {
            preflight: PendingPreflight::Contest(plan),
            ..
        } = room.submit_commit_only(request).await
        {
            room.submit_commit_only(Request::ContestCancel {
                request_id: EventId::new(),
                contest_id: plan.contest_id,
                plan_id: plan.plan_id,
            })
            .await;
        }
    }
    room.shutdown().await.unwrap();
    let rows = std::fs::read_to_string(&log_path).unwrap();
    assert!(!rows.is_empty(), "bus diagnostics were recorded");
    // Trace identities survive: request ids, event/correlation ids and commit sequences.
    for id in &request_ids {
        assert!(
            rows.contains(&id.to_string()),
            "request {id} missing from diagnostics"
        );
    }
    assert!(
        rows.contains("correlation_id") && rows.contains("seq") && rows.contains("commit_index")
    );
    // No private context, personas, blind-judge maps, prompts or credentials.
    for forbidden in [
        "PRIVATE_CONTEXT_CANARY",
        "PRIVATE_PERSONA",
        "labelMap",
        "label_map",
        "tournament_labels",
        "DIAG-SECRET-KEY",
        "systemPrompt",
    ] {
        assert!(
            !rows.contains(forbidden),
            "{forbidden} leaked into diagnostics"
        );
    }
    assert_eq!(
        std::os::unix::fs::PermissionsExt::mode(
            &std::fs::metadata(&log_path).unwrap().permissions()
        ) & 0o777,
        0o600
    );
}
