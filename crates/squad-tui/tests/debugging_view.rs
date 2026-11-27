use squad_protocol::*;
use squad_tui::widgets::debugging::*;
#[test]
fn committed_state_is_idempotent_and_pending_shape_is_visible() {
    let id = ContestId::new();
    let state = DebuggingState {
        contest_id: id,
        state_version: 3,
        state: DebuggingStatus::AwaitingHuman,
        step_id: Some(StepId::new()),
        round: 1,
        artifact_event_ids: vec![],
        terminal_reason: None,
    };
    let mut v = View::default();
    let event = EventId::new();
    v.apply(event, &EnvelopePayload::DebuggingState(state.clone()));
    v.apply(event, &EnvelopePayload::DebuggingState(state.clone()));
    let mut stale = state;
    stale.state_version = 2;
    stale.state = DebuggingStatus::Selecting;
    v.apply(EventId::new(), &EnvelopePayload::DebuggingState(stale));
    assert_eq!(v.state.as_ref().unwrap().state_version, 3);
    let lines = render(&v).join("\n");
    assert!(lines.contains("DEBUG"));
    assert!(lines.contains("RUN THE TEST"));
    assert!(lines.contains("/answer O1 <what you saw>"));
}

#[test]
fn discovery_uses_the_enabled_switch() {
    assert!(squad_tui::typeahead::enabled("$debug"));
    assert_eq!(
        squad_tui::typeahead::describe("$debug", &[]),
        "Narrow a failure with tests you run"
    );
    assert!(
        squad_protocol::parser::help_examples()
            .iter()
            .any(|s| s.contains("$debug"))
    );
}

#[tokio::test]
async fn composed_differential_greys_only_eliminated_rows() {
    use ratatui::{Terminal, backend::TestBackend, style::Color};
    use serde_json::json;
    use squad_tui::{
        app::App,
        fx::TerminalCaps,
        session::{FakeRoom, RoomPort},
        ui,
    };
    use std::time::Duration;
    let rows = json!([
        {"hypothesis":{"hypothesisId":"H1","cause":"ELIMINATED_CAUSE","sources":["A:a"],"supporting":[],"contradicting":[],"priorSpread":["high"]},"status":"eliminated","consistentResults":0,"failedPrediction":"O2"},
        {"hypothesis":{"hypothesisId":"H2","cause":"LIVE_CAUSE","sources":["A:b"],"supporting":[],"contradicting":[],"priorSpread":["low"]},"status":"live","consistentResults":1,"failedPrediction":null}
    ]);
    for (kind, body) in [
        ("debug.elimination", json!({"round":1,"hypotheses":rows})),
        (
            "debug.report",
            json!({"brief":"A failure","limits":"Read only","differential":rows,"tables":{},"provenance":{},"rounds":[],"quotes":[],"skipped":[],"rawAnswers":[],"reason":"stopped","patchAllowed":false,"narrative":null}),
        ),
    ] {
        let contest = ContestId::new();
        let state = DebuggingState {
            contest_id: contest,
            state_version: 1,
            state: DebuggingStatus::Stopped,
            step_id: None,
            round: 1,
            artifact_event_ids: vec![],
            terminal_reason: Some("stopped".parse().unwrap()),
        };
        let artifact = ContestArtifact {
            contest_id: contest,
            step_id: StepId::new(),
            artifact_kind: kind.parse().unwrap(),
            work_id: None,
            source_refs: vec![],
            data: json!({}),
            kind: Some(ContestKind::Debugging),
            round: Some(1),
            actor: Some(Actor::System),
            audience: Some(ArtifactAudience::Human),
            body: Some(body),
        };
        let mut snapshot = FakeRoom::new().state().await;
        snapshot.context.active_contest = Some(ContestState::Debugging(state.clone()));
        snapshot.events = [
            EnvelopePayload::DebuggingState(state),
            EnvelopePayload::ContestArtifact(artifact),
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
        let caps = TerminalCaps::resolve(
            |k| (k == "SQUAD_FX").then(|| "off".into()),
            true,
            (140, 60).into(),
        )
        .unwrap();
        let mut app = App::new(caps);
        app.session.restore(snapshot);
        let mut terminal = Terminal::new(TestBackend::new(140, 60)).unwrap();
        terminal
            .draw(|f| ui::render(&mut app, f, Duration::ZERO))
            .unwrap();
        let buffer = terminal.backend().buffer();
        for (text, dim) in [("ELIMINATED_CAUSE", true), ("LIVE_CAUSE", false)] {
            let (x, y) = (0..60)
                .find_map(|y| {
                    let line = (0..140)
                        .map(|x| buffer[(x, y)].symbol())
                        .collect::<String>();
                    line.find(text)
                        .map(|x| (line[..x].chars().count() as u16, y))
                })
                .expect("differential row missing");
            for offset in 0..text.len() as u16 {
                assert_eq!(
                    buffer[(x + offset, y)].fg == Color::DarkGray,
                    dim,
                    "{kind}/{text}"
                );
            }
        }
    }
}
