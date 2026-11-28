use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use squad_protocol::{parser::RecordSummary, *};
use squad_tui::{
    app::App,
    fx::TerminalCaps,
    save::{self, SaveJob},
    session::{RoomError, RoomFeed, RoomFuture, RoomPort, RoomSnapshot},
};
use std::{path::PathBuf, time::Duration};

const FORGE_EXPORT: &str = "# FORGE\n\nFusion by @alpha beat the best draft.\n\nThe winning artifact body.\nSecond line.\n\n## Borda\n\n| @alpha | 9 |\n\n## Provenance\n\nDraft A by @hidden-provider\n";

struct Stub(String);
impl RoomPort for Stub {
    fn submit(&self, _: Request) -> RoomFuture<'_, RequestResult> {
        unimplemented!()
    }
    fn subscribe(&self, _: Option<Cursor>) -> Result<RoomFeed, RoomError> {
        unimplemented!()
    }
    fn page_events(
        &self,
        _: Option<Cursor>,
        _: usize,
    ) -> RoomFuture<'_, Result<Vec<CommittedEnvelope>, RoomError>> {
        unimplemented!()
    }
    fn state(&self) -> RoomFuture<'_, RoomSnapshot> {
        unimplemented!()
    }
    fn export(
        &self,
        _: ExportScope,
        _: Option<ContestId>,
    ) -> RoomFuture<'_, Result<String, RoomError>> {
        let text = self.0.clone();
        Box::pin(async move { Ok(text) })
    }
    fn shutdown(&self) -> RoomFuture<'_, Result<(), RoomError>> {
        unimplemented!()
    }
}
fn app() -> App {
    App::new(TerminalCaps::resolve(|_| None, false, (80, 24).into()).unwrap())
}
fn record(seq: i64, author: Actor, text: &str, contest: Option<ContestId>) -> RecordSummary {
    RecordSummary {
        event_id: EventId::new(),
        message_id: Some(MessageId::new()),
        seq: Cursor::new(seq).unwrap(),
        kind: Text::new("message.final").unwrap(),
        author,
        recipients: vec![],
        content: text.into(),
        in_reply_to_message_id: None,
        contest_id: contest,
        case_id: None,
        plan_event_id: None,
        grid_event_id: None,
        cell_ids: vec![],
        state_version: None,
        superseded: false,
        eligible: true,
    }
}
fn finished_forge(app: &mut App) -> ContestId {
    let id = ContestId::new();
    app.session.contests.entry(id).or_default().state =
        Some(ContestState::Generic(GenericContestState {
            contest_id: id,
            kind: ContestKind::Forge,
            state_version: 3,
            status: Text::new("COMPLETE").unwrap(),
            step_id: None,
            artifact_event_ids: vec![],
            terminal_reason: None,
        }));
    app.session.snapshot.context.records = vec![
        record(1, Actor::Me, "Write a launch plan for Squad!", Some(id)),
        record(
            2,
            Actor::Member(Nick::new("alpha").unwrap()),
            "a draft",
            None,
        ),
    ];
    id
}
fn job(app: &App, path: Option<&str>, cwd: PathBuf) -> SaveJob {
    save::plan(
        &app.session,
        None,
        path.map(str::to_owned),
        EventId::new(),
        cwd,
    )
    .unwrap()
}
fn saved(result: RequestResult) -> String {
    match result {
        RequestResult::Internal(InternalResult::Text { text }) => text,
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn forge_winner_is_saved_alone_with_default_name_and_0600() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app();
    finished_forge(&mut app);
    let job = job(&app, None, dir.path().into());
    let text = saved(save::save_file(&Stub(FORGE_EXPORT.into()), job).await);
    assert_eq!(text, "Saved write-a-launch-plan-1.md");
    let path = dir.path().join("write-a-launch-plan-1.md");
    let body = std::fs::read_to_string(&path).unwrap();
    assert_eq!(body, "The winning artifact body.\nSecond line.\n");
    assert!(!body.contains("@alpha") && !body.contains("hidden-provider"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(path.metadata().unwrap().permissions().mode() & 0o777, 0o600);
    }
}

#[tokio::test]
async fn saving_twice_never_overwrites() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app();
    finished_forge(&mut app);
    let room = Stub(FORGE_EXPORT.into());
    for expected in [
        "Saved write-a-launch-plan-1.md",
        "Saved write-a-launch-plan-2.md",
        "Saved write-a-launch-plan-3.md",
    ] {
        let text = saved(save::save_file(&room, job(&app, None, dir.path().into())).await);
        assert_eq!(text, expected);
    }
}

#[tokio::test]
async fn explicit_path_is_relative_to_cwd_and_refuses_existing_files() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("out")).unwrap();
    let mut app = app();
    finished_forge(&mut app);
    let room = Stub(FORGE_EXPORT.into());
    let first =
        saved(save::save_file(&room, job(&app, Some("out/plan.md"), dir.path().into())).await);
    assert_eq!(first, "Saved out/plan.md");
    let second = save::save_file(&room, job(&app, Some("out/plan.md"), dir.path().into())).await;
    let RequestResult::Rejected { error, .. } = second else {
        panic!("existing explicit path must be refused");
    };
    assert!(
        error.text.as_str().contains("try plan-1.md"),
        "{}",
        error.text
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("out/plan.md")).unwrap(),
        "The winning artifact body.\nSecond line.\n"
    );
    let missing = save::save_file(&room, job(&app, Some("nope/x.md"), dir.path().into())).await;
    assert!(matches!(missing, RequestResult::Rejected { .. }));
}

