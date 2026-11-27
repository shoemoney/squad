use serde_json::json;
use squad_protocol::{negotiate::*, *};
fn input() -> serde_json::Value {
    json!({"wordLimit":1200,"tableCount":2})
}
fn brief() -> serde_json::Value {
    json!({"strategySheet":"Protect fallback","publicScenario":"Public scenario","counterpartBriefs":(0..2).map(|_|json!({"persona":"Supplier","priorities":["Cash"],"reservations":[{"issue":"price","limit":"100"}],"hiddenConstraint":"Needs cash","walkAwayRule":"Below 100"})).collect::<Vec<_>>(),"assumptionsToConfirm":[]})
}
#[test]
fn strict_brief_count_and_projection_budgets() {
    let mut v = brief();
    assert!(validate_output(&WorkKind::NegotiateBrief, &input(), &v).is_ok());
    v["counterpartBriefs"].as_array_mut().unwrap().pop();
    assert!(validate_output(&WorkKind::NegotiateBrief, &input(), &v).is_err());
    v = brief();
    v["counterpartBriefs"][0]["hiddenConstraint"] = json!("word ".repeat(301));
    assert!(validate_output(&WorkKind::NegotiateBrief, &input(), &v).is_err());
    v = brief();
    v["strategySheet"] = json!("word ".repeat(601));
    assert!(validate_output(&WorkKind::NegotiateBrief, &input(), &v).is_err());
    v = brief();
    v["extra"] = json!(true);
    assert!(validate_output(&WorkKind::NegotiateBrief, &input(), &v).is_err());
}
#[test]
fn counter_enums_required_nullable_and_word_budget() {
    let mut v = json!({"message":"No thanks","response":"walk_away","counterOffer":null,"violatedInterest":null});
    let d = json!({"wordLimit":200});
    assert!(validate_output(&WorkKind::NegotiateCounter, &d, &v).is_ok());
    v["response"] = json!("concede");
    assert!(validate_output(&WorkKind::NegotiateCounter, &d, &v).is_err());
    v["response"] = json!("counter");
    assert!(validate_output(&WorkKind::NegotiateCounter, &d, &v).is_err());
    v["counterOffer"] = json!("110");
    assert!(validate_output(&WorkKind::NegotiateCounter, &d, &v).is_ok());
    v.as_object_mut().unwrap().remove("violatedInterest");
    assert!(validate_output(&WorkKind::NegotiateCounter, &d, &v).is_err());
    let v = json!({"message":"word ".repeat(201),"offer":"100"});
    assert!(validate_output(&WorkKind::NegotiateMove, &d, &v).is_err());
}
#[test]
fn playbook_references_overlap_and_assumptions() {
    let mut v = json!({"opening":"Ask","anchors":[],"concessionLadder":[],"objections":[],"walkAwayLine":"Prefer fallback","zopaMap":[{"issue":"price","ourLimit":"110","theirModelledLimit":"100","overlap":"unknown"}],"questionsToAsk":[],"tableNotes":[{"table":1,"leftOnTable":"Timing","unrealisticConcessions":[]},{"table":2,"leftOnTable":"Timing","unrealisticConcessions":[]}],"dependsOnAssumptions":[]});
    assert!(validate_output(&WorkKind::NegotiatePlaybook, &input(), &v).is_ok());
    v["zopaMap"][0]["overlap"] = json!("maybe");
    assert!(validate_output(&WorkKind::NegotiatePlaybook, &input(), &v).is_err());
    v["zopaMap"][0]["overlap"] = json!("yes");
    v["tableNotes"][1]["table"] = json!(3);
    assert!(validate_output(&WorkKind::NegotiatePlaybook, &input(), &v).is_err());
    v["tableNotes"][1]["table"] = json!(1);
    assert!(validate_output(&WorkKind::NegotiatePlaybook, &input(), &v).is_err());
    v["tableNotes"][1]["table"] = json!(2);
    let mut d = input();
    d["hasInferredInterests"] = json!(true);
    assert!(validate_output(&WorkKind::NegotiatePlaybook, &d, &v).is_err());
    v["dependsOnAssumptions"] = json!(["Cash pressure"]);
    assert!(validate_output(&WorkKind::NegotiatePlaybook, &d, &v).is_ok());
}
#[test]
fn turn_for_wrong_table_or_role_is_invalid() {
    let a: MemberNick = "a".parse().unwrap();
    let b: MemberNick = "b".parse().unwrap();
    let table = NegotiateTable {
        table: 1,
        negotiator: a.clone(),
        counterpart: b.clone(),
        rounds_used: 0,
        outcome: None,
        turns: vec![],
    };
    let mut turn = NegotiateTurn {
        table: 1,
        round: 1,
        actor: a,
        counterpart: false,
        body: json!({"message":"Hello","offer":"100"}),
    };
    assert!(turn.validate_against(&table, 3).is_ok());
    turn.table = 2;
    assert!(turn.validate_against(&table, 3).is_err());
    turn.table = 1;
    turn.actor = b;
    assert!(turn.validate_against(&table, 3).is_err());
    turn.round = 4;
    assert!(turn.validate_against(&table, 3).is_err());
}
#[test]
fn parser_switch_schema_and_minimum_members() {
    assert_eq!(ContestKind::Negotiate.is_built(), ENABLED);
    let ctx = parser::ParseContext {
        roster: ["a", "b", "c", "d"].map(|n| n.parse().unwrap()).to_vec(),
        ..Default::default()
    };
    let p = parse(
        "@all $negotiate Delivery before June -- Supplier needs cash",
        &parser::SubmissionMeta::default(),
        &ctx,
    );
    if ENABLED {
        assert!(matches!(p.unwrap(), ParsedInput::Negotiate { .. }));
    } else {
        assert!(p.is_err());
    }
    let ctx = parser::ParseContext {
        roster: ["a", "b", "c"].map(|n| n.parse().unwrap()).to_vec(),
        ..Default::default()
    };
    assert!(
        parse(
            "@all $negotiate Delivery",
            &parser::SubmissionMeta::default(),
            &ctx
        )
        .is_err()
    );
    for k in [
        WorkKind::NegotiateMap,
        WorkKind::NegotiateBrief,
        WorkKind::NegotiatePackage,
        WorkKind::NegotiateMove,
        WorkKind::NegotiateCounter,
        WorkKind::NegotiatePlaybook,
    ] {
        assert!(output_schema(&k).is_some());
    }
}
