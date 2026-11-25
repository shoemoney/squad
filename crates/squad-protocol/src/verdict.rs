//! Strict model schemas and application-owned arithmetic.
use crate::validation::{ValidationError, ensure};
use crate::*;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub fn validate_round_response(
    value: &Value,
    debaters: &[MemberNick; 2],
) -> Result<RoundJudgeResponse, ValidationError> {
    let response: RoundJudgeResponse = crate::validation::decode(value)?;
    validate_scores(&response.scores, debaters)?;
    Ok(response)
}
pub fn validate_scores(
    scores: &BTreeMap<MemberNick, RoundScore>,
    debaters: &[MemberNick; 2],
) -> Result<(), ValidationError> {
    exact_keys(scores, debaters)?;
    for score in scores.values() {
        ensure(
            (1..=10).contains(&score.score),
            "round score must be an integer from 1 to 10",
        )?;
        bounded(&score.reason, 200, "round reason")?;
    }
    Ok(())
}
pub fn exact_keys<T>(
    map: &BTreeMap<MemberNick, T>,
    debaters: &[MemberNick; 2],
) -> Result<(), ValidationError> {
    ensure(
        debaters[0] != debaters[1]
            && map.len() == 2
            && debaters.iter().all(|n| map.contains_key(n)),
        "map must contain exactly the two debater nicks",
    )
}
fn bounded(s: &str, max: usize, name: &str) -> Result<(), ValidationError> {
    ensure(
        !s.trim().is_empty() && s.chars().count() <= max,
        format!("{name} must be nonempty and at most {max} characters"),
    )
}
fn weight(criterion: &CriterionId) -> u32 {
    match criterion {
        CriterionId::Fit | CriterionId::Feasibility => 30,
        CriterionId::Originality => 15,
        CriterionId::SurvivedAttacks => 25,
    }
}
/// Validate provider output. Authoritative totals and roundTally are not accepted in this schema.
pub fn validate_judge_verdict(
    value: &Value,
    debaters: &[MemberNick; 2],
    tally: &BTreeMap<MemberNick, u32>,
) -> Result<Verdict, ValidationError> {
    let judge: JudgeVerdict = crate::validation::decode(value)?;
    assemble_verdict(judge, debaters, tally)
}
pub fn assemble_verdict(
    judge: JudgeVerdict,
    debaters: &[MemberNick; 2],
    tally: &BTreeMap<MemberNick, u32>,
) -> Result<Verdict, ValidationError> {
    exact_keys(tally, debaters)?;
    ensure(debaters.contains(&judge.winner), "winner must be a debater")?;
    ensure(judge.scores.len() == 2, "scores must contain both debaters")?;
    let mut seen = BTreeSet::new();
    let mut weighted_totals = Vec::new();
    for score in &judge.scores {
        ensure(
            debaters.contains(&score.nick) && seen.insert(score.nick.clone()),
            "duplicate or unknown score nick",
        )?;
        ensure(score.criteria.len() == 4, "all four criteria are required")?;
        let mut criteria = BTreeSet::new();
        let mut numerator = 0_u32;
        for item in &score.criteria {
            ensure(
                criteria.insert(serde_json::to_string(&item.criterion_id).expect("criterion")),
                "duplicate criterion",
            )?;
            ensure(
                (1..=10).contains(&item.score),
                "criterion score must be 1 to 10",
            )?;
            bounded(&item.reasoning, 4000, "criterion reasoning")?;
            numerator += u32::from(item.score) * weight(&item.criterion_id);
        }
        weighted_totals.push(WeightedTotal {
            nick: score.nick.clone(),
            total: f64::from(numerator) / 100.0,
        });
    }
    bounded(&judge.reasoning, 12000, "overall reasoning")?;
    ensure(
        judge.what_to_steal_from_loser.len() <= 10,
        "at most ten borrowing suggestions",
    )?;
    for suggestion in &judge.what_to_steal_from_loser {
        bounded(suggestion, 2000, "borrowing suggestion")?;
    }
    let winner_total = weighted_totals
        .iter()
        .find(|t| t.nick == judge.winner)
        .expect("validated winner")
        .total;
    ensure(
        weighted_totals.iter().all(|t| t.total <= winner_total),
        "winner must have the highest weighted total",
    )?;
    let winner_tally = tally[&judge.winner];
    if tally.values().any(|v| *v > winner_tally) {
        ensure(
            judge
                .tally_override_reason
                .as_ref()
                .is_some_and(|s| !s.trim().is_empty()),
            "winner overriding round tally requires tallyOverrideReason",
        )?;
    }
    Ok(Verdict {
        verdict: judge,
        weighted_totals,
        round_tally: tally.clone(),
    })
}
/// Check committed totals against authoritative accepted scores supplied by the router.
pub fn validate_committed_verdict(
    verdict: &Verdict,
    debaters: &[MemberNick; 2],
    tally: &BTreeMap<MemberNick, u32>,
) -> Result<(), ValidationError> {
    let expected = assemble_verdict(verdict.verdict.clone(), debaters, tally)?;
    ensure(
        verdict.round_tally == *tally,
        "roundTally differs from router tally",
    )?;
    ensure(
        verdict.weighted_totals.len() == 2,
        "weighted totals must cover both debaters",
    )?;
    for total in &expected.weighted_totals {
        ensure(
            verdict
                .weighted_totals
                .iter()
                .filter(|v| v.nick == total.nick && v.total == total.total)
                .count()
                == 1,
            "incorrect weighted total",
        )?;
    }
    Ok(())
}
pub fn validate_round_score(
    score: &DebateRoundScore,
    debaters: &[MemberNick; 2],
    total_rounds: u32,
) -> Result<(), ValidationError> {
    ensure(
        (1..=total_rounds).contains(&score.round) && total_rounds <= 20,
        "score requires admitted numbered round",
    )?;
    exact_keys(&score.tally, debaters)?;
    match &score.scores {
        Some(scores) => {
            validate_scores(scores, debaters)?;
            ensure(score.reason.is_none(), "scored round must omit reason")
        }
        None => ensure(
            score
                .reason
                .as_ref()
                .is_some_and(|r| r.as_str() == "unscored"),
            "unscored round requires reason unscored",
        ),
    }
}
