use squad_protocol::{trace::*, *};
use squad_tui::widgets::trace::*;
#[test]
fn phases_and_duplicate_events_are_stable() {
    let mut v = View::default();
    let mut s = TraceState {
        contest_id: ContestId::new(),
        state_version: 1,
        state: TraceStatus::Extracting,
        step_id: None,
        round: 0,
        artifact_event_ids: vec![],
        terminal_reason: None,
    };
    v.apply(EventId::new(), &EnvelopePayload::TraceState(s.clone()));
    assert!(render(&v).contains(&"EXTRACT".into()));
    s.state = TraceStatus::Merging;
    s.state_version = 2;
    v.apply(EventId::new(), &EnvelopePayload::TraceState(s.clone()));
    assert!(render(&v).contains(&"QUOTE CHECK".into()));
    let old = s.clone();
    s.state = TraceStatus::Verifying;
    s.state_version = 3;
    v.apply(EventId::new(), &EnvelopePayload::TraceState(s.clone()));
    v.apply(EventId::new(), &EnvelopePayload::TraceState(old));
    assert!(render(&v).contains(&"VERIFY".into()));
    s.state = TraceStatus::Assembling;
    s.state_version = 4;
    v.apply(EventId::new(), &EnvelopePayload::TraceState(s));
    assert!(render(&v).contains(&"BRIEF".into()));
}
#[test]
fn selecting_marker_shows_only_its_quoted_evidence() {
    let c = ContestId::new();
    let state = TraceState {
        contest_id: c,
        state_version: 1,
        state: TraceStatus::Verifying,
        step_id: None,
        round: 0,
        artifact_event_ids: vec![],
        terminal_reason: None,
    };
    let body = serde_json::json!({"claims":[{"claimId":"C1","text":"Claim","status":"supported","quotes":[{"memberLabel":"Reader 1","localId":"L1","counter":false,"quote":{"sourceId":"S1","quote":"Records retained for thirty days."},"status":"verified","span":{"start":0,"end":32}}],"independentSources":1,"contributingMembers":1,"verdicts":{}}]});
    let a = ContestArtifact {
        contest_id: c,
        step_id: StepId::new(),
        artifact_kind: "trace.ledger".parse().unwrap(),
        work_id: None,
        source_refs: vec![],
        data: serde_json::json!({}),
        kind: Some(ContestKind::Trace),
        round: Some(0),
        actor: Some(Actor::System),
        audience: Some(ArtifactAudience::Human),
        body: Some(body),
    };
    let mut v = View::default();
    v.apply(EventId::new(), &EnvelopePayload::TraceState(state));
    let id = EventId::new();
    v.apply(id, &EnvelopePayload::ContestArtifact(a.clone()));
    v.apply(id, &EnvelopePayload::ContestArtifact(a));
    assert_eq!(v.artifacts.len(), 1);
    v.select_claim("[C1]");
    assert!(v.inspect().iter().any(|s| s.contains("S1 bytes 0..32")));
    v.select_claim("[C2]");
    assert!(v.inspect().is_empty());
    let mut app = squad_tui::app::App::new(
        squad_tui::fx::TerminalCaps::resolve(|_| None, false, ratatui::layout::Size::new(100, 30))
            .unwrap(),
    );
    app.session.contests.entry(c).or_default().trace = v;
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    app.handle(
        Event::Key(KeyEvent::new(KeyCode::F(9), KeyModifiers::NONE)),
        std::time::Duration::ZERO,
    );
    let view = &app.session.contests[&c].trace;
    assert_eq!(view.selected_claim.as_deref(), Some("C1"));
    assert!(
        view.inspect()
            .iter()
            .any(|line| line.contains("S1 bytes 0..32"))
    );
    assert!(!app.select_trace_claim("[C2]"));
    assert!(app.pending.is_none());
}
#[test]
fn discovery_and_tip_rotation_follow_activation() {
    let ctx = parser::ParseContext {
        roster: vec![
            "one".parse().unwrap(),
            "two".parse().unwrap(),
            "three".parse().unwrap(),
        ],
        ..Default::default()
    };
    assert_eq!(
        squad_tui::typeahead::MODES
            .iter()
            .any(|(name, _)| *name == "$trace"),
        ENABLED
    );
    assert_eq!(squad_tui::typeahead::enabled("$trace"), ENABLED);
    assert_eq!(
        squad_tui::typeahead::examples(&ctx)
            .iter()
            .any(|e| e.contains("$trace")),
        ENABLED
    );
}
