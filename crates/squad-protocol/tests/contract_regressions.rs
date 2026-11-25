mod common;
use common::*;
use serde_json::{Value, json};
use squad_protocol::*;

fn fixture(kind: &str) -> Value {
    fixtures().into_iter().find(|(k, _)| *k == kind).unwrap().1
}

#[test]
fn schema_errors_identify_selected_fields_without_private_content() {
    let mut payload = fixture("delta");
    payload["text"] = json!("PRIVATE_MESSAGE_DO_NOT_ECHO");
    payload["chunkIndex"] = json!(-1);
    let error = ProducerEnvelope::from_value(&envelope("delta", payload))
        .unwrap_err()
        .to_string();
    assert!(error.contains("/payload/chunkIndex"), "{error}");
    assert!(!error.contains("PRIVATE_MESSAGE"));
    let mut payload = completion();
    payload.as_object_mut().unwrap().remove("usage");
    let error = ProducerEnvelope::from_value(&envelope("agent.reply", payload))
        .unwrap_err()
        .to_string();
    assert!(error.contains("/payload/usage"), "{error}");
    let mut payload = fixture("delta");
    payload["messageId"] = json!("PRIVATE_INVALID_UUID");
    let error = ProducerEnvelope::from_value(&envelope("delta", payload))
        .unwrap_err()
        .to_string();
    assert!(!error.contains("PRIVATE_INVALID_UUID"));
}

#[test]
fn generic_carriers_cannot_impersonate_typed_debate_contracts() {
    for kind in ["contest.setup", "contest.state", "contest.preflight"] {
        let mut payload = fixture(kind);
        if kind == "contest.state" {
            payload["state"]["kind"] = json!("debate");
        } else {
            payload["kind"] = json!("debate");
        }
        let mut value = envelope(kind, payload);
        assert!(ProducerEnvelope::from_value(&value).is_err(), "{kind}");
        value.as_object_mut().unwrap().extend(
            json!({"seq":"1","acceptedAt":TIME,"commitIndex":0,"commitSize":1})
                .as_object()
                .unwrap()
                .clone(),
        );
        assert!(CommittedEnvelope::from_value(&value).is_err(), "{kind}");
    }
    let mut payload = fixture("contest.picked");
    payload["kind"] = json!("grill");
    payload["nicks"] = json!(["a"]);
    payload["sourceMessageIds"] = json!([ID]);
    assert!(ProducerEnvelope::from_value(&envelope("contest.picked", payload.clone())).is_ok());
    payload["nicks"] = json!(["a", "b"]);
    assert!(ProducerEnvelope::from_value(&envelope("contest.picked", payload.clone())).is_err());
    payload["nicks"] = json!(["a"]);
    payload["sourceMessageIds"] = json!([]);
    assert!(ProducerEnvelope::from_value(&envelope("contest.picked", payload)).is_err());
}

#[test]
fn anonymity_is_enforced_even_on_trusted_serde_paths() {
    assert!(serde_json::from_value::<HistoryMetadata>(json!({"anonymous":true})).is_ok());
    for value in [
        json!({"anonymous":false}),
        json!({"anonymous":true,"originEventId":ID,"originSeq":"1","author":"a","projectionKey":"x"}),
    ] {
        assert!(serde_json::from_value::<HistoryMetadata>(value).is_err());
    }
}

#[test]
fn required_nullable_is_declared_by_field_not_spelling() {
    #[derive(serde::Deserialize, schemars::JsonSchema)]
    #[allow(dead_code)]
    struct Example {
        #[schemars(extend("x-required-nullable" = true))]
        novel_name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        usage: Option<String>,
    }
    assert!(validation::decode::<Example>(&json!({"novel_name":null})).is_ok());
    assert!(validation::decode::<Example>(&json!({"usage":null})).is_err());
}

#[test]
fn judge_only_estimate_is_explicit() {
    let mut preflight: DebatePreflight = validation::decode(&fixture("debate.preflight")).unwrap();
    preflight.estimate.model_calls = 1;
    assert!(preflight.validate().is_err());
    preflight.judge_only = true;
    preflight.judge_requested = true;
    assert!(preflight.validate().is_ok());
    assert_eq!(preflight.planned_calls(), 1);
    assert_eq!(preflight.reserved_calls(), 3);
    preflight.estimate.model_calls = 2;
    assert!(preflight.validate().is_err());
}

