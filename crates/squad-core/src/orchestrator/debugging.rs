//! Bounded evidence loop. Only human outcomes can eliminate a cause.
use crate::router::{
    Router,
    debugging::{drive_move, finish, invalid, load_state},
};
use squad_protocol::debugging::*;
use squad_protocol::*;
use squad_store::{Result, commit::CommitOutcome};
use std::collections::{BTreeMap, BTreeSet};
use validation::{ValidationResult, decode, ensure};
pub fn prepare(p: &ParsedInput) -> std::result::Result<(), crate::prompts::PromptError> {
    if matches!(p, ParsedInput::Debugging { .. }) {
        Ok(())
    } else {
        Err(crate::prompts::PromptError::new(
            "debug",
            None,
            "expected debug",
        ))
    }
}
pub fn call_bounds(m: u32, r: u32) -> (u32, u32) {
    ((r + 1) * (m + 1) + 1, 2 * m * (r + 1) + 3 * r + 6)
}
pub fn patch_allowed(rows: &[DebuggingLiveHypothesis]) -> bool {
    let live = rows
        .iter()
        .filter(|h| h.status == DebuggingHypothesisStatus::Live)
        .collect::<Vec<_>>();
    live.len() == 1 && live[0].consistent_results > 0
}
pub fn eliminate(
    rows: &[DebuggingLiveHypothesis],
    selection: &DebuggingSelection,
    result: &DebuggingResult,
) -> DebuggingElimination {
    let mut rows = rows.to_vec();
    if result.outcome != "other" && result.outcome != "unknown" {
        for h in &mut rows {
            if h.status != DebuggingHypothesisStatus::Live {
                continue;
            }
            if let Some(p) = selection
                .predictions
                .iter()
                .find(|p| p.hypothesis_id == h.hypothesis.hypothesis_id)
            {
                if selection.outcomes.iter().any(|o| o.label == p.outcome) {
                    if p.outcome == result.outcome {
                        h.consistent_results += 1;
                    } else {
                        h.status = DebuggingHypothesisStatus::Eliminated;
                        h.failed_prediction = Some(format!(
                            "round {} predicted {}, observed {}",
                            result.round, p.outcome, result.outcome
                        ));
                    }
                }
            }
        }
    }
    DebuggingElimination {
        round: result.round,
        hypotheses: rows,
    }
}
pub fn loop_end(rows: &[DebuggingLiveHypothesis], round: u32, max: u32) -> Option<&'static str> {
    match rows
        .iter()
        .filter(|h| h.status == DebuggingHypothesisStatus::Live)
        .count()
    {
        0 => Some("all_eliminated"),
        1 => Some("isolated"),
        _ if round >= max => Some("rounds_complete"),
        _ => None,
    }
}
pub fn check_quotes(h: &DebuggingHypothesis, brief: &str) -> Vec<DebuggingQuoteCheck> {
    h.supporting
        .iter()
        .chain(&h.contradicting)
        .map(|quote| DebuggingQuoteCheck {
            quote: quote.clone(),
            unanchored: super::verbatim::verbatim(quote, brief).is_none(),
        })
        .collect()
}
pub fn prompt_evidence(s: &str) -> String {
    s.split_whitespace().take(400).collect::<Vec<_>>().join(" ")
}
/// Remove configured identities from blind prose, retaining byte provenance for quotes.
pub fn blind(text: &str, tokens: &[String]) -> (String, Vec<std::ops::Range<usize>>) {
    let mut tokens = tokens
        .iter()
        .filter(|s| !s.trim().is_empty())
        .cloned()
        .collect::<Vec<_>>();
    for slug in tokens.clone() {
        if slug.contains('/') {
            tokens.extend(
                slug.split(['/', '-', '_', '.', '~'])
                    .filter(|p| p.chars().any(char::is_alphabetic))
                    .map(str::to_owned),
            );
        }
    }
    tokens.sort_by_key(|s| std::cmp::Reverse(s.len()));
    tokens.dedup();
    if tokens.is_empty() {
        return (text.into(), (0..text.len()).map(|i| i..i + 1).collect());
    }
    let pattern = format!(
        r"(?i)\b(?:{})\b",
        tokens
            .iter()
            .map(|s| regex::escape(s.trim_start_matches('~')))
            .collect::<Vec<_>>()
            .join("|")
    );
    let regex = regex::Regex::new(&pattern).expect("escaped tokens");
    let mut out = String::new();
    let mut spans = vec![];
    let mut end = 0;
    for m in regex.find_iter(text) {
        out.push_str(&text[end..m.start()]);
        spans.extend((end..m.start()).map(|i| i..i + 1));
        out.push_str("[redacted]");
        spans.extend((0..10).map(|_| m.start()..m.end()));
        end = m.end();
    }
    out.push_str(&text[end..]);
    spans.extend((end..text.len()).map(|i| i..i + 1));
    (out, spans)
}
pub fn blind_value(value: &mut serde_json::Value, tokens: &[String]) {
    match value {
        serde_json::Value::String(s) => *s = blind(s, tokens).0,
        serde_json::Value::Array(a) => {
            for v in a {
                blind_value(v, tokens)
            }
        }
        serde_json::Value::Object(o) => {
            for (key, v) in o {
                if !matches!(
                    key.as_str(),
                    "localId"
                        | "hypothesisId"
                        | "sources"
                        | "outcome"
                        | "label"
                        | "testRef"
                        | "prior"
                        | "priorSpread"
                        | "status"
                        | "effort"
                        | "safety"
                ) {
                    blind_value(v, tokens)
                }
            }
        }
        _ => {}
    }
}