#[tokio::test]
async fn no_contest_falls_back_to_last_assistant_message_or_the_focused_one() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app();
    let first = record(
        2,
        Actor::Member(Nick::new("alpha").unwrap()),
        "older reply",
        None,
    );
    let focus = first.event_id;
    app.session.snapshot.context.records = vec![
        record(1, Actor::Me, "hello there", None),
        first,
        record(
            3,
            Actor::Member(Nick::new("beta").unwrap()),
            "the newest reply",
            None,
        ),
    ];
    let room = Stub(String::new());
    let latest = job(&app, None, dir.path().into());
    saved(save::save_file(&room, latest).await);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("the-newest-reply-1.md")).unwrap(),
        "the newest reply\n"
    );
    let focused = save::plan(
        &app.session,
        Some(focus),
        None,
        EventId::new(),
        dir.path().into(),
    )
    .unwrap();
    saved(save::save_file(&room, focused).await);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("older-reply-1.md")).unwrap(),
        "older reply\n"
    );
    app.session.snapshot.context.records.clear();
    assert!(save::plan(&app.session, None, None, EventId::new(), dir.path().into()).is_err());
}

#[test]
fn newest_finished_contest_wins_over_older_finished_and_newer_unfinished() {
    let mut app = app();
    let ids: Vec<ContestId> = (1..=4)
        .map(|n| format!("00000000-0000-7000-8000-{n:012}").parse().unwrap())
        .collect();
    for (id, status) in ids
        .iter()
        .zip(["COMPLETE", "COMPLETE", "FAILED", "RUNNING"])
    {
        app.session.contests.entry(*id).or_default().state =
            Some(ContestState::Generic(GenericContestState {
                contest_id: *id,
                kind: ContestKind::Forge,
                state_version: 3,
                status: Text::new(status).unwrap(),
                step_id: None,
                artifact_event_ids: vec![],
                terminal_reason: None,
            }));
    }
    let selected = job(&app, None, PathBuf::from("."));
    assert!(
        matches!(selected.source, save::SaveSource::Export { id, .. } if id == ids[1]),
        "must select the last successful contest, got {:?}",
        selected.source
    );
}

