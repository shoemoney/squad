//! Production binding of CLI requests to the core room.
use crate::room_api::{OpenPolicy, Progress, RoomApi, RoomError};
use serde::Serialize;
use squad_core::{room::RoomHandle, state::RoomState};
use squad_protocol::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

pub fn error(e: squad_core::room::RoomError) -> RoomError {
    RoomError {
        code: e.code,
        message: e.message,
        retryable: false,
    }
}
#[derive(Serialize)]
pub struct State {
    #[serde(flatten)]
    pub room: RoomState,
    #[serde(skip)]
    pub events: Vec<CommittedEnvelope>,
    #[serde(skip)]
    pub read_failed: bool,
}

// These keys identify executions and contests, not quoted/source material.
fn identities(event: &CommittedEnvelope) -> BTreeSet<String> {
    fn visit(value: &serde_json::Value, output: &mut BTreeSet<String>) {
        if let serde_json::Value::Object(map) = value {
            for (key, value) in map {
                if matches!(
                    key.as_str(),
                    "workId"
                        | "replacementWorkId"
                        | "triggerEventId"
                        | "contestId"
                        | "debateId"
                        | "caseId"
                        | "relatedEventId"
                ) {
                    if let Some(id) = value.as_str() {
                        output.insert(id.to_owned());
                    }
                } else if value.is_object() {
                    visit(value, output);
                }
            }
        }
    }
    let mut result = BTreeSet::new();
    visit(
        &serde_json::to_value(&event.content).expect("protocol serializes"),
        &mut result,
    );
    result
}
fn related(state: &State, request: EventId) -> BTreeSet<EventId> {
    let mut ids = BTreeSet::from([request.to_string()]);
    let mut events = BTreeSet::new();
    loop {
        let old = events.len();
        for event in &state.events {
            let links = identities(event);
            if ids.contains(&event.base.id.to_string())
                || event
                    .base
                    .causation_id
                    .is_some_and(|id| ids.contains(&id.to_string()))
                || !links.is_disjoint(&ids)
            {
                events.insert(event.base.id);
                ids.insert(event.base.id.to_string());
                ids.extend(links);
            }
        }
        if events.len() == old {
            return events;
        }
    }
}
impl RoomApi for RoomHandle {
    type State = State;
    async fn submit(&self, request: Request) -> RequestResult {
        self.submit(request).await
    }
    async fn submit_commit_only(&self, request: Request) -> RequestResult {
        self.submit_commit_only(request).await
    }
    fn subscribe(&self) -> tokio::sync::broadcast::Receiver<FeedItem> {
        self.subscribe()
    }
    async fn page_events(
        &self,
        before: Option<Cursor>,
        limit: usize,
    ) -> Result<Vec<CommittedEnvelope>, RoomError> {
        self.page_events(before, limit).await.map_err(error)
    }
    async fn state(&self) -> State {
        let events = squad_tui::room_adapter::history(self).await;
        let room = self.try_state().await;
        let read_failed = events.is_err() || room.is_err();
        State {
            room: match room {
                Ok(state) => state,
                Err(_) => self.state().await,
            },
            events: events.unwrap_or_default(),
            read_failed,
        }
    }
    async fn export(&self, scope: ExportScope, id: Option<ContestId>) -> Result<String, RoomError> {
        self.export(scope, id).await.map_err(error)
    }
    async fn shutdown(&self) -> Result<(), RoomError> {
        self.shutdown().await.map_err(error)
    }
    fn parse_context(state: &State) -> parser::ParseContext {
        squad_tui::room_adapter::snapshot(&state.room, state.events.clone()).context
    }
    fn progress(state: &State, request: EventId) -> Progress {
        if state.read_failed {
            return Progress::Failed;
        }
        let related = related(state, request);
        // Causal relevance may reach earlier material (for example regeneration).
        // Only transitions at or after this invocation can determine its result.
        let start = state
            .events
            .iter()
            .filter(|e| e.base.id == request || e.base.causation_id == Some(request))
            .map(|e| e.commit.seq)
            .min();
        let mut work = BTreeMap::new();
        let mut debates = BTreeMap::new();
        let mut surveys = BTreeMap::new();
        let mut addressed = BTreeSet::new();
        let mut terminal = BTreeSet::new();
        let mut failed = false;
        let unscored_cases: BTreeSet<_> = state
            .events
            .iter()
            .filter_map(|e| match &e.content {
                EnvelopePayload::DebateRoundScore(s) if s.scores.is_none() => Some(s.case_id),
                _ => None,
            })
            .collect();
        for event in state.events.iter().filter(|e| {
            related.contains(&e.base.id) && start.is_none_or(|start| e.commit.seq >= start)
        }) {
            match &event.content {
                EnvelopePayload::UserMessage(m) if event.base.id == request => {
                    addressed.extend(event.base.recipients.iter().filter_map(|r| match r {
                        Recipient::Member(n) => Some(n.to_string()),
                        Recipient::Judge => Some("judge".into()),
                        _ => None,
                    }));
                    if m.case_id.is_some() {
                        addressed.insert("judge".into());
                    }
                }
                EnvelopePayload::MessageSuperseded(s) => {
                    work.entry(s.replacement_work_id)
                        .or_insert(WorkStatus::Queued);
                }
                EnvelopePayload::CaseOpened(c) => {
                    if let Some(id) = c.work_id {
                        work.entry(id).or_insert(WorkStatus::Queued);
                    }
                }
                EnvelopePayload::CaseFollowup(c) => {
                    work.entry(c.work_id).or_insert(WorkStatus::Queued);
                }
                EnvelopePayload::MessageFinal(c) | EnvelopePayload::AgentReply(c) => {
                    work.insert(c.completed.generation.work_id, WorkStatus::Completed);
                    if let Actor::Member(n) = &event.base.sender {
                        terminal.insert(n.to_string());
                    }
                }
                EnvelopePayload::CaseRuling(c) => {
                    work.insert(c.completed.generation.work_id, WorkStatus::Completed);
                    terminal.insert("judge".into());
                }
                EnvelopePayload::WorkState(w) => {
                    let unscored = w.target == Target::Judge
                        && w.case_id.is_some_and(|id| unscored_cases.contains(&id));
                    let status = if unscored && w.status == WorkStatus::Failed {
                        WorkStatus::Completed
                    } else {
                        w.status.clone()
                    };
                    if matches!(
                        status,
                        WorkStatus::Completed | WorkStatus::Failed | WorkStatus::Cancelled
                    ) {
                        terminal.insert(match &w.target {
                            Target::Member(n) => n.to_string(),
                            Target::Judge => "judge".into(),
                        });
                    }
                    work.insert(w.work_id, status);
                }
                EnvelopePayload::Error(e) => {
                    failed |= e.work_id.is_none()
                        && !e.retryable
                        && !e.case_id.is_some_and(|id| unscored_cases.contains(&id));
                    if e.code == ErrorCode::AGENT_QUEUE_FULL {
                        for nick in &addressed {
                            if e.text.as_str().contains(&format!("@{nick} ")) {
                                terminal.insert(nick.clone());
                            }
                        }
                    }
                }
                EnvelopePayload::SurveyState(s) => {
                    surveys.insert(s.contest_id, s.state);
                }
                EnvelopePayload::DebateState(d) => {
                    debates.insert(d.debate_id, d.state.clone());
                }
                _ => (),
            }
        }
        if !surveys.is_empty() && surveys.values().all(|s| s.is_terminal()) {
            return if surveys.values().all(|s| *s == SurveyStatus::Complete) {
                Progress::Succeeded
            } else {
                Progress::Failed
            };
        }
        if surveys.values().any(|s| !s.is_terminal())
            || !addressed.is_subset(&terminal)
            || work
                .values()
                .any(|s| matches!(s, WorkStatus::Queued | WorkStatus::Running))
            || debates.values().any(|s| {
                matches!(
                    s,
                    DebateStatus::Preparing
                        | DebateStatus::WaitingForDebaters
                        | DebateStatus::RoundRunning
                        | DebateStatus::RoundCommitting
                        | DebateStatus::Judging
                        | DebateStatus::Stopping
                )
            })
        {
            return Progress::Running;
        }
        if failed
            || work
                .values()
                .any(|s| matches!(s, WorkStatus::Failed | WorkStatus::Cancelled))
            || debates.values().any(|s| {
                matches!(
                    s,
                    DebateStatus::Failed
                        | DebateStatus::CompleteUnjudged
                        | DebateStatus::Stopped
                        | DebateStatus::Cancelled
                        | DebateStatus::Rejected
                        | DebateStatus::Expired
                )
            })
        {
            return Progress::Failed;
        }
        Progress::Succeeded
    }
    fn relevant(state: &State, request: EventId, event: &CommittedEnvelope) -> bool {
        related(state, request).contains(&event.base.id)
    }
}
impl<R: RoomApi> RoomApi for Arc<R> {
    type State = R::State;
    async fn submit(&self, request: Request) -> RequestResult {
        (**self).submit(request).await
    }
    async fn submit_commit_only(&self, request: Request) -> RequestResult {
        (**self).submit_commit_only(request).await
    }
    fn subscribe(&self) -> tokio::sync::broadcast::Receiver<FeedItem> {
        (**self).subscribe()
    }
    async fn page_events(
        &self,
        before: Option<Cursor>,
        limit: usize,
    ) -> Result<Vec<CommittedEnvelope>, RoomError> {
        (**self).page_events(before, limit).await
    }
    async fn state(&self) -> Self::State {
        (**self).state().await
    }
    async fn export(&self, scope: ExportScope, id: Option<ContestId>) -> Result<String, RoomError> {
        (**self).export(scope, id).await
    }
    async fn shutdown(&self) -> Result<(), RoomError> {
        (**self).shutdown().await
    }
    fn parse_context(state: &Self::State) -> parser::ParseContext {
        R::parse_context(state)
    }
    fn progress(state: &Self::State, request: EventId) -> Progress {
        R::progress(state, request)
    }
    fn relevant(state: &Self::State, request: EventId, event: &CommittedEnvelope) -> bool {
        R::relevant(state, request, event)
    }
}
pub struct Runtime {
    pub config: SquadConfig,
}
impl crate::lifecycle::Runtime for Runtime {
    type Room = Arc<RoomHandle>;
    async fn open(&self, config: SquadConfig, policy: OpenPolicy) -> Result<Self::Room, RoomError> {
        RoomHandle::open(config, "main", policy.mode)
            .await
            .map(Arc::new)
            .map_err(error)
    }
    async fn tui(&self, room: &Self::Room) -> Result<(), RoomError> {
        squad_tui::run(self.config.clone(), room.clone())
            .await
            .map_err(|_| RoomError {
                code: ErrorCode::INTERNAL_ERROR,
                message: "Terminal application failed.".into(),
                retryable: false,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn state() -> State {
        State {
            room: RoomState {
                room: "main".into(),
                vars: squad_core::state::RoomVars {
                    rounds: 3,
                    grill_rounds: 3,
                    census_passes: 3,
                },
                participants: vec![],
                debates: vec![],
                pending_preflights: vec![],
                models: vec![],
                scoreboard: vec![],
                daily_spend: squad_core::state::DailySpend {
                    day: "2026-11-24".into(),
                    spent_usd: 0.,
                    reserved_usd: 0.,
                    limit_usd: 2.,
                    unknown_calls: 0,
                    has_unreconciled_charges: false,
                },
                trophies: BTreeMap::new(),
                active_contest: None,
            },
            events: vec![],
            read_failed: false,
        }
    }
    fn event(seq: i64, request: EventId, content: EnvelopePayload) -> CommittedEnvelope {
        CommittedEnvelope {
            base: EnvelopeBase {
                version: 1,
                id: EventId::new(),
                room_id: MainRoom::Main,
                correlation_id: request,
                causation_id: Some(request),
                sender: Actor::System,
                recipients: vec![Recipient::Me],
                created_at: std::time::SystemTime::now().into(),
            },
            commit: CommitFields {
                seq: Cursor::new(seq).unwrap(),
                accepted_at: std::time::SystemTime::now().into(),
                commit_index: 0,
                commit_size: 1,
            },
            content,
        }
    }
    fn work(request: EventId, nick: &str, status: WorkStatus) -> EnvelopePayload {
        EnvelopePayload::WorkState(WorkStateEvent {
            work_id: WorkId::new(),
            trigger_event_id: request,
            target: Target::Member(nick.parse().unwrap()),
            context_id: ContextId(nick.into()),
            attempt: 1,
            status,
            error: None,
            debate_id: None,
            case_id: None,
            contest_id: None,
        })
    }
    #[test]
    fn addressed_executions_wait_for_every_terminal_result_and_keep_failure() {
        let request = EventId::new();
        let mut state = state();
        let mut root = event(
            1,
            request,
            EnvelopePayload::UserMessage(UserMessage {
                input: UserMessageInput {
                    message_id: request.to_string().parse().unwrap(),
                    text: Text::new("@a @b question").unwrap(),
                    quote: None,
                    reply_to: None,
                },
                body: "question".into(),
                resolved_quotes: vec![],
                debate_id: None,
                case_id: None,
                contest_id: None,
            }),
        );
        root.base.id = request;
        root.base.recipients = vec![
            Recipient::Member("a".parse().unwrap()),
            Recipient::Member("b".parse().unwrap()),
        ];
        state.events.push(root);
        assert_eq!(RoomHandle::progress(&state, request), Progress::Running);
        state
            .events
            .push(event(2, request, work(request, "a", WorkStatus::Failed)));
        assert_eq!(RoomHandle::progress(&state, request), Progress::Running);
        state
            .events
            .push(event(3, request, work(request, "b", WorkStatus::Completed)));
        assert_eq!(RoomHandle::progress(&state, request), Progress::Failed);
    }
    #[test]
    fn relevance_follows_execution_identity_across_correlations() {
        let request = EventId::new();
        let mut state = state();
        let seed = event(1, request, work(request, "a", WorkStatus::Running));
        let mut linked = seed.clone();
        linked.base.id = EventId::new();
        linked.base.correlation_id = EventId::new();
        linked.base.causation_id = None;
        linked.commit.seq = Cursor::new(2).unwrap();
        let mut unrelated = event(
            3,
            request,
            EnvelopePayload::VarChange(VariableChange {
                name: VariableName::Rounds,
                value: 4,
                previous_value: 3,
            }),
        );
        unrelated.base.causation_id = None;
        state.events = vec![seed, linked.clone(), unrelated.clone()];
        assert!(RoomHandle::relevant(&state, request, &linked));
        assert!(!RoomHandle::relevant(&state, request, &unrelated));
    }
    #[test]
    fn unscored_round_judging_does_not_fail_the_submission() {
        let request = EventId::new();
        let case = CaseId::new();
        let mut state = state();
        let mut failed = match work(request, "a", WorkStatus::Failed) {
            EnvelopePayload::WorkState(w) => w,
            _ => unreachable!(),
        };
        failed.target = Target::Judge;
        failed.case_id = Some(case);
        state
            .events
            .push(event(1, request, EnvelopePayload::WorkState(failed)));
        state.events.push(event(
            2,
            request,
            EnvelopePayload::DebateRoundScore(DebateRoundScore {
                debate_id: ContestId::new(),
                case_id: case,
                round: 1,
                scores: None,
                tally: BTreeMap::new(),
                reason: Some(Text::new("unscored").unwrap()),
            }),
        ));
        assert_eq!(RoomHandle::progress(&state, request), Progress::Succeeded);
    }
    #[test]
    fn regeneration_waits_for_replacement_without_inheriting_old_failure() {
        let old = EventId::new();
        let request = EventId::new();
        let replacement = WorkId::new();
        let mut state = state();
        state
            .events
            .push(event(1, old, work(old, "a", WorkStatus::Failed)));
        state.events.push(event(
            2,
            request,
            EnvelopePayload::MessageSuperseded(MessageSuperseded {
                superseded_event_id: EventId::new(),
                superseded_message_id: MessageId::new(),
                member: "a".parse().unwrap(),
                in_reply_to_message_id: MessageId::new(),
                replacement_work_id: replacement,
                replacement_message_id: MessageId::new(),
            }),
        ));
        assert_eq!(RoomHandle::progress(&state, request), Progress::Running);
        let mut completed = match work(old, "a", WorkStatus::Completed) {
            EnvelopePayload::WorkState(w) => w,
            _ => unreachable!(),
        };
        completed.work_id = replacement;
        state
            .events
            .push(event(3, old, EnvelopePayload::WorkState(completed)));
        assert_eq!(RoomHandle::progress(&state, request), Progress::Succeeded);
    }
}
