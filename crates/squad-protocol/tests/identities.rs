mod common;
use common::*;
use serde_json::json;
use squad_protocol::*;
#[test]
fn cursor_is_numeric_and_serializes_as_positive_decimal_string() {
    assert!(Cursor::new(2).unwrap() < Cursor::new(10).unwrap());
    for v in [
        json!(0),
        json!("0"),
        json!("01"),
        json!("-1"),
        json!("+1"),
        json!("9223372036854775808"),
        json!(""),
        json!("1.0"),
    ] {
        assert!(serde_json::from_value::<Cursor>(v.clone()).is_err());
        assert!(serde_json::from_value::<HistoryPosition>(v).is_err());
    }
    assert_eq!(
        serde_json::to_value(Cursor::new(i64::MAX).unwrap()).unwrap(),
        json!(i64::MAX.to_string())
    );
}
#[test]
fn ids_keep_identity_without_conflating_types_or_attempts() {
    let event: EventId = ID.parse().unwrap();
    let message: MessageId = ID.parse().unwrap();
    assert_eq!(event.as_uuid(), message.as_uuid());
    assert!(ID.parse::<CellId>().is_err());
    assert!(uuid::Uuid::new_v4().to_string().parse::<WorkId>().is_err());
    let id = WorkId::new();
    assert_eq!(
        serde_json::from_value::<WorkId>(serde_json::to_value(id).unwrap()).unwrap(),
        id
    );
    assert!(serde_json::from_value::<MemberNick>(json!("judge")).is_err());
    assert!(serde_json::from_value::<Target>(json!("me")).is_err());
    let mut g = generation();
    let first: GenerationIdentity = validation::decode(&g).unwrap();
    g["attempt"] = json!(2);
    let second: GenerationIdentity = validation::decode(&g).unwrap();
    assert_eq!(first.message_id, second.message_id);
    assert_eq!(first.work_id, second.work_id);
}
#[test]
fn full_error_code_wire_registry_is_exact() {
    let schema = validation::schema::<ErrorCode>();
    let values = schema["enum"].as_array().unwrap();
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/error_codes.json")).unwrap();
    assert_eq!(serde_json::Value::Array(values.clone()), expected);
    for value in values {
        let code: ErrorCode = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(code).unwrap(), *value);
    }
    assert!(serde_json::from_value::<ErrorCode>(json!("UNKNOWN")).is_err());
}
