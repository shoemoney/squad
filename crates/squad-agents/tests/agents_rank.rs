use serde_json::json;
use squad_protocol::{rank::*, *};
#[test]
fn strict_rank_outputs_and_prose_accounting() {
    let output =
        json!({"choices":[{"pairId":"P1","winner":"B","reason":"Higher practical impact"}]});
    validate_artifact("rank.compare", &output).unwrap();
    assert_eq!(prose_words(&output), 3);
    let mut leak = output;
    leak["member"] = "a".into();
    assert!(validate_artifact("rank.compare", &leak).is_err());
    assert_eq!(squad_agents::agents::rank::enabled(), ENABLED);
    assert_eq!(composition(&WorkKind::RankCompare), Some("rank.compare"));
    assert_eq!(WorkKind::RankCompare.new_mode_visible_tokens(), Some(4096));
}
