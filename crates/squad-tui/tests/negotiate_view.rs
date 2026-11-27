use serde_json::json;
use squad_protocol::*;
use squad_tui::widgets::negotiate::*;
fn setup() -> serde_json::Value {
    json!({"packVersion":"fixture","ourSide":"Our side","counterpart":"Their side","participants":["a","b","c","d"],"models":{},"prompts":{},"rounds":3,"tables":[{"table":1,"negotiator":"a","counterpart":"b","roundsUsed":1,"outcome":"deal","turns":[{"table":1,"round":1,"actor":"b","counterpart":true,"body":{"message":"Accepted"}}]},{"table":2,"negotiator":"c","counterpart":"d","roundsUsed":1,"outcome":"no_deal","turns":[]}],"roomWords":500,"planWords":1200,"redactionTokens":[]})
}
fn artifact(c: ContestId, kind: &str, body: serde_json::Value) -> EnvelopePayload {
    EnvelopePayload::ContestArtifact(ContestArtifact {
        kind: Some(ContestKind::Negotiate),
        round: Some(0),
        actor: Some(Actor::System),
        audience: Some(ArtifactAudience::Human),
        body: Some(body),
        contest_id: c,
        step_id: StepId::new(),
        artifact_kind: kind.parse().unwrap(),
        work_id: None,
        source_refs: vec![],
        data: json!({}),
    })
}
#[test]
fn view_replay_stale_state_and_partial_turns() {
    let c = ContestId::new();
    let mut v = View::default();
    let s = NegotiateState {
        contest_id: c,
        state_version: 2,
        state: NegotiateStatus::Rehearsing,
        step_id: None,
        round: 1,
        artifact_event_ids: vec![],
        terminal_reason: None,
    };
    v.apply(EventId::new(), &EnvelopePayload::NegotiateState(s.clone()));
    let mut stale = s;
    stale.state_version = 1;
    stale.state = NegotiateStatus::Mapping;
    v.apply(EventId::new(), &EnvelopePayload::NegotiateState(stale));
    assert_eq!(v.state.as_ref().unwrap().state, NegotiateStatus::Rehearsing);
    let a = artifact(
        c,
        "negotiate.turn",
        json!({"table":1,"round":1,"actor":"b","counterpart":true,"body":{"message":"Walk away","response":"walk_away","counterOffer":null,"violatedInterest":null}}),
    );
    let id = EventId::new();
    v.apply(id, &a);
    v.apply(id, &a);
    assert_eq!(v.artifacts.len(), 1);
    let text = render(&v).join("\n");
    assert!(text.contains("NEGOTIATE"));
    assert!(text.contains("PREP"));
    assert!(text.contains("TABLE 1"));
    assert!(text.contains("counterpart: Walk away"));
}
#[test]
fn wide_columns_and_narrow_stack() {
    let c = ContestId::new();
    let mut v = View::default();
    let report = json!({"setup":setup(),"maps":{},"brief":null,"packages":{},"tables":setup()["tables"],"playbook":null,"counts":{"deal":1,"no_deal":1},"skipped":[],"noRehearsalEvidence":false});
    v.apply(EventId::new(), &artifact(c, "negotiate.report", report));
    let wide = render_width(&v, 140).join("\n");
    assert!(wide.contains('│'));
    assert!(wide.contains("TABLE 1"));
    assert!(wide.contains("TABLE 2"));
    assert!(wide.contains("counterpart: Accepted"));
    assert!(wide.contains("PLAYBOOK"));
    let narrow = render_width(&v, 60).join("\n");
    assert!(!narrow.contains('│'));
    assert!(narrow.contains("TABLE 1"));
    assert!(narrow.contains("TABLE 2"));
}
#[test]
fn switch_controls_help_typeahead_and_parser_examples() {
    let enabled = negotiate::ENABLED;
    assert_eq!(
        squad_tui::typeahead::MODES
            .iter()
            .any(|(n, _)| *n == "$negotiate"),
        enabled
    );
    assert_eq!(
        squad_tui::actions::help_lines()
            .join("\n")
            .contains("$negotiate"),
        enabled
    );
    let context = parser::ParseContext {
        roster: ["a", "b", "c", "d"].map(|n| n.parse().unwrap()).to_vec(),
        ..Default::default()
    };
    let examples = squad_tui::typeahead::examples(&context);
    assert_eq!(examples.iter().any(|s| s.contains("$negotiate")), enabled);
    for s in examples.iter().filter(|s| s.contains("$negotiate")) {
        assert!(matches!(
            parse(s, &parser::SubmissionMeta::default(), &context).unwrap(),
            ParsedInput::Negotiate { .. }
        ));
    }
}

#[tokio::test]
async fn composed_negotiate_reflows_tables_at_the_actual_viewport_width() {
    use ratatui::{Terminal, backend::TestBackend};
    use squad_tui::{
        app::App,
        fx::TerminalCaps,
        session::{FakeRoom, RoomPort},
        ui,
    };
    use std::time::Duration;
    let contest = ContestId::new();
    let state = NegotiateState {
        contest_id: contest,
        state_version: 1,
        state: NegotiateStatus::Complete,
        step_id: None,
        round: 1,
        artifact_event_ids: vec![],
        terminal_reason: Some("complete".parse().unwrap()),
    };
    let report = json!({"setup":setup(),"maps":{},"brief":null,"packages":{},"tables":setup()["tables"],"playbook":null,"counts":{"deal":1,"no_deal":1},"skipped":[],"noRehearsalEvidence":false});
    let room = FakeRoom::new();
    let mut snapshot = room.state().await;
    snapshot.events = [
        EnvelopePayload::NegotiateState(state.clone()),
        artifact(contest, "negotiate.report", report),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, content)| {
        let now = std::time::SystemTime::now().into();
        CommittedEnvelope {
            base: EnvelopeBase {
                version: 1,
                id: EventId::new(),
                room_id: MainRoom::Main,
                correlation_id: EventId::new(),
                causation_id: None,
                sender: Actor::System,
                recipients: vec![Recipient::Me],
                created_at: now,
            },
            commit: CommitFields {
                seq: Cursor::new(i as i64 + 1).unwrap(),
                accepted_at: now,
                commit_index: 0,
                commit_size: 1,
            },
            content,
        }
    })
    .collect();
    snapshot.context.active_contest = Some(ContestState::Negotiate(state));
    let caps = TerminalCaps::resolve(
        |key| (key == "SQUAD_FX").then(|| "off".into()),
        true,
        (160, 60).into(),
    )
    .unwrap();
    let mut app = App::new(caps);
    app.session.restore(snapshot);
    for width in [160, 80, 160] {
        let mut terminal = Terminal::new(TestBackend::new(width, 60)).unwrap();
        terminal
            .draw(|f| ui::render(&mut app, f, Duration::ZERO))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let lines = (0..60)
            .map(|y| {
                (0..width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>();
        assert!(lines.iter().any(|s| s.contains("TABLE 1")));
        assert!(lines.iter().any(|s| s.contains("TABLE 2")));
        assert_eq!(
            lines
                .iter()
                .any(|s| s.contains("TABLE 1") && s.contains("TABLE 2")),
            width >= 120
        );
    }
}
