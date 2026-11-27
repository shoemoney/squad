//! Bounded readback loop and deterministic rewrite verification.
use crate::router::{
    Router,
    readback::{drive_move, finish, invalid, load_state},
};
use squad_protocol::readback::*;
use squad_protocol::*;
use squad_store::{Result, commit::CommitOutcome};
use std::collections::{BTreeMap, BTreeSet};
pub fn prepare(parsed: &ParsedInput) -> std::result::Result<(), crate::prompts::PromptError> {
    if matches!(parsed, ParsedInput::Readback { .. }) {
        Ok(())
    } else {
        Err(crate::prompts::PromptError::new(
            "readback",
            None,
            "expected readback",
        ))
    }
}
pub fn call_bounds(m: u32, r: u32) -> (u32, u32) {
    (r * (m + 2), r * (2 * m + 6))
}
pub fn convergence(rounds: &[squad_store::readback::Round]) -> Vec<ReadbackConvergence> {
    rounds
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let entries = r.ledger.as_ref().map_or(0, |l| l.entries.len()) as u32;
            let resolved = r.choices.as_ref().map_or(0, |c| c.answers.len()) as u32;
            ReadbackConvergence {
                round: i as u32 + 1,
                spec_version: i as u32 + 1,
                accepted_interpretations: r.interpretations.len() as u32,
                ledger_entries: entries,
                resolved,
                open: entries - resolved,
                cut: r.cut,
            }
        })
        .collect()
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
                if key != "localId" {
                    blind_value(v, tokens)
                }
            }
        }
        _ => {}
    }
}
/// A quote accepted against the blind source is restored to its exact original passage.
pub fn original_passage(quote: &str, spec: &str, tokens: &[String]) -> Option<String> {
    let (blinded, spans) = blind(spec, tokens);
    let span = super::verbatim::verbatim(quote, &blinded)?;
    let start = spans.get(span.start)?.start;
    let end = spans.get(span.end - 1)?.end;
    Some(spec[start..end].to_owned())
}
pub fn verify_ledger(
    ledger: &ReadbackLedger,
    spec: &str,
    labels: &BTreeSet<String>,
) -> validation::ValidationResult {
    ledger.validate()?;
    for e in &ledger.entries {
        validation::ensure(
            super::verbatim::verbatim(&e.passage, spec).is_some(),
            "ledger passage is not verbatim",
        )?;
        validation::ensure(
            e.readings
                .iter()
                .all(|r| r.source_labels.iter().all(|s| labels.contains(s))),
            "unknown source label",
        )?;
    }
    Ok(())
}
/// Exact reconstruction also forbids edits outside the resolved source spans.
pub fn verify_rewrite(
    rewrite: &ReadbackRewrite,
    old: &str,
    ledger: &ReadbackLedger,
    choices: &BTreeMap<String, String>,
    first_bytes: usize,
    word_limit: usize,
) -> validation::ValidationResult {
    use validation::ensure;
    rewrite.validate()?;
    ensure(
        rewrite.spec.len() <= first_bytes * 2
            && rewrite.spec.split_whitespace().count() <= word_limit,
        "rewrite size bound exceeded",
    )?;
    let expected = choices.keys().collect::<BTreeSet<_>>();
    let supplied = rewrite
        .changes
        .iter()
        .map(|c| &c.entry_id)
        .collect::<BTreeSet<_>>();
    ensure(
        expected == supplied,
        "change log must cover exactly resolved entries",
    )?;
    let mut replacements = vec![];
    for change in &rewrite.changes {
        let entry = ledger
            .entries
            .iter()
            .find(|e| e.entry_id == change.entry_id)
            .ok_or_else(|| validation::ValidationError {
                message: "unknown changed entry".into(),
            })?;
        let span = super::verbatim::verbatim(&change.before, old).ok_or_else(|| {
            validation::ValidationError {
                message: "before is not verbatim".into(),
            }
        })?;
        let passage = super::verbatim::verbatim(&entry.passage, old).ok_or_else(|| {
            validation::ValidationError {
                message: "entry is not verbatim".into(),
            }
        })?;
        ensure(
            span == passage,
            "before must cover the resolved passage exactly",
        )?;
        replacements.push((span, change.after.as_str()));
    }
    replacements.sort_by_key(|r| r.0.start);
    replacements.dedup();
    ensure(
        replacements.windows(2).all(|w| w[0].0.end <= w[1].0.start),
        "overlapping changes",
    )?;
    let mut reconstructed = old.to_owned();
    for (span, after) in replacements.into_iter().rev() {
        reconstructed.replace_range(span, after);
    }
    ensure(
        reconstructed == rewrite.spec,
        "rewrite changed text outside resolved passages",
    )?;
    for e in &ledger.entries {
        if !choices.contains_key(&e.entry_id) {
            let span = super::verbatim::verbatim(&e.passage, old).expect("validated ledger");
            ensure(
                rewrite.spec.contains(&old[span]),
                "unresolved passage changed",
            )?;
        }
    }
    Ok(())
}
pub fn validate_output(
    kind: &WorkKind,
    data: &serde_json::Value,
    output: &serde_json::Value,
) -> validation::ValidationResult {
    let err = || validation::ValidationError {
        message: "invalid frozen readback input".into(),
    };
    let spec = data["spec"].as_str().ok_or_else(err)?;
    match kind {
        WorkKind::ReadbackInterpret => {
            let v = validation::decode::<ReadbackInterpretation>(output)?;
            v.validate()?;
            for b in &v.behaviours {
                validation::ensure(
                    super::verbatim::verbatim(&b.spec_quote, spec).is_some(),
                    "behaviour quote is not verbatim",
                )?;
            }
            Ok(())
        }
        WorkKind::ReadbackDiff => {
            let labels = data["interpretations"]
                .as_array()
                .ok_or_else(err)?
                .iter()
                .map(|i| i["label"].as_str().map(str::to_owned).ok_or_else(err))
                .collect::<std::result::Result<BTreeSet<_>, _>>()?;
            verify_ledger(
                &validation::decode::<ReadbackLedger>(output)?,
                spec,
                &labels,
            )
        }
        WorkKind::ReadbackRewrite => verify_rewrite(
            &validation::decode::<ReadbackRewrite>(output)?,
            spec,
            &serde_json::from_value(data["ledger"].clone()).map_err(|_| err())?,
            &serde_json::from_value(data["choices"].clone()).map_err(|_| err())?,
            data["firstBytes"].as_u64().ok_or_else(err)? as usize,
            data["wordLimit"].as_u64().ok_or_else(err)? as usize,
        ),
        _ => Err(err()),
    }
}
pub fn drive(router: &mut Router) -> Result<Vec<CommitOutcome>> {
    let ids = router.store.call(|db| {
        let mut q =
            db.prepare("SELECT contest_id FROM contests WHERE active=1 AND kind='readback'")?;
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
            if state.state.is_terminal() || state.state == ReadbackStatus::AwaitingHuman {
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
                    tracing::error!(?e,%contest,"Readback engine failed");
                    outcomes.push(router.store.commit(
                        EventId::new(),
                        "contest-engine".into(),
                        "readback-failure".into(),
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
