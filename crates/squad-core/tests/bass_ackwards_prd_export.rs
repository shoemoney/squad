#[path = "../../squad-protocol/tests/bass_ackwards_prd.rs"]
mod fixtures;
use squad_core::{
    export::bass_ackwards_prd::render_document,
    orchestrator::bass_ackwards_prd::{checks, mint_keys},
};
use squad_protocol::{bass_ackwards_prd::*, *};
#[test]
fn house_style_includes_check_findings_and_positions_not_keys() {
    let mut d: BassAckwardsPrdDocument = serde_json::from_value(fixtures::document()).unwrap();
    mint_keys(&mut d);
    let done = BassAckwardsPrdDone {
        items: ["D1", "D2", "D3"]
            .map(|id| BassAckwardsPrdDoneEntry {
                id: id.into(),
                test: format!("Test {id}"),
                verified_by: "Inspection".into(),
                support: 2,
                source_refs: vec![],
            })
            .into(),
        single_source: vec![],
    };
    let s = BassAckwardsPrdSetup {
        contest_id: ContestId::new(),
        end_result: "Live status page".into(),
        today: None,
        participants: vec![],
        human_review: false,
        rotation: vec![],
        models: Default::default(),
        prompt_refs: Default::default(),
        room_words: 500,
        plan_words: 1000,
        prd_words: 2400,
        data_dir: "/tmp".into(),
        redaction_tokens: vec![],
    };
    let c = checks(&d, &done);
    let text = render_document(&s, &done, &d, &c);
    assert!(text.contains("Checks have findings"));
    for section in [
        "## Definition of done",
        "## Dependency order",
        "## Milestones",
        "#### Phase M1.P1",
        "- [ ] M1.P1.L1",
        "### Checks",
        "D3",
    ] {
        assert!(text.contains(section));
    }
    assert!(!text.contains(d.lines()[0].key.as_ref().unwrap()));
}
