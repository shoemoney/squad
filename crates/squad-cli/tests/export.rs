mod support;
use serde_json::json;
use squad_cli::args::Export;
use squad_protocol::*;
use support::*;

#[test]
fn offline_export_reads_committed_content_while_room_is_locked() {
    let temp = tempfile::tempdir().unwrap();
    let _db = database(temp.path(), vec![reply(1, "Keep the answer.")]);
    let _lock = squad_store::room_lock::RoomLock::acquire(&temp.path().join("main.lock")).unwrap();
    let options = Export {
        scope: ExportScope::Room,
        id: None,
        output: None,
    };
    let mut output = Vec::new();
    squad_cli::export::run(temp.path(), &options, &mut output).unwrap();
    let text = String::from_utf8(output).unwrap();
    assert!(text.contains("Keep the answer."));
    assert!(text.contains("alpha"));
    insta::assert_snapshot!(text);
    let readonly = squad_cli::export::open_readonly(temp.path()).unwrap();
    assert!(readonly.execute("DELETE FROM events", []).is_err());
}

#[test]
fn output_file_contains_markdown_and_stdout_only_contains_path() {
    let temp = tempfile::tempdir().unwrap();
    let _db = database(temp.path(), vec![reply(1, "Answer")]);
    let path = temp.path().join("output.md");
    let options = Export {
        scope: ExportScope::Room,
        id: None,
        output: Some(path.clone()),
    };
    let mut output = Vec::new();
    squad_cli::export::run(temp.path(), &options, &mut output).unwrap();
    assert_eq!(
        String::from_utf8(output).unwrap().trim(),
        path.to_str().unwrap()
    );
    assert!(std::fs::read_to_string(path).unwrap().contains("Answer"));
}

fn debate_state(n: u32, status: &str, round: u32) -> CommittedEnvelope {
    event(
        n,
        "debate.state",
        json!({"debateId":ID,"state":status,"debaters":["alpha","beta"],"round":round,"totalRounds":3,"judgeModel":"judge/model","judgeRequested":false,"stateVersion":n,"terminalReason":null,"caseId":null,"caseIds":[]}),
    )
}
#[test]
fn offline_debate_uses_fixed_opening_round_final_and_verdict_banners() {
    let temp = tempfile::tempdir().unwrap();
    let _db = database(
        temp.path(),
        vec![
            debate_state(1, "ROUND_RUNNING", 0),
            debate_state(2, "ROUND_RUNNING", 1),
            debate_state(3, "ROUND_RUNNING", 3),
            debate_state(4, "JUDGING", 3),
        ],
    );
    let options = Export {
        scope: ExportScope::Debate,
        id: Some(ID.parse().unwrap()),
        output: None,
    };
    let mut output = Vec::new();
    squad_cli::export::run(temp.path(), &options, &mut output).unwrap();
    let text = String::from_utf8(output).unwrap();
    for title in ["OPENING", "ROUND 1", "FINAL ROUND", "VERDICT"] {
        assert!(
            text.contains(&squad_core::banner::render_default(title)),
            "{title}: {text}"
        );
    }
    insta::assert_snapshot!(text);
}

#[test]
fn every_scope_delegates_to_core_and_preserves_errors() {
    let temp = tempfile::tempdir().unwrap();
    let _db = database(temp.path(), vec![]);
    for scope in [
        ExportScope::Debate,
        ExportScope::Tournament,
        ExportScope::Grill,
        ExportScope::Census,
        ExportScope::Crosscheck,
        ExportScope::Survey,
        ExportScope::Probe,
    ] {
        let options = Export {
            scope: scope.clone(),
            id: None,
            output: None,
        };
        assert_eq!(
            squad_cli::export::run(temp.path(), &options, &mut Vec::new()).unwrap_err(),
            "contest export requires an ID"
        );
        let options = Export {
            scope,
            id: Some(ID.parse().unwrap()),
            output: None,
        };
        assert!(squad_cli::export::run(temp.path(), &options, &mut Vec::new()).is_err());
    }
}

// ---- M3.P3.L3.T5 / T7 / T10 / T14 / T19 ----

fn typed(n: u32, payload: EnvelopePayload) -> CommittedEnvelope {
    let value = serde_json::to_value(&payload).unwrap();
    event(n, value["kind"].as_str().unwrap(), value["payload"].clone())
}