// Export-shaped fixtures mirror the human report sections in squad-core/src/export.
// Embedded Markdown headings belong to the artifact, not to the exporter.
macro_rules! deliverable_test {
    ($name:ident, $kind:ident, $md:expr, $expected:expr) => {
        #[test]
        fn $name() {
            assert_eq!(save::headline($md, ContestKind::$kind), $expected);
        }
    };
}
deliverable_test!(
    headline_forge,
    Forge,
    "# FORGE\n\nFusion by @alpha beat the best draft.\n\n## Launch plan\n\nShip the CLI.\n\n## Rollout\n\nCanary first.\n\n## Borda\n\nScores\n\n## Provenance\n\nDraft A\n",
    "## Launch plan\n\nShip the CLI.\n\n## Rollout\n\nCanary first.\n"
);
deliverable_test!(
    headline_tune,
    Tune,
    "# TUNE\n\nPromoted\n\nReason: `better`\n\n## Winning prompt\n\n## Instructions\nBe brief.\n```markdown\n## Diff\nkeep this literal\n```\n\n## Diff\n\n+Be brief\n\n## Lineage\nold\n",
    "## Instructions\nBe brief.\n```markdown\n## Diff\nkeep this literal\n```\n"
);
deliverable_test!(
    headline_readback,
    Readback,
    "# READBACK\n\nResult: converged\n\n## Final spec\n\n## API\nReturn 201.\n\n## Errors\nReturn 409 on conflict.\n\n## Convergence\n\nRound 2\n",
    "## API\nReturn 201.\n\n## Errors\nReturn 409 on conflict.\n"
);
deliverable_test!(
    headline_tournament,
    Tournament,
    "## Tournament 1\n\nPrompt\n\n## Accepted ranking · round 2 · replay 0\n\n1. alpha\n\n## Champion: alpha · trophies 1\n\n1. alpha\n\n2. beta\n",
    "## Champion: alpha · trophies 1\n\n1. alpha\n\n2. beta\n"
);
macro_rules! complete_report_test {
    ($name:ident, $kind:ident, $md:expr) => {
        deliverable_test!($name, $kind, $md, $md);
    };
}
complete_report_test!(
    headline_picks_the_deliverable_per_mode_debug,
    Debugging,
    "# DEBUG\n\nResult: resolved\n\n## Brief\n\nZero is treated as missing.\n\nLimits: fixture only\n\n## DIFFERENTIAL\n\n| ID | Cause | Status | Consistent results | Failed prediction |\n| --- | --- | --- | --- | --- |\n| H1 | false default | live | 3 | |\n\n## ROUND 1\n\nRUN THE TEST: submit zero\n\nRESULT: O1\n\n[110.0]\n\n## Report\n\nThe truthiness check replaces valid zero values.\n\nNext test: None proposed\n\n## Suggested patch\n\n```diff\n- value = input || fallback\n+ value = input ?? fallback\n```\n\nUse null checks to preserve zero.\n- Risk: audit other callers\n"
);
complete_report_test!(
    headline_bass_ackwards_prd,
    BassAckwardsPrd,
    "# Launch PRD\n\n**End result:** Shipped CLI\n**Starting point:** prototype\n\n## Overview\nDeliver a stable CLI.\n\n## Definition of done\n- Offline tests pass\n\n## Dependency order\n**Critical path:** M1.P1.L1\n\n## Milestones\n### M1 — Release\n#### Phase M1.P1 — Validate\n- L1 Run tests\n\n## Open questions\nNone\n\n## Appendix\n### Checks\n- No dependency cycle: true\n"
);

