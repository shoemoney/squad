use squad_core::orchestrator::{machine, types::*};
use squad_protocol::*;
use squad_store::{Result, StoreError, commit::CommitOutcome};
use std::collections::{BTreeMap, BTreeSet};

pub fn nick(value: &str) -> MemberNick {
    MemberNick::new(value).unwrap()
}
pub fn now() -> DateTime {
    "2026-11-24T12:00:00Z".parse().unwrap()
}
pub fn model(name: &str) -> FrozenModel {
    FrozenModel {
        nick: Some(nick(name)),
        resolved_slug: format!("vendor/{name}"),
        context_tokens: 1_000_000,
        input_usd_per_million: 1.0,
        output_usd_per_million: 2.0,
        reasoning_tokens: 1024,
        reasoning_effort: Some(ReasoningEffort::Low),
    }
}
pub fn plan(rounds: u32, opening: bool, judge: bool) -> DebatePlan {
    let debaters = [nick("alpha"), nick("beta")];
    let topic = MessageId::new();
    DebatePlan {
        preflight: DebatePreflight {
            judge_only: false,
            debate_id: ContestId::new(),
            plan_id: PlanId::new(),
            expires_at: now() + chrono::Duration::minutes(10),
            estimate: DebateEstimate {
                estimated_usd: 0.0,
                reserved_usd: 0.0,
                remaining_usd: 100.0,
                model_calls: 2 * (rounds + u32::from(opening)) + if judge { rounds + 1 } else { 0 },
            },
            judge_model: judge.then(|| "vendor/referee".into()),
            warning: None,
            confirmation_required: false,
            debaters: debaters.clone(),
            rounds,
            judge_requested: judge,
            opening_round_required: opening,
        },
        sources: FrozenSources {
            topic_message_id: topic,
            topic: "Choose a design".into(),
            instruction: "Compare tradeoffs".into(),
            solutions: (!opening).then(|| {
                debaters.clone().map(|nick| DebateSolution {
                    nick,
                    message_id: MessageId::new(),
                    text: "A supplied solution".into(),
                })
            }),
            cutoff: 100,
        },
        members: [model("alpha"), model("beta")],
        judge: judge.then(|| model("referee")),
        prompts: FrozenPrompts {
            pack_version: "test-v1".into(),
            composition_hashes: [
                "debate.round0.md",
                "debate.round1.md",
                "debate.rebuttal.md",
                "judge.round.md",
                "judge.user.md",
            ]
            .map(|id| (id.into(), format!("hash:{id}")))
            .into_iter()
            .collect(),
            turn_words: 200,
            final_turn_words: 400,
        },
        script_version: "debate-v1".into(),
        calls: Vec::new(),
        prior_terminal: None,
    }
}
pub fn state(plan: &DebatePlan, status: DebateStatus) -> DebateState {
    DebateState {
        debate_id: plan.preflight.debate_id,
        state: status,
        debaters: plan.preflight.debaters.clone(),
        round: 1,
        total_rounds: plan.preflight.rounds,
        judge_model: plan.preflight.judge_model.clone().unwrap_or_default(),
        judge_requested: plan.preflight.judge_requested,
        state_version: 1,
        terminal_reason: None,
        case_id: None,
        case_ids: Vec::new(),
    }
}
pub fn work(plan: &DebatePlan, index: usize, round: u32) -> Work {
    Work {
        work_id: WorkId::new(),
        message_id: MessageId::new(),
        in_reply_to_message_id: None,
        kind: WorkKind::DebateTurn,
        target: Target::Member(plan.preflight.debaters[index].clone()),
        context_id: ContextId(plan.preflight.debaters[index].to_string()),
        trigger_event_id: EventId::new(),
        history_through: HistoryPosition::new(50 + index as i64).unwrap(),
        attempt: 1,
        debate_id: Some(plan.preflight.debate_id),
        round: Some(round),
        replay: None,
        case_id: None,
        prompt_ref: None,
        max_output_tokens: None,
        contest_id: Some(plan.preflight.debate_id),
        step_id: None,
    }
}
pub fn judge_work(plan: &DebatePlan, state: &DebateState, round: Option<u32>) -> Work {
    let mut w = work(plan, 0, 1);
    w.kind = if round.is_some() {
        WorkKind::JudgeRound
    } else {
        WorkKind::Judge
    };
    w.target = Target::Judge;
    w.case_id = state.case_id;
    w.context_id = squad_core::orchestrator::cases::context(state.case_id.unwrap());
    w.round = round;
    squad_core::orchestrator::rounds::prepare_judge(plan, w).unwrap()
}
pub fn case(state: &mut DebateState) {
    let id = CaseId::new();
    state.case_id = Some(id);
    state.case_ids.push(id);
}
pub fn attempt(work: Work, phase: AttemptPhase) -> AttemptState {
    AttemptState {
        work,
        phase,
        deadline: Some(now() + chrono::Duration::seconds(180)),
        repair_used: false,
    }
}
pub fn envelope(content: EnvelopePayload) -> ProducerEnvelope {
    squad_core::router::envelope(EventId::new(), Actor::System, vec![Recipient::Me], content)
}
pub fn assert_code<T: std::fmt::Debug>(result: Result<T>, code: ErrorCode) {
    assert!(matches!(result, Err(StoreError::Domain(c, _)) if c == code));
}

