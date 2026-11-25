//! Survey is a script of frozen fanouts, barriers, tallies and judge moves.
use squad_protocol::*;
use std::collections::BTreeSet;

pub fn transition(
    state: SurveyStatus,
    signal: SurveyTransition,
    collect: bool,
) -> Option<SurveyStatus> {
    use SurveyStatus::*;
    use SurveyTransition::*;
    if state.is_terminal() {
        return None;
    }
    Some(match (state, signal) {
        (_, Stop) => Stopped,
        (_, Fail) => Failed,
        (Preflight, ParticipantsReady) | (WaitingForParticipants, ParticipantsReady) => {
            if collect {
                Collecting
            } else {
                Extracting
            }
        }
        (Collecting, CollectionLocked) => Extracting,
        (Extracting, ClaimsLocked) => Voting,
        (Voting, VotesLocked) => Selecting,
        (Selecting, SelectionLocked) => Revoting,
        (Revoting, RevotesLocked) => Reporting,
        (Reporting, ReportLocked) => Complete,
        _ => return None,
    })
}
fn item<'a>(votes: &'a [SurveyVote], nick: &MemberNick, id: &str) -> Option<&'a SurveyBallotItem> {
    votes
        .iter()
        .find(|v| &v.voter == nick)?
        .ballot
        .votes
        .iter()
        .find(|v| v.claim_id == id)
}
fn tally<'a>(items: impl Iterator<Item = Option<&'a SurveyBallotItem>>) -> SurveyTally {
    let mut t = SurveyTally::default();
    for v in items {
        match v.map(|v| v.vote) {
            Some(SurveyChoice::Agree) => t.agree += 1,
            Some(SurveyChoice::Disagree) => t.disagree += 1,
            Some(SurveyChoice::Unsure) => t.unsure += 1,
            None => t.missing += 1,
        }
    }
    t
}
/// Rational comparisons avoid float ordering and preserve stable claim identities.
pub fn select(
    claims: &SurveyExtraction,
    participants: &[MemberNick],
    first: &[SurveyVote],
) -> Vec<String> {
    let mut ranked = claims
        .claims
        .iter()
        .map(|c| {
            (
                c.id.clone(),
                tally(participants.iter().map(|n| item(first, n, &c.id))),
            )
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|(a, x), (b, y)| {
        let dx = x.agree + x.disagree;
        let dy = y.agree + y.disagree;
        (dx == 0)
            .cmp(&(dy == 0))
            .then_with(|| {
                (x.agree.abs_diff(x.disagree) * dy).cmp(&(y.agree.abs_diff(y.disagree) * dx))
            })
            .then_with(|| dy.cmp(&dx))
            .then_with(|| a.cmp(b))
    });
    ranked.into_iter().take(5).map(|(id, _)| id).collect()
}
/// Remove identifying spans and deduplicate after redaction. Order carries no voter grouping.
pub fn blind_text(text: &str, setup: &SurveySetup) -> String {
    let mut result = text.to_owned();
    // Replace longer identifiers first to keep overlapping nicks deterministic.
    let mut names = setup
        .participants
        .iter()
        .map(|n| n.as_str())
        .collect::<Vec<_>>();
    names.sort_by_key(|n| std::cmp::Reverse(n.len()));
    for name in names {
        // Nick grammar is ASCII; matching lowercase preserves UTF-8 byte offsets.
        loop {
            let lower = result.to_lowercase();
            let Some(pos) = lower.find(&name.to_lowercase()) else {
                break;
            };
            if !result.is_char_boundary(pos) || !result.is_char_boundary(pos + name.len()) {
                break;
            }
            result.replace_range(pos..pos + name.len(), "[member]");
        }
    }
    result
}
pub fn selection(
    setup: &SurveySetup,
    step_id: StepId,
    claims: &SurveyExtraction,
    first: &[SurveyVote],
) -> SurveySelection {
    let selected = select(claims, &setup.participants, first);
    let claims = selected
        .iter()
        .filter_map(|id| claims.claims.iter().find(|c| &c.id == id))
        .map(|c| {
            let reasons = |choice| {
                first
                    .iter()
                    .flat_map(|v| &v.ballot.votes)
                    .filter(|v| v.claim_id == c.id && v.vote == choice)
                    .map(|v| blind_text(&v.reason, setup))
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect()
            };
            SurveyAnonymousClaim {
                claim_id: c.id.clone(),
                claim: blind_text(&c.claim, setup),
                agree_reasons: reasons(SurveyChoice::Agree),
                disagree_reasons: reasons(SurveyChoice::Disagree),
                unsure_reasons: reasons(SurveyChoice::Unsure),
            }
        })
        .collect();
    SurveySelection {
        contest_id: setup.contest_id,
        step_id,
        claims,
    }
}
/// Derive classifications once in application code. Judge prose never supplies counts.
pub fn map(
    setup: &SurveySetup,
    claims: &SurveyExtraction,
    selected: &[String],
    first: &[SurveyVote],
    second: &[SurveyVote],
) -> Vec<SurveyClaimMap> {
    claims
        .claims
        .iter()
        .map(|c| {
            let final_item = |n| {
                if selected.contains(&c.id) {
                    item(second, n, &c.id).or_else(|| item(first, n, &c.id))
                } else {
                    item(first, n, &c.id)
                }
            };
            let first_tally = tally(setup.participants.iter().map(|n| item(first, n, &c.id)));
            let final_tally = tally(setup.participants.iter().map(final_item));
            let m = setup.participants.len() as u32;
            let origins = setup
                .answers
                .iter()
                .filter(|a| c.source_answer_ids.contains(&a.message_id))
                .map(|a| &a.nick)
                .collect::<BTreeSet<_>>();
            let others = setup
                .participants
                .iter()
                .filter(|n| !origins.contains(n))
                .collect::<Vec<_>>();
            let mut bands = vec![];
            if final_tally.agree >= m.saturating_sub(1)
                && final_tally.unsure == 0
                && final_tally.missing == 0
            {
                bands.push(SurveyBand::Consensus);
            }
            if final_tally.agree > 0 && final_tally.disagree > 0 {
                bands.push(SurveyBand::Contested);
            }
            if origins.len() == 1
                && others
                    .iter()
                    .all(|n| final_item(n).is_some_and(|v| v.vote == SurveyChoice::Agree))
            {
                bands.push(SurveyBand::LoneInsights);
            }
            if final_tally.disagree == 1 {
                bands.push(SurveyBand::LoneDissents);
            }
            if (1..=2).contains(&origins.len())
                && !others.is_empty()
                && others
                    .iter()
                    .all(|n| final_item(n).is_some_and(|v| v.vote == SurveyChoice::Disagree))
            {
                bands.push(SurveyBand::RejectedOutliers);
            }
            let switches = if selected.contains(&c.id) {
                setup
                    .participants
                    .iter()
                    .filter_map(|n| {
                        let a = item(first, n, &c.id)?;
                        let b = item(second, n, &c.id)?;
                        (a.vote != b.vote).then(|| SurveySwitch {
                            voter: n.clone(),
                            from: a.vote,
                            to: b.vote,
                        })
                    })
                    .collect()
            } else {
                vec![]
            };
            let not_revoted = if selected.contains(&c.id) {
                setup
                    .participants
                    .iter()
                    .filter(|n| item(second, n, &c.id).is_none())
                    .cloned()
                    .collect()
            } else {
                vec![]
            };
            SurveyClaimMap {
                claim_id: c.id.clone(),
                first: first_tally,
                final_tally,
                not_revoted,
                switches,
                bands,
            }
        })
        .collect()
}