fn live(
    data: &serde_json::Value,
) -> std::result::Result<Vec<DebuggingLiveHypothesis>, validation::ValidationError> {
    decode(&data["differential"])
}
pub fn proposals(
    data: &serde_json::Value,
) -> std::result::Result<BTreeMap<String, DebuggingTestProposal>, validation::ValidationError> {
    decode(&data["proposals"])
}
pub fn tests(p: &BTreeMap<String, DebuggingTestProposal>) -> BTreeMap<String, DebuggingTest> {
    p.iter()
        .flat_map(|(label, p)| {
            p.tests
                .iter()
                .map(move |t| (format!("{label}:{}", t.local_id), t.clone()))
        })
        .collect()
}
fn can_split(t: &DebuggingTest) -> bool {
    t.safety != DebuggingSafety::Risky
        && t.predictions
            .iter()
            .filter_map(|p| p.outcome.as_ref())
            .collect::<BTreeSet<_>>()
            .len()
            >= 2
}
pub fn validate_output(
    kind: &WorkKind,
    data: &serde_json::Value,
    output: &serde_json::Value,
) -> ValidationResult {
    debugging::validate_artifact(debugging::composition(kind).unwrap_or(""), output)?;
    match kind {
        WorkKind::DebuggingHypothesize => Ok(()),
        WorkKind::DebuggingMerge => {
            let merged: DebuggingDifferential = decode(output)?;
            let tables: BTreeMap<String, DebuggingHypotheses> = decode(&data["tables"])?;
            let sources = tables
                .iter()
                .flat_map(|(label, t)| {
                    t.hypotheses
                        .iter()
                        .map(move |h| (format!("{label}:{}", h.local_id), h))
                })
                .collect::<BTreeMap<_, _>>();
            let used = merged
                .hypotheses
                .iter()
                .flat_map(|h| h.sources.iter())
                .collect::<Vec<_>>();
            ensure(
                validation::unique(&used)
                    && used.iter().copied().collect::<BTreeSet<_>>()
                        == sources.keys().collect::<BTreeSet<_>>(),
                "merge must retain every source exactly once",
            )?;
            for h in &merged.hypotheses {
                let originals = h.sources.iter().map(|s| sources[s]).collect::<Vec<_>>();
                let support = originals
                    .iter()
                    .flat_map(|o| o.supporting.iter())
                    .collect::<BTreeSet<_>>();
                let contra = originals
                    .iter()
                    .flat_map(|o| o.contradicting.iter())
                    .collect::<BTreeSet<_>>();
                ensure(
                    h.supporting.iter().collect::<BTreeSet<_>>() == support
                        && h.contradicting.iter().collect::<BTreeSet<_>>() == contra,
                    "merge must preserve supporting and contradicting quotes",
                )?;
                ensure(
                    originals.iter().all(|o| h.prior_spread.contains(&o.prior))
                        && h.prior_spread
                            .iter()
                            .all(|p| originals.iter().any(|o| &o.prior == p)),
                    "merge must retain prior spread",
                )?;
            }
            Ok(())
        }
        WorkKind::DebuggingTest => {
            let p: DebuggingTestProposal = decode(output)?;
            let rows = live(data)?;
            let ids = rows
                .iter()
                .filter(|h| h.status == DebuggingHypothesisStatus::Live)
                .map(|h| &h.hypothesis.hypothesis_id)
                .collect::<BTreeSet<_>>();
            ensure(
                p.new_hypothesis.is_none() || data["allowNewHypothesis"] == true,
                "new hypothesis requires unexpected result",
            )?;
            for t in p.tests {
                ensure(
                    t.predictions
                        .iter()
                        .map(|p| &p.hypothesis_id)
                        .collect::<BTreeSet<_>>()
                        == ids,
                    "test must predict exactly every live hypothesis",
                )?;
            }
            Ok(())
        }
        WorkKind::DebuggingSelect => {
            let s: DebuggingSelection = decode(output)?;
            let p = proposals(data)?;
            let ts = tests(&p);
            let rows = live(data)?;
            let Some(reference) = &s.test_ref else {
                ensure(
                    s.not_selected
                        .iter()
                        .map(|n| n.test_ref.clone())
                        .collect::<BTreeSet<_>>()
                        == ts.keys().cloned().collect::<BTreeSet<_>>()
                        && s.not_selected.iter().all(|n| !n.why.trim().is_empty()),
                    "null selection must explain all proposals",
                )?;
                return ensure(
                    !ts.values().any(can_split),
                    "a discriminating safe test exists",
                );
            };
            let t = ts
                .get(reference)
                .ok_or_else(|| validation::ValidationError {
                    message: "unproposed test".into(),
                })?;
            ensure(
                t.safety != DebuggingSafety::Risky,
                "risky test cannot be selected",
            )?;
            ensure(
                s.instruction == t.procedure,
                "instruction must preserve proposed procedure",
            )?;
            ensure(
                s.outcomes
                    .iter()
                    .map(|o| &o.description)
                    .collect::<Vec<_>>()
                    == t.outcomes.iter().collect::<Vec<_>>(),
                "selection must retain proposed outcomes in order",
            )?;
            let mut ids = rows
                .iter()
                .filter(|h| h.status == DebuggingHypothesisStatus::Live)
                .map(|h| h.hypothesis.hypothesis_id.clone())
                .collect::<BTreeSet<_>>();
            for h in &s.added_hypotheses {
                ensure(
                    data["allowNewHypothesis"] == true
                        && !rows
                            .iter()
                            .any(|r| r.hypothesis.hypothesis_id == h.hypothesis_id)
                        && ids.insert(h.hypothesis_id.clone()),
                    "invalid added hypothesis",
                )?;
                DebuggingDifferential {
                    hypotheses: vec![h.clone()],
                }
                .validate()?;
                for source in &h.sources {
                    let (label, id) =
                        source
                            .rsplit_once(':')
                            .ok_or_else(|| validation::ValidationError {
                                message: "invalid addition source".into(),
                            })?;
                    let original = p
                        .get(label)
                        .and_then(|p| p.new_hypothesis.as_ref())
                        .filter(|h| h.local_id == id)
                        .ok_or_else(|| validation::ValidationError {
                            message: "unproposed hypothesis".into(),
                        })?;
                    ensure(
                        h.cause == original.cause
                            && h.supporting == original.supporting
                            && h.contradicting == original.contradicting
                            && h.prior_spread == vec![original.prior],
                        "addition must preserve proposed hypothesis",
                    )?;
                }
            }
            ensure(
                s.predictions
                    .iter()
                    .map(|p| p.hypothesis_id.clone())
                    .collect::<BTreeSet<_>>()
                    == ids,
                "selection must predict every live hypothesis",
            )?;
            for prediction in &s.predictions {
                if let Some(original) = t
                    .predictions
                    .iter()
                    .find(|p| p.hypothesis_id == prediction.hypothesis_id)
                {
                    let expected = original
                        .outcome
                        .as_ref()
                        .and_then(|o| t.outcomes.iter().position(|v| v == o))
                        .map(|i| format!("O{}", i + 1))
                        .unwrap_or_else(|| "cannot_predict".into());
                    // Disagreement may withhold elimination; never invent a different outcome.
                    ensure(
                        prediction.outcome == expected
                            || (prediction.outcome == "disputed"
                                && ts.values().any(|other| {
                                    other.procedure == t.procedure
                                        && other.outcomes == t.outcomes
                                        && other.predictions.iter().any(|p| {
                                            p.hypothesis_id == prediction.hypothesis_id
                                                && p.outcome != original.outcome
                                        })
                                })),
                        "merged prediction changed the proposed outcome",
                    )?;
                } else {
                    ensure(
                        prediction.outcome == "cannot_predict",
                        "new hypothesis has no committed prediction",
                    )?;
                }
            }
            ensure(
                s.predictions
                    .iter()
                    .filter(|p| {
                        rows.iter().any(|h| {
                            h.status == DebuggingHypothesisStatus::Live
                                && h.hypothesis.hypothesis_id == p.hypothesis_id
                        })
                    })
                    .filter(|p| s.outcomes.iter().any(|o| o.label == p.outcome))
                    .map(|p| &p.outcome)
                    .collect::<BTreeSet<_>>()
                    .len()
                    >= 2,
                "test must split two live hypotheses",
            )?;
            ensure(
                s.not_selected
                    .iter()
                    .map(|n| n.test_ref.clone())
                    .collect::<BTreeSet<_>>()
                    == ts
                        .keys()
                        .filter(|k| *k != reference)
                        .cloned()
                        .collect::<BTreeSet<_>>()
                    && s.not_selected.iter().all(|n| !n.why.trim().is_empty()),
                "notSelected must explain every other proposed test",
            )?;
            Ok(())
        }
        WorkKind::DebuggingReport => {
            let s: DebuggingSummary = decode(output)?;
            ensure(
                s.patch.is_none() || (data["patchAllowed"] == true && patch_allowed(&live(data)?)),
                "patch is not allowed by evidence",
            )?;
            ensure(
                data["patchAllowed"] == true
                    || s.patch_rationale == crate::export::debugging::NO_PATCH,
                "missing no-patch explanation",
            )?;
            ensure(
                s.patch.as_ref().is_none_or(|p| !p.trim().is_empty()),
                "empty patch",
            )
        }
        _ => ensure(false, "invalid debug work"),
    }
}
pub fn drive(router: &mut Router) -> Result<Vec<CommitOutcome>> {
    let ids = router.store.call(|db| {
        let mut q =
            db.prepare("SELECT contest_id FROM contests WHERE active=1 AND kind='debug'")?;
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
            if state.state.is_terminal() || state.state == DebuggingStatus::AwaitingHuman {
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
                    tracing::error!(?e,%contest,"Debugging engine failed");
                    outcomes.push(router.store.commit(
                        EventId::new(),
                        "contest-engine".into(),
                        "debugging-failure".into(),
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