complete_report_test!(
    headline_blow_it_up,
    BlowItUp,
    "BLOW IT UP\n\nWHAT SURVIVED\n\n### High / Likely · Missing auth · score 9\n\nAnonymous callers can delete records.\n\nBlast radius: all records\n\nMitigation: require ownership\n\nRebuttal: survived. Residual: cross-tenant risk\n\n## Target snapshot\n\nDELETE /records\n\nScope: API\n"
);
complete_report_test!(
    headline_census,
    Census,
    "## Census 1\n\nCENSUS · pass 2 · plan version 1\n\n3 gaps found.\n\n- Must fix: no rollback — outage cannot be reversed\n  Finders (2): alpha, beta · lens: operations\n\nDiscovery curve\nPass 1: 2; pass 2: 1\n"
);
complete_report_test!(
    headline_forecast,
    Forecast,
    "# FORECAST\n\nWill launch ship?\n\nResolution: 2026-12-01 — released tag\nSource: repository\nVoid rule: cancelled release\n\nMedian: 70%\n\n| Member | First | Final | Revision |\n|---|---:|---:|---|\n| alpha | 60 | 70 | pilot passed |\n\n## Cruxes\n\nBuild stability\n\n## Update if you see\n\nCI fails twice.\n\n## Shared assumptions\n\nModels share training data and blind spots.\n"
);
complete_report_test!(
    headline_rank,
    Rank,
    "# RANK\n\nCriterion: reliability\n\nResult: complete\n\n## ORDER\n\n1. I2 — SQLite (score 0.9; 3–0)\n\n### Why the top items\n\n- I2: transactional\n\nCaveats: small sample\n\n## CYCLES\n\n0 total\n"
);
complete_report_test!(
    headline_trace,
    Trace,
    "# TRACE\n\nRelease audit\n\nResult: complete\n\n## BRIEF\n\n### Status\n\nThe release passed tests [S1].\n\n## Disputed claims\n\nPerformance remains disputed.\n\n## Evidence gaps\n\nNo macOS benchmark.\n\n## Claim-to-source ledger\n\n### [C1] Tests passed\n\nS1: test log\n"
);
complete_report_test!(
    headline_negotiate,
    Negotiate,
    "# NEGOTIATE\n\n## Strategy sheet\n\nCap spend at 100.\n\n## Public scenario\n\nVendor renewal\n\n## TABLE 1 — settled\n\nPrice 90.\n\n## PLAYBOOK\n\n### Opening\n\nOffer 80.\n\n### Walk-away line\n\nAbove 100.\n"
);

complete_report_test!(
    headline_probe,
    Probe,
    "## Probe 1\n\n## Diagnostic brief\n\nIntermittent timeout\n\n## Prediction matrix\n\n## Explanation E1\n\nC1: high predicts O1\n\n## Decision tree\n\nStart at N1\n\n## Node N1\n\nInspect active connections; O1 points to E1.\n"
);
complete_report_test!(
    headline_survey,
    Survey,
    "## SURVEY 1\n\nCompare caches\n\nSource: room\n\n## Survey claim map\n\n### Consensus\n\n### C1\n\nBound cache growth.\n\nFirst: AGREE 2 · DISAGREE 0 · UNSURE 0 · MISSING 0\n"
);
complete_report_test!(
    headline_crosscheck,
    Crosscheck,
    "## CROSSCHECK · 1\n\nPlan version 2\n\nRelease checklist\n\n### Extracted goal\n\nShip safely.\n\n### Ruling\n\nClose rollback gap.\n\n### Coverage ledger\n\nC1: covered by canary.\n"
);

// Render committed fixtures through the core exporter, so these tests exercise
// its actual headings rather than invented report shapes.
fn export_event(n: u32, kind: &str, payload: serde_json::Value) -> CommittedEnvelope {
    serde_json::from_value(serde_json::json!({
        "version": 1, "id": format!("01900000-0000-7000-8000-{n:012x}"),
        "roomId": "main", "correlationId": EventId::new(), "sender": "system",
        "recipients": ["me"], "createdAt": "2026-11-28T00:00:00Z",
        "seq": n.to_string(), "acceptedAt": "2026-11-28T00:00:00Z",
        "commitIndex": 0, "commitSize": 1, "kind": kind, "payload": payload
    }))
    .unwrap()
}

#[test]
fn headline_debate() {
    // Checked-in output of the real exporter, including completed rounds,
    // adjudication and the terminal state following the verdict.
    let snapshot = include_str!("../../squad-core/tests/snapshots/export__debate.snap");
    let md = snapshot.split_once("\n---\n").unwrap().1;
    assert!(md.contains("Index the pages.") && md.contains("round 1 score"));
    let result = save::headline(md, ContestKind::Debate);
    assert!(result.contains("Winner: alpha"));
    assert!(result.contains("The design fits the brief."));
    assert!(result.contains("Borrow the index."));
    for evidence in [
        "Index the pages.",
        "round 1 score",
        "Original solution",
        "Timed out",
    ] {
        assert!(
            !result.contains(evidence),
            "retained debate evidence: {evidence}"
        );
    }
}

