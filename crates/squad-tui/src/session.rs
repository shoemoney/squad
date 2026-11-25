//! Room boundary and deterministic projection. Subscribe before taking a snapshot;
//! queued duplicate events are reconciled by canonical identity after restoration.
use crate::{
    fx::{FeedOrigin, FxCue, FxQueue},
    transcript::Transcript,
};
use squad_protocol::{
    parser::{ParseContext, PendingPick, QuestionFirstMode, RecordSummary},
    *,
};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::{broadcast, watch};

pub type RoomFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
#[derive(Debug, Clone, PartialEq)]
pub struct RoomError {
    pub code: ErrorCode,
    pub message: String,
}
impl std::fmt::Display for RoomError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for RoomError {}
#[derive(Debug, Clone, Default)]
pub struct DailySpendView {
    pub day: String,
    pub spent_usd: f64,
    pub reserved_usd: f64,
    pub limit_usd: f64,
    pub unknown_calls: u32,
}
impl DailySpendView {
    pub fn label(&self) -> String {
        format!(
            "Today {}${:.4} / ${:.2} · pending ${:.4} · remaining ${:.4}",
            if self.unknown_calls > 0 { "~" } else { "" },
            self.spent_usd,
            self.limit_usd,
            self.reserved_usd,
            (self.limit_usd - self.spent_usd - self.reserved_usd).max(0.)
        )
    }
}
/// Temporary RoomState projection until M3 exposes its concrete type. All values
/// here are authoritative snapshot values; the TUI never derives daily totals.
#[derive(Debug, Clone, Default)]
pub struct RoomSnapshot {
    pub cursor: Option<Cursor>,
    pub context: ParseContext,
    pub participants: Vec<Presence>,
    pub models: Vec<Model>,
    pub preflights: Vec<PendingPreflight>,
    pub daily_spend: DailySpendView,
    pub trophies: BTreeMap<Nick, u64>,
    pub scoreboard: Vec<Score>,
    pub events: Vec<CommittedEnvelope>,
}
/// M3 implements this port for RoomHandle. Receivers report lag explicitly.
/// A subscribe cursor requests committed events strictly after that position;
/// transient deltas are never replayed. Paging is newest-first, exclusive before.
#[derive(Debug, Clone)]
pub struct FeedDelivery {
    pub item: FeedItem,
    pub origin: FeedOrigin,
}
pub struct RoomFeed {
    replay: VecDeque<FeedItem>,
    live: broadcast::Receiver<FeedItem>,
}
impl RoomFeed {
    pub fn new(live: broadcast::Receiver<FeedItem>, replay: Vec<CommittedEnvelope>) -> Self {
        Self {
            replay: replay.into_iter().map(FeedItem::Committed).collect(),
            live,
        }
    }
    pub async fn recv(&mut self) -> Result<FeedDelivery, broadcast::error::RecvError> {
        if let Some(item) = self.replay.pop_front() {
            return Ok(FeedDelivery {
                item,
                origin: FeedOrigin::Replay,
            });
        }
        self.live.recv().await.map(|item| FeedDelivery {
            item,
            origin: FeedOrigin::Live,
        })
    }
    pub fn try_recv(&mut self) -> Result<FeedDelivery, broadcast::error::TryRecvError> {
        if let Some(item) = self.replay.pop_front() {
            return Ok(FeedDelivery {
                item,
                origin: FeedOrigin::Replay,
            });
        }
        self.live.try_recv().map(|item| FeedDelivery {
            item,
            origin: FeedOrigin::Live,
        })
    }
}
pub trait RoomPort: Send + Sync {
    fn submit(&self, request: Request) -> RoomFuture<'_, RequestResult>;
    fn subscribe(&self, after: Option<Cursor>) -> Result<RoomFeed, RoomError>;
    fn page_events(
        &self,
        before: Option<Cursor>,
        limit: usize,
    ) -> RoomFuture<'_, Result<Vec<CommittedEnvelope>, RoomError>>;
    fn state(&self) -> RoomFuture<'_, RoomSnapshot>;
    fn export(
        &self,
        scope: ExportScope,
        id: Option<ContestId>,
    ) -> RoomFuture<'_, Result<String, RoomError>>;
    fn shutdown(&self) -> RoomFuture<'_, Result<(), RoomError>>;
}
#[derive(Debug, Clone, Default)]
pub struct ContestView {
    pub state: Option<ContestState>,
    pub setup: Option<ContestSetup>,
    pub artifacts: BTreeMap<EventId, ContestArtifact>,
    pub reports: BTreeMap<EventId, ContestReport>,
    pub debate: Option<DebateState>,
    pub scores: BTreeMap<u32, DebateRoundScore>,
    pub verdict: Option<DebateVerdict>,
}
#[derive(Debug, Clone)]
pub struct PickView {
    pub request: ContestPickRequested,
}
#[derive(Debug, Default)]
pub struct Session {
    pub presentation_epoch: u64,
    pub snapshot: RoomSnapshot,
    pub transcript: Transcript,
    pub contests: BTreeMap<ContestId, ContestView>,
    pub pick: Option<PickView>,
    pub seen: BTreeSet<EventId>,
    pub last_error: Option<String>,
    pub debates: BTreeMap<ContestId, crate::widgets::debate::DebateView>,
    pub tournaments: BTreeMap<ContestId, crate::widgets::tournament::TournamentView>,
    pub debate_messages: BTreeSet<MessageId>,
    pub debate_streams: BTreeMap<MessageId, Delta>,
    pub cases: BTreeMap<CaseId, crate::widgets::case::CaseView>,
    pub stream_started: BTreeMap<MessageId, Duration>,
    pub stream_rates: BTreeMap<Nick, f64>,
}
impl Session {
    pub fn restore(&mut self, mut snapshot: RoomSnapshot) {
        self.presentation_epoch = self.presentation_epoch.wrapping_add(1);
        let anchor = self.transcript.anchor;
        let follow = self.transcript.follow;
        let events = std::mem::take(&mut snapshot.events);
        self.snapshot = snapshot;
        self.seen.clear();
        self.contests.clear();
        self.debates.clear();
        self.tournaments.clear();
        self.tournaments.clear();
        self.debate_streams.clear();
        self.debate_messages.clear();
        self.cases.clear();
        self.pick = None;
        self.transcript.clear();
        // Reduce history, then restore the newer authoritative room snapshot.
        let authoritative = self.snapshot.clone();
        let mut queue = FxQueue::new();
        let mut events = events;
        events.sort_by_key(|e| e.commit.seq);
        for event in events {
            self.reduce(
                FeedItem::Committed(event),
                FeedOrigin::Replay,
                Duration::ZERO,
                &mut queue,
            );
        }
        self.snapshot.context = authoritative.context;
        self.snapshot.cursor = authoritative.cursor;
        self.snapshot.participants = authoritative.participants;
        self.snapshot.preflights = authoritative.preflights;
        self.snapshot.daily_spend = authoritative.daily_spend;
        self.snapshot.trophies = authoritative.trophies;
        self.snapshot.scoreboard = authoritative.scoreboard;
        if let Some(state) = self.snapshot.context.active_contest.clone() {
            self.restore_contest(state);
        }
        if self.snapshot.context.active_contest.is_none()
            && self.snapshot.context.pending_pick.is_none()
        {
            self.pick = None;
        }
        self.transcript.anchor = anchor.filter(|(id, _)| self.transcript.cards.contains_key(id));
        self.transcript.follow = follow;
    }
    /// Add an older durable page while preserving newer authoritative state,
    /// reading anchor and the caller-owned pending request/draft.
    pub async fn page_older(&mut self, room: &dyn RoomPort) -> Result<usize, RoomError> {
        let before = self.snapshot.events.iter().map(|e| e.commit.seq).min();
        let page = room.page_events(before, 256).await?;
        let count = page
            .iter()
            .filter(|e| !self.seen.contains(&e.base.id))
            .count();
        let mut snapshot = self.snapshot.clone();
        snapshot.events.extend(page);
        self.restore(snapshot);
        Ok(count)
    }
    fn restore_contest(&mut self, state: ContestState) {
        let id = match &state {
            ContestState::Generic(s) => s.contest_id,
            ContestState::Debate(s) => s.debate_id,
            ContestState::Tournament(s) => s.contest_id,
            ContestState::Selection(s) => {
                if s.state == ContestSelectionStatus::AwaitingPick {
                    let eligible = self
                        .snapshot
                        .context
                        .pending_pick
                        .as_ref()
                        .filter(|p| p.contest_id == s.contest_id)
                        .map(|p| p.eligible.clone())
                        .unwrap_or_else(|| s.answers.iter().map(|a| a.nick.clone()).collect());
                    self.pick = Some(PickView {
                        request: ContestPickRequested {
                            contest_id: s.contest_id,
                            kind: s.kind,
                            question_message_id: s.question_message_id,
                            eligible: eligible.clone(),
                            required_picks: s.required_picks,
                            banner: String::new(),
                        },
                    });
                    self.snapshot.context.pending_pick = Some(PendingPick {
                        contest_id: s.contest_id,
                        mode: if s.kind == ContestKind::Grill {
                            QuestionFirstMode::Grill
                        } else {
                            QuestionFirstMode::Debate
                        },
                        eligible,
                    });
                } else {
                    self.pick = None;
                    self.snapshot.context.pending_pick = None;
                }
                s.contest_id
            }
        };
        let view = self.contests.entry(id).or_default();
        if let ContestState::Debate(s) = &state {
            view.debate = Some(s.clone());
            self.debates
                .entry(id)
                .and_modify(|v| {
                    v.apply_state(s, &DebateTransition::RoundEntered);
                    v.verdict_entered |=
                        matches!(s.state, DebateStatus::Judging | DebateStatus::Completed);
                })
                .or_insert_with(|| s.into());
        }
        view.state = Some(state);
    }
    pub fn active_contest(&self) -> bool {
        self.snapshot.context.active_contest.is_some()
    }
    /// Returns whether static content changed immediately. Stream deltas become
    /// visible on their 50 ms batching deadline, independent of feed pressure.
    pub fn reduce(
        &mut self,
        item: FeedItem,
        origin: FeedOrigin,
        now: Duration,
        queue: &mut FxQueue,
    ) -> bool {
        let (base, content, committed) = match &item {
            FeedItem::Committed(e) => (&e.base, &e.content, Some(&e.commit)),
            FeedItem::Delta(e) | FeedItem::Presence(e) => (&e.base, &e.content, None),
        };
        if let Some(commit) = committed {
            if !self.seen.insert(base.id) {
                return false;
            }
            self.snapshot.cursor = Some(
                self.snapshot
                    .cursor
                    .map_or(commit.seq, |c| c.max(commit.seq)),
            );
        }
        let generation = match content {
            EnvelopePayload::MessageFinal(m) | EnvelopePayload::AgentReply(m) => {
                Some(&m.completed.generation)
            }
            EnvelopePayload::DebateTurn(t) => Some(&t.generation),
            _ => None,
        };
        if generation.is_some_and(|g| {
            self.transcript
                .cards
                .get(&g.message_id)
                .is_some_and(|c| c.complete && c.attempt >= g.attempt)
        }) {
            return false;
        }
        let mut cue = None;
        let mut immediate = true;
        match content {
            EnvelopePayload::Delta(d) => {
                use std::collections::btree_map::Entry;
                // Durable terminal outcomes always win over delayed/repeated deltas.
                let idx = self.transcript.cards.entry(d.generation.message_id);
                let card = match idx {
                    Entry::Vacant(v) => v.insert(crate::transcript::ReplyCard {
                        id: d.generation.message_id,
                        work_id: Some(d.generation.work_id),
                        terminal: None,
                        author: base.sender.clone(),
                        attempt: d.generation.attempt,
                        text: String::new(),
                        pending_text: String::new(),
                        next_chunk: 0,
                        complete: false,
                    }),
                    Entry::Occupied(v) => v.into_mut(),
                };
                if d.generation.attempt < card.attempt
                    || card.complete && d.generation.attempt == card.attempt
                {
                    return false;
                }
                if d.generation.attempt > card.attempt {
                    card.attempt = d.generation.attempt;
                    card.work_id = Some(d.generation.work_id);
                    card.terminal = None;
                    card.text.clear();
                    card.pending_text.clear();
                    card.next_chunk = 0;
                    card.complete = false;
                }
                if d.chunk_index < card.next_chunk {
                    return false;
                }
                if card.text.is_empty()
                    && card.pending_text.is_empty()
                    && let Actor::Member(model) = &base.sender
                {
                    cue = Some(FxCue::BubbleOpened {
                        id: card.id,
                        model: model.clone(),
                    });
                }
                self.stream_started.entry(card.id).or_insert(now);
                if let Actor::Member(nick) = &base.sender {
                    let elapsed = now
                        .saturating_sub(self.stream_started[&card.id])
                        .as_secs_f64();
                    if elapsed > 0.1 {
                        self.stream_rates.insert(
                            nick.clone(),
                            (card.text.len() + card.pending_text.len()) as f64 / 4. / elapsed,
                        );
                    }
                }
                if d.debate_id.is_some()
                    || d.contest_id
                        .is_some_and(|id| self.tournaments.contains_key(&id))
                {
                    self.debate_messages.insert(card.id);
                    self.debate_streams.insert(card.id, d.clone());
                }
                self.transcript.revision = self.transcript.revision.wrapping_add(1);
                card.next_chunk = d.chunk_index + 1;
                card.pending_text.push_str(&d.text);
                self.transcript
                    .batch_deadline
                    .get_or_insert(now + Duration::from_millis(50));
                immediate = false;
            }
            EnvelopePayload::UserMessage(m) => {
                cue = Some(FxCue::Catalogue {
                    effect: crate::fx::CatalogueEffect::Dispatch,
                    target: m.input.message_id.to_string(),
                    duration: Duration::from_millis(600),
                });
                self.transcript.complete(
                    m.input.message_id,
                    base.sender.clone(),
                    0,
                    m.body.clone(),
                );
                self.record(
                    base,
                    committed,
                    "user.message",
                    Some(m.input.message_id),
                    None,
                    m.body.clone(),
                );
            }
            EnvelopePayload::MessageFinal(m) | EnvelopePayload::AgentReply(m) => {
                self.transcript.complete(
                    m.completed.generation.message_id,
                    base.sender.clone(),
                    m.completed.generation.attempt,
                    m.content.clone(),
                );
                cue = Some(FxCue::BubbleCompleted {
                    id: m.completed.generation.message_id,
                });
                self.record(
                    base,
                    committed,
                    "message.final",
                    Some(m.completed.generation.message_id),
                    Some(m.in_reply_to_message_id),
                    m.content.clone(),
                );
            }
            EnvelopePayload::WorkState(w)
                if matches!(w.status, WorkStatus::Failed | WorkStatus::Cancelled) =>
            {
                for card in self
                    .transcript
                    .cards
                    .values_mut()
                    .filter(|c| c.work_id == Some(w.work_id) && c.attempt == w.attempt)
                {
                    card.text.push_str(&std::mem::take(&mut card.pending_text));
                    card.complete = true;
                    card.terminal = Some(w.status.clone());
                    cue = Some(FxCue::BubbleFailed { id: card.id });
                }
                if let Some(error) = &w.error {
                    self.last_error = Some(error.text.to_string());
                }
            }
            EnvelopePayload::Presence(p) => {
                self.snapshot.participants.retain(|old| old.nick != p.nick);
                self.snapshot.participants.push(p.clone());
            }
            EnvelopePayload::VarChange(v) => {
                self.snapshot.context.vars.insert(v.name.clone(), v.value);
            }
            EnvelopePayload::BudgetState(b) => {
                self.snapshot.daily_spend = DailySpendView {
                    day: b.day.clone(),
                    spent_usd: b.spent_usd,
                    reserved_usd: b.reserved_usd,
                    limit_usd: b.limit_usd,
                    unknown_calls: b.unknown_calls,
                };
                cue = Some(FxCue::DailySpendChanged);
            }
            EnvelopePayload::ScoresChanged(s) => {
                self.snapshot.scoreboard = s.scores.clone();
                self.snapshot.trophies = s
                    .scores
                    .iter()
                    .map(|score| (score.nick.clone(), score.trophies))
                    .collect();
            }
            EnvelopePayload::TournamentSetup(s) => {
                self.tournaments
                    .entry(s.contest_id)
                    .or_default()
                    .apply_payload(content);
            }
            EnvelopePayload::TournamentState(s) => {
                self.tournaments
                    .entry(s.contest_id)
                    .or_default()
                    .apply_payload(content);
                self.restore_contest(ContestState::Tournament(s.clone()));
                self.snapshot.context.active_contest =
                    (!s.state.is_terminal()).then(|| ContestState::Tournament(s.clone()));
            }
            EnvelopePayload::TournamentEntry(e) => {
                self.debate_messages.insert(e.generation.message_id);
                if let Some(card) = self.transcript.cards.get_mut(&e.generation.message_id) {
                    card.complete = true;
                }
                self.tournaments
                    .entry(e.contest_id)
                    .or_default()
                    .apply_payload(content);
            }
            EnvelopePayload::TournamentRanking(r) => {
                cue = self
                    .tournaments
                    .entry(r.contest_id)
                    .or_default()
                    .apply_payload(content);
            }
            EnvelopePayload::TournamentResult(r) => {
                cue = self
                    .tournaments
                    .entry(r.contest_id)
                    .or_default()
                    .apply_payload(content);
            }
            EnvelopePayload::TournamentChampion(c) => {
                cue = self
                    .tournaments
                    .entry(c.contest_id)
                    .or_default()
                    .apply_payload(content);
                self.snapshot
                    .trophies
                    .insert(c.champion.clone(), c.trophies);
            }
            EnvelopePayload::ContestSetup(s) => {
                self.contests.entry(s.contest_id).or_default().setup = Some(s.clone())
            }
            EnvelopePayload::ContestState(s) => {
                self.restore_contest(s.clone());
                self.snapshot.context.active_contest = if contest_active(s) {
                    Some(s.clone())
                } else {
                    None
                };
            }
            EnvelopePayload::DebateState(s) => {
                let prior = self
                    .contests
                    .get(&s.debate_id)
                    .and_then(|v| v.debate.as_ref());
                if s.state == DebateStatus::Judging && prior.is_none_or(|p| p.state != s.state) {
                    cue = Some(FxCue::VerdictReady {
                        contest_id: s.debate_id.to_string().parse().expect("UUIDv7"),
                    });
                } else if s.state == DebateStatus::RoundRunning
                    && prior.is_none_or(|p| p.round != s.round || p.state != s.state)
                {
                    cue = Some(FxCue::RoundStarted {
                        contest_id: s.debate_id.to_string().parse().expect("UUIDv7"),
                        n: s.round,
                        final_round: s.round == s.total_rounds,
                    });
                }
                self.restore_contest(ContestState::Debate(s.clone()));
                self.snapshot.context.active_contest =
                    if contest_active(&ContestState::Debate(s.clone())) {
                        Some(ContestState::Debate(s.clone()))
                    } else {
                        None
                    };
            }
            EnvelopePayload::DebateSetup(s) => {
                if let Some(view) = self.debates.get_mut(&s.debate_id) {
                    *view = view.clone().with_setup(s, true);
                }
            }
            EnvelopePayload::CaseOpened(c) => {
                self.cases.insert(c.case_id, c.into());
            }
            EnvelopePayload::CaseRuling(r) => {
                if let Some(c) = self.cases.get_mut(&r.case_id) {
                    c.add_ruling(r);
                }
            }
            EnvelopePayload::CaseFollowup(f) => {
                let text = self
                    .snapshot
                    .context
                    .record(f.user_event_id)
                    .map(|r| r.content.clone())
                    .unwrap_or_default();
                if let Some(c) = self.cases.get_mut(&f.case_id) {
                    c.add_followup(f, &text);
                }
            }
            EnvelopePayload::DebateTurn(t) => {
                self.debate_messages.insert(t.generation.message_id);
                if let Some(view) = self.debates.get_mut(&t.debate_id) {
                    view.apply_turn(t);
                }
                let text = match &t.outcome {
                    DebateTurnOutcome::Completed { content, .. } => content.clone(),
                    DebateTurnOutcome::Forfeit { text, .. } => text.to_string(),
                };
                self.transcript.complete(
                    t.generation.message_id,
                    Actor::Member(t.speaker.clone()),
                    t.generation.attempt,
                    text,
                );
                cue = Some(FxCue::BubbleCompleted {
                    id: t.generation.message_id,
                });
            }
            EnvelopePayload::DebateRoundScore(s) => {
                if let Some(view) = self.debates.get_mut(&s.debate_id) {
                    view.apply_score(s);
                }
                self.contests
                    .entry(s.debate_id)
                    .or_default()
                    .scores
                    .insert(s.round, s.clone());
                cue = Some(FxCue::ScorePosted {
                    contest_id: s.debate_id.to_string().parse().expect("UUIDv7"),
                    round: s.round,
                });
            }
            EnvelopePayload::Verdict(v) => {
                self.contests.entry(v.debate_id).or_default().verdict = Some(v.clone());
                self.record(
                    base,
                    committed,
                    "debate.verdict",
                    Some(v.completed.generation.message_id),
                    None,
                    v.verdict.verdict.reasoning.clone(),
                );
                cue = Some(FxCue::Catalogue {
                    effect: crate::fx::CatalogueEffect::Verdict,
                    target: v.debate_id.to_string(),
                    duration: Duration::from_secs(6),
                });
            }
            EnvelopePayload::ContestArtifact(a) => {
                self.contests
                    .entry(a.contest_id)
                    .or_default()
                    .artifacts
                    .insert(base.id, a.clone());
            }
            EnvelopePayload::ContestReport(r) => {
                self.contests
                    .entry(r.contest_id)
                    .or_default()
                    .reports
                    .insert(base.id, r.clone());
            }
            EnvelopePayload::ContestCollecting(c) => {
                self.snapshot.context.active_contest =
                    Some(ContestState::Selection(ContestSelectionState {
                        contest_id: c.contest_id,
                        kind: c.kind,
                        state: ContestSelectionStatus::CollectingAnswers,
                        state_version: 1,
                        question_message_id: c.question_message_id,
                        addressed: c.addressed.clone(),
                        answers: vec![],
                        required_picks: if c.kind == ContestKind::Grill { 1 } else { 2 },
                    }));
            }
            EnvelopePayload::ContestPicked(_) => {
                self.pick = None;
                self.snapshot.context.pending_pick = None;
            }
            EnvelopePayload::ContestPickRequested(p) => {
                if let Some(ContestState::Selection(state)) =
                    &mut self.snapshot.context.active_contest
                    && state.contest_id == p.contest_id
                {
                    state.state = ContestSelectionStatus::AwaitingPick;
                }
                cue = Some(FxCue::PickRequested {
                    contest_id: p.contest_id.to_string().parse().expect("UUIDv7"),
                });
                self.pick = Some(PickView { request: p.clone() });
                self.snapshot.context.pending_pick = Some(PendingPick {
                    contest_id: p.contest_id,
                    mode: if p.kind == ContestKind::Grill {
                        QuestionFirstMode::Grill
                    } else {
                        QuestionFirstMode::Debate
                    },
                    eligible: p.eligible.clone(),
                });
            }
            EnvelopePayload::TournamentPreflight(p) => self
                .snapshot
                .preflights
                .push(PendingPreflight::Contest(p.base.clone())),
            EnvelopePayload::DebatePreflight(p) => self
                .snapshot
                .preflights
                .push(PendingPreflight::Debate(p.clone())),
            EnvelopePayload::ContestPreflight(p) => self
                .snapshot
                .preflights
                .push(PendingPreflight::Contest(p.clone())),
            EnvelopePayload::DebatePreflightResult(p) => self.snapshot.preflights.retain(
                |old| !matches!(old, PendingPreflight::Debate(x) if x.plan_id == p.plan_id),
            ),
            EnvelopePayload::ContestPreflightResult(p) => self.snapshot.preflights.retain(
                |old| !matches!(old, PendingPreflight::Contest(x) if x.plan_id == p.plan_id),
            ),
            EnvelopePayload::Error(e) => self.last_error = Some(e.text.to_string()),
            EnvelopePayload::RoomCleared(_) => {
                self.presentation_epoch = self.presentation_epoch.wrapping_add(1);
                self.transcript.clear();
                self.contests.clear();
                self.debates.clear();
                self.tournaments.clear();
                self.debate_streams.clear();
                self.debate_messages.clear();
                self.cases.clear();
                self.pick = None;
                self.snapshot.context.records.clear();
                self.snapshot.context.active_contest = None;
                self.snapshot.context.pending_pick = None;
                self.snapshot.preflights.clear();
                self.snapshot.trophies.clear();
                self.snapshot.scoreboard.clear();
            }
            _ => {}
        }
        if let Some(cue) = cue {
            queue.from_feed(
                origin,
                base.id,
                base.created_at,
                std::time::SystemTime::now().into(),
                now,
                cue,
            );
        }
        if let FeedItem::Committed(e) = item {
            self.snapshot.events.push(e);
        }
        immediate
    }
    fn record(
        &mut self,
        base: &EnvelopeBase,
        committed: Option<&CommitFields>,
        kind: &str,
        message_id: Option<MessageId>,
        reply: Option<MessageId>,
        content: String,
    ) {
        let Some(c) = committed else {
            return;
        };
        self.snapshot
            .context
            .records
            .retain(|r| r.event_id != base.id);
        self.snapshot.context.records.push(RecordSummary {
            event_id: base.id,
            message_id,
            seq: c.seq,
            kind: Text::new(kind).expect("constant"),
            author: base.sender.clone(),
            recipients: base.recipients.clone(),
            content,
            in_reply_to_message_id: reply,
            contest_id: None,
            case_id: None,
            plan_event_id: None,
            grid_event_id: None,
            cell_ids: vec![],
            state_version: None,
            superseded: false,
            eligible: true,
        });
    }
}
pub fn contest_active(s: &ContestState) -> bool {
    match s {
        ContestState::Tournament(s) => !s.state.is_terminal(),
        ContestState::Debate(d) => matches!(
            d.state,
            DebateStatus::Preparing
                | DebateStatus::AwaitingConfirmation
                | DebateStatus::WaitingForDebaters
                | DebateStatus::RoundRunning
                | DebateStatus::RoundCommitting
                | DebateStatus::Judging
                | DebateStatus::Stopping
        ),
        ContestState::Selection(s) => matches!(
            s.state,
            ContestSelectionStatus::CollectingAnswers | ContestSelectionStatus::AwaitingPick
        ),
        ContestState::Generic(s) => !matches!(
            s.status.as_str(),
            "COMPLETE" | "COMPLETED" | "STOPPED" | "FAILED" | "REJECTED" | "CANCELLED" | "EXPIRED"
        ),
    }
}

