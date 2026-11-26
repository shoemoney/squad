//! M4 first-half audit: exercise the actual app, not only presentation helpers.
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};
use squad_protocol::*;
use squad_tui::{
    app::App,
    fx::TerminalCaps,
    ui, validation,
    widgets::{completion::CompletionState, input::EditorAction},
};
use std::time::Duration;

fn app() -> App {
    let mut app = App::new(
        TerminalCaps::resolve(
            |key| match key {
                "SQUAD_FX" => Some("off".into()),
                "NO_COLOR" => Some("1".into()),
                _ => None,
            },
            false,
            (120, 40).into(),
        )
        .unwrap(),
    );
    app.session.snapshot.context.roster = ["alpha", "beta", "gamma"]
        .map(|n| n.parse().unwrap())
        .to_vec();
    app
}
fn draw(app: &mut App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| ui::render(app, frame, Duration::ZERO))
        .unwrap();
    let buffer = terminal.backend().buffer();
    assert!(
        buffer
            .content
            .iter()
            .all(|cell| cell.fg == ratatui::style::Color::Reset)
    );
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
#[test]
fn four_mode_completion_protected_text_and_rejected_drafts_use_the_parser() {
    for verb in ["census", "crosscheck", "survey", "probe"] {
        let mut app = app();
        let meta = parser::SubmissionMeta::default();
        let context = &app.session.snapshot.context;
        let token = format!("${verb}");
        assert!(
            CompletionState::at(&token, token.len(), context, &meta)
                .items
                .contains(&token)
        );
        let protected = format!("@alpha `{token}` is just code");
        assert!(matches!(
            validation::validate(&protected, &meta, context)
                .unwrap()
                .parsed,
            ParsedInput::Message { .. }
        ));
        // Unknown recipients are rejected before any mode work is submitted.
        let raw = format!("@missing {token} investigate");
        let expected = parse(&raw, &meta, context).unwrap_err().message;
        app.input.apply(EditorAction::Insert(raw.clone())).unwrap();
        app.submit();
        assert_eq!(app.notice.as_deref(), Some(expected.as_str()));
        assert_eq!(app.input.text(), raw);
        assert!(app.pending.is_none());
        let quote = EventId::new();
        app.quotes.toggle(quote);
        app.submit();
        assert_eq!(app.quotes.ids, [quote]);
        assert_eq!(app.input.text(), raw);
    }
}
#[test]
fn census_pass_assignment_stays_pending_and_rejection_preserves_quotes() {
    let mut app = app();
    app.input
        .apply(EditorAction::Insert("$CENSUS_PASSES=3".into()))
        .unwrap();
    app.submit();
    assert_eq!(
        app.pending_updates,
        [VariableUpdate {
            name: VariableName::CensusPasses,
            value: 3
        }]
    );
    assert!(matches!(app.pending, Some(Request::SetVars { .. })));
    assert!(draw(&mut app, 120, 40).contains("pending"));
    app.acknowledge(RequestResult::Accepted {
        request_id: EventId::new(),
        event_ids: vec![],
        cursor: None,
    });
    assert!(app.pending_updates.is_empty());
    assert!(app.input.text().is_empty());
    app.input
        .apply(EditorAction::Insert("$CENSUS_PASSES".into()))
        .unwrap();
    app.submit();
    assert!(app.awaiting_ack);
}
#[test]
fn expanded_input_and_f12_redacted_diagnostics_snapshot_at_both_viewports() {
    for (width, height) in [(120, 40), (80, 24)] {
        let mut app = app();
        app.input
            .apply(EditorAction::Insert(
                (1..=10)
                    .map(|n| format!("line {n}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
            ))
            .unwrap();
        insta::assert_snapshot!(
            format!("expanded_input_{width}"),
            draw(&mut app, width, height)
        );
        app.draws = 23;
        app.feed_items = 41;
        app.max_feed_batch = 17;
        app.handle(
            Event::Key(KeyEvent::new(KeyCode::F(12), KeyModifiers::NONE)),
            Duration::ZERO,
        );
        let debug = draw(&mut app, width, height);
        assert!(debug.contains("max_feed_batch 17"));
        assert!(debug.contains("feed_items 41"));
        insta::assert_snapshot!(format!("debug_{width}"), debug);
    }
}

#[tokio::test]
async fn census_report_search_reveals_copy_and_quote_without_delivering() {
    use squad_tui::session::{FakeRoom, RoomPort};
    let room = FakeRoom::new();
    let id = ContestId::new();
    room.commit(
        Actor::System,
        EnvelopePayload::CensusState(CensusState {
            contest_id: id,
            state_version: 1,
            status: CensusStatus::Complete,
            pass: 1,
            plan_version: 1,
            step_id: None,
            artifact_event_ids: vec![],
            unfinished_work_ids: vec![],
            terminal_reason: None,
        }),
    );
    let report_id = room.commit(
        Actor::System,
        EnvelopePayload::CensusReport(CensusReport {
            contest_id: id,
            pass: 1,
            plan_version: 1,
            gaps: vec![],
            additions: vec![],
            missing: vec![CensusMissing {
                member: "alpha".parse().unwrap(),
                step: Text::new("unique-needle").unwrap(),
            }],
            input_gap_ids: vec![],
            f1: 0,
            f2: 0,
            formula_branch: Text::new("zero_doubletons").unwrap(),
            unseen: 0.,
            estimate_low: None,
            estimate_high: None,
            discovery: vec![],
        }),
    );
    let mut app = app();
    app.session.restore(room.state().await);
    assert!(app.session.snapshot.context.record(report_id).is_some());
    draw(&mut app, 120, 40);
    app.search.query = "unique-needle".into();
    app.refresh_search();
    assert_eq!(app.search.matches.len(), 1);
    assert_eq!(app.msg_focus, Some(report_id));
    assert!(app.composed.verdict_expanded);
    assert!(!app.session.transcript.follow);
    assert!(draw(&mut app, 120, 40).contains("unique-needle"));
    app.copy_selected();
    assert!(app.take_copy().is_some());
    assert!(app.pending.is_none());
    app.close_search();
    app.input
        .apply(EditorAction::Insert("draft remains".into()))
        .unwrap();
    app.focus_input = false;
    for _ in 0..2 {
        app.handle(
            Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Duration::ZERO,
        );
    }
    assert_eq!(app.quotes.ids, [report_id]);
    assert_eq!(app.input.text(), "draft remains");
    assert!(app.pending.is_none());
    let anchor = app.composed.anchor;
    draw(&mut app, 80, 24);
    assert_eq!(app.composed.anchor, anchor);
}

#[test]
fn source_references_and_shared_script_progress_reach_the_ui() {
    use squad_tui::widgets::preflight::PreflightView;
    let id: ContestId = "01900000-0000-7000-8000-000000000001".parse().unwrap();
    let source: EventId = "01900000-0000-7000-8000-000000000002".parse().unwrap();
    let preflight = PreflightView {
        pending: false,
        plan: PendingPreflight::Contest(ContestPreflight {
            contest_id: id,
            kind: ContestKind::Crosscheck,
            plan_id: PlanId::new(),
            expires_at: "2026-11-25T00:00:00Z".parse().unwrap(),
            estimate: DebateEstimate {
                model_calls: 11,
                estimated_usd: 0.1,
                reserved_usd: 0.2,
                remaining_usd: 3.,
            },
            participants: vec!["alpha".parse().unwrap()],
            confirmation_required: true,
            warning: None,
            settings: serde_json::json!({"sourceEventIds": [source], "extractionRequired": true}),
        }),
    };
    assert!(preflight.text().contains(&source.to_string()));
    assert!(preflight.text().contains("extraction call included"));
    let mut app = app();
    let contest = app.session.contests.entry(id).or_default();
    contest.crosscheck.state = Some(CrosscheckState {
        contest_id: id,
        state_version: 1,
        status: CrosscheckStatus::Failed,
        step_id: None,
        plan_id: PlanId::new(),
        plan_version: 1,
        grid_version: 1,
        terminal_reason: None,
    });
    contest.steps.insert(
        1,
        ContestStep {
            contest_id: id,
            step_id: StepId::new(),
            ordinal: 1,
            script_version: Text::new("crosscheck.v1").unwrap(),
            move_name: ContestMove::Fanout,
            inputs: vec![],
            assignments: vec![],
        },
    );
    let work_id = WorkId::new();
    contest.work.insert(
        work_id,
        squad_tui::widgets::contest_status::WorkCard {
            work_id,
            target: "alpha".into(),
            attempt: 2,
            status: WorkStatus::Failed,
            error: Some("fixture failure".into()),
        },
    );
    let screen = draw(&mut app, 120, 40);
    assert!(screen.contains("Step 1 · fan out"), "{screen}");
    assert!(screen.contains("Plan hidden"), "{screen}");
    assert!(screen.contains("alpha · failed · retry 1"), "{screen}");
}