#[test]
fn headline_grill() {
    use serde_json::json;
    let id = ContestId::new();
    let plan = export_event(
        2,
        "grill.plan",
        json!({
            "contestId": id, "leader": "alpha", "round": 1, "version": 2,
            "text": "## Launch\nCanary first.\n\n## Rollback\nRestore previous binary.\n\n```markdown\n## Complete attributed question-and-answer appendix\nPlan example\n```"
        }),
    );
    let snapshot = squad_core::export::ExportSnapshot {
        events: vec![
            export_event(
                1,
                "grill.plan",
                json!({
                    "contestId": id, "leader": "alpha", "round": 0, "version": 1,
                    "text": "Original plan before review"
                }),
            ),
            plan.clone(),
            export_event(
                3,
                "grill.complete",
                json!({
                    "contestId": id, "finalPlanEventId": plan.base.id, "finalPlanVersion": 2,
                    "appendix": [{"questionEventId": EventId::new(), "questionIndex": 0,
                        "griller": "beta", "question": "Who pays?", "answer": "Users pay",
                        "planChanges": "Add pricing"}]
                }),
            ),
        ],
        ..Default::default()
    };
    let md = squad_core::export::render(&snapshot, ExportScope::Grill, Some(id)).unwrap();
    assert!(md.contains("Who pays?") && md.contains("Users pay"));
    let result = save::headline(&md, ContestKind::Grill);
    assert!(result.contains("## Launch\nCanary first."));
    assert!(result.contains("## Rollback\nRestore previous binary."));
    assert!(result.contains("Plan example"));
    assert_eq!(
        result
            .matches("Complete attributed question-and-answer appendix")
            .count(),
        1,
        "only the fenced example belongs in the saved plan"
    );
    for evidence in [
        "Original plan before review",
        "Who pays?",
        "Users pay",
        "Add pricing",
        "@beta",
    ] {
        assert!(
            !result.contains(evidence),
            "retained grill evidence: {evidence}"
        );
    }
}

#[test]
fn headline_decide() {
    use serde_json::json;
    let id = ContestId::new();
    let report = json!({
        "proposers": {},
        "options": [{"optionId": "B", "name": "SQLite", "description": "embedded database"}],
        "rubric": {"kept": [], "dropped": []},
        "matrix": {"cells": [], "ranking": [{"optionId": "B", "total": 8.0}],
            "margin": 1.0, "winner": "B", "leaveOneOut": [], "flips": [],
            "robust": true, "memberSupport": 2},
        "explanation": {"flipConditions": [], "dissents": [], "evidenceGaps": ["Unmeasured latency."]},
        "skipped": []
    });
    let artifact = |n, kind, body| {
        export_event(
            n,
            "contest.artifact",
            json!({
                "contestId": id, "kind": "decide", "stepId": StepId::new(),
                "artifactKind": kind, "audience": {"kind": "human"}, "sourceRefs": [], "data": {}, "body": body
            }),
        )
    };
    let snapshot = squad_core::export::ExportSnapshot {
        events: vec![
            export_event(
                1,
                "decide.setup",
                json!({
                    "contestId": id, "kind": "decide", "scriptVersion": "v1",
                    "participants": ["alpha", "beta"], "sourceEventIds": [], "settings": {}
                }),
            ),
            artifact(2, "decide.report", report),
            artifact(
                3,
                "decide.ballot",
                json!({"ballotEvidence": "raw score sheet"}),
            ),
        ],
        ..Default::default()
    };
    let md = squad_core::export::render(&snapshot, ExportScope::Decide, Some(id)).unwrap();
    assert!(md.contains("## Committed proposals and tally inputs"));
    assert!(md.contains("ballotEvidence"));
    let result = save::headline(&md, ContestKind::Decide);
    for answer in [
        "B: SQLite — embedded database",
        "Winner: B",
        "WHAT FLIPS IT",
        "Unmeasured latency.",
    ] {
        assert!(
            result.contains(answer),
            "missing decision content: {answer}"
        );
    }
    for evidence in [
        "Committed proposals and tally inputs",
        "decide.ballot",
        "ballotEvidence",
        "```json",
    ] {
        assert!(
            !result.contains(evidence),
            "retained decide evidence: {evidence}"
        );
    }
}

