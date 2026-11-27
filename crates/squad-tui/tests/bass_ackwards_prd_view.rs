use squad_protocol::*;
use squad_tui::widgets::bass_ackwards_prd::*;
#[test]
fn view_has_house_dividers_collapsed_trees_last_look_and_saved_path() {
    let mut v = View::default();
    let s = BassAckwardsPrdState {
        contest_id: ContestId::new(),
        state_version: 1,
        state: BassAckwardsPrdStatus::AwaitingHuman,
        step_id: None,
        round: 0,
        artifact_event_ids: vec![],
        terminal_reason: None,
    };
    v.apply(
        EventId::new(),
        &EnvelopePayload::BassAckwardsPrdState(s.clone()),
    );
    let body = serde_json::json!({"filename":"prd-test.md","path":"/private/exports/prd-test.md","bytes":1,"sha256":"hash"});
    let a = ContestArtifact {
        contest_id: s.contest_id,
        step_id: StepId::new(),
        kind: Some(ContestKind::BassAckwardsPrd),
        artifact_kind: "bass_ackwards_prd.saved".parse().unwrap(),
        round: Some(0),
        actor: Some(Actor::System),
        audience: Some(ArtifactAudience::Human),
        body: Some(body.clone()),
        work_id: None,
        source_refs: vec![],
        data: body,
    };
    v.apply(EventId::new(), &EnvelopePayload::ContestArtifact(a));
    let text = render(&v).join("\n");
    for marker in [
        "BASS ACKWARDS",
        "DONE FIRST",
        "BACKWARD TREES",
        "MERGED PRD",
        "REVIEW",
        "LAST MUST-HAVES",
        "CHECKS",
        "SAVED",
        "LAST LOOK",
        "collapsed",
        "PRD saved to /private/exports/prd-test.md",
    ] {
        assert!(text.contains(marker), "{marker}");
    }
}
