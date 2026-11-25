use serde_json::{Value, json};
use squad_core::prompts::inputs::*;
use squad_protocol::*;
const ID: &str = "01900000-0000-7000-8000-000000000001";

fn kind_work(kind: &str, target: &str) -> Work {
    let judge = target == "judge";
    let context = if judge {
        format!("judge:{ID}")
    } else {
        format!("contest:{ID}:{ID}:{target}")
    };
    let work: Work = serde_json::from_value(json!({"workId":ID,"messageId":ID,"kind":kind,"target":target,"contextId":context,"triggerEventId":ID,"historyThrough":"10","attempt":1,"contestId":ID,"stepId":ID,"caseId":if judge {json!(ID)} else {Value::Null},"maxOutputTokens":if judge {4096} else {2048},"promptRef":{"templateId":kind,"packVersion":"1.0.0","templateHash":"frozen"}})).unwrap();
    work.validate().unwrap_or_else(|e| panic!("{kind}: {e:?}"));
    work
}

#[test]
fn census_survey_and_probe_moves_select_their_registered_compositions() {
    use ContestMove::*;
    for (kind, target, mv, expected) in [
        ("census.hunt", "alpha", Fanout, "census.hunt"),
        ("census.rate", "alpha", Fanout, "census.rate"),
        ("census.merge", "judge", Judge, "census.merge"),
        ("census.final", "judge", Judge, "census.final"),
        ("survey.vote", "alpha", Fanout, "survey.vote"),
        ("survey.revote", "alpha", Fanout, "survey.revote"),
        ("survey.extract", "judge", Judge, "survey.extract"),
        ("survey.report", "judge", Judge, "survey.report"),
        ("probe.explain", "alpha", Fanout, "probe.explain"),
        ("probe.check", "alpha", Fanout, "probe.check"),
        ("probe.predict", "alpha", Fanout, "probe.predict"),
        ("probe.report", "judge", Judge, "probe.report"),
    ] {
        let work = kind_work(kind, target);
        assert_eq!(
            shared_move_composition(&mv, &work, 1).unwrap(),
            Some(expected),
            "{kind}"
        );
    }
}

#[test]
fn model_free_moves_and_mismatched_actor_moves_have_no_composition() {
    use ContestMove::*;
    let hunt = kind_work("census.hunt", "alpha");
    for mv in [Pick, Tally, Barrier, Report] {
        assert_eq!(shared_move_composition(&mv, &hunt, 1).unwrap(), None);
    }
    // A judge move can never select a member composition, nor a fanout a judge one.
    assert!(shared_move_composition(&Judge, &hunt, 1).is_err());
    assert!(shared_move_composition(&Fanout, &kind_work("census.merge", "judge"), 1).is_err());
    assert!(shared_move_composition(&Fanout, &kind_work("survey.report", "judge"), 1).is_err());
    assert!(shared_move_composition(&Fanout, &kind_work("probe.report", "judge"), 1).is_err());
    assert!(shared_move_composition(&Collect, &kind_work("probe.check", "alpha"), 1).is_err());
}
