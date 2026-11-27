use serde_json::json;
use squad_protocol::{readback::*, *};
#[test]
fn member_output_and_word_budgets_do_not_accept_identity_metadata() {
    let output = json!({"build":"Save records","behaviours":[{"localId":"one","situation":"Record arrives","behaviour":"Retain record","specQuote":"The service retains records."}],"edgeCases":[],"assumptions":[]});
    validate_artifact("readback.interpret", &output).unwrap();
    assert_eq!(prose_words(&output), 10);
    let mut invalid = output;
    invalid["model"] = json!("provider/model");
    assert!(validate_artifact("readback.interpret", &invalid).is_err());
    assert_eq!(squad_agents::agents::readback::enabled(), ENABLED);
    assert_eq!(
        composition(&WorkKind::ReadbackInterpret),
        Some("readback.interpret")
    );
}
