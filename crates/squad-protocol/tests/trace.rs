use serde_json::json;
use squad_protocol::{trace::*, *};
#[test]
fn typed_outputs_reject_missing_references_qualifiers_and_paragraph_citations() {
    for (kind, bad) in [
        (
            "trace.merge",
            json!({"claims":[{"claimId":"C1","text":"Claim","sources":[],"sharedOrigin":[]}]}),
        ),
        (
            "trace.verify",
            json!({"verdicts":[{"claimId":"C1","entailment":"partial","scope":"missing_qualifier","qualifierQuote":null,"note":"Missing condition"}]}),
        ),
        (
            "trace.assemble",
            json!({"sections":[{"heading":"Summary","paragraphs":[{"text":"Uncited claim","claimIds":[]}]}],"disputed":[],"gaps":[]}),
        ),
        (
            "trace.extract",
            json!({"claims":[{"localId":"L1","claim":"Claim","quotes":[],"counterQuotes":[],"scopeNote":""}]}),
        ),
    ] {
        assert!(validate_artifact(kind, &bad).is_err(), "{kind}");
    }
    let valid = json!({"claims":[{"localId":"L1","claim":"Claim","quotes":[{"sourceId":"S1","quote":"The packet supports this sentence."}],"counterQuotes":[],"scopeNote":""}]});
    assert!(validate_artifact("trace.extract", &valid).is_ok());
}
#[test]
fn schemas_and_state_contract_are_complete() {
    for k in [
        WorkKind::TraceExtract,
        WorkKind::TraceMerge,
        WorkKind::TraceVerify,
        WorkKind::TraceAssemble,
    ] {
        let s = output_schema(&k).unwrap();
        assert!(s.is_object());
        assert!(composition(&k).unwrap().starts_with("trace."));
    }
    assert!(output_schema(&WorkKind::Reply).is_none());
    let mut s = TraceState {
        contest_id: ContestId::new(),
        state_version: 1,
        state: TraceStatus::Complete,
        step_id: None,
        round: 0,
        artifact_event_ids: vec![],
        terminal_reason: None,
    };
    assert!(s.validate().is_err());
    s.terminal_reason = Some("complete".parse().unwrap());
    assert!(s.validate().is_ok());
    s.round = 1;
    assert!(s.validate().is_err());
}

#[test]
fn source_aware_extraction_validation_rejects_unknown_source_but_not_missing_quote() {
    let e:TraceExtraction=serde_json::from_value(json!({"claims":[{"localId":"L1","claim":"Claim","quotes":[{"sourceId":"S2","quote":"An imaginary passage is not in the source"}],"counterQuotes":[],"scopeNote":""}]})).unwrap();
    let source = TraceSource {
        source_id: "S1".into(),
        text: "A real source passage.".into(),
    };
    assert!(e.validate_sources(std::slice::from_ref(&source)).is_err());
    let mut known = e;
    known.claims[0].quotes[0].source_id = "S1".into();
    assert!(
        known
            .validate_sources(std::slice::from_ref(&source))
            .is_ok()
    );
    known.claims[0].quotes[0].quote.clear();
    assert!(known.validate_sources(&[source]).is_ok());
}
#[test]
fn parser_example_and_help_follow_the_activation_switch() {
    let ctx = parser::ParseContext {
        roster: vec![
            "one".parse().unwrap(),
            "two".parse().unwrap(),
            "three".parse().unwrap(),
        ],
        ..Default::default()
    };
    let parsed = parse(
        "@all $trace What is retained?\n--\nRecords are retained for thirty days.",
        &parser::SubmissionMeta::default(),
        &ctx,
    );
    assert_eq!(parsed.is_ok(), ENABLED);
    assert_eq!(
        parser::help_examples().iter().any(|e| e.contains("$trace")),
        ENABLED
    );
}
