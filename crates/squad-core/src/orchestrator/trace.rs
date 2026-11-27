//! Trace script and deterministic quote, assignment and evidence policy.
use crate::router::{
    Router,
    trace::{drive_move, finish, invalid, load_state},
};
use serde_json::{Value, json};
use squad_protocol::trace::*;
use squad_protocol::*;
use squad_store::{Result, commit::CommitOutcome, trace::Run};
use std::collections::{BTreeMap, BTreeSet};
pub fn prepare(parsed: &ParsedInput) -> std::result::Result<(), crate::prompts::PromptError> {
    if matches!(parsed, ParsedInput::Trace { .. }) {
        Ok(())
    } else {
        Err(crate::prompts::PromptError::new(
            "trace",
            None,
            "expected trace",
        ))
    }
}
pub fn call_bounds(m: u32) -> (u32, u32) {
    (2 * m + 2, 4 * m + 6)
}
pub fn check_quotes(
    label: &str,
    e: &TraceExtraction,
    sources: &[trace::TraceSource],
) -> Vec<TraceQuoteCheck> {
    e.claims
        .iter()
        .flat_map(|c| {
            c.quotes
                .iter()
                .map(|q| (false, q))
                .chain(c.counter_quotes.iter().map(|q| (true, q)))
                .map(move |(counter, q)| {
                    let span = sources
                        .iter()
                        .find(|s| s.source_id == q.source_id)
                        .and_then(|s| super::verbatim::verbatim(&q.quote, &s.text));
                    TraceQuoteCheck {
                        member_label: label.into(),
                        local_id: c.local_id.clone(),
                        counter,
                        quote: q.clone(),
                        status: if span.is_some() {
                            "verified"
                        } else {
                            "quote_not_found"
                        }
                        .into(),
                        span: span.map(|s| TraceSpan {
                            start: s.start,
                            end: s.end,
                        }),
                    }
                })
        })
        .collect()
}
/// Two distinct reviewers. Contributors fill a slot only after distinct noncontributors are exhausted.
/// Seeded digest order makes ties stable across restarts and independent of roster order.
pub fn assign_verifiers(
    claims: &TraceClaims,
    provenance: &BTreeMap<String, MemberNick>,
    members: &[MemberNick],
    seed: &str,
) -> BTreeMap<MemberNick, Vec<String>> {
    let mut out = members
        .iter()
        .cloned()
        .map(|n| (n, vec![]))
        .collect::<BTreeMap<_, _>>();
    for c in &claims.claims {
        let contributors = c
            .sources
            .iter()
            .filter_map(|r| provenance.get(&r.member_label))
            .collect::<BTreeSet<_>>();
        let mut eligible = members.iter().collect::<Vec<_>>();
        eligible.sort_by_key(|n| {
            (
                contributors.contains(n),
                out[*n].len(),
                squad_store::commit::digest(&(seed, &c.claim_id, *n)).expect("digest"),
            )
        });
        for n in eligible.into_iter().take(2) {
            out.get_mut(n).expect("member").push(c.claim_id.clone());
        }
    }
    out
}
pub fn claim_status(quotes: usize, verdicts: &[TraceVerdict]) -> TraceClaimStatus {
    if quotes == 0
        || (!verdicts.is_empty()
            && verdicts
                .iter()
                .all(|v| v.entailment == TraceEntailment::NotSupported))
    {
        TraceClaimStatus::Unsupported
    } else if verdicts.is_empty() {
        TraceClaimStatus::Unverified
    } else if verdicts
        .iter()
        .all(|v| v.entailment == TraceEntailment::Supported && v.scope == TraceScope::Ok)
    {
        TraceClaimStatus::Supported
    } else {
        TraceClaimStatus::Disputed
    }
}
/// Connected components of declared shared origins, including overlapping groups.
pub fn independent_sources(ids: &[String], groups: &[Vec<String>]) -> u32 {
    let mut components = ids
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|s| BTreeSet::from([s]))
        .collect::<Vec<_>>();
    for g in groups {
        let mut merged = g.iter().cloned().collect::<BTreeSet<_>>();
        let mut changed = true;
        while changed {
            changed = false;
            let mut i = 0;
            while i < components.len() {
                if !components[i].is_disjoint(&merged) {
                    merged.extend(components.remove(i));
                    changed = true;
                } else {
                    i += 1;
                }
            }
        }
        components.push(merged);
    }
    let actual = ids.iter().collect::<BTreeSet<_>>();
    components
        .iter()
        .filter(|g| g.iter().any(|s| actual.contains(s)))
        .count() as u32
}
pub fn ledger(
    claims: &TraceClaims,
    checks: &[TraceQuoteCheck],
    verifications: &BTreeMap<MemberNick, TraceVerification>,
) -> TraceLedger {
    TraceLedger {
        claims: claims
            .claims
            .iter()
            .map(|c| {
                let mut quotes = vec![];
                for q in checks.iter().filter(|q| {
                    q.span.is_some()
                        && c.sources
                            .iter()
                            .any(|r| r.member_label == q.member_label && r.local_id == q.local_id)
                }) {
                    if !quotes.iter().any(|p: &TraceQuoteCheck| {
                        p.counter == q.counter && p.quote == q.quote && p.span == q.span
                    }) {
                        quotes.push(q.clone());
                    }
                }
                let verdicts = verifications
                    .iter()
                    .filter_map(|(n, v)| {
                        v.verdicts
                            .iter()
                            .find(|v| v.claim_id == c.claim_id)
                            .map(|v| (n.clone(), v.clone()))
                    })
                    .collect::<BTreeMap<_, _>>();
                // Only verified SUPPORTING quotes count toward status and independence;
                // counter-quotes stay in `quotes` (flagged) for the contradictions section.
                let supporting = quotes.iter().filter(|q| !q.counter).collect::<Vec<_>>();
                let ids = supporting
                    .iter()
                    .map(|q| q.quote.source_id.clone())
                    .collect::<Vec<_>>();
                TraceLedgerEntry {
                    claim_id: c.claim_id.clone(),
                    text: c.text.clone(),
                    status: claim_status(
                        supporting.len(),
                        &verdicts.values().cloned().collect::<Vec<_>>(),
                    ),
                    independent_sources: independent_sources(&ids, &c.shared_origin),
                    contributing_members: c
                        .sources
                        .iter()
                        .map(|s| &s.member_label)
                        .collect::<BTreeSet<_>>()
                        .len() as u32,
                    quotes,
                    verdicts,
                }
            })
            .collect(),
    }
}
pub fn validate_output(
    kind: &WorkKind,
    data: &Value,
    output: &Value,
) -> validation::ValidationResult {
    use validation::{decode, ensure};
    let err = || validation::ValidationError {
        message: "invalid frozen trace input".into(),
    };
    trace::validate_artifact(trace::composition(kind).ok_or_else(err)?, output)?;
    match kind {
        WorkKind::TraceExtract => {
            let e = decode::<TraceExtraction>(output)?;
            let sources = decode::<Vec<trace::TraceSource>>(&data["sources"])?;
            e.validate_sources(&sources)?;
            // Existence is deliberately NOT an output validation/repair criterion (D85).
            Ok(())
        }
        WorkKind::TraceMerge => {
            let c = decode::<TraceClaims>(output)?;
            let extracted = &data["extractions"];
            let sources = decode::<Vec<trace::TraceSource>>(&data["sources"])?;
            for c in c.claims {
                for r in c.sources {
                    let e = decode::<TraceExtraction>(&extracted[&r.member_label])?;
                    ensure(
                        e.claims.iter().any(|c| c.local_id == r.local_id),
                        "unknown member claim",
                    )?;
                }
                for g in c.shared_origin {
                    ensure(
                        g.iter()
                            .all(|id| sources.iter().any(|s| s.source_id == *id)),
                        "unknown shared-origin source",
                    )?;
                }
            }
            Ok(())
        }
        WorkKind::TraceVerify => {
            let v = decode::<TraceVerification>(output)?;
            let claims = decode::<Vec<TraceAssignedClaim>>(&data["claims"])?;
            ensure(
                v.verdicts
                    .iter()
                    .map(|v| &v.claim_id)
                    .collect::<BTreeSet<_>>()
                    == claims.iter().map(|c| &c.claim_id).collect::<BTreeSet<_>>(),
                "verdicts must cover exactly assigned claims",
            )?;
            let sources = decode::<Vec<trace::TraceSource>>(&data["sources"])?;
            for v in v.verdicts {
                if let Some(q) = v.qualifier_quote {
                    ensure(
                        sources.iter().any(|s| {
                            s.source_id == q.source_id
                                && super::verbatim::verbatim(&q.quote, &s.text).is_some()
                        }),
                        "qualifier quote is not verbatim",
                    )?;
                }
            }
            Ok(())
        }
        WorkKind::TraceAssemble => {
            let brief = decode::<TraceBrief>(output)?;
            let ledger = decode::<TraceLedger>(&data["ledger"])?;
            for p in brief.sections.iter().flat_map(|s| &s.paragraphs) {
                ensure(
                    p.claim_ids.iter().all(|id| {
                        ledger
                            .claims
                            .iter()
                            .any(|c| c.claim_id == *id && c.status == TraceClaimStatus::Supported)
                    }),
                    "body may cite supported claims only",
                )?;
            }
            for d in &brief.disputed {
                ensure(
                    ledger.claims.iter().any(|c| {
                        c.claim_id == d.claim_id && c.status == TraceClaimStatus::Disputed
                    }),
                    "disputed entry must cite disputed claim",
                )?;
            }
            Ok(())
        }
        _ => Err(err()),
    }
}
/// Only checked quotes leave extraction; raw output remains human-exportable.
pub fn input(kind: &WorkKind, setup: &TraceSetup, run: &Run, target: &Target) -> Result<Value> {
    let sources = &setup.sources;
    let question = &setup.question;
    Ok(match kind {
        WorkKind::TraceExtract => json!({"question":question,"sources":sources}),
        WorkKind::TraceMerge => {
            let mut extractions = run.extractions.clone();
            for (label, e) in &mut extractions {
                for c in &mut e.claims {
                    c.quotes.retain(|q| {
                        run.quote_checks.iter().any(|v| {
                            v.member_label == *label
                                && v.local_id == c.local_id
                                && !v.counter
                                && v.quote == *q
                                && v.span.is_some()
                        })
                    });
                    c.counter_quotes.retain(|q| {
                        run.quote_checks.iter().any(|v| {
                            v.member_label == *label
                                && v.local_id == c.local_id
                                && v.counter
                                && v.quote == *q
                                && v.span.is_some()
                        })
                    });
                }
            }
            json!({"question":question,"sources":sources,"extractions":extractions})
        }
        WorkKind::TraceVerify => {
            let Target::Member(n) = target else {
                return Err(invalid());
            };
            let ids = run.assignments.get(n).ok_or_else(invalid)?;
            let l = ledger(&run.claims, &run.quote_checks, &BTreeMap::new());
            // These fields contain no contributor labels, counts or private maps.
            let claims = l
                .claims
                .into_iter()
                .filter(|c| ids.contains(&c.claim_id))
                .map(|c| TraceAssignedClaim {
                    claim_id: c.claim_id,
                    text: c.text,
                    quotes: c
                        .quotes
                        .into_iter()
                        .map(|q| TraceEvidence {
                            source_id: q.quote.source_id,
                            quote: q.quote.quote,
                            span: q.span.expect("verified"),
                            counter: q.counter,
                        })
                        .collect(),
                })
                .collect::<Vec<_>>();
            json!({"question":question,"sources":sources,"claims":claims})
        }
        WorkKind::TraceAssemble => json!({"question":question,"ledger":run.ledger}),
        _ => return Err(invalid()),
    })
}
/// Deduplicate verified evidence within a provider request. Validation keeps the
/// complete typed ledger; models receive lossless quote references and one pool.
pub fn provider_projection(kind: &WorkKind, data: &Value) -> Value {
    let mut out = data.clone();
    let claims = match kind {
        WorkKind::TraceVerify => out.get_mut("claims"),
        WorkKind::TraceAssemble => out.get_mut("ledger").and_then(|l| l.get_mut("claims")),
        _ => return out,
    };
    let Some(claims) = claims.and_then(Value::as_array_mut) else {
        return out;
    };
    let mut pool = serde_json::Map::new();
    let mut ids = BTreeMap::<String, String>::new();
    for claim in claims {
        // Verdicts are keyed by verifier nick in the typed ledger. The blind judge
        // only needs the verdicts themselves: drop the keys and order by content, so
        // neither a nick nor roster order reaches the prompt. Nicks stay in the
        // ledger for the human report.
        if let Some(map) = claim.get("verdicts").and_then(Value::as_object) {
            let mut anonymous = map.values().cloned().collect::<Vec<_>>();
            anonymous.sort_by_key(Value::to_string);
            claim["verdicts"] = Value::Array(anonymous);
        }
        let quotes = claim
            .as_object_mut()
            .and_then(|c| c.remove("quotes"))
            .unwrap_or(json!([]));
        let refs = quotes
            .as_array()
            .into_iter()
            .flatten()
            .map(|q| {
                let (source, text) = if q["quote"].is_object() {
                    (&q["quote"]["sourceId"], &q["quote"]["quote"])
                } else {
                    (&q["sourceId"], &q["quote"])
                };
                let evidence = json!({"sourceId":source,"quote":text,"span":q["span"]});
                let key = evidence.to_string();
                let id = if let Some(id) = ids.get(&key) {
                    id.clone()
                } else {
                    let id = format!("Q{}", ids.len() + 1);
                    ids.insert(key, id.clone());
                    pool.insert(id.clone(), evidence);
                    id
                };
                json!({"quoteId":id,"counter":q["counter"]})
            })
            .collect::<Vec<_>>();
        claim["quoteRefs"] = json!(refs);
    }
    out["quotePool"] = Value::Object(pool);
    out
}
/// Identity information is excluded from the blind judge header entirely.
pub fn blind_input(mut data: Value, tokens: &[String]) -> Value {
    if let Some(extractions) = data["extractions"].as_object_mut() {
        for e in extractions.values_mut() {
            if let Some(claims) = e["claims"].as_array_mut() {
                for c in claims {
                    for key in ["claim", "scopeNote"] {
                        if let Some(s) = c[key].as_str() {
                            c[key] = super::tournament::redact(s, tokens).into();
                        }
                    }
                }
            }
        }
    }
    data
}
pub fn drive(router: &mut Router) -> Result<Vec<CommitOutcome>> {
    let ids = router.store.call(|db| {
        let mut q =
            db.prepare("SELECT contest_id FROM contests WHERE active=1 AND kind='trace'")?;
        Ok(q.query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    })?;
    let mut outcomes = vec![];
    for raw in ids {
        let contest: ContestId = raw.parse().map_err(|_| invalid())?;
        if router
            .engine_scope
            .as_ref()
            .is_some_and(|s| !s.contains(&contest))
        {
            continue;
        }
        for _ in 0..64 {
            let state = router.store.call(move |db| load_state(db, contest))?;
            if state.state.is_terminal() {
                break;
            }
            let id = EventId::new();
            let digest = squad_store::commit::digest(&state)?;
            match router
                .store
                .commit(id, "contest-engine".into(), digest, move |tx, g| {
                    drive_move(tx, g, id, state)
                }) {
                Ok(o) if o.events.is_empty() => break,
                Ok(o) => outcomes.push(o),
                Err(e) => {
                    tracing::error!(?e,%contest,"Trace engine failed");
                    outcomes.push(router.store.commit(
                        EventId::new(),
                        "contest-engine".into(),
                        "trace-failure".into(),
                        move |tx, g| {
                            let mut s = load_state(tx, contest)?;
                            finish(tx, g, id, &mut s, "invalid_step", true)?;
                            Ok(g.accepted(id))
                        },
                    )?);
                    break;
                }
            }
        }
    }
    Ok(outcomes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claims(sources: &[(&str, &str)]) -> TraceClaims {
        TraceClaims {
            claims: vec![TraceCanonicalClaim {
                claim_id: "C1".into(),
                text: "claim".into(),
                sources: sources
                    .iter()
                    .map(|(l, id)| TraceClaimRef {
                        member_label: (*l).into(),
                        local_id: (*id).into(),
                    })
                    .collect(),
                shared_origin: vec![],
            }],
        }
    }
    fn check(label: &str, id: &str, source: &str, counter: bool) -> TraceQuoteCheck {
        TraceQuoteCheck {
            member_label: label.into(),
            local_id: id.into(),
            counter,
            quote: TraceQuote {
                source_id: source.into(),
                quote: format!("quote from {source}"),
            },
            status: "found".into(),
            span: Some(TraceSpan { start: 0, end: 5 }),
        }
    }

    #[test]
    fn a_claim_backed_only_by_counter_quotes_is_unsupported() {
        let l = ledger(
            &claims(&[("Reader 1", "a")]),
            &[check("Reader 1", "a", "S1", true)],
            &BTreeMap::new(),
        );
        assert_eq!(l.claims[0].status, TraceClaimStatus::Unsupported);
        assert_eq!(l.claims[0].independent_sources, 0);
        assert_eq!(
            l.claims[0].quotes.len(),
            1,
            "the counter-quote stays in the ledger"
        );
    }

    #[test]
    fn counter_quote_sources_are_not_independent_support() {
        let l = ledger(
            &claims(&[("Reader 1", "a")]),
            &[
                check("Reader 1", "a", "S1", false),
                check("Reader 1", "a", "S2", true),
            ],
            &BTreeMap::new(),
        );
        assert_eq!(l.claims[0].independent_sources, 1);
        assert_eq!(l.claims[0].status, TraceClaimStatus::Unverified);
    }

    #[test]
    fn assemble_projection_has_no_member_nick_keys() {
        let mut l = ledger(
            &claims(&[("Reader 1", "a")]),
            &[check("Reader 1", "a", "S1", false)],
            &BTreeMap::new(),
        );
        l.claims[0].verdicts.insert(
            MemberNick::new("zebra").unwrap(),
            TraceVerdict {
                claim_id: "C1".into(),
                entailment: TraceEntailment::Supported,
                scope: TraceScope::Ok,
                qualifier_quote: None,
                note: "ok".into(),
            },
        );
        let data = json!({"question":"q","ledger":l});
        let projected = provider_projection(&WorkKind::TraceAssemble, &data).to_string();
        assert!(!projected.contains("zebra"), "{projected}");
        assert!(projected.contains("\"verdicts\":[{"));
    }
}
