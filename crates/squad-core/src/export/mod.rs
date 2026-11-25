//! Deterministic Markdown from one committed read snapshot.
//!
//! Store adapters must load all events and public artifact projections in one read
//! transaction. Neither this renderer nor `ExportSource` opens the room runtime.
//! Opaque generic artifact/report data is deliberately never serialized here.
pub mod census;
mod crosscheck;
mod probe;
pub mod survey;
mod tournament;
use crate::banner::{Banner, render_default};
use squad_protocol::*;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Write};

/// A store adapter supplies a consistent, committed snapshot, without a runtime lock.
pub trait ExportSource {
    fn snapshot(&self) -> Result<ExportSnapshot, ExportError>;
}

#[derive(Debug, Clone, Default)]
pub struct ExportSnapshot {
    pub events: Vec<CommittedEnvelope>,
    /// Explicit public projections of committed tournament/grill artifacts.
    /// Keys must identify an artifact/report event in `events`.
    pub artifacts: BTreeMap<EventId, PublicArtifact>,
    /// Mode adapter derives these only from owning committed generic state boundaries.
    pub boundaries: BTreeMap<EventId, Banner>,
}

/// Only human-visible mode fields belong here. No settings, prompts, label maps,
/// work metadata, or arbitrary JSON may cross this projection boundary.
#[derive(Debug, Clone)]
pub enum PublicArtifact {
    Tournament {
        round: u32,
        entries: Vec<(MemberNick, String)>,
        rankings: Vec<Ranking>,
        champion: Option<MemberNick>,
    },
    Grill {
        plans: Vec<PlanVersion>,
        questions: Vec<GrillQuestion>,
    },
}
#[derive(Debug, Clone)]
pub struct Ranking {
    pub nick: MemberNick,
    pub placement: u32,
    pub score: f64,
    pub critique: String,
    pub eliminated: bool,
    pub failed_replay: bool,
}
#[derive(Debug, Clone)]
pub struct PlanVersion {
    pub version: u32,
    pub author: MemberNick,
    pub content: String,
    pub final_plan: bool,
}
#[derive(Debug, Clone)]
pub struct GrillQuestion {
    pub round: u32,
    pub griller: MemberNick,
    pub question: String,
    pub leader: MemberNick,
    pub answer: String,
    pub plan_changes: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportError {
    RoomIdForbidden,
    ContestIdRequired,
    ContestNotFound,
    ScopeMismatch,
    UnsupportedScope,
    InvalidSnapshot,
    ReadFailed,
}
impl fmt::Display for ExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::RoomIdForbidden => "room export does not accept an ID",
            Self::ContestIdRequired => "contest export requires an ID",
            Self::ContestNotFound => "contest not found",
            Self::ScopeMismatch => "contest does not match export scope",
            Self::UnsupportedScope => "export scope is not implemented",
            Self::InvalidSnapshot => "invalid committed export snapshot",
            Self::ReadFailed => "could not read export snapshot",
        })
    }
}
impl std::error::Error for ExportError {}

/// Shared live/offline entry point. The source controls read transaction ownership.
pub fn export(
    source: &impl ExportSource,
    scope: ExportScope,
    id: Option<ContestId>,
) -> Result<String, ExportError> {
    check_scope(&scope, id)?;
    render(&source.snapshot()?, scope, id)
}
/// Read-only adapter entry point; no model, runtime, credential, or terminal access.
/// The store integration constructs the source from its data directory and room.
pub fn export_offline(
    source: &impl ExportSource,
    scope: ExportScope,
    id: Option<ContestId>,
) -> Result<String, ExportError> {
    export(source, scope, id)
}
fn check_scope(scope: &ExportScope, id: Option<ContestId>) -> Result<(), ExportError> {
    if *scope == ExportScope::Room {
        return if id.is_some() {
            Err(ExportError::RoomIdForbidden)
        } else {
            Ok(())
        };
    }
    if id.is_none() {
        return Err(ExportError::ContestIdRequired);
    }
    Ok(())
}