fn mode_events(contest: ContestId) -> Vec<(ExportScope, ContestId, Vec<CommittedEnvelope>)> {
    let id = contest.to_string();
    let census = CensusState {
        contest_id: contest,
        state_version: 3,
        status: CensusStatus::Stopped,
        pass: 1,
        plan_version: 1,
        step_id: None,
        artifact_event_ids: vec![],
        unfinished_work_ids: vec![],
        terminal_reason: Some(Text::new("stopped").unwrap()),
    };
    vec![
        (
            ExportScope::Census,
            contest,
            vec![typed(10, EnvelopePayload::CensusState(census))],
        ),
        (
            ExportScope::Crosscheck,
            contest,
            vec![
                event(
                    20,
                    "crosscheck.setup",
                    json!({"contestId":id,"planId":ID,"planVersion":1,"plan":"Maintain service with the existing staff.","participants":["alpha","beta","gamma"],"goal":null}),
                ),
                event(
                    21,
                    "crosscheck.state",
                    json!({"contestId":id,"planId":ID,"planVersion":1,"gridVersion":0,"stateVersion":2,"status":"STOPPED","stepId":null,"terminalReason":"stopped"}),
                ),
            ],
        ),
        (
            ExportScope::Survey,
            contest,
            vec![
                event(
                    30,
                    "survey.setup",
                    json!({"contestId":id,"questionMessageId":"01900000-0000-7000-8000-000000000070","question":"Is the proposal feasible?","participants":["alpha","beta"],"answers":[{"nick":"alpha","messageId":"01900000-0000-7000-8000-000000000071","text":"It fits the budget."},{"nick":"beta","messageId":"01900000-0000-7000-8000-000000000072","text":"A load test is needed."}],"collect":false}),
                ),
                event(
                    31,
                    "survey.state",
                    json!({"contestId":id,"state":"STOPPED","stateVersion":2,"stepId":null,"artifactEventIds":[],"terminalReason":"Stopped"}),
                ),
            ],
        ),
        (
            ExportScope::Probe,
            contest,
            vec![
                event(
                    40,
                    "probe.setup",
                    json!({"contestId":id,"brief":{"observation":"Noise only while charging","ruledOut":"Cooling fan","inspectable":"External indicator only"},"participants":["alpha","beta","gamma","delta"],"explainers":["alpha","beta"],"checkers":["gamma","delta"],"sittingOut":null}),
                ),
                event(
                    41,
                    "probe.state",
                    json!({"contestId":id,"state":"STOPPED","stateVersion":3,"stepId":null,"artifactEventIds":[],"skippedWorkIds":[],"skippedMembers":["delta"],"terminalReason":"Stopped"}),
                ),
            ],
        ),
    ]
}

