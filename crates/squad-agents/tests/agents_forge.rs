use serde_json::json;
use squad_protocol::{forge::validate_output, *};
#[test]
fn executor_validator_rejects_missing_votes_and_long_repair_output() {
    let data = json!({"labels":["D1","D2"],"maxWords":2,"wordLimit":1200});
    assert!(
        validate_output(
            &WorkKind::ForgeRank,
            &data,
            &json!({"ranking":[],"notes":[]})
        )
        .is_err()
    );
    for _attempt in 1..=2 {
        assert!(
            validate_output(
                &WorkKind::ForgeFuse,
                &data,
                &json!({"body":"three long words","provenance":[],"handledFlaws":[]})
            )
            .is_err()
        );
    }
    assert!(
        validate_output(
            &WorkKind::ForgeFuse,
            &data,
            &json!({"body":"two words","provenance":[],"handledFlaws":[]})
        )
        .is_ok()
    );
}
