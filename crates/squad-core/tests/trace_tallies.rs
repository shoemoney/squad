use serde_json::json;
use squad_core::orchestrator::trace::*;
use squad_protocol::{trace::*, *};
use std::collections::{BTreeMap, BTreeSet};
fn verdict(e: TraceEntailment, s: TraceScope) -> TraceVerdict {
    TraceVerdict {
        claim_id: "C1".into(),
        entailment: e,
        scope: s,
        qualifier_quote: None,
        note: String::new(),
    }
}
#[test]
fn statuses_include_missing_verifier_and_counter_evidence() {
    use TraceClaimStatus::*;
    let good = verdict(TraceEntailment::Supported, TraceScope::Ok);
    let bad = verdict(TraceEntailment::NotSupported, TraceScope::Ok);
    assert_eq!(claim_status(0, std::slice::from_ref(&good)), Unsupported);
    assert_eq!(claim_status(1, &[]), Unverified);
    assert_eq!(claim_status(1, std::slice::from_ref(&good)), Supported); // missing second reviewer is not a vote
    assert_eq!(claim_status(1, std::slice::from_ref(&bad)), Unsupported);
    assert_eq!(claim_status(1, &[good, bad]), Disputed);
    assert_eq!(
        claim_status(1, &[verdict(TraceEntailment::Partial, TraceScope::Ok)]),
        Disputed
    );
    assert_eq!(
        claim_status(
            1,
            &[verdict(TraceEntailment::Supported, TraceScope::Overstated)]
        ),
        Disputed
    );
}
#[test]
fn shared_origins_collapse_distinct_sources_not_model_counts() {
    let ids = vec!["S1".into(), "S1".into(), "S2".into(), "S3".into()];
    assert_eq!(independent_sources(&ids, &[]), 3);
    assert_eq!(
        independent_sources(&ids, &[vec!["S1".into(), "S2".into()]]),
        2
    );
    assert_eq!(
        independent_sources(
            &ids,
            &[
                vec!["S1".into(), "S2".into()],
                vec!["S2".into(), "S3".into()]
            ]
        ),
        1
    );
    assert_eq!(
        independent_sources(&[], &[vec!["S1".into(), "S2".into()]]),
        0
    );
}
#[test]
fn seeded_assignment_is_balanced_and_excludes_contributors() {
    let claims = TraceClaims {
        claims: (1..=12)
            .map(|i| TraceCanonicalClaim {
                claim_id: format!("C{i}"),
                text: "Claim".into(),
                sources: vec![TraceClaimRef {
                    member_label: "Reader 1".into(),
                    local_id: "L1".into(),
                }],
                shared_origin: vec![],
            })
            .collect(),
    };
    let members = (0..6)
        .map(|i| format!("member{i}").parse::<MemberNick>().unwrap())
        .collect::<Vec<_>>();
    let map = BTreeMap::from([("Reader 1".into(), members[0].clone())]);
    let a = assign_verifiers(&claims, &map, &members, "seed");
    assert_eq!(a, assign_verifiers(&claims, &map, &members, "seed"));
    assert!(a[&members[0]].is_empty());
    let loads = members[1..].iter().map(|n| a[n].len()).collect::<Vec<_>>();
    assert!(loads.iter().max().unwrap() - loads.iter().min().unwrap() <= 1);
    for c in &claims.claims {
        assert_eq!(
            a.values().filter(|ids| ids.contains(&c.claim_id)).count(),
            2
        );
    }
    let all = claims
        .claims
        .iter()
        .map(|c| &c.claim_id)
        .collect::<BTreeSet<_>>();
    assert_eq!(a.values().flatten().collect::<BTreeSet<_>>(), all);
    let provenance = members[..5]
        .iter()
        .enumerate()
        .map(|(i, n)| (format!("Reader {}", i + 1), n.clone()))
        .collect::<BTreeMap<_, _>>();
    let consensus = TraceClaims {
        claims: vec![TraceCanonicalClaim {
            claim_id: "C1".into(),
            text: "Consensus claim".into(),
            sources: provenance
                .keys()
                .map(|label| TraceClaimRef {
                    member_label: label.clone(),
                    local_id: "L1".into(),
                })
                .collect(),
            shared_origin: vec![],
        }],
    };
    let fallback = assign_verifiers(&consensus, &provenance, &members, "seed");
    assert_eq!(fallback.values().map(Vec::len).sum::<usize>(), 2);
    assert_eq!(fallback[&members[5]], vec!["C1"]);
    assert_eq!(call_bounds(6), (14, 30));
    assert_eq!(call_bounds(12), (26, 54));
}
#[test]
fn matcher_folds_punctuation_whitespace_but_never_case_or_stitched_text() {
    use squad_core::orchestrator::verbatim::verbatim;
    let source = "Before: “Keep—these records”\n for thirty days. After.";
    let q = "\"Keep-these records\" for thirty days.";
    let span = verbatim(q, source).unwrap();
    assert_eq!(&source[span], "“Keep—these records”\n for thirty days.");
    assert!(verbatim("\"keep-these records\" for thirty days.", source).is_none());
    assert!(verbatim("Keep...thirty days", source).is_none());
    assert!(verbatim("Keep…thirty days", source).is_none());
    assert!(verbatim("records", source).is_none());
    assert!(verbatim("Full width ＡＢＣＤ", "Full width ABCD").is_some());
}
#[test]
fn validation_checks_unknown_sources_qualifier_spans_and_body_status() {
    let extract = json!({"claims":[{"localId":"L1","claim":"Claim","quotes":[{"sourceId":"S2","quote":"A fabricated quotation"}],"counterQuotes":[],"scopeNote":""}]});
    let data = json!({"sources":[{"sourceId":"S1","text":"A real packet quotation."}]});
    assert!(validate_output(&WorkKind::TraceExtract, &data, &extract).is_err());
    let mut e = extract.clone();
    e["claims"][0]["quotes"][0]["sourceId"] = json!("S1");
    assert!(validate_output(&WorkKind::TraceExtract, &data, &e).is_ok());
    let checks = check_quotes(
        "Reader 1",
        &serde_json::from_value(e).unwrap(),
        &serde_json::from_value::<Vec<trace::TraceSource>>(data["sources"].clone()).unwrap(),
    );
    assert_eq!(checks[0].status, "quote_not_found");
    assert!(checks[0].span.is_none());
    let ledger = json!({"claims":[{"claimId":"C1","text":"Claim","status":"unsupported","quotes":[],"independentSources":0,"contributingMembers":1,"verdicts":{}}]});
    let brief = json!({"sections":[{"heading":"Summary","paragraphs":[{"text":"Claim","claimIds":["C1"]}]}],"disputed":[],"gaps":[]});
    assert!(validate_output(&WorkKind::TraceAssemble, &json!({"ledger":ledger}), &brief).is_err());
    let disputed = json!({"sections":[],"disputed":[{"claimId":"C1","text":"Claim"}],"gaps":[]});
    assert!(
        validate_output(
            &WorkKind::TraceAssemble,
            &json!({"ledger":ledger}),
            &disputed
        )
        .is_err()
    );
    let v = json!({"verdicts":[{"claimId":"C1","entailment":"partial","scope":"missing_qualifier","qualifierQuote":{"sourceId":"S1","quote":"A fabricated qualifier"},"note":"Qualifier missing"}]});
    let input =
        json!({"claims":[{"claimId":"C1","text":"Claim","quotes":[]}],"sources":data["sources"]});
    assert!(validate_output(&WorkKind::TraceVerify, &input, &v).is_err());
}
#[test]
fn oversized_packet_is_rejected_by_structure_derived_pricing() {
    let store = squad_store::Store::open(":memory:").unwrap();
    let n: MemberNick = "member".parse().unwrap();
    let bindings = BTreeMap::from([
        (
            n.to_string(),
            ExecutorBinding {
                model: "fixture/member".parse().unwrap(),
                effort: ReasoningEffort::Low,
            },
        ),
        (
            "judge".into(),
            ExecutorBinding {
                model: "fixture/judge".parse().unwrap(),
                effort: ReasoningEffort::Low,
            },
        ),
    ]);
    let steps = squad_core::router::trace::steps(
        &[n],
        "Question",
        &[trace::TraceSource {
            source_id: "S1".into(),
            text: "packet ".repeat(100000),
        }],
        &[100; 4],
        500,
    );
    let err=store.call(move|db|{for b in bindings.values(){db.execute("INSERT INTO catalog VALUES (?,32768,8192,'{\"prompt\":\"0.000001\",\"completion\":\"0.000002\"}','[]','2026-11-26T00:00:00Z')",[b.model.as_str()])?;}squad_core::router::bounds::price(db,&bindings,&steps)}).unwrap_err();
    assert!(matches!(
        err,
        squad_store::StoreError::Domain(ErrorCode::CONTEXT_TOO_LARGE, _)
    ));
}

