use serde_json::json;
use squad_protocol::{bass_ackwards_prd::*, *};
#[test]
fn member_contracts_require_one_lens_and_cap_additions() {
    let review = json!({"build":null,"comments":[]});
    validate_output(
        &WorkKind::BassAckwardsPrdReview,
        &json!({"lens":"GAPS"}),
        &review,
    )
    .unwrap();
    assert!(
        validate_output(
            &WorkKind::BassAckwardsPrdReview,
            &json!({"lens":"READBACK"}),
            &review
        )
        .is_err()
    );
    let mut addition = json!({"additions":[]});
    for i in 0..4 {
        addition["additions"].as_array_mut().unwrap().push(json!({"localId":format!("a{i}"),"text":"guide","under":"m","dependsOn":[],"serves":["D1"],"ownerRole":"Operator","risk":"Missing guide","why":"Operations"}));
    }
    assert!(validate_artifact("bass_ackwards_prd.must_have", &addition).is_err());
    let mutation = json!({"additions":[],"remove":["key"]});
    assert!(validate_artifact("bass_ackwards_prd.must_have", &mutation).is_err());
}
#[test]
fn prose_budget_excludes_identifiers() {
    assert_eq!(
        squad_agents::agents::bass_ackwards_prd::prose_words(
            &json!({"text":"two words","key":"a b c","serves":["D1"]})
        ),
        2
    );
}