#[test]
fn four_mode_offline_exports_match_core_cover_partial_runs_and_exclude_secrets() {
    let contest: ContestId = "01900000-0000-7000-8000-000000000077".parse().unwrap();
    for (scope, id, events) in mode_events(contest) {
        let temp = tempfile::tempdir().unwrap();
        let mut all = events;
        all.push(event(
            90,
            "contest.artifact",
            json!({"contestId":id,"stepId":ID,"artifactKind":"private","sourceRefs":[],"data":{"prompt":"SECRET-PROMPT","labelMap":"SECRET-LABEL-MAP"}}),
        ));
        let _db = database(temp.path(), all);
        let _lock =
            squad_store::room_lock::RoomLock::acquire(&temp.path().join("main.lock")).unwrap();
        let mut out = Vec::new();
        squad_cli::export::run(
            temp.path(),
            &Export {
                scope: scope.clone(),
                id: Some(id),
                output: None,
            },
            &mut out,
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        // The CLI is a thin delegate: identical to the shared offline export, minus terminal sanitising.
        let core =
            squad_core::room::export_offline(temp.path(), "main", scope.clone(), Some(id)).unwrap();
        assert_eq!(text, squad_tui::markdown::sanitize(&core), "{scope:?}");
        assert!(!text.contains("SECRET"), "{scope:?}");
        assert!(!text.is_empty());
        // Other scopes never match this contest; a missing contest is a typed failure, not an empty export.
        let wrong = if scope == ExportScope::Survey {
            ExportScope::Probe
        } else {
            ExportScope::Survey
        };
        assert!(
            squad_cli::export::run(
                temp.path(),
                &Export {
                    scope: wrong,
                    id: Some(id),
                    output: None
                },
                &mut Vec::new()
            )
            .is_err()
        );
        assert!(
            squad_cli::export::run(
                temp.path(),
                &Export {
                    scope: scope.clone(),
                    id: Some(ContestId::new()),
                    output: None
                },
                &mut Vec::new()
            )
            .is_err()
        );
    }
}

#[test]
fn late_unscored_scores_stay_with_their_round_in_the_exported_debate() {
    let temp = tempfile::tempdir().unwrap();
    let _db = database(
        temp.path(),
        vec![
            event(
                1,
                "debate.state",
                json!({"debateId":ID,"state":"ROUND_RUNNING","debaters":["alpha","beta"],"round":3,"totalRounds":3,"judgeModel":"j/m","judgeRequested":true,"stateVersion":1,"terminalReason":null,"caseId":ID,"caseIds":[ID]}),
            ),
            event(
                2,
                "debate.round_score",
                json!({"debateId":ID,"caseId":ID,"round":2,"scores":null,"tally":{"alpha":1,"beta":0},"reason":"unscored"}),
            ),
        ],
    );
    let mut out = Vec::new();
    squad_cli::export::run(
        temp.path(),
        &Export {
            scope: ExportScope::Debate,
            id: Some(ID.parse().unwrap()),
            output: None,
        },
        &mut out,
    )
    .unwrap();
    let text = String::from_utf8(out).unwrap();
    assert!(text.to_lowercase().contains("round 2"), "{text}");
    assert!(text.to_lowercase().contains("unscored"), "{text}");
    assert!(text.contains("alpha") && text.contains('1'));
}

#[test]
fn export_text_is_independent_of_width_banner_effects_and_colour_settings() {
    let temp = tempfile::tempdir().unwrap();
    let _db = database(
        temp.path(),
        vec![
            event(
                1,
                "debate.state",
                json!({"debateId":ID,"state":"ROUND_RUNNING","debaters":["alpha","beta"],"round":0,"totalRounds":3,"judgeModel":"j/m","judgeRequested":false,"stateVersion":1,"terminalReason":null,"caseId":null,"caseIds":[]}),
            ),
            event(
                2,
                "debate.state",
                json!({"debateId":ID,"state":"ROUND_RUNNING","debaters":["alpha","beta"],"round":1,"totalRounds":3,"judgeModel":"j/m","judgeRequested":false,"stateVersion":2,"terminalReason":null,"caseId":null,"caseIds":[]}),
            ),
            event(
                3,
                "debate.state",
                json!({"debateId":ID,"state":"ROUND_RUNNING","debaters":["alpha","beta"],"round":3,"totalRounds":3,"judgeModel":"j/m","judgeRequested":false,"stateVersion":3,"terminalReason":null,"caseId":null,"caseIds":[]}),
            ),
            event(
                4,
                "debate.state",
                json!({"debateId":ID,"state":"JUDGING","debaters":["alpha","beta"],"round":3,"totalRounds":3,"judgeModel":"j/m","judgeRequested":false,"stateVersion":4,"terminalReason":null,"caseId":null,"caseIds":[]}),
            ),
        ],
    );
    let run = |env: &[(&str, &str)]| {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_squad"));
        command
            .env_clear()
            .env("HOME", temp.path())
            .current_dir(temp.path())
            .args([
                "export",
                "--data-dir",
                temp.path().to_str().unwrap(),
                "debate",
                ID,
            ]);
        for (name, value) in env {
            command.env(name, value);
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    };
    let baseline = run(&[]);
    for title in ["OPENING", "ROUND 1", "FINAL ROUND", "VERDICT"] {
        assert!(
            baseline.contains(&squad_core::banner::render_default(title)),
            "{title}"
        );
    }
    for env in [
        vec![("COLUMNS", "20"), ("LINES", "5")],
        vec![("COLUMNS", "300")],
        vec![("SQUAD_BANNERS", "off")],
        vec![("SQUAD_BANNERS", "compact")],
        vec![("SQUAD_FX", "off")],
        vec![("SQUAD_FX", "lite"), ("SQUAD_FX_DEBUG", "1")],
        vec![("NO_COLOR", "1")],
        vec![
            ("TERM", "dumb"),
            ("NO_COLOR", "1"),
            ("SQUAD_BANNERS", "off"),
            ("SQUAD_FX", "off"),
            ("COLUMNS", "40"),
        ],
    ] {
        assert_eq!(run(&env), baseline, "{env:?}");
    }
}

#[test]
fn export_succeeds_without_credentials_or_models_so_it_can_never_call_the_provider() {
    let temp = tempfile::tempdir().unwrap();
    let _db = database(temp.path(), vec![reply(1, "Keep the answer.")]);
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_squad"))
        .env_clear()
        .env("HOME", temp.path())
        .env("OPENROUTER_BASE_URL", "http://127.0.0.1:9/")
        .current_dir(temp.path())
        .args([
            "export",
            "--data-dir",
            temp.path().to_str().unwrap(),
            "room",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("Keep the answer.")
    );
}