#[test]
fn ambiguous_exporter_headings_preserve_the_complete_artifact() {
    for (kind, md) in [
        (
            ContestKind::Debate,
            "## Verdict · example inside a round\nKeep this argument.\n\n## Verdict · debate 1 · judge fixture\nWinner: alpha\n",
        ),
        (
            ContestKind::Grill,
            "## FINAL PLAN v2 · 1\n## Complete attributed question-and-answer appendix\nA section inside the plan.\n## Complete attributed question-and-answer appendix\nEvidence\n",
        ),
        (
            ContestKind::Decide,
            "# Decide 1\n## Committed proposals and tally inputs\nA section inside the answer.\n## Committed proposals and tally inputs\nEvidence\n",
        ),
        (
            ContestKind::Forge,
            "# FORGE\n\nFusion by @alpha beat the best draft.\n\n## Borda\nExplain how ballots are counted.\n\n## Implementation\nKeep this winning implementation.\n\n## Borda\n\n| Author | Mean score |\n| alpha | 9 |\n",
        ),
        (
            ContestKind::Tune,
            "# TUNE\n\nPromoted\n\n## Winning prompt\n\n## Diff\nExplain the user's diff.\n\n## Response\nPreserve this response template.\n\n## Diff\n\n```diff\n+prompt\n```\n",
        ),
        (
            ContestKind::Readback,
            "# READBACK\n\nResult: converged\n\n## Final spec\n\n## Convergence\nStop after three matching readings.\n\n## Failure handling\nPreserve this recovery procedure.\n\n## Convergence\n\n| Round | Version |\n| 1 | 1 |\n",
        ),
        (
            ContestKind::Tournament,
            "## Proposal\n\n## Champion: a heading inside the proposal\n\nProposed design.\n\n## Champion: alpha · trophies 1\n\n1. alpha\n\n2. beta\n",
        ),
    ] {
        assert_eq!(
            save::headline(md, kind),
            md,
            "{kind:?}: ambiguous exporter headings must not discard content"
        );
    }
}

#[test]
fn slug_omits_routing_and_caps_length() {
    assert_eq!(save::slug("  Hello, World -- ok?! "), "hello-world-ok");
    assert_eq!(save::slug(&"x".repeat(100)).len(), 20);
    assert_eq!(
        save::slug("@all $tournament Write a full plan for an idle-compute marketplace"),
        "write-a-full-plan-fo"
    );
}

#[test]
fn slash_save_runs_through_submit_without_reaching_the_room() {
    let mut app = app();
    finished_forge(&mut app);
    for c in "/save".chars() {
        app.handle(
            Event::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)),
            Duration::ZERO,
        );
    }
    app.handle(
        Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        Duration::ZERO,
    );
    assert!(app.pending.is_none());
    assert!(app.pending_save.is_some());
    assert_eq!(app.input.text(), "");
}

#[test]
fn numbering_continues_past_gaps() {
    let dir = tempfile::tempdir().unwrap();
    for n in [1, 3] {
        std::fs::write(dir.path().join(format!("idea-{n}.md")), "x").unwrap();
    }
    std::fs::write(dir.path().join("idea-x.md"), "x").unwrap();
    let path = save::write_numbered(dir.path(), "idea", "body\n").unwrap();
    assert_eq!(path.file_name().unwrap(), "idea-4.md");
    let fresh = tempfile::tempdir().unwrap();
    let path = save::write_numbered(fresh.path(), "idea", "body\n").unwrap();
    assert_eq!(path.file_name().unwrap(), "idea-1.md");
}