/// In-memory room with deterministic scripted protocol events and real streaming
/// cadence. No provider, filesystem, credentials or network are involved.
#[derive(Clone)]
pub struct FakeRoom {
    inner: Arc<Mutex<Session>>,
    feed: broadcast::Sender<FeedItem>,
    stop: watch::Sender<bool>,
}
impl Default for FakeRoom {
    fn default() -> Self {
        Self::new()
    }
}
impl FakeRoom {
    pub fn new() -> Self {
        let (feed, _) = broadcast::channel(4096);
        let (stop, _) = watch::channel(false);
        let mut session = Session::default();
        session.snapshot.context.roster = crate::fx::palette::MEMBERS
            .iter()
            .map(|s| Nick::new(s).expect("demo nick"))
            .collect();
        let slugs = [
            "~anthropic/claude-opus-latest",
            "~anthropic/claude-fable-latest",
            "~openai/gpt-sol-latest",
            "google/gemini-4-neon",
            "~x-ai/grok-latest",
            "meta/muse-wonder",
        ];
        session.snapshot.models = session
            .snapshot
            .context
            .roster
            .iter()
            .enumerate()
            .map(|(i, nick)| Model {
                nick: Actor::Member(nick.clone()),
                roster_order: Some(i as u32),
                requested_slug: Text::new(slugs[i]).unwrap(),
                resolved_slug: Text::new(slugs[i]).unwrap(),
                context_length: 128000,
                pricing: Default::default(),
                resolved_at: "2026-11-24T00:00:00Z".parse().unwrap(),
            })
            .collect();
        session.snapshot.daily_spend.limit_usd = 25.;
        session.snapshot.daily_spend.day = "2026-11-24".into();
        Self {
            inner: Arc::new(Mutex::new(session)),
            feed,
            stop,
        }
    }
    pub fn emit(&self, item: FeedItem) {
        let mut state = self.inner.lock().expect("fake room mutex");
        state.reduce(
            item.clone(),
            FeedOrigin::Replay,
            Duration::ZERO,
            &mut FxQueue::new(),
        );
        let _ = self.feed.send(item);
    }
    pub fn commit(&self, sender: Actor, content: EnvelopePayload) -> EventId {
        let mut state = self.inner.lock().expect("fake room mutex");
        let seq =
            Cursor::new(state.snapshot.cursor.map_or(1, |c| c.get() + 1)).expect("positive cursor");
        let mut base = demo_base(sender);
        if let EnvelopePayload::UserMessage(message) = &content {
            base.id = message
                .input
                .message_id
                .to_string()
                .parse()
                .expect("UUIDv7");
            let meta = squad_protocol::parser::SubmissionMeta {
                quote: message.input.quote.clone().unwrap_or_default(),
                reply_to: message.input.reply_to.clone().unwrap_or_default(),
            };
            if let Ok(valid) = crate::validation::validate(
                message.input.text.as_str(),
                &meta,
                &state.snapshot.context,
            ) {
                let recipients = valid
                    .recipients
                    .iter()
                    .filter_map(|name| {
                        if name == "@judge" {
                            Some(Recipient::Judge)
                        } else {
                            Nick::new(name.trim_start_matches('@'))
                                .ok()
                                .map(Recipient::Member)
                        }
                    })
                    .collect::<Vec<_>>();
                if !recipients.is_empty() {
                    base.recipients = recipients;
                }
            }
        }
        let id = base.id;
        let event = CommittedEnvelope {
            commit: CommitFields {
                seq,
                accepted_at: base.created_at,
                commit_index: 0,
                commit_size: 1,
            },
            base,
            content,
        };
        state.reduce(
            FeedItem::Committed(event.clone()),
            FeedOrigin::Replay,
            Duration::ZERO,
            &mut FxQueue::new(),
        );
        let _ = self.feed.send(FeedItem::Committed(event));
        id
    }
    pub fn start_demo(&self) -> tokio::task::JoinHandle<()> {
        let room = self.clone();
        tokio::spawn(async move {
            let mut stopped = room.stop.subscribe();
            tokio::select! { _ = stopped.changed() => {}, _ = room.demo_script() => {} }
        })
    }
    async fn demo_script(&self) {
        let roster = self
            .inner
            .lock()
            .expect("fake room mutex")
            .snapshot
            .context
            .roster
            .clone();
        tokio::time::sleep(Duration::from_millis(1400)).await;
        self.commit(
            Actor::System,
            EnvelopePayload::BudgetState(BudgetStateEvent {
                day: "2026-11-24".into(),
                spent_usd: 0.42,
                reserved_usd: 0.16,
                limit_usd: 2.,
                estimated_usd: 0.,
                unresolved_attempts: 0,
                unknown_calls: 0,
            }),
        );
        let question = MessageId::new();
        self.commit(
            Actor::Me,
            EnvelopePayload::UserMessage(UserMessage {
                input: UserMessageInput {
                    message_id: question,
                    text: Text::new("@all How should we build a reliable agent room?").unwrap(),
                    quote: None,
                    reply_to: None,
                },
                body: "How should we build a reliable agent room?".into(),
                resolved_quotes: vec![],
                debate_id: None,
                case_id: None,
                contest_id: None,
            }),
        );
        let replies = [
            "**Trust starts with a receipt.** Give one writer ownership of the durable log. A token is a preview; a committed answer is a promise.\n\n```rust\ncommit(outcome).await?;\nacknowledge(receipt);\n```\n\nRecovery should replay facts, never repeat side effects.",
            "I picture a quiet control room: six coloured rails, one clear draft, and room to think. Let the models disagree without making the person fight the interface. **Motion should explain a change**, then get out of the way.",
            "Make the contract executable. Typed requests enter one validated parser; a bounded scheduler prioritizes keyboard input. Keep provisional tokens separate from final outcomes.\n\n- Preserve rejected drafts\n- Deduplicate by message and attempt\n- Measure latency during twelve simultaneous streams",
            "The interesting failure is a restart between commit and acknowledgement. Give each operation an identity and return its original receipt on retry. Then test the same history at every interruption point. A reliable room makes recovery boring, even when its models are surprising.",
            "Twelve models talking at once? Excellent way to discover your supposedly asynchronous UI is a slideshow. Set a frame budget, bound the queues, and inject a slow judge. **If typing stutters, the benchmark fails.** Pretty pixels do not get an exemption.",
            "Ship a rehearsal before asking for credentials. Let someone watch an idea grow, survive a challenge, and become a better plan. The first minute should show the whole journey: question, disagreement, evidence, and a verdict that keeps the best part of both answers.",
        ];
        let jobs = roster
            .iter()
            .zip(replies)
            .map(|(nick, text)| {
                let room = self.clone();
                let nick = nick.clone();
                tokio::spawn(async move {
                    room.stream(nick, question, text, None).await;
                })
            })
            .collect::<Vec<_>>();
        for job in jobs {
            let _ = job.await;
        }
        let id = ContestId::new();
        let case = CaseId::new();
        let mut state = DebateState {
            debate_id: id,
            state: DebateStatus::RoundRunning,
            debaters: [roster[0].clone(), roster[2].clone()],
            round: 0,
            total_rounds: 3,
            judge_model: "demo/judge".into(),
            judge_requested: true,
            state_version: 1,
            terminal_reason: None,
            case_id: Some(case),
            case_ids: vec![case],
        };
        tokio::time::sleep(Duration::from_secs(2)).await;
        self.commit(Actor::System, EnvelopePayload::DebateState(state.clone()));
        self.commit(
            Actor::System,
            EnvelopePayload::DebateOpening(DebateOpening {
                debate_id: id,
                debaters: state.debaters.clone(),
                topic_message_id: question,
                topic_text: Text::new("How should we build a reliable agent room?").unwrap(),
                instruction: String::new(),
                rounds: state.total_rounds,
                round: 0,
            }),
        );
        tokio::time::sleep(Duration::from_millis(1700)).await;
        self.stream(roster[0].clone(),question,"I defend **durability**: one writer, stable identities, and receipts that survive every restart.",Some((id,0))).await;
        self.stream(roster[2].clone(),question,"I defend **responsiveness**: bounded queues, cached presentation, and a keyboard that never waits for models.",Some((id,0))).await;
        let arguments = [
            [
                "**Opening: make promises durable.** A single writer commits the answer before the room acknowledges it. If the process dies between those steps, the client retries with the same identity and receives the original receipt. No answer is lost or charged twice.",
                "**Opening: protect the human loop.** A durable answer is useful only if the person can still type while six models stream. Drain a bounded feed batch, paint the newest prepared rows, and service input first. Put a measurable latency target in the contract.",
            ],
            [
                "Your scheduling contract is necessary, but what does it preserve after a crash? Keep transient deltas outside the authoritative log. The reducer must replace previews with committed outcomes by message and attempt, so a reconnect cannot turn one answer into three copies.",
                "Agreed on identities. Now test the hostile path: a long code fence, a resize, and a delayed verdict while twelve replies arrive. Cache Markdown blocks, virtualize visible rows, and reserve a frame deadline. Correct recovery and responsive presentation should share one test harness.",
            ],
            [
                "**Closing: start with one source of truth.** Commit receipts, usage, and final outcomes atomically. Replay reconstructs the room without replaying celebrations. I accept the input-first scheduler as part of the delivery contract: durability is the foundation, and latency is how people experience it.",
                "**Closing: make both promises observable.** Track commit receipts and p99 draw time in the same rehearsal. Fail the release if either regresses. Keep the writer small, keep input immediate, and let a deterministic demo prove the product before a real provider is connected.",
            ],
        ];
        for round in 1..=3 {
            state.round = round;
            state.state_version += 1;
            self.commit(Actor::System, EnvelopePayload::DebateState(state.clone()));
            tokio::time::sleep(Duration::from_millis(1700)).await;
            self.stream(
                roster[0].clone(),
                question,
                arguments[(round - 1) as usize][0],
                Some((id, round)),
            )
            .await;
            self.stream(
                roster[2].clone(),
                question,
                arguments[(round - 1) as usize][1],
                Some((id, round)),
            )
            .await;
            self.commit(
                Actor::Judge,
                EnvelopePayload::DebateRoundScore(DebateRoundScore {
                    debate_id: id,
                    case_id: case,
                    round,
                    scores: Some(
                        [
                            (
                                roster[0].clone(),
                                RoundScore {
                                    score: 8,
                                    reason: "Clear durability contract".into(),
                                },
                            ),
                            (
                                roster[2].clone(),
                                RoundScore {
                                    score: 7,
                                    reason: "Strong responsiveness argument".into(),
                                },
                            ),
                        ]
                        .into(),
                    ),
                    tally: [
                        (roster[0].clone(), 8 * round),
                        (roster[2].clone(), 7 * round),
                    ]
                    .into(),
                    reason: None,
                }),
            );
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        tokio::time::sleep(Duration::from_millis(1400)).await;
        state.state = DebateStatus::Judging;
        state.state_version += 1;
        self.commit(Actor::System, EnvelopePayload::DebateState(state.clone()));
        tokio::time::sleep(Duration::from_millis(1800)).await;
        let generation = GenerationIdentity {
            message_id: MessageId::new(),
            work_id: WorkId::new(),
            trigger_event_id: EventId::new(),
            attempt: 1,
        };
        let verdict = squad_protocol::verdict::assemble_verdict(JudgeVerdict {
            winner: roster[0].clone(),
            scores: [(&roster[0], 8), (&roster[2], 7)].into_iter().map(|(nick, score)| JudgeScore {
                nick: nick.clone(), criteria: [CriterionId::Fit, CriterionId::Feasibility, CriterionId::Originality, CriterionId::SurvivedAttacks].into_iter().map(|criterion_id| CriterionScore { criterion_id, score, reasoning: "A concrete, testable proposal supported by the discussion.".into() }).collect()
            }).collect(),
            reasoning: "Durability wins the foundation; take bounded, input-first scheduling from the other proposal. Together they make a responsive room that survives restarts.".into(),
            what_to_steal_from_loser: vec!["Bounded input-first scheduling".into()], tally_override_reason: None,
        }, &state.debaters, &[(roster[0].clone(), 24), (roster[2].clone(), 21)].into()).expect("valid demo verdict");
        let verdict_event = self.commit(
            Actor::Judge,
            EnvelopePayload::Verdict(DebateVerdict {
                completed: CompletedGeneration {
                    generation,
                    model: Text::new("demo/judge").unwrap(),
                    finish_reason: FinishReason::Stop,
                    usage: None,
                },
                debate_id: id,
                case_id: state.case_id.expect("demo case"),
                judge_invocation_id: EventId::new(),
                rubric_version: Text::new("demo").unwrap(),
                verdict,
            }),
        );
        state.state = DebateStatus::Completed;
        state.state_version += 1;
        self.commit(Actor::System, EnvelopePayload::DebateState(state));
        self.commit(
            Actor::System,
            EnvelopePayload::ScoresChanged(ScoresChanged {
                verdict_event_id: verdict_event,
                scores: vec![
                    Score {
                        nick: roster[0].clone(),
                        wins: 1,
                        losses: 0,
                        trophies: 0,
                    },
                    Score {
                        nick: roster[2].clone(),
                        wins: 0,
                        losses: 1,
                        trophies: 0,
                    },
                ],
            }),
        );
        self.commit(
            Actor::System,
            EnvelopePayload::BudgetState(BudgetStateEvent {
                day: "2026-11-24".into(),
                spent_usd: 1.57,
                reserved_usd: 0.1,
                limit_usd: 2.,
                estimated_usd: 0.,
                unresolved_attempts: 0,
                unknown_calls: 0,
            }),
        );
        tokio::time::sleep(Duration::from_secs(8)).await;
        // M1 has a typed question-first grill collection, but no typed grill
        // exchanges yet. Showcase that supported boundary without inventing M5 JSON.
        let grill = ContestId::new();
        self.commit(
            Actor::System,
            EnvelopePayload::ContestCollecting(ContestCollecting {
                contest_id: grill,
                kind: ContestKind::Grill,
                question_message_id: question,
                question: "Stress-test the rollout plan".into(),
                addressed: roster.clone(),
            }),
        );
        self.stream(roster[4].clone(),question,"**Grill me on this plan:** ship one writer, one reducer, and a twelve-stream release gate. Ask what happens when the writer stalls; my answer is bounded backpressure and visible pending receipts, never a frozen keyboard.",None).await;
        self.commit(
            Actor::System,
            EnvelopePayload::ContestPickRequested(ContestPickRequested {
                contest_id: grill,
                kind: ContestKind::Grill,
                question_message_id: question,
                eligible: vec![roster[4].clone()],
                required_picks: 1,
                banner: "Stress-test the rollout plan".into(),
            }),
        );
    }
    async fn stream(
        &self,
        nick: Nick,
        question: MessageId,
        text: &str,
        debate: Option<(ContestId, u32)>,
    ) {
        let generation = GenerationIdentity {
            message_id: MessageId::new(),
            work_id: WorkId::new(),
            trigger_event_id: EventId::new(),
            attempt: 1,
        };
        self.emit(FeedItem::Presence(ProducerEnvelope {
            base: demo_base(Actor::Member(nick.clone())),
            content: EnvelopePayload::Presence(Presence {
                nick: nick.clone(),
                status: PresenceStatus::Streaming,
                work_id: Some(generation.work_id),
                queue_depth: 0,
                error_code: None,
                observed_at: std::time::SystemTime::now().into(),
            }),
        }));
        let speed = match nick.as_str() {
            "claude" => 130,
            "openai" => 110,
            "gemini" => 85,
            "grok" => 65,
            "fable" => 110,
            _ => 125,
        };
        let cancelled = self.stop.subscribe();
        for (chunk, word) in text.split_inclusive(' ').enumerate() {
            if cancelled.has_changed().unwrap_or(true) {
                return;
            }
            self.emit(FeedItem::Delta(ProducerEnvelope {
                base: demo_base(Actor::Member(nick.clone())),
                content: EnvelopePayload::Delta(Delta {
                    generation: generation.clone(),
                    chunk_index: chunk as u64,
                    text: word.into(),
                    debate_id: debate.map(|d| d.0),
                    round: debate.map(|d| d.1),
                    case_id: None,
                    contest_id: None,
                    step_id: None,
                }),
            }));
            tokio::time::sleep(Duration::from_millis(speed)).await;
        }
        let model = self
            .inner
            .lock()
            .expect("fake room mutex")
            .snapshot
            .models
            .iter()
            .find(|m| m.nick == Actor::Member(nick.clone()))
            .map(|m| m.resolved_slug.clone())
            .unwrap_or_else(|| Text::new("demo/model").unwrap());
        let content = if let Some((debate_id, round)) = debate {
            EnvelopePayload::DebateTurn(DebateTurn {
                generation,
                debate_id,
                round,
                speaker: nick.clone(),
                outcome: DebateTurnOutcome::Completed {
                    content: text.into(),
                    model: model.clone(),
                    finish_reason: FinishReason::Stop,
                    usage: None,
                },
            })
        } else {
            EnvelopePayload::MessageFinal(Completion {
                completed: CompletedGeneration {
                    generation,
                    model: model.clone(),
                    finish_reason: FinishReason::Stop,
                    usage: None,
                },
                in_reply_to_message_id: question,
                content: text.into(),
            })
        };
        self.commit(Actor::Member(nick.clone()), content);
        self.emit(FeedItem::Presence(ProducerEnvelope {
            base: demo_base(Actor::Member(nick.clone())),
            content: EnvelopePayload::Presence(Presence {
                nick,
                status: PresenceStatus::Idle,
                work_id: None,
                queue_depth: 0,
                error_code: None,
                observed_at: std::time::SystemTime::now().into(),
            }),
        }));
    }
}
fn demo_base(sender: Actor) -> EnvelopeBase {
    EnvelopeBase {
        version: 1,
        id: EventId::new(),
        room_id: MainRoom::Main,
        correlation_id: EventId::new(),
        causation_id: None,
        sender,
        recipients: vec![Recipient::Me],
        created_at: std::time::SystemTime::now().into(),
    }
}
impl RoomPort for FakeRoom {
    fn submit(&self, request: Request) -> RoomFuture<'_, RequestResult> {
        Box::pin(async move {
            let mut accepted_events = Vec::new();
            let request_id = match request {
                Request::UserMessage { input, parsed, .. } => {
                    let id = input.message_id.to_string().parse().expect("UUIDv7");
                    let meta = squad_protocol::parser::SubmissionMeta {
                        quote: input.quote.clone().unwrap_or_default(),
                        reply_to: input.reply_to.clone().unwrap_or_default(),
                    };
                    let context = self.inner.lock().unwrap().snapshot.context.clone();
                    let resolved_quotes = meta
                        .quote
                        .iter()
                        .filter_map(|id| {
                            context.record(*id).map(|r| ResolvedQuote {
                                event_id: *id,
                                author: r.author.clone(),
                                text: r.content.clone(),
                            })
                        })
                        .collect();
                    let recipients = crate::validation::validate(parsed.raw(), &meta, &context)
                        .map(|v| v.recipients)
                        .unwrap_or_default();
                    let body = match &parsed {
                        ParsedInput::Message { body, .. } => body.clone(),
                        _ => parsed.raw().into(),
                    };
                    accepted_events.push(self.commit(
                        Actor::Me,
                        EnvelopePayload::UserMessage(UserMessage {
                            input: input.clone(),
                            body,
                            resolved_quotes,
                            debate_id: None,
                            case_id: None,
                            contest_id: None,
                        }),
                    ));
                    for name in recipients {
                        if let Ok(nick) = Nick::new(name.trim_start_matches('@')) {
                            let room = self.clone();
                            let question = input.message_id;
                            tokio::spawn(async move {
                                room.stream(nick, question, "This is a local demo reply. The same request boundary will connect to the live room when M3 is available.", None).await;
                            });
                        }
                    }
                    id
                }
                Request::SetVars {
                    request_id,
                    updates,
                } => {
                    for v in updates {
                        let previous_value = self
                            .inner
                            .lock()
                            .unwrap()
                            .snapshot
                            .context
                            .vars
                            .get(&v.name)
                            .copied()
                            .unwrap_or(2);
                        accepted_events.push(self.commit(
                            Actor::System,
                            EnvelopePayload::VarChange(VariableChange {
                                name: v.name,
                                value: v.value,
                                previous_value,
                            }),
                        ));
                    }
                    request_id
                }
                Request::Clear { request_id } => {
                    accepted_events.push(self.commit(
                        Actor::System,
                        EnvelopePayload::RoomCleared(RoomCleared {
                            epoch: 1,
                            previous_through: None,
                        }),
                    ));
                    request_id
                }
                Request::Stop { request_id, .. } => {
                    self.stop.send_replace(true);
                    let (cards, contest) = {
                        let state = self.inner.lock().unwrap();
                        (
                            state
                                .transcript
                                .cards
                                .values()
                                .filter(|c| !c.complete)
                                .cloned()
                                .collect::<Vec<_>>(),
                            state.snapshot.context.active_contest.clone(),
                        )
                    };
                    for card in cards {
                        if let (Some(work_id), Actor::Member(nick)) = (card.work_id, card.author) {
                            accepted_events.push(self.commit(
                                Actor::System,
                                EnvelopePayload::WorkState(WorkStateEvent {
                                    work_id,
                                    trigger_event_id: request_id,
                                    target: Target::Member(nick.clone()),
                                    context_id: ContextId(nick.to_string()),
                                    attempt: card.attempt,
                                    status: WorkStatus::Cancelled,
                                    error: None,
                                    debate_id: None,
                                    case_id: None,
                                    contest_id: None,
                                }),
                            ));
                        }
                    }
                    if let Some(ContestState::Debate(mut state)) = contest {
                        state.state = DebateStatus::Stopped;
                        state.state_version += 1;
                        accepted_events
                            .push(self.commit(Actor::System, EnvelopePayload::DebateState(state)));
                    }
                    request_id
                }
                Request::Help => {
                    return RequestResult::Internal(InternalResult::Text {
                        text: squad_protocol::parser::HELP_EXAMPLES.join("\n"),
                    });
                }
                Request::Who => {
                    return RequestResult::Internal(InternalResult::Text {
                        text: self
                            .inner
                            .lock()
                            .unwrap()
                            .snapshot
                            .context
                            .roster
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join(", "),
                    });
                }
                _ => {
                    return RequestResult::Rejected {
                        request_id: EventId::new(),
                        error: ErrorPayload {
                            code: ErrorCode::INVALID_COMMAND,
                            text: Text::new("This control is not implemented by FakeRoom.")
                                .unwrap(),
                            retryable: false,
                            related_event_id: None,
                            work_id: None,
                            debate_id: None,
                            case_id: None,
                            earliest_cursor: None,
                            retry_action: None,
                            contest_id: None,
                        },
                    };
                }
            };
            RequestResult::Accepted {
                request_id,
                event_ids: accepted_events,
                cursor: self.inner.lock().unwrap().snapshot.cursor,
            }
        })
    }
    fn subscribe(&self, after: Option<Cursor>) -> Result<RoomFeed, RoomError> {
        let state = self.inner.lock().unwrap();
        if after.is_some_and(|c| state.snapshot.cursor.is_none_or(|last| c > last)) {
            return Err(RoomError {
                code: ErrorCode::CURSOR_INVALID,
                message: "Invalid feed cursor.".into(),
            });
        }
        // Snapshot restoration supplies durable catchup. The receiver is registered
        // under the same lock as commit so state+subscription has no lost window.
        let live = self.feed.subscribe();
        let replay = after
            .map(|cursor| {
                state
                    .snapshot
                    .events
                    .iter()
                    .filter(|e| e.commit.seq > cursor)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        Ok(RoomFeed::new(live, replay))
    }
    fn page_events(
        &self,
        before: Option<Cursor>,
        limit: usize,
    ) -> RoomFuture<'_, Result<Vec<CommittedEnvelope>, RoomError>> {
        Box::pin(async move {
            Ok(self
                .inner
                .lock()
                .unwrap()
                .snapshot
                .events
                .iter()
                .rev()
                .filter(|e| before.is_none_or(|c| e.commit.seq < c))
                .take(limit.min(256))
                .cloned()
                .collect())
        })
    }
    fn state(&self) -> RoomFuture<'_, RoomSnapshot> {
        Box::pin(async move { self.inner.lock().unwrap().snapshot.clone() })
    }
    fn export(
        &self,
        scope: ExportScope,
        id: Option<ContestId>,
    ) -> RoomFuture<'_, Result<String, RoomError>> {
        Box::pin(async move {
            if scope != ExportScope::Room || id.is_some() {
                return Err(RoomError {
                    code: ErrorCode::INVALID_COMMAND,
                    message: "Scoped exports are not implemented by FakeRoom.".into(),
                });
            }
            Ok(self
                .inner
                .lock()
                .unwrap()
                .transcript
                .cards
                .values()
                .map(|c| format!("{:?}\n\n{}\n", c.author, c.text))
                .collect::<Vec<_>>()
                .join("\n"))
        })
    }
    fn shutdown(&self) -> RoomFuture<'_, Result<(), RoomError>> {
        Box::pin(async move {
            self.stop.send_replace(true);
            Ok(())
        })
    }
}
