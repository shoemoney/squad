//! Pure tune mechanics and the durable shared-engine script driver.
use sha2::{Digest, Sha256};
use squad_protocol::tune::*;
use squad_protocol::*;
use std::collections::BTreeMap;
pub fn prepare(parsed: &ParsedInput) -> Result<(), crate::prompts::PromptError> {
    if matches!(parsed, ParsedInput::Tune { .. }) {
        Ok(())
    } else {
        Err(crate::prompts::PromptError::new(
            "tune",
            None,
            "invalid tune start",
        ))
    }
}
/// Stable private order, independent of process and accepted-result timing.
pub fn seeded_key(seed: &str, scope: &str, value: &str) -> Vec<u8> {
    Sha256::digest(format!("{seed}\0{scope}\0{value}").as_bytes()).to_vec()
}
pub fn shuffled<T: Clone>(items: &[T], seed: &str, scope: &str) -> Vec<T> {
    let mut keyed: Vec<_> = items
        .iter()
        .enumerate()
        .map(|(i, x)| (seeded_key(seed, scope, &i.to_string()), x.clone()))
        .collect();
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    keyed.into_iter().map(|(_, x)| x).collect()
}
pub fn split(cases: &[TuneFrozenCase], seed: &str) -> (Vec<TuneFrozenCase>, Vec<TuneFrozenCase>) {
    let mut c = shuffled(cases, seed, "split");
    let train = c.split_off(3.min(c.len() / 2));
    let mut holdout = c;
    let mut train = train;
    for (i, c) in train.iter_mut().enumerate() {
        c.label = format!("T{}", i + 1);
    }
    for (i, c) in holdout.iter_mut().enumerate() {
        c.label = format!("H{}", i + 1);
    }
    (train, holdout)
}
pub fn mutators(rotation: &[MemberNick], generation: u32) -> Vec<MemberNick> {
    (0..3)
        .map(|i| rotation[((generation as usize - 1) * 3 + i) % rotation.len()].clone())
        .collect()
}
pub fn scores(
    sheet: &TuneScoreSheet,
    maps: &BTreeMap<String, BTreeMap<String, String>>,
) -> BTreeMap<String, f64> {
    let mut totals: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    for c in &sheet.cases {
        for o in &c.outputs {
            if let Some(id) = maps.get(&c.case_label).and_then(|m| m.get(&o.label)) {
                let (sum, count) = totals.entry(id.clone()).or_default();
                *sum += o.scores.iter().map(|s| u64::from(s.score)).sum::<u64>();
                *count += 4 * o.scores.len() as u64;
            }
        }
    }
    totals
        .into_iter()
        .map(|(k, (s, n))| (k, s as f64 / n as f64))
        .collect()
}
pub fn promotion(
    candidates: &[TuneCandidate],
    generation: u32,
    champion: &str,
    scores: &BTreeMap<String, f64>,
    seed: &str,
) -> TuneGeneration {
    let mut new: Vec<_> = candidates
        .iter()
        .filter(|c| c.generation == generation && !c.incomplete && scores.contains_key(&c.id))
        .collect();
    new.sort_by(|a, b| {
        scores[&b.id]
            .total_cmp(&scores[&a.id])
            .then_with(|| tune::words(&a.prompt).cmp(&tune::words(&b.prompt)))
            .then_with(|| seeded_key(seed, "tie", &a.id).cmp(&seeded_key(seed, "tie", &b.id)))
    });
    let best = new.first().map(|c| c.id.clone());
    let promoted = best
        .as_ref()
        .is_some_and(|b| scores[b] - scores[champion] >= 0.05 - 1e-12);
    TuneGeneration {
        generation,
        scores: scores.clone(),
        best: best.clone(),
        champion: if promoted {
            best.expect("best")
        } else {
            champion.into()
        },
        promoted,
    }
}
pub fn final_pick(
    original: &str,
    chronological: &[String],
    scores: &BTreeMap<String, f64>,
) -> String {
    let mut winner = original.to_owned();
    let mut highest = scores[original];
    for c in chronological {
        if scores[c] - scores[original] >= 0.05 - 1e-12
            && (winner == original || scores[c] > highest + 1e-12)
        {
            winner = c.clone();
            highest = scores[c];
        }
    }
    winner
}
pub fn digest(
    sheet: &TuneScoreSheet,
    maps: &BTreeMap<String, BTreeMap<String, String>>,
    champion: &str,
    cases: &[TuneFrozenCase],
) -> TuneDigest {
    TuneDigest {
        cases: sheet
            .cases
            .iter()
            .filter_map(|c| {
                let row = c.outputs.iter().find(|o| {
                    maps.get(&c.case_label)
                        .and_then(|m| m.get(&o.label))
                        .is_some_and(|id| id == champion)
                })?;
                Some(TuneDigestCase {
                    case_label: c.case_label.clone(),
                    kind: cases.iter().find(|t| t.label == c.case_label)?.case.kind,
                    scores: row.scores.clone(),
                    note: row.note.clone(),
                })
            })
            .collect(),
    }
}
/// Compare with the same NFKC/quote/dash/whitespace normalization as verbatim,
/// including short-word windows that fall below verbatim's 12-character minimum.
pub fn note_leaks(note: &str, source: &str) -> bool {
    use unicode_normalization::UnicodeNormalization;
    let normalize = |s: &str| {
        s.nfkc()
            .map(|c| match c {
                '‘' | '’' | '‚' | '‛' => '\'',
                '“' | '”' | '„' | '‟' => '"',
                '–' | '—' => '-',
                c => c,
            })
            .collect::<String>()
            .split_whitespace()
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let note = normalize(note).join(" ");
    let source = normalize(source);
    source
        .windows(6)
        .any(|words| note.contains(&words.join(" ")))
}
pub fn validate_output(
    input: &TuneCaseMaterial,
    value: &serde_json::Value,
) -> validation::ValidationResult {
    input.validate_output(value)?;
    if let TuneCaseMaterial::Score { rubric, cases, .. } = input {
        let sheet: TuneScoreSheet = validation::decode(value)?;
        for row in &sheet.cases {
            let c = cases
                .iter()
                .find(|c| c.case_label == row.case_label)
                .expect("validated");
            for o in &row.outputs {
                let sources = std::iter::once(c.case.input.as_str())
                    .chain(c.case.expect.iter().map(String::as_str))
                    .chain(c.case.must_not.iter().map(String::as_str))
                    .chain(rubric.iter().map(|r| r.description.as_str()));
                validation::ensure(
                    !sources.into_iter().any(|s| note_leaks(&o.note, s)),
                    "score note copies six consecutive hidden words",
                )?;
            }
        }
    }
    Ok(())
}
pub fn drive(
    router: &mut crate::router::Router,
) -> squad_store::Result<Vec<squad_store::commit::CommitOutcome>> {
    let ids = router.store.call(|db| {
        let mut q = db.prepare("SELECT contest_id FROM contests WHERE active=1 AND kind='tune'")?;
        Ok(q.query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    })?;
    let mut outcomes = vec![];
    for raw in ids {
        let contest: ContestId = raw
            .parse()
            .map_err(|_| squad_store::StoreError::Unavailable)?;
        if router
            .engine_scope
            .as_ref()
            .is_some_and(|s| !s.contains(&contest))
        {
            continue;
        }
        // Each transaction is a durable move. Stop when waiting on paid work.
        for _ in 0..8 {
            let ready = router
                .store
                .call(move |db| crate::router::tune::ready(db, contest))?;
            if !ready {
                break;
            }
            let id = EventId::new();
            let max = router.daily_max;
            outcomes.push(router.store.commit(
                id,
                "contest-engine".into(),
                squad_store::commit::digest(&(contest, id))?,
                move |tx, g| {
                    crate::router::tune::advance(tx, g, id, contest, max)?;
                    Ok(g.accepted(id))
                },
            )?);
        }
    }
    Ok(outcomes)
}

/// Generated prose is bounded independently of the provider's token ceiling.
pub fn validate_word_budget(
    input: &TuneCaseMaterial,
    value: &serde_json::Value,
    room_words: usize,
) -> validation::ValidationResult {
    fn count(v: &serde_json::Value) -> usize {
        match v {
            serde_json::Value::String(s) => s.split_whitespace().count(),
            serde_json::Value::Array(a) => a.iter().map(count).sum(),
            serde_json::Value::Object(o) => o.values().map(count).sum(),
            _ => 0,
        }
    }
    let limit = match input {
        TuneCaseMaterial::Cases { .. } | TuneCaseMaterial::Freeze { .. } => room_words,
        TuneCaseMaterial::Score { .. } => 600,
        _ => return Ok(()),
    };
    validation::ensure(
        count(value) <= limit,
        "generated output exceeds its word budget",
    )
}
