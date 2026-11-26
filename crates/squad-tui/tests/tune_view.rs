use squad_protocol::{tune::*, *};
use squad_tui::widgets::tune::*;
#[test]
fn human_view_dividers_and_report_are_committed_only() {
    let id = ContestId::new();
    let mut view = View::default();
    let mut s = TuneState {
        contest_id: id,
        state_version: 1,
        state: TuneStatus::BuildingCases,
        step_id: None,
        round: 0,
        artifact_event_ids: vec![],
        terminal_reason: None,
    };
    view.apply(EventId::new(), &EnvelopePayload::TuneState(s.clone()));
    assert_eq!(render(&view), ["TUNE", "TEST SET"]);
    s.state = TuneStatus::Mutating;
    s.round = 2;
    s.state_version += 1;
    view.apply(EventId::new(), &EnvelopePayload::TuneState(s.clone()));
    assert!(render(&view).contains(&"GENERATION 2".into()));
    let old = s.clone();
    s.state = TuneStatus::Complete;
    s.terminal_reason = Some(Text::new("tuned").unwrap());
    s.state_version += 1;
    view.apply(EventId::new(), &EnvelopePayload::TuneState(s));
    view.apply(EventId::new(), &EnvelopePayload::TuneState(old));
    assert!(render(&view).contains(&"RESULT".into()));
    let r = TuneReport {
        reason: "tuned".into(),
        result_line: "Tuned: 0.58 to 0.81 on held-out cases.".into(),
        winner: "c1".into(),
        prompt: "prompt".into(),
        diff: "diff".into(),
    };
    let body = serde_json::to_value(r).unwrap();
    let a = ContestArtifact {
        kind: Some(ContestKind::Tune),
        round: Some(2),
        actor: Some(Actor::System),
        audience: Some(ArtifactAudience::Human),
        body: Some(body.clone()),
        contest_id: id,
        step_id: StepId::new(),
        artifact_kind: Text::new("tune.report").unwrap(),
        work_id: None,
        source_refs: vec![],
        data: body,
    };
    view.apply(EventId::new(), &EnvelopePayload::ContestArtifact(a));
    assert!(render(&view).iter().any(|l| l.starts_with("Tuned: 0.58")));
}