#[test]
fn blind_projections_remove_identity_prose_without_changing_evidence() {
    let input = json!({"sources":[{"sourceId":"S1","text":"Fable wrote this evidence."}],"extractions":{"Reader 1":{"claims":[{"localId":"Fable1","claim":"FABLE says retention is conditional","scopeNote":"From fable","quotes":[{"sourceId":"S1","quote":"Fable wrote this evidence."}],"counterQuotes":[]}]}}});
    let out = blind_input(input.clone(), &["fable".into()]);
    assert_eq!(out["sources"], input["sources"]);
    assert_eq!(
        out["extractions"]["Reader 1"]["claims"][0]["quotes"],
        input["extractions"]["Reader 1"]["claims"][0]["quotes"]
    );
    assert!(
        out["extractions"]["Reader 1"]["claims"][0]["claim"]
            .as_str()
            .unwrap()
            .contains("[redacted]")
    );
    assert_eq!(
        out["extractions"]["Reader 1"]["claims"][0]["localId"],
        "Fable1"
    );
    assert!(!blind_system("You are @fable with private identity").contains("@fable"));
}

#[test]
fn ledger_counts_support_and_counter_sources_independently() {
    let sources = vec![
        trace::TraceSource {
            source_id: "S1".into(),
            text: "Records are retained for thirty days.".into(),
        },
        trace::TraceSource {
            source_id: "S2".into(),
            text: "Retention applies only to production.".into(),
        },
    ];
    let extraction = TraceExtraction {
        claims: vec![TraceExtractedClaim {
            local_id: "L1".into(),
            claim: "Retention".into(),
            quotes: vec![TraceQuote {
                source_id: "S1".into(),
                quote: sources[0].text.clone(),
            }],
            counter_quotes: vec![TraceQuote {
                source_id: "S2".into(),
                quote: sources[1].text.clone(),
            }],
            scope_note: String::new(),
        }],
    };
    let checks = check_quotes("Reader 1", &extraction, &sources);
    let mut claims = TraceClaims {
        claims: vec![TraceCanonicalClaim {
            claim_id: "C1".into(),
            text: "Retention".into(),
            sources: vec![TraceClaimRef {
                member_label: "Reader 1".into(),
                local_id: "L1".into(),
            }],
            shared_origin: vec![],
        }],
    };
    let result = ledger(&claims, &checks, &BTreeMap::new());
    assert_eq!(result.claims[0].independent_sources, 2);
    assert_eq!(result.claims[0].status, TraceClaimStatus::Unverified);
    assert!(result.claims[0].quotes.iter().any(|q| q.counter));
    claims.claims[0].shared_origin = vec![vec!["S1".into(), "S2".into()]];
    assert_eq!(
        ledger(&claims, &checks, &BTreeMap::new()).claims[0].independent_sources,
        1
    );
}
