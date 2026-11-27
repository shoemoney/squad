//! Negotiation policy, private role draws and table outcome arithmetic.
use squad_protocol::negotiate::*;
use squad_protocol::*;
use std::collections::BTreeMap;
pub fn prepare(parsed: &ParsedInput) -> Result<(), crate::prompts::PromptError> {
    if matches!(parsed, ParsedInput::Negotiate { .. }) {
        Ok(())
    } else {
        Err(crate::prompts::PromptError::new(
            "negotiate",
            None,
            "expected negotiate input",
        ))
    }
}
pub const SCRIPT: [(NegotiateStatus, ContestMove); 9] = [
    (NegotiateStatus::Mapping, ContestMove::Fanout),
    (NegotiateStatus::Mapping, ContestMove::Barrier),
    (NegotiateStatus::Briefing, ContestMove::Judge),
    (NegotiateStatus::Packaging, ContestMove::Fanout),
    (NegotiateStatus::Packaging, ContestMove::Barrier),
    (NegotiateStatus::Rehearsing, ContestMove::Turns),
    (NegotiateStatus::WritingPlaybook, ContestMove::Judge),
    (NegotiateStatus::Reporting, ContestMove::Tally),
    (NegotiateStatus::Reporting, ContestMove::Report),
];
pub fn draw(participants: &[MemberNick]) -> Vec<NegotiateTable> {
    let mut order = participants
        .iter()
        .map(|n| (CellId::new().to_string(), n.clone()))
        .collect::<Vec<_>>();
    order.sort();
    let p = table_count(participants.len());
    (0..p)
        .map(|i| NegotiateTable {
            table: i as u32 + 1,
            counterpart: order[i].1.clone(),
            negotiator: order[i + p].1.clone(),
            rounds_used: 0,
            outcome: None,
            turns: vec![],
        })
        .collect()
}
pub fn outcome(response: NegotiateResponse, round: u32, max: u32) -> Option<NegotiateOutcome> {
    match response {
        NegotiateResponse::Accept => Some(NegotiateOutcome::Deal),
        NegotiateResponse::WalkAway => Some(NegotiateOutcome::NoDeal),
        _ if round >= max => Some(NegotiateOutcome::Unfinished),
        _ => None,
    }
}
pub fn tally(tables: &[NegotiateTable]) -> BTreeMap<String, u32> {
    let mut counts = BTreeMap::from([
        ("deal".into(), 0),
        ("no_deal".into(), 0),
        ("unfinished".into(), 0),
        ("abandoned".into(), 0),
    ]);
    for t in tables {
        if let Some(o) = t.outcome {
            let key = serde_json::to_value(o)
                .expect("enum")
                .as_str()
                .expect("string")
                .to_owned();
            *counts.entry(key).or_default() += 1;
        }
    }
    counts
}
/// Number of whole rounds made impossible by a terminal table.
pub fn unused_turns(tables: &[NegotiateTable], max: u32) -> u32 {
    tables
        .iter()
        .filter(|t| t.outcome.is_some())
        .map(|t| max.saturating_sub(t.rounds_used))
        .sum()
}
pub fn blind(text: &str, setup: &NegotiateSetup) -> String {
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
            // A short provider such as `x-ai` is split into pieces too small to keep, so
            // the provider is also redacted whole.
            if let Some((provider, _)) = slug.trim_start_matches('~').split_once('/') {
                tokens.push(provider.to_owned());
            }
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
        .replace_all(text, "[redacted]")
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
                    "SELECT state_json FROM contests WHERE active=1 AND kind='negotiate'",
                    [],
                    |r| r.get(0),
                )
                .optional()?;
            raw.map(|v| serde_json::from_str::<NegotiateState>(&v).map_err(Into::into))
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
                    crate::router::negotiate::advance(tx, g, id, state)?;
                    Ok(g.accepted(id))
                },
            )
            .inspect_err(|e| tracing::error!(error=?e,"Negotiate move failed"))?;
        // State version is checked independently; no work or accepted step is replayed.
        outcomes.push(out);
        let after = router.store.call(|db| {
            use rusqlite::OptionalExtension;
            Ok(db
                .query_row(
                    "SELECT state_version FROM contests WHERE active=1 AND kind='negotiate'",
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

/// Calls never allocated after newly terminal tables; a failed move also skips its reply.
pub fn released_calls(ended: &[NegotiateTable], max: u32, counter_wave: bool) -> (u32, u32) {
    let rounds = unused_turns(ended, max);
    let skipped_replies = if counter_wave {
        0
    } else {
        ended
            .iter()
            .filter(|t| t.outcome == Some(NegotiateOutcome::Abandoned))
            .count() as u32
    };
    (rounds, rounds + skipped_replies)
}