/// In-memory transaction stand-in: mirrors the router's receipt/version gate and
/// preflight auto-admission. It never executes providers or synthesizes results.
pub struct FakeRouter {
    pub state: DebateState,
    pub opening: bool,
    pub preflights: Vec<DebatePreflight>,
    pub work: BTreeMap<WorkId, Work>,
    pub deliveries: usize,
    receipts: BTreeSet<EventId>,
}
impl FakeRouter {
    pub fn new(plan: &DebatePlan) -> Self {
        Self {
            state: state(plan, DebateStatus::Preparing),
            opening: plan.preflight.opening_round_required,
            preflights: Vec::new(),
            work: BTreeMap::new(),
            deliveries: 0,
            receipts: BTreeSet::new(),
        }
    }
    fn outcome(&mut self, id: EventId) -> CommitOutcome {
        let duplicate = !self.receipts.insert(id);
        CommitOutcome {
            result: RequestResult::Accepted {
                request_id: id,
                event_ids: Vec::new(),
                cursor: None,
            },
            events: Vec::new(),
            duplicate,
        }
    }
    fn version(&self, contest: ContestId, version: u64) -> Result<()> {
        if self.state.debate_id != contest || self.state.state_version != version {
            return Err(StoreError::Domain(
                ErrorCode::INVALID_STATE_TRANSITION,
                "Stale state",
            ));
        }
        Ok(())
    }
}
impl ContestRouter for FakeRouter {
    fn grill_state(&self, _: ContestId) -> Result<GrillState> {
        Err(StoreError::Unavailable)
    }
    fn open_debate_case(
        &mut self,
        event: ProducerEnvelope,
        _: DebateAdjudicationBundle,
    ) -> Result<CommitOutcome> {
        self.produce(event)
    }
    fn debate_state(&self, id: DebateId) -> Result<DebateState> {
        if self.state.debate_id != id {
            return Err(StoreError::Domain(
                ErrorCode::DEBATE_NOT_FOUND,
                "Unknown debate",
            ));
        }
        Ok(self.state.clone())
    }
    fn produce(&mut self, envelope: ProducerEnvelope) -> Result<CommitOutcome> {
        let id = envelope.base.id;
        if self.receipts.contains(&id) {
            return Ok(self.outcome(id));
        }
        match envelope.content {
            EnvelopePayload::DebatePreflight(preflight) => {
                preflight.validate().unwrap();
                self.state.state = if preflight.confirmation_required {
                    DebateStatus::AwaitingConfirmation
                } else {
                    DebateStatus::WaitingForDebaters
                };
                self.state.state_version += 1;
                self.preflights.push(preflight);
            }
            EnvelopePayload::CaseOpened(case) => {
                self.state.case_id = Some(case.case_id);
                self.state.case_ids.push(case.case_id);
            }
            _ => {}
        }
        Ok(self.outcome(id))
    }
    fn transition(
        &mut self,
        id: EventId,
        submission: ContestTransitionSubmission,
    ) -> Result<CommitOutcome> {
        if self.receipts.contains(&id) {
            return Ok(self.outcome(id));
        }
        self.version(submission.contest_id, submission.expected_state_version)?;
        let (_, _, transition) = submission.into_debate().unwrap();
        self.state = machine::advance(&self.state, &transition, self.opening)
            .map_err(|code| StoreError::Domain(code, "Invalid transition"))?;
        Ok(self.outcome(id))
    }
    fn barrier(&mut self, id: EventId, barrier: BarrierCommit) -> Result<CommitOutcome> {
        if self.receipts.contains(&id) {
            return Ok(self.outcome(id));
        }
        self.version(barrier.contest_id, barrier.expected_state_version)?;
        self.deliveries += barrier.projections.len();
        for work in barrier.next_work {
            self.work.insert(work.work_id, work);
        }
        Ok(self.outcome(id))
    }
    fn allocate_work(
        &mut self,
        id: EventId,
        contest: ContestId,
        version: u64,
        work: Vec<Work>,
    ) -> Result<CommitOutcome> {
        if self.receipts.contains(&id) {
            return Ok(self.outcome(id));
        }
        self.version(contest, version)?;
        for w in &work {
            w.validate().unwrap();
        }
        for w in work {
            self.work.insert(w.work_id, w);
        }
        Ok(self.outcome(id))
    }
    fn allocate_step(
        &mut self,
        id: EventId,
        step: ContestStep,
        projections: Vec<HistoryProjection>,
        work: Vec<Work>,
        _: BTreeMap<String, serde_json::Value>,
    ) -> Result<CommitOutcome> {
        if self.receipts.contains(&id) {
            return Ok(self.outcome(id));
        }
        for w in &work {
            assert_eq!(w.step_id, Some(step.step_id));
        }
        self.deliveries += projections.len();
        for w in work {
            self.work.insert(w.work_id, w);
        }
        Ok(self.outcome(id))
    }
}