#[test]
fn stateful_debate_validation_rejects_foreign_participants_and_cases() {
    let mut state: DebateState = validation::decode(&fixture("debate.state")).unwrap();
    state.case_id = Some(ID.parse().unwrap());
    state.case_ids = vec![ID.parse().unwrap()];
    let verdict: DebateVerdict = validation::decode(&verdict()).unwrap();
    let score: DebateRoundScore = validation::decode(&fixture("debate.round_score")).unwrap();
    assert!(
        verdict
            .validate_against(&state, &verdict.verdict.round_tally)
            .is_ok()
    );
    assert!(score.validate_against(&state).is_ok());
    state.debaters = [Nick::new("x").unwrap(), Nick::new("y").unwrap()];
    assert!(
        verdict
            .validate_against(&state, &verdict.verdict.round_tally)
            .is_err()
    );
    assert!(score.validate_against(&state).is_err());
    state.debaters = [Nick::new("a").unwrap(), Nick::new("b").unwrap()];
    state.case_id = Some(CaseId::new());
    state.case_ids.push(state.case_id.unwrap());
    assert!(score.validate_against(&state).is_err());
}

#[test]
fn system_can_record_only_forfeits_and_unscored_rounds() {
    let mut turn = envelope("debate.turn", fixture("debate.turn"));
    turn["sender"] = json!("system");
    assert!(ProducerEnvelope::from_value(&turn).is_err());
    turn["payload"]["outcome"] =
        json!({"status":"forfeit","reasonCode":"timeout","text":"turn deadline"});
    assert!(ProducerEnvelope::from_value(&turn).is_ok());
    let mut score = envelope("debate.round_score", fixture("debate.round_score"));
    score["sender"] = json!("system");
    assert!(ProducerEnvelope::from_value(&score).is_err());
    score["payload"]["scores"] = Value::Null;
    score["payload"]["reason"] = json!("unscored");
    assert!(ProducerEnvelope::from_value(&score).is_ok());
}

/// Run with cargo test -p squad-protocol --release cached_delta_validation -- --nocapture.
#[test]
fn cached_delta_validation() {
    let value = envelope("delta", fixture("delta"));
    ProducerEnvelope::from_value(&value).unwrap(); // Compile outside the timed stream.
    let start = std::time::Instant::now();
    for _ in 0..10_000 {
        std::hint::black_box(ProducerEnvelope::from_value(std::hint::black_box(&value)).unwrap());
    }
    let per_delta = start.elapsed() / 10_000;
    eprintln!("cached delta validation: {per_delta:?}/delta");
    if !cfg!(debug_assertions) {
        assert!(
            per_delta < std::time::Duration::from_micros(100),
            "{per_delta:?}"
        );
    }
}

#[test]
fn selection_terminals_preserve_the_shared_selection_carrier() {
    for state in ["FAILED", "STOPPED"] {
        let value = json!({"contestId":ID,"kind":"debate","state":state,"stateVersion":3,"questionMessageId":ID,"addressed":["a","b"],"answers":[],"requiredPicks":2});
        assert!(
            ProducerEnvelope::from_value(&envelope(
                "contest.state",
                json!({"kind":"selection","state":value})
            ))
            .is_ok()
        );
    }
}

#[test]
fn executor_results_do_not_require_router_envelope_metadata() {
    let result = ExecutorRequest::Result {
        work_id: ID.parse().unwrap(),
        attempt: 1,
        fence: 1,
        epoch: 1,
        output: EnvelopePayload::AgentReply(validation::decode(&completion()).unwrap()),
    };
    let encoded = serde_json::to_value(result).unwrap();
    let output = &encoded["Result"]["output"];
    assert_eq!(output["kind"], "agent.reply");
    for field in ["id", "recipients", "correlationId", "createdAt"] {
        assert!(output.get(field).is_none());
    }
}

#[test]
fn errors_redact_dynamic_keys_and_identify_missing_discriminators() {
    #[derive(serde::Deserialize, schemars::JsonSchema)]
    #[allow(dead_code)]
    struct RecordMap {
        records: std::collections::BTreeMap<String, u32>,
    }
    for key in ["PRIVATE_MAP_KEY", "123456"] {
        let error = validation::decode::<RecordMap>(&json!({"records":{key:-1}}))
            .err()
            .unwrap()
            .to_string();
        assert!(error.contains("/records/*"), "{error}");
        assert!(!error.contains(key), "{error}");
    }
    let mut value = envelope("delta", fixture("delta"));
    value.as_object_mut().unwrap().remove("kind");
    let error = ProducerEnvelope::from_value(&value)
        .unwrap_err()
        .to_string();
    assert!(error.contains("/kind"), "{error}");
}
