use serde_json::{Value, json};
use squad_protocol::{bass_ackwards_prd::*, *};
pub fn document() -> Value {
    json!({"title":"Status page","overview":"A hosted public page with alerts.","milestones":[{"key":null,"localId":"m1","title":"Foundation","goal":"Serve status","phases":[{"key":null,"localId":"p1","title":"Build","lineItems":[{"key":null,"localId":"l1","text":"Deploy the public status page","ownerRole":"Developer","precondition":"Host available","risk":"Host outage","serves":["D1"],"dependsOn":[],"sources":["T1:n1"]}]}],"provenance":{"sourceLabels":["T1"],"note":"From member tree"}},{"key":null,"localId":"m2","title":"Alerts","goal":"Notify operators","phases":[{"key":null,"localId":"p2","title":"Notify","lineItems":[{"key":null,"localId":"l2","text":"Deliver outage alerts","ownerRole":"Operator","precondition":"Page available","risk":"Delivery failure","serves":["D2"],"dependsOn":["l1"],"sources":["T1:n2"]}]}],"provenance":{"sourceLabels":["T1"],"note":"From member tree"}}],"overviewProvenance":{"sourceLabels":["T1"],"note":"Shared plan"},"openQuestions":[]})
}
pub fn tree() -> Value {
    json!({"nodes":[{"nodeId":"n1","parent":null,"precondition":"Page available","deliverable":"Public page","ownerRole":"Developer","risk":"Host unavailable","serves":["D1"]},{"nodeId":"n2","parent":"n1","precondition":"Host prepared","deliverable":"Container deployment","ownerRole":"Operator","risk":"Bad configuration","serves":["D1"]},{"nodeId":"n3","parent":"n2","precondition":"Repository created","deliverable":"Source control","ownerRole":"Developer","risk":"Missing access","serves":["D2"]}],"reachesToday":true,"gapNote":null,"todayAssumptions":[]})
}
pub fn data() -> Value {
    json!({"done":{"items":[{"id":"D1"},{"id":"D2"}]},"sourceIds":["T1:n1","T1:n2"],"sourceLabels":["T1"]})
}
#[test]
fn tree_rejects_unknown_parent_cycle_depth_five_and_unmapped_node() {
    validate_artifact("bass_ackwards_prd.tree", &tree()).unwrap();
    let mut missing = tree();
    missing["nodes"][0]["parent"] = json!("missing");
    assert!(validate_artifact("bass_ackwards_prd.tree", &missing).is_err());
    let mut v = tree();
    v["nodes"][0]["parent"] = json!("n3");
    assert!(validate_artifact("bass_ackwards_prd.tree", &v).is_err());
    let mut v = tree();
    v["nodes"][1]["serves"] = json!([]);
    assert!(validate_artifact("bass_ackwards_prd.tree", &v).is_err());
    let mut v = tree();
    for i in 4..=5 {
        let mut n = v["nodes"][2].clone();
        n["nodeId"] = json!(format!("n{i}"));
        n["parent"] = json!(format!("n{}", i - 1));
        v["nodes"].as_array_mut().unwrap().push(n);
    }
    assert!(validate_artifact("bass_ackwards_prd.tree", &v).is_err());
}
#[test]
fn document_requires_sources_dod_and_forward_acyclic_dependencies() {
    validate_output(&WorkKind::BassAckwardsPrdMerge, &data(), &document()).unwrap();
    for (field, value) in [
        ("sources", json!([])),
        ("serves", json!([])),
        ("serves", json!(["D99"])),
        ("dependsOn", json!(["unknown"])),
        ("dependsOn", json!(["l2"])),
    ] {
        let mut v = document();
        v["milestones"][0]["phases"][0]["lineItems"][0][field] = value;
        assert!(validate_output(&WorkKind::BassAckwardsPrdMerge, &data(), &v).is_err());
    }
    let mut v = document();
    v["milestones"][1]["phases"][0]["lineItems"][0]["dependsOn"] = json!(["l2"]);
    assert!(validate_artifact("bass_ackwards_prd.merge", &v).is_err());
}
#[test]
fn freeze_cannot_reword_or_omit_a_proposal() {
    let input = json!({"items":[{"ref":"A:x","label":"A","test":"Page is reachable"},{"ref":"B:x","label":"B","test":"Page is reachable"}]});
    let valid = json!({"groups":[{"text":"Page is reachable","sourceRefs":["A:x","B:x"]}]});
    validate_output(&WorkKind::BassAckwardsPrdFreeze, &input, &valid).unwrap();
    let mut wrong = valid.clone();
    wrong["groups"][0]["text"] = json!("Page works");
    assert!(validate_output(&WorkKind::BassAckwardsPrdFreeze, &input, &wrong).is_err());
    wrong = valid;
    wrong["groups"][0]["sourceRefs"] = json!(["A:x"]);
    assert!(validate_output(&WorkKind::BassAckwardsPrdFreeze, &input, &wrong).is_err());
}
#[test]
fn schemas_are_available_for_all_nine_work_kinds() {
    for kind in [
        WorkKind::BassAckwardsPrdDone,
        WorkKind::BassAckwardsPrdFreeze,
        WorkKind::BassAckwardsPrdTree,
        WorkKind::BassAckwardsPrdMerge,
        WorkKind::BassAckwardsPrdReview,
        WorkKind::BassAckwardsPrdRevise,
        WorkKind::BassAckwardsPrdMustHave,
        WorkKind::BassAckwardsPrdFinalize,
        WorkKind::BassAckwardsPrdApply,
    ] {
        assert!(output_schema(&kind).is_some());
    }
}

#[test]
fn empty_quote_is_an_accepted_comment_for_app_drop_without_repair() {
    let output = json!({"build":null,"comments":[{"localId":"x","quote":"","comment":"Check this passage","proposed":null}]});
    validate_output(
        &WorkKind::BassAckwardsPrdReview,
        &json!({"lens":"GAPS"}),
        &output,
    )
    .unwrap();
}
#[test]
fn tree_node_cap_and_uninvented_keys_are_enforced() {
    let mut t = tree();
    let first = t["nodes"][0].clone();
    for i in 4..=21 {
        let mut node = first.clone();
        node["nodeId"] = json!(format!("n{i}"));
        t["nodes"].as_array_mut().unwrap().push(node);
    }
    assert!(validate_artifact("bass_ackwards_prd.tree", &t).is_err());
    let mut d = document();
    d["milestones"][0]["key"] = json!("invented-permanent-key");
    assert!(validate_output(&WorkKind::BassAckwardsPrdMerge, &data(), &d).is_err());
}
