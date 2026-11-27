use serde_json::json;
use squad_protocol::{negotiate::validate_output, *};
#[test]
fn validators_do_not_turn_missing_or_invalid_output_into_concessions() {
    for kind in [
        WorkKind::NegotiateMap,
        WorkKind::NegotiateBrief,
        WorkKind::NegotiatePackage,
        WorkKind::NegotiateMove,
        WorkKind::NegotiateCounter,
        WorkKind::NegotiatePlaybook,
    ] {
        for attempt in 1..=constants::attempt_limit(&kind) {
            assert!(
                validate_output(&kind, &json!({"wordLimit":200}), &json!({})).is_err(),
                "{kind:?} {attempt}"
            );
        }
    }
    assert!(validate_output(&WorkKind::NegotiateCounter,&json!({"wordLimit":200}),&json!({"message":"Accept","response":"accept","counterOffer":null,"violatedInterest":null})).is_ok());
}
