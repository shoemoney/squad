mod common;
use common::*;
use serde_json::json;
use squad_protocol::*;
#[test]
fn every_m1_registry_fixture_round_trips_and_rejects_unknown_keys() {
    for (kind, payload) in fixtures() {
        let value = envelope(kind, payload);
        let parsed = ProducerEnvelope::from_value(&value).unwrap_or_else(|e| panic!("{kind}: {e}"));
        assert_eq!(serde_json::to_value(&parsed).unwrap(), value, "{kind}");
        let mut bad = value.clone();
        bad["unexpected"] = json!(true);
        assert!(
            ProducerEnvelope::from_value(&bad).is_err(),
            "{kind} unknown envelope field"
        );
        let mut bad = value.clone();
        bad["payload"]["unexpected"] = json!(true);
        assert!(
            ProducerEnvelope::from_value(&bad).is_err(),
            "{kind} unknown payload field"
        );
        let mut committed = value.clone();
        committed.as_object_mut().unwrap().extend(
            json!({"seq":"1","acceptedAt":TIME,"commitIndex":0,"commitSize":1})
                .as_object()
                .unwrap()
                .clone(),
        );
        assert_eq!(
            CommittedEnvelope::from_value(&committed).is_ok(),
            parsed.content.is_durable(),
            "{kind} durability"
        );
        assert!(
            ProducerEnvelope::from_value(&committed).is_err(),
            "producer cannot provide commit fields"
        );
    }
}
#[test]
fn nullable_is_required_and_omitted_optionals_stay_absent() {
    for (kind, field) in [
        ("message.final", "usage"),
        ("usage.record", "responseModel"),
        ("debate.state", "caseId"),
        ("debate.state", "terminalReason"),
        ("verdict", "tallyOverrideReason"),
        ("debate.round_score", "scores"),
    ] {
        let payload = fixtures().into_iter().find(|(k, _)| *k == kind).unwrap().1;
        let mut v = envelope(kind, payload);
        v["payload"].as_object_mut().unwrap().remove(field);
        assert!(
            ProducerEnvelope::from_value(&v).is_err(),
            "{kind} missing {field}"
        );
    }
    let mut c = completion();
    c["usage"] = json!({"inputTokens":1,"outputTokens":2,"costUsd":null});
    assert!(ProducerEnvelope::from_value(&envelope("message.final", c.clone())).is_ok());
    c["usage"].as_object_mut().unwrap().remove("costUsd");
    assert!(ProducerEnvelope::from_value(&envelope("message.final", c)).is_err());
}
#[test]
fn actor_quotes_presence_and_finish_rules() {
    let mut v = envelope("message.final", completion());
    v["sender"] = json!("me");
    assert!(ProducerEnvelope::from_value(&v).is_err());
    v["sender"] = json!("a");
    v["payload"]["content"] = json!("");
    assert!(ProducerEnvelope::from_value(&v).is_err());
    v["payload"]["finishReason"] = json!("content_filter");
    assert!(ProducerEnvelope::from_value(&v).is_ok());
    let mut v = envelope(
        "presence",
        json!({"nick":"a","status":"thinking","queueDepth":1,"observedAt":TIME}),
    );
    assert!(ProducerEnvelope::from_value(&v).is_err());
    v["payload"]["workId"] = json!(ID);
    assert!(ProducerEnvelope::from_value(&v).is_ok());
    let mut v = envelope(
        "user.message",
        json!({"messageId":ID,"text":"hi","body":"hi","resolvedQuotes":[],"quote":[ID,ID]}),
    );
    assert!(ProducerEnvelope::from_value(&v).is_err());
    v["payload"].as_object_mut().unwrap().remove("quote");
    v["payload"]["replyTo"] = json!([ID]);
    assert!(ProducerEnvelope::from_value(&v).is_err());
}
#[test]
fn scored_ties_and_unscored_rounds() {
    let (_, payload) = fixtures()
        .into_iter()
        .find(|(k, _)| *k == "debate.round_score")
        .unwrap();
    let mut v = envelope("debate.round_score", payload);
    assert!(ProducerEnvelope::from_value(&v).is_ok());
    v["payload"]["scores"] = json!(null);
    assert!(ProducerEnvelope::from_value(&v).is_err());
    v["payload"]["reason"] = json!("unscored");
    assert!(ProducerEnvelope::from_value(&v).is_ok());
    v["recipients"] = json!(["a"]);
    assert!(ProducerEnvelope::from_value(&v).is_err());
}
