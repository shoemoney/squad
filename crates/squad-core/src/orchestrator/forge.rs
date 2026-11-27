//! Forge policy: mean normalized Borda, private ties, and an immutable winner.
use squad_protocol::forge::*;
use squad_protocol::*;
use std::collections::BTreeMap;
pub fn prepare(parsed: &ParsedInput) -> Result<(), crate::prompts::PromptError> {
    if matches!(parsed, ParsedInput::Forge { .. }) {
        Ok(())
    } else {
        Err(crate::prompts::PromptError::new(
            "forge",
            None,
            "expected forge input",
        ))
    }
}
/// The owned driver executes these shared moves atomically at each barrier.
pub const SCRIPT: [(ForgeStatus, ContestMove); 10] = [
    (ForgeStatus::Drafting, ContestMove::Fanout),
    (ForgeStatus::Drafting, ContestMove::Barrier),
    (ForgeStatus::Ranking, ContestMove::Fanout),
    (ForgeStatus::Ranking, ContestMove::Barrier),
    (ForgeStatus::Ranking, ContestMove::Tally),
    (ForgeStatus::Fusing, ContestMove::Fanout),
    (ForgeStatus::Fusing, ContestMove::Barrier),
    (ForgeStatus::Judging, ContestMove::Judge),
    (ForgeStatus::Reporting, ContestMove::Tally),
    (ForgeStatus::Reporting, ContestMove::Report),
];

pub type AcceptedBallot = (MemberNick, ForgeBallot, BTreeMap<String, MemberNick>);

/// Ballots have already been translated from each voter's private labels to authors.
pub fn tally(
    drafts: &BTreeMap<MemberNick, ForgeDraft>,
    ballots: &[AcceptedBallot],
    ties: &BTreeMap<MemberNick, String>,
) -> ForgeTally {
    let mut scores = vec![];
    let mut totals = BTreeMap::new();
    for author in drafts.keys() {
        let mut points = 0_u64;
        let mut voters = 0_u64;
        let mut first = 0;
        let mut flaws = 0;
        for (_, ballot, map) in ballots {
            if let Some(p) = ballot
                .ranking
                .iter()
                .position(|l| map.get(l) == Some(author))
            {
                let k = ballot.ranking.len();
                // 2520 is divisible by every possible denominator (1..=10).
                // Keep exact integer fractions for tie comparisons.
                points += (k - 1 - p) as u64 * (2520 / (k - 1) as u64);
                voters += 1;
                if p == 0 {
                    first += 1;
                }
                if ballot
                    .notes
                    .iter()
                    .any(|n| map.get(&n.label) == Some(author) && n.fatal_flaw.is_some())
                {
                    flaws += 1;
                }
            }
        }
        totals.insert(author.clone(), (points, voters.max(1)));
        scores.push(ForgeScore {
            author: author.clone(),
            score: points as f64 / (2520 * voters.max(1)) as f64,
            first_places: first,
            flaw_voters: flaws,
            tie_key: ties[author].clone(),
        });
    }
    scores.sort_by(|a, b| {
        let (a_points, a_voters) = totals[&a.author];
        let (b_points, b_voters) = totals[&b.author];
        (b_points * a_voters)
            .cmp(&(a_points * b_voters))
            .then(b.first_places.cmp(&a.first_places))
            .then(a.flaw_voters.cmp(&b.flaw_voters))
            .then(a.tie_key.cmp(&b.tie_key))
    });
    ForgeTally {
        best_original: scores[0].author.clone(),
        fusers: scores.iter().take(2).map(|s| s.author.clone()).collect(),
        max_words: drafts.values().map(|d| words(&d.body)).max().unwrap_or(0),
        scores,
    }
}
/// Draw once, persist the result before any work can run.
pub fn labels(authors: impl IntoIterator<Item = MemberNick>) -> BTreeMap<String, MemberNick> {
    let mut order = authors
        .into_iter()
        .map(|n| (CellId::new().to_string(), n))
        .collect::<Vec<_>>();
    order.sort();
    order
        .into_iter()
        .enumerate()
        .map(|(i, (_, n))| (format!("D{}", i + 1), n))
        .collect()
}
pub fn blind(text: &str, setup: &ForgeSetup) -> String {
    let mut tokens = setup.redaction_tokens.clone();
    tokens.extend(setup.participants.iter().map(ToString::to_string));
    tokens.extend(setup.models.values().map(|m| m.model.to_string()));
    for slug in tokens.clone() {
        if slug.contains('/') {
            tokens.extend(
                slug.split(['/', '-', '_', '.', '~'])
                    .filter(|p| p.len() > 2 && p.chars().any(char::is_alphabetic))
                    .map(str::to_owned),
            );
        }
    }
    tokens.retain(|s| !s.trim().is_empty());
    tokens.sort_by_key(|s| std::cmp::Reverse(s.len()));
    tokens.dedup();
    if tokens.is_empty() {
        return text.to_owned();
    }
    let alternatives = tokens
        .iter()
        .map(|t| regex::escape(t.trim_start_matches('~')))
        .collect::<Vec<_>>()
        .join("|");
    regex::Regex::new(&format!(r"(?i)\b(?:{alternatives})\b"))
        .expect("escaped tokens")
        .replace_all(text, "?")
        .into_owned()
}
pub fn drive(
    router: &mut crate::router::Router,
) -> squad_store::Result<Vec<squad_store::commit::CommitOutcome>> {
    let mut outcomes = vec![];
    for _ in 0..24 {
        let current = router.store.call(|db| {
            use rusqlite::OptionalExtension;
            let raw: Option<String> = db
                .query_row(
                    "SELECT state_json FROM contests WHERE active=1 AND kind='forge'",
                    [],
                    |r| r.get(0),
                )
                .optional()?;
            raw.map(|v| serde_json::from_str::<ForgeState>(&v).map_err(Into::into))
                .transpose()
        })?;
        let Some(state) = current else {
            break;
        };
        if router
            .engine_scope
            .as_ref()
            .is_some_and(|scope| !scope.contains(&state.contest_id))
        {
            break;
        }
        let before = state.state_version;
        let id = EventId::new();
        let out = router
            .store
            .commit(
                id,
                "contest-engine".into(),
                squad_store::commit::digest(&state)?,
                move |tx, g| {
                    crate::router::forge::advance(tx, g, id, state)?;
                    Ok(g.accepted(id))
                },
            )
            .inspect_err(|e| tracing::error!(error=?e,"Forge move failed"))?;
        // State version is checked independently; no work or accepted step is replayed.
        outcomes.push(out);
        let after = router.store.call(|db| {
            use rusqlite::OptionalExtension;
            Ok(db
                .query_row(
                    "SELECT state_version FROM contests WHERE active=1 AND kind='forge'",
                    [],
                    |r| r.get::<_, i64>(0),
                )
                .optional()?)
        })?;
        if after == Some(before as i64) {
            break;
        }
        if after.is_none() {
            break;
        }
    }
    Ok(outcomes)
}