/// Render only accepted human-visible fields. Input order is canonicalized by cursor.
pub fn render(
    snapshot: &ExportSnapshot,
    scope: ExportScope,
    id: Option<ContestId>,
) -> Result<String, ExportError> {
    check_scope(&scope, id)?;
    let mut events: Vec<_> = snapshot.events.iter().collect();
    events.sort_by_key(|e| e.commit.seq);
    let mut ids = BTreeSet::new();
    let mut seqs = BTreeSet::new();
    for e in &events {
        if !e.content.is_durable() || !ids.insert(e.base.id) || !seqs.insert(e.commit.seq) {
            return Err(ExportError::InvalidSnapshot);
        }
    }
    // A snapshot may include clear history; only the current room epoch is visible.
    let start = events
        .iter()
        .rposition(|e| matches!(e.content, EnvelopePayload::RoomCleared(_)))
        .map_or(0, |i| i + 1);
    let events = &events[start..];
    let mut kinds = BTreeMap::new();
    for (id, kind) in events.iter().filter_map(|e| contest_kind(&e.content)) {
        if kinds
            .insert(id, kind)
            .is_some_and(|previous| previous != kind)
        {
            return Err(ExportError::InvalidSnapshot);
        }
    }
    for (event_id, artifact) in &snapshot.artifacts {
        let event = snapshot
            .events
            .iter()
            .find(|e| e.base.id == *event_id)
            .ok_or(ExportError::InvalidSnapshot)?;
        if !matches!(
            event.content,
            EnvelopePayload::ContestArtifact(_) | EnvelopePayload::ContestReport(_)
        ) {
            return Err(ExportError::InvalidSnapshot);
        }
        // Old-epoch projections are ignored with their events.
        if let Some(kind) = contest_id(&event.content).and_then(|id| kinds.get(&id))
            && !matches!(
                (kind, artifact),
                (ContestKind::Tournament, PublicArtifact::Tournament { .. })
                    | (ContestKind::Grill, PublicArtifact::Grill { .. })
            )
        {
            return Err(ExportError::InvalidSnapshot);
        }
    }
    if let Some(id) = id {
        let kind = kinds.get(&id).ok_or(ExportError::ContestNotFound)?;
        let expected = match scope {
            ExportScope::Debate => ContestKind::Debate,
            ExportScope::Tournament => ContestKind::Tournament,
            ExportScope::Grill => ContestKind::Grill,
            ExportScope::Census => ContestKind::Census,
            ExportScope::Crosscheck => ContestKind::Crosscheck,
            ExportScope::Survey => ContestKind::Survey,
            ExportScope::Probe => ContestKind::Probe,
            _ => unreachable!(),
        };
        if *kind != expected {
            return Err(ExportError::ScopeMismatch);
        }
    }
    let mut references = BTreeSet::new();
    for e in events {
        if id.is_some() && contest_id(&e.content) == id {
            match &e.content {
                EnvelopePayload::SurveySetup(s) => {
                    references.insert(s.question_message_id);
                    references.extend(s.answers.iter().map(|a| a.message_id));
                }
                EnvelopePayload::DebateSetup(s) => {
                    references.insert(s.topic_message_id);
                    references.extend(s.solutions.iter().map(|s| s.message_id));
                }
                EnvelopePayload::ContestCollecting(s) => {
                    references.insert(s.question_message_id);
                }
                EnvelopePayload::ContestPicked(s) => {
                    references.extend(&s.source_message_ids);
                }
                EnvelopePayload::ContestState(ContestState::Selection(s)) => {
                    references.insert(s.question_message_id);
                    references.extend(s.answers.iter().map(|a| a.message_id));
                }
                _ => {}
            }
        }
    }
    // Replies and regenerations are linked by logical message identity. Expand
    // within this contest's selected source chain without including other room replies.
    loop {
        let before = references.len();
        for e in events {
            match &e.content {
                EnvelopePayload::MessageFinal(s)
                    if references.contains(&s.in_reply_to_message_id) =>
                {
                    references.insert(s.completed.generation.message_id);
                }
                EnvelopePayload::MessageSuperseded(s)
                    if references.contains(&s.superseded_message_id) =>
                {
                    references.insert(s.replacement_message_id);
                }
                _ => {}
            }
        }
        if references.len() == before {
            break;
        }
    }
    let cases: BTreeSet<_> = events
        .iter()
        .filter_map(|e| match &e.content {
            EnvelopePayload::CaseOpened(s)
                if s.contest_id.or(s.debate_id) == id && id.is_some() =>
            {
                Some(s.case_id)
            }
            _ => None,
        })
        .collect();
    let superseded: BTreeSet<_> = events
        .iter()
        .filter_map(|e| match &e.content {
            EnvelopePayload::MessageSuperseded(s) => Some(s.superseded_message_id),
            _ => None,
        })
        .collect();
    let typed_crosscheck_work: BTreeSet<_> = events
        .iter()
        .filter_map(|e| match &e.content {
            EnvelopePayload::CrosscheckInventory(v) => Some(v.work_id),
            EnvelopePayload::CrosscheckReview(v) => Some(v.work_id),
            EnvelopePayload::CrosscheckAudit(v) => Some(v.work_id),
            _ => None,
        })
        .collect();
    let mut out = String::from("# Squad transcript\n\n");
    let mut boundaries: BTreeMap<ContestId, BTreeSet<String>> = BTreeMap::new();
    for e in events {
        let message_id = match &e.content {
            EnvelopePayload::MessageFinal(s) => Some(s.completed.generation.message_id),
            EnvelopePayload::UserMessage(s) => Some(s.input.message_id),
            _ => None,
        };
        if id.is_some()
            && contest_id(&e.content) != id
            && !message_id.is_some_and(|m| references.contains(&m))
            && !matches!(&e.content, EnvelopePayload::CaseRuling(s) if cases.contains(&s.case_id))
            && !matches!(&e.content, EnvelopePayload::UserMessage(s) if s.case_id.is_some_and(|case| cases.contains(&case)))
        {
            continue;
        }
        if census::render(&mut out, &e.content) {
            continue;
        }
        if let EnvelopePayload::ContestArtifact(a) = &e.content
            && a.work_id
                .is_some_and(|w| !typed_crosscheck_work.contains(&w))
            && let Some(text) = crosscheck::partial(a, &e.base.sender)
        {
            out.push_str(&text);
            continue;
        }
        if let Some(text) = crosscheck::render(&e.content) {
            out.push_str(&text);
            continue;
        }
        if let Some(text) = survey::render(&e.content) {
            out.push_str(&text);
            continue;
        }
        if probe::render(&mut out, &e.content) {
            continue;
        }
        match &e.content {
            EnvelopePayload::UserMessage(s) => {
                heading(
                    &mut out,
                    &format!("{} · {}", e.base.sender, s.input.message_id),
                );
                for q in &s.resolved_quotes {
                    block(
                        &mut out,
                        &format!("Quote from {} · {}", q.author, q.event_id),
                        &q.text,
                    );
                }
                paragraph(&mut out, &s.body);
            }
            EnvelopePayload::MessageFinal(s) => {
                heading(
                    &mut out,
                    &format!(
                        "{} · {}{}",
                        e.base.sender,
                        s.completed.generation.message_id,
                        if superseded.contains(&s.completed.generation.message_id) {
                            " · superseded"
                        } else {
                            ""
                        }
                    ),
                );
                paragraph(&mut out, &s.content);
            }
            EnvelopePayload::DebateSetup(s) => {
                heading(&mut out, &format!("Debate {}", s.debate_id));
                block(&mut out, "Topic", &s.topic_text);
                paragraph(&mut out, &format!("Judge: {}", s.judge_model));
                for solution in &s.solutions {
                    block(
                        &mut out,
                        &format!("Original solution · {}", solution.nick),
                        &solution.text,
                    );
                }
            }
            EnvelopePayload::DebateState(s)
            | EnvelopePayload::ContestState(ContestState::Debate(s)) => {
                let boundary = if s.state == DebateStatus::RoundRunning {
                    Some(Banner::debate_round(
                        s.round as usize,
                        s.total_rounds as usize,
                    ))
                } else if s.state == DebateStatus::Judging {
                    Some(Banner::Verdict)
                } else {
                    None
                };
                if let Some(b) = boundary {
                    let seen = boundaries.entry(s.debate_id).or_default();
                    if seen.insert(b.text()) {
                        banner(&mut out, b);
                    }
                }
                if s.state.is_terminal() {
                    block(
                        &mut out,
                        &format!("Debate {} · {:?}", s.debate_id, s.state),
                        s.terminal_reason.as_deref().unwrap_or(""),
                    );
                }
            }
            EnvelopePayload::DebateTurn(s) => {
                heading(
                    &mut out,
                    &format!("{} · debate {} · round {}", s.speaker, s.debate_id, s.round),
                );
                match &s.outcome {
                    DebateTurnOutcome::Completed { content, .. } => paragraph(&mut out, content),
                    DebateTurnOutcome::Forfeit { reason_code, text } => {
                        paragraph(&mut out, &format!("Forfeit ({reason_code:?}): {text}"))
                    }
                }
            }
            EnvelopePayload::DebateRoundScore(s) => {
                heading(
                    &mut out,
                    &format!("Debate {} · round {} score", s.debate_id, s.round),
                );
                if let Some(scores) = &s.scores {
                    for (nick, score) in scores {
                        paragraph(
                            &mut out,
                            &format!("{nick}: {}/10 — {}", score.score, score.reason),
                        );
                    }
                } else {
                    paragraph(&mut out, "Unscored");
                }
                for (nick, tally) in &s.tally {
                    paragraph(&mut out, &format!("Tally · {nick}: {tally}"));
                }
            }
            EnvelopePayload::Verdict(s) => {
                let v = &s.verdict;
                heading(
                    &mut out,
                    &format!(
                        "Verdict · debate {} · judge {}",
                        s.debate_id, s.completed.model
                    ),
                );
                paragraph(&mut out, &format!("Winner: {}", v.verdict.winner));
                // These are committed public adjudication reasons, not provider reasoning traces.
                block(&mut out, "Decision", &v.verdict.reasoning);
                for score in &v.verdict.scores {
                    for c in &score.criteria {
                        block(
                            &mut out,
                            &format!("{} · {:?} · {}/10", score.nick, c.criterion_id, c.score),
                            &c.reasoning,
                        );
                    }
                }
                for total in &v.weighted_totals {
                    paragraph(
                        &mut out,
                        &format!("Weighted total · {}: {}", total.nick, total.total),
                    );
                }
                for (nick, tally) in &v.round_tally {
                    paragraph(&mut out, &format!("roundTally · {nick}: {tally}"));
                }
                block(
                    &mut out,
                    "tallyOverrideReason",
                    v.verdict.tally_override_reason.as_deref().unwrap_or("None"),
                );
                for suggestion in &v.verdict.what_to_steal_from_loser {
                    block(&mut out, "What to steal from loser", suggestion);
                }
            }
            payload if tournament::render(&mut out, payload) => {
                if let EnvelopePayload::TournamentEntry(entry) = payload {
                    let failed=events.iter().any(|e|matches!(&e.content,EnvelopePayload::TournamentState(s) if s.contest_id==entry.contest_id && ((s.round==entry.round && s.replay>entry.replay) || (s.state==TournamentStatus::Failed && s.round==entry.round && s.replay==entry.replay))));
                    if failed {
                        paragraph(
                            &mut out,
                            "Failed replay run: no accepted ranking or elimination from this execution.",
                        );
                    }
                }
            }
            EnvelopePayload::CaseOpened(s) => {
                if let CaseMaterial::Standalone { messages, question } = &s.material {
                    block(
                        &mut out,
                        &format!("Case {} · judge {}", s.case_id, s.judge_model),
                        question.as_str(),
                    );
                    for q in messages {
                        block(
                            &mut out,
                            &format!("Source · {} · {}", q.author, q.event_id),
                            &q.text,
                        );
                    }
                }
            }
            EnvelopePayload::CaseRuling(s) => block(
                &mut out,
                &format!("Judge {} · case {}", s.completed.model, s.case_id),
                s.content.as_str(),
            ),
            EnvelopePayload::ContestCollecting(s) => block(
                &mut out,
                &format!("{} {} · question", s.kind.as_str(), s.contest_id),
                &s.question,
            ),
            EnvelopePayload::ContestPickRequested(s) => paragraph(
                &mut out,
                &format!(
                    "Pick {} for {}: {}",
                    s.required_picks,
                    s.contest_id,
                    s.eligible
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            ),
            EnvelopePayload::ContestPicked(s) => paragraph(
                &mut out,
                &format!(
                    "Selected for {}: {}",
                    s.contest_id,
                    s.nicks
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            ),
            EnvelopePayload::ContestState(ContestState::Selection(s)) => {
                heading(
                    &mut out,
                    &format!("Selection {} · {:?}", s.contest_id, s.state),
                );
                for a in &s.answers {
                    block(
                        &mut out,
                        &format!("Answer · {} · {}", a.nick, a.message_id),
                        &a.text,
                    );
                }
            }
            EnvelopePayload::ContestState(ContestState::Generic(s)) => {
                if let Some(b) = snapshot.boundaries.get(&e.base.id) {
                    let valid = matches!(
                        (s.kind, b),
                        (
                            ContestKind::Tournament,
                            Banner::Tournament
                                | Banner::TournamentRound { .. }
                                | Banner::Final
                                | Banner::Champion
                        ) | (
                            ContestKind::Grill,
                            Banner::Grill
                                | Banner::GrillRound(_)
                                | Banner::FinalPlan
                                | Banner::Survived
                        )
                    );
                    if !valid {
                        return Err(ExportError::InvalidSnapshot);
                    }
                    if boundaries.entry(s.contest_id).or_default().insert(b.text()) {
                        banner(&mut out, *b);
                    }
                }
                heading(
                    &mut out,
                    &format!("{} {} · {}", s.kind.as_str(), s.contest_id, s.status),
                );
                if let Some(reason) = &s.terminal_reason {
                    paragraph(&mut out, reason.as_str());
                }
            }
            EnvelopePayload::ContestArtifact(_) | EnvelopePayload::ContestReport(_) => {
                if let Some(artifact) = snapshot.artifacts.get(&e.base.id) {
                    render_artifact(&mut out, artifact);
                }
            }
            EnvelopePayload::GrillSetup(v) => {
                banner(&mut out, Banner::Grill);
                heading(
                    &mut out,
                    &format!("Grill {} · leader @{}", v.contest_id, v.leader),
                );
                paragraph(
                    &mut out,
                    &format!(
                        "Shared floor: {}",
                        v.participants()
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                );
                paragraph(&mut out, &v.focus);
            }
            EnvelopePayload::GrillState(v) => {
                heading(
                    &mut out,
                    &format!("Grill round {} · {:?}", v.round, v.state),
                );
                if let Some(reason) = &v.terminal_reason {
                    paragraph(&mut out, reason);
                }
            }
            EnvelopePayload::GrillPlan(v) => block(
                &mut out,
                &format!("Plan v{} · round {}", v.version, v.round),
                &v.text,
            ),
            EnvelopePayload::GrillQuestion(v) => {
                heading(
                    &mut out,
                    &format!(
                        "Round {} · @{} asks @{} · {}",
                        v.round, v.griller, v.leader, v.angle
                    ),
                );
                for (i, q) in v.questions.iter().enumerate() {
                    block(&mut out, &format!("Question {}", i + 1), q);
                }
            }
            EnvelopePayload::GrillAnswer(v) => {
                heading(
                    &mut out,
                    &format!("@{} answers · {}", v.leader, v.question_event_id),
                );
                for answer in &v.answers {
                    block(
                        &mut out,
                        &format!("Answer {}", answer.question_index + 1),
                        &answer.answer,
                    );
                    block(&mut out, "Plan changes", &answer.plan_changes);
                }
            }
            EnvelopePayload::GrillPass(v) => heading(
                &mut out,
                &format!(
                    "Round {} · @{} · {}",
                    v.round,
                    v.griller,
                    match v.reason {
                        squad_protocol::GrillPassReason::Pass => "PASS",
                        squad_protocol::GrillPassReason::Timeout => "SKIPPED (timeout)",
                        squad_protocol::GrillPassReason::Error => "SKIPPED (error)",
                    }
                ),
            ),
            EnvelopePayload::GrillComplete(v) => {
                heading(
                    &mut out,
                    &format!(
                        "FINAL PLAN v{} · {}",
                        v.final_plan_version, v.final_plan_event_id
                    ),
                );
                if let Some(plan) = events.iter().find_map(|e| match &e.content {
                    EnvelopePayload::GrillPlan(p) if e.base.id == v.final_plan_event_id => Some(p),
                    _ => None,
                }) {
                    paragraph(&mut out, &plan.text);
                }
                heading(&mut out, "Complete attributed question-and-answer appendix");
                for item in &v.appendix {
                    block(
                        &mut out,
                        &format!("@{} · question {}", item.griller, item.question_index + 1),
                        &item.question,
                    );
                    if let Some(answer) = &item.answer {
                        block(&mut out, "Answer", answer);
                    }
                    if let Some(changes) = &item.plan_changes {
                        block(&mut out, "Plan changes", changes);
                    }
                }
            }
            EnvelopePayload::SystemNotice(s) => block(&mut out, "Notice", s.text.as_str()),
            _ => {} // Explicit allowlist: private records and generic JSON never render.
        }
    }
    Ok(out)
}
fn contest_kind(p: &EnvelopePayload) -> Option<(ContestId, ContestKind)> {
    Some(match p {
        EnvelopePayload::TournamentSetup(s) => (s.contest_id, ContestKind::Tournament),
        EnvelopePayload::TournamentState(s) => (s.contest_id, ContestKind::Tournament),
        EnvelopePayload::ContestState(ContestState::Tournament(s)) => {
            (s.contest_id, ContestKind::Tournament)
        }
        EnvelopePayload::CensusSetup(s) => (s.contest_id, ContestKind::Census),
        EnvelopePayload::CensusState(s)
        | EnvelopePayload::ContestState(ContestState::Census(s)) => {
            (s.contest_id, ContestKind::Census)
        }
        EnvelopePayload::CrosscheckSetup(s) => (s.contest_id, ContestKind::Crosscheck),
        EnvelopePayload::CrosscheckState(s) => (s.contest_id, ContestKind::Crosscheck),
        EnvelopePayload::SurveySetup(s) => (s.contest_id, ContestKind::Survey),
        EnvelopePayload::SurveyState(s) => (s.contest_id, ContestKind::Survey),
        EnvelopePayload::ContestState(ContestState::Survey(s)) => {
            (s.contest_id, ContestKind::Survey)
        }
        EnvelopePayload::GrillSetup(s) => (s.contest_id, ContestKind::Grill),
        EnvelopePayload::GrillState(s) => (s.contest_id, ContestKind::Grill),
        EnvelopePayload::GrillQuestion(s) => (s.contest_id, ContestKind::Grill),
        EnvelopePayload::GrillAnswer(s) => (s.contest_id, ContestKind::Grill),
        EnvelopePayload::GrillPass(s) => (s.contest_id, ContestKind::Grill),
        EnvelopePayload::GrillPlan(s) => (s.contest_id, ContestKind::Grill),
        EnvelopePayload::GrillComplete(s) => (s.contest_id, ContestKind::Grill),
        EnvelopePayload::ProbeSetup(s) => (s.contest_id, ContestKind::Probe),
        EnvelopePayload::ProbeState(s) | EnvelopePayload::ContestState(ContestState::Probe(s)) => {
            (s.contest_id, ContestKind::Probe)
        }
        EnvelopePayload::DebateSetup(s) => (s.debate_id, ContestKind::Debate),
        EnvelopePayload::DebateState(s) => (s.debate_id, ContestKind::Debate),
        EnvelopePayload::ContestSetup(s) => (s.contest_id, s.kind),
        EnvelopePayload::ContestCollecting(s) => (s.contest_id, s.kind),
        EnvelopePayload::ContestState(ContestState::Generic(s)) => (s.contest_id, s.kind),
        EnvelopePayload::ContestState(ContestState::Selection(s)) => (s.contest_id, s.kind),
        EnvelopePayload::ContestState(ContestState::Debate(s)) => {
            (s.debate_id, ContestKind::Debate)
        }
        _ => return None,
    })
}
fn contest_id(p: &EnvelopePayload) -> Option<ContestId> {
    if let Some((id, _)) = contest_kind(p) {
        return Some(id);
    }
    match p {
        EnvelopePayload::TournamentPreflight(s) => Some(s.base.contest_id),
        EnvelopePayload::TournamentEntry(s) => Some(s.contest_id),
        EnvelopePayload::TournamentRanking(s) => Some(s.contest_id),
        EnvelopePayload::TournamentResult(s) => Some(s.contest_id),
        EnvelopePayload::TournamentChampion(s) => Some(s.contest_id),
        EnvelopePayload::CensusPlan(s) => Some(s.contest_id),
        EnvelopePayload::CensusHunt(s) => Some(s.contest_id),
        EnvelopePayload::CensusLedger(s) => Some(s.contest_id),
        EnvelopePayload::CensusRating(s) => Some(s.contest_id),
        EnvelopePayload::CensusReport(s) => Some(s.contest_id),
        EnvelopePayload::CrosscheckGoal(s) => Some(s.contest_id),
        EnvelopePayload::CrosscheckInventory(s) => Some(s.contest_id),
        EnvelopePayload::CrosscheckGrid(s) => Some(s.contest_id),
        EnvelopePayload::CrosscheckReview(s) => Some(s.contest_id),
        EnvelopePayload::CrosscheckAudit(s) => Some(s.contest_id),
        EnvelopePayload::CrosscheckRuling(s) => Some(s.contest_id),
        EnvelopePayload::CrosscheckAcceptance(s) => Some(s.contest_id),
        EnvelopePayload::CrosscheckReport(s) => Some(s.contest_id),
        EnvelopePayload::CrosscheckPreflight(s) => Some(s.base.contest_id),
        EnvelopePayload::SurveyClaims(s) => Some(s.contest_id),
        EnvelopePayload::SurveyVote(s) | EnvelopePayload::SurveyRevote(s) => Some(s.contest_id),
        EnvelopePayload::SurveySelection(s) => Some(s.contest_id),
        EnvelopePayload::SurveyReport(s) => Some(s.contest_id),
        EnvelopePayload::ProbeExplanation(s) => Some(s.contest_id),
        EnvelopePayload::ProbeCheck(s) => Some(s.contest_id),
        EnvelopePayload::ProbePrediction(s) => Some(s.contest_id),
        EnvelopePayload::ProbeReport(s) => Some(s.contest_id),
        EnvelopePayload::ProbePreflight(s) => Some(s.setup.contest_id),
        EnvelopePayload::UserMessage(s) => s.contest_id.or(s.debate_id),
        EnvelopePayload::DebateTurn(s) => Some(s.debate_id),
        EnvelopePayload::DebateOpening(s) => Some(s.debate_id),
        EnvelopePayload::DebateRoundScore(s) => Some(s.debate_id),
        EnvelopePayload::Verdict(s) => Some(s.debate_id),
        EnvelopePayload::CaseOpened(s) => s.contest_id.or(s.debate_id),
        EnvelopePayload::ContestPickRequested(s) => Some(s.contest_id),
        EnvelopePayload::ContestPicked(s) => Some(s.contest_id),
        EnvelopePayload::ContestArtifact(s) => Some(s.contest_id),
        EnvelopePayload::ContestReport(s) => Some(s.contest_id),
        EnvelopePayload::SystemNotice(s) => s.contest_id.or(s.debate_id),
        _ => None,
    }
}
fn heading(out: &mut String, text: &str) {
    let _ = writeln!(out, "## {}\n", text.replace(['\n', '\r'], " "));
}
fn paragraph(out: &mut String, text: &str) {
    let _ = writeln!(out, "{}\n", text.trim_end());
}
fn block(out: &mut String, title: &str, text: &str) {
    heading(out, title);
    paragraph(out, text);
}
fn banner(out: &mut String, b: Banner) {
    let _ = writeln!(out, "```text\n{}\n```\n", render_default(&b.text()));
}
fn render_artifact(out: &mut String, artifact: &PublicArtifact) {
    match artifact {
        PublicArtifact::Tournament {
            round,
            entries,
            rankings,
            champion,
        } => {
            heading(out, &format!("Tournament · round {round}"));
            for (nick, content) in entries {
                block(out, &format!("Entry · {nick}"), content);
            }
            for r in rankings {
                block(
                    out,
                    &format!(
                        "Placement {} · {} · score {}{}{}",
                        r.placement,
                        r.nick,
                        r.score,
                        if r.eliminated { " · eliminated" } else { "" },
                        if r.failed_replay {
                            " · failed replay"
                        } else {
                            ""
                        }
                    ),
                    &r.critique,
                );
            }
            if let Some(nick) = champion {
                block(out, "Champion", nick.as_str());
            }
        }
        PublicArtifact::Grill { plans, questions } => {
            for p in plans {
                block(
                    out,
                    &format!(
                        "{} · version {} · {}",
                        if p.final_plan { "Final plan" } else { "Plan" },
                        p.version,
                        p.author
                    ),
                    &p.content,
                );
            }
            heading(out, "Question-and-answer appendix");
            for q in questions {
                block(
                    out,
                    &format!("Round {} · {} asks", q.round, q.griller),
                    &q.question,
                );
                block(out, &format!("{} answers", q.leader), &q.answer);
                block(out, "Resulting plan changes", &q.plan_changes);
            }
        }
    }
}

/// Read-only SQLite source usable while an interactive process holds the room lock.
#[derive(Debug, Clone)]
pub struct StoreExportSource {
    path: std::path::PathBuf,
}
impl StoreExportSource {
    pub fn new(path: impl Into<std::path::PathBuf>) -> Self {
        Self { path: path.into() }
    }
}
impl ExportSource for StoreExportSource {
    fn snapshot(&self) -> Result<ExportSnapshot, ExportError> {
        let mut db = rusqlite::Connection::open_with_flags(
            &self.path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .map_err(|_| ExportError::ReadFailed)?;
        let events = squad_store::export::room(&mut db).map_err(|_| ExportError::ReadFailed)?;
        Ok(ExportSnapshot {
            events,
            ..Default::default()
        })
    }
}
impl ExportSource for squad_store::Store {
    fn snapshot(&self) -> Result<ExportSnapshot, ExportError> {
        self.call(|db| {
            let events = squad_store::export::room(db)?;
            Ok(ExportSnapshot {
                events,
                ..Default::default()
            })
        })
        .map_err(|_| ExportError::ReadFailed)
    }
}
