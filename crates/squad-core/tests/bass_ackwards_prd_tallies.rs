#[path = "../../squad-protocol/tests/bass_ackwards_prd.rs"]
mod fixtures;
use serde_json::json;
use squad_core::orchestrator::bass_ackwards_prd::*;
use squad_protocol::{bass_ackwards_prd::*, *};
#[test]
fn quorum_freeze_caps_and_order_are_application_owned() {
    let items=json!((0..14).flat_map(|i|[json!({"ref":format!("A:{i}"),"label":"A","test":format!("test {i}"),"verifiedBy":"inspection"}),json!({"ref":format!("B:{i}"),"label":if i==13{"A"}else{"B"},"test":format!("test {i}"),"verifiedBy":"inspection"})]).collect::<Vec<_>>());
    let groups = BassAckwardsPrdGrouping {
        groups: (0..14)
            .map(|i| BassAckwardsPrdGroup {
                text: format!("test {i}"),
                source_refs: vec![format!("A:{i}"), format!("B:{i}")],
            })
            .collect(),
    };
    let d = freeze(&groups, &items);
    assert_eq!(d.items.len(), 12);
    assert_eq!(d.items[11].id, "D12");
    assert_eq!(d.single_source.len(), 1);
    assert!(d.items.iter().all(|i| i.support == 2));
}
#[test]
fn lens_deal_and_call_counts() {
    for m in [3, 6, 7] {
        let n = (0..m)
            .map(|i| format!("n{i}").parse().unwrap())
            .collect::<Vec<MemberNick>>();
        let lenses = n
            .iter()
            .map(|p| lens(&n, p))
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(lenses.len(), 3);
        assert_eq!(readback(&n, &n[2], 2), 0);
    }
    assert_eq!(calls(6, false), (28, 60));
    assert_eq!(calls(3, false), (16, 36));
    assert_eq!(calls(6, true), (29, 63));
}
#[test]
fn keys_levels_critical_chain_and_slug() {
    let mut d: BassAckwardsPrdDocument = serde_json::from_value(fixtures::document()).unwrap();
    mint_keys(&mut d);
    d.validate().unwrap();
    let first = d.lines()[0].key.clone().unwrap();
    assert_eq!(d.lines()[1].depends_on, [first.clone()]);
    let done = BassAckwardsPrdDone {
        items: ["D1", "D2"]
            .map(|id| BassAckwardsPrdDoneEntry {
                id: id.into(),
                test: "observable".into(),
                verified_by: "test".into(),
                support: 2,
                source_refs: vec![],
            })
            .into(),
        single_source: vec![],
    };
    let c = checks(&d, &done);
    assert!(c.mapped && c.covered && c.dependencies && c.acyclic);
    assert_eq!(c.levels.len(), 2);
    assert_eq!(c.critical_path[0], first);
    assert_eq!(slug("  Héllo WORLD!! "), "h-llo-world");
    assert_eq!(slug("💥"), "prd");
    assert_eq!(slug(&"a".repeat(100)).len(), 48);
    let old = d.clone();
    mint_keys(&mut d);
    assert_eq!(d, old);
}
#[test]
fn revision_integrity_decision_coverage_and_add_only() {
    let mut doc: BassAckwardsPrdDocument = serde_json::from_value(fixtures::document()).unwrap();
    mint_keys(&mut doc);
    let mut data = fixtures::data();
    data["document"] = serde_json::to_value(&doc).unwrap();
    data["sourceIds"] = json!(["R1"]);
    let key = doc.lines()[0].key.clone().unwrap();
    let mut output = json!({"decisions":[{"ref":"R1","verdict":"reject","reason":"Already covered","appliedTo":[],"removes":[]}],"document":doc});
    validate_output(&WorkKind::BassAckwardsPrdRevise, &data, &output).unwrap();
    output["document"]["milestones"][0]["phases"][0]["lineItems"][0]["text"] =
        json!("Silent rewording");
    assert!(validate_output(&WorkKind::BassAckwardsPrdRevise, &data, &output).is_err());
    output["decisions"][0]["verdict"] = json!("accept");
    output["decisions"][0]["appliedTo"] = json!([key]);
    validate_output(&WorkKind::BassAckwardsPrdRevise, &data, &output).unwrap();
    assert!(validate_output(&WorkKind::BassAckwardsPrdFinalize, &data, &output).is_err());
    output["decisions"] = json!([]);
    assert!(validate_output(&WorkKind::BassAckwardsPrdRevise, &data, &output).is_err());
}

#[test]
fn diamond_and_equal_chains_tie_by_document_order() {
    let mut d: BassAckwardsPrdDocument = serde_json::from_value(fixtures::document()).unwrap();
    let mut left = d.milestones[0].phases[0].line_items[0].clone();
    left.local_id = "left".into();
    let mut right = left.clone();
    right.local_id = "right".into();
    d.milestones[0].phases[0].line_items = vec![left, right];
    let mut end = d.milestones[1].phases[0].line_items[0].clone();
    end.depends_on = vec!["right".into(), "left".into()];
    d.milestones[1].phases[0].line_items = vec![end];
    let done = BassAckwardsPrdDone {
        items: vec![],
        single_source: vec![],
    };
    let c = checks(&d, &done);
    assert_eq!(c.critical_path, vec!["left", "l2"]);
    assert_eq!(c.levels, vec![vec!["left", "right"], vec!["l2"]]);
    let mut extra = d.milestones[1].phases[0].line_items[0].clone();
    extra.local_id = "other_end".into();
    extra.depends_on = vec!["right".into()];
    d.milestones[1].phases[0].line_items[0].depends_on = vec!["left".into()];
    d.milestones[1].phases[0].line_items.push(extra);
    assert_eq!(checks(&d, &done).critical_path, vec!["left", "l2"]);
}
