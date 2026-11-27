use serde_json::json;
use squad_agents::agents::blow_it_up::validate_shape;
use squad_protocol::*;
#[test]
fn adapter_rejects_missing_diversity_and_wrong_work_kinds() {
    let good = json!({"vectors":[{"localId":"a","title":"Access","targetQuote":"No authentication is required.","exploit":"Anyone writes","severity":"high","likelihood":"likely","blastRadius":"All data","diversity":true}]});
    assert!(validate_shape(&WorkKind::BlowItUpAttack, &good).is_ok());
    let mut invalid = good.clone();
    invalid["vectors"][0]["diversity"] = json!(false);
    assert!(validate_shape(&WorkKind::BlowItUpAttack, &invalid).is_err());
    assert!(validate_shape(&WorkKind::Reply, &good).is_err());
    assert!(validate_shape(&WorkKind::BlowItUpChallenge, &good).is_err());
}
#[test]
fn adapter_rejects_invalid_challenge_dependencies_and_invented_judge_fields() {
    assert!(validate_shape(&WorkKind::BlowItUpChallenge,&json!({"verdicts":[{"vectorId":"a","verdict":"false_positive","reason":"No proof","duplicateOf":null,"defeatingQuote":null}],"additions":[]})).is_err());
    assert!(validate_shape(&WorkKind::BlowItUpRule, &json!({"winner":"alpha"})).is_err());
    assert!(
        validate_shape(
            &WorkKind::BlowItUpRule,
            &json!({"findings":[],"killedNotes":[]})
        )
        .is_ok()
    );
}
