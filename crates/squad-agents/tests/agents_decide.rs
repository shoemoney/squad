use serde_json::json;
use squad_protocol::{decide::*, *};
#[test]
fn explanation_cannot_name_winner_or_unknown_sensitivity() {
    let m = DecideMatrix {
        cells: vec![],
        ranking: vec![],
        margin: 0.0,
        winner: None,
        leave_one_out: vec![],
        flips: vec![],
        robust: false,
        member_support: 0,
    };
    let data = json!({"matrix":m});
    validate_output(
        &WorkKind::DecideExplain,
        &data,
        &json!({"flipConditions":[],"dissents":[],"evidenceGaps":[]}),
    )
    .unwrap();
    for output in [
        json!({"winner":"a","flipConditions":[],"dissents":[],"evidenceGaps":[]}),
        json!({"flipConditions":[{"criterionId":"c","rivalOptionId":"b","text":"An invented condition"}],"dissents":[],"evidenceGaps":[]}),
    ] {
        assert!(validate_output(&WorkKind::DecideExplain, &data, &output).is_err());
    }
}
