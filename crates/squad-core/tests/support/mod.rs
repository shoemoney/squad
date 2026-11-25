#![allow(dead_code)]
#[path = "../../../squad-agents/tests/support/fake_openrouter.rs"]
pub mod fake_openrouter;
use fake_openrouter::*;
use squad_agents::{openrouter::OpenRouterClient, resolver::*};
use squad_core::{
    bus::InProcessBus,
    router::Router,
    runtime::{self, RuntimeHandle},
};
use squad_protocol::*;
use squad_store::Store;
use std::{collections::BTreeMap, time::Duration};
pub struct Fixture {
    pub dir: tempfile::TempDir,
    pub store: Store,
    pub bus: InProcessBus,
    pub config: SquadConfig,
    pub fake: FakeOpenRouter,
    pub snapshot: ResolutionSnapshot,
}
impl Fixture {
    pub async fn new(count: usize, max: i64) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let env = BTreeMap::from([("OPENROUTER_API_KEY".into(), "SENTINEL-PRIVATE-KEY".into())]);
        let options = config::LoadOptions {
            env_file: None,
            data_dir: None,
            working_dir: dir.path().into(),
            config_dir: Some(dir.path().join("config")),
            default_data_dir: dir.path().join("data"),
        };
        let mut config = SquadConfig::load_with_env(&options, &env).unwrap();
        config.agents = (0..count)
            .map(|i| config::AgentConfig {
                nick: nick(&format!("member{i}")),
                model: MODEL.into(),
                persona: format!("PERSONA_{i}"),
                reasoning_effort: None,
            })
            .collect();
        config.judge.model = MODEL.into();
        let model = squad_agents::resolver::Model {
            id: MODEL.into(),
            context_length: 131072,
            output_limit: 65536,
            supported_parameters: vec!["reasoning".into()],
            reasoning_efforts: vec!["low".into(), "high".into()],
            pricing: serde_json::json!({}),
        };
        let pinned = |name: String| PinnedModel {
            nick: name,
            configured: MODEL.into(),
            model: Some(model.clone()),
            error: None,
            probes: vec![],
        };
        let snapshot = ResolutionSnapshot {
            catalog: CatalogSnapshot {
                fetched_at: 0,
                models: BTreeMap::from([(MODEL.into(), model.clone())]),
            },
            cached: false,
            members: config
                .agents
                .iter()
                .map(|a| pinned(a.nick.to_string()))
                .collect(),
            judge: pinned("judge".into()),
            diagnostics: vec![],
        };
        let mut bindings: BTreeMap<_, _> = config
            .agents
            .iter()
            .map(|a| {
                (
                    a.nick.to_string(),
                    ExecutorBinding {
                        model: Text::new(MODEL).unwrap(),
                        effort: ReasoningEffort::Medium,
                    },
                )
            })
            .collect();
        bindings.insert(
            "judge".into(),
            ExecutorBinding {
                model: Text::new(MODEL).unwrap(),
                effort: ReasoningEffort::High,
            },
        );
        let store = Store::open(dir.path().join("room.sqlite")).unwrap();
        let bus = InProcessBus::start(
            Router::new(
                store.clone(),
                config.agents.iter().map(|a| a.nick.clone()).collect(),
                bindings,
                max,
            )
            .unwrap(),
        );
        let fake = FakeOpenRouter::start().await;
        Self {
            dir,
            store,
            bus,
            config,
            fake,
            snapshot,
        }
    }
    pub fn start(&self, mode: OpenMode) -> RuntimeHandle {
        runtime::start_with_mode(
            &self.config,
            self.store.clone(),
            self.bus.clone(),
            OpenRouterClient::new(&self.fake.base_url(), "SENTINEL-PRIVATE-KEY").unwrap(),
            self.snapshot.clone(),
            mode,
        )
        .unwrap()
    }
    pub async fn send(&self, text: &str) -> Vec<WorkId> {
        let ctx = parser::ParseContext {
            roster: self.config.agents.iter().map(|a| a.nick.clone()).collect(),
            ..Default::default()
        };
        let meta = parser::SubmissionMeta::default();
        let parsed = parse(text, &meta, &ctx).unwrap();
        let id = EventId::new();
        let result = self
            .bus
            .send(Request::from_parsed(id, parsed, &meta, false).unwrap())
            .await;
        assert!(
            matches!(result, RequestResult::Accepted { .. }),
            "{result:?}"
        );
        self.store
            .call(move |db| {
                let mut query =
                    db.prepare("SELECT work_id FROM work WHERE trigger_event_id=? ORDER BY rowid")?;
                query
                    .query_map([id.to_string()], |r| r.get::<_, String>(0))?
                    .map(|s| Ok(s?.parse().unwrap()))
                    .collect::<squad_store::Result<Vec<_>>>()
            })
            .unwrap()
    }
    pub fn counts(&self) -> (i64, i64, i64) {
        self.store
            .call(|db| {
                Ok((
                    db.query_row(
                        "SELECT COUNT(*) FROM events WHERE kind='message.final'",
                        [],
                        |r| r.get(0),
                    )?,
                    db.query_row("SELECT COUNT(*) FROM attempts", [], |r| r.get(0))?,
                    db.query_row(
                        "SELECT COALESCE(SUM(spent_micros),0) FROM spend_daily",
                        [],
                        |r| r.get(0),
                    )?,
                ))
            })
            .unwrap()
    }
    pub async fn until(&self, condition: impl Fn() -> bool) {
        tokio::time::timeout(Duration::from_secs(10), async {
            while !condition() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
    }
    pub async fn claim(&self, id: WorkId, owner: ExecutorKey) -> ExecutionGrant {
        let submit = self.bus.executor_submit(owner);
        let RequestResult::Internal(InternalResult::Claimed(grant)) =
            submit.submit(ExecutorRequest::Claim { work_id: id }).await
        else {
            panic!("claim");
        };
        let RequestResult::Internal(InternalResult::Started(grant)) = submit
            .submit(ExecutorRequest::StartAttempt {
                work_id: id,
                attempt: grant.work.attempt,
                fence: grant.fence,
            })
            .await
        else {
            panic!("start");
        };
        grant
    }
}
pub fn nick(s: &str) -> MemberNick {
    MemberNick::new(s).unwrap()
}
pub fn reply(grant: &ExecutionGrant) -> ExecutorRequest {
    ExecutorRequest::Result {
        work_id: grant.work.work_id,
        attempt: grant.work.attempt,
        fence: grant.fence,
        epoch: grant.epoch,
        output: EnvelopePayload::AgentReply(Completion {
            completed: CompletedGeneration {
                generation: squad_agents::agents::identity(grant),
                model: grant.binding.model.clone(),
                finish_reason: FinishReason::Stop,
                usage: None,
            },
            in_reply_to_message_id: grant.work.in_reply_to_message_id.unwrap(),
            content: "accepted".into(),
        }),
    }
}
pub fn account(grant: &ExecutionGrant, cost: Option<f64>) -> UsageRecord {
    let mut record = squad_agents::accounting::AttemptAccounting::new(
        grant,
        chrono::Utc::now(),
        chrono::Local::now().format("%Y-%m-%d").to_string(),
    )
    .record;
    record.outcome = UsageOutcome::Completed;
    record.ended_at = Some(chrono::Utc::now());
    record.usage = Some(Usage {
        input_tokens: 11,
        output_tokens: 17,
        cost_usd: cost,
    });
    record
}
impl Fixture {
    pub fn standalone(&self, label: &str) -> (WorkId, CaseId) {
        let id = EventId::new();
        let label = label.to_owned();
        let quote = EventId::new();
        let outcome = self
            .store
            .commit(id, "fixture".into(), format!("{id}"), move |tx, group| {
                squad_core::router::cases::standalone(
                    tx,
                    group,
                    id,
                    UserMessageInput {
                        message_id: MessageId::from_event(id),
                        text: Text::new("@judge decide").unwrap(),
                        quote: Some(vec![quote]),
                        reply_to: None,
                    },
                    format!("QUESTION_{label}"),
                    vec![ResolvedQuote {
                        event_id: quote,
                        author: Actor::Member(nick("member0")),
                        text: format!("EVIDENCE_{label}"),
                    }],
                    &ExecutorBinding {
                        model: Text::new(MODEL).unwrap(),
                        effort: ReasoningEffort::High,
                    },
                )?;
                Ok(group.accepted(id))
            })
            .unwrap();
        outcome
            .events
            .iter()
            .find_map(|e| {
                if let EnvelopePayload::CaseOpened(c) = &e.content {
                    Some((c.work_id.unwrap(), c.case_id))
                } else {
                    None
                }
            })
            .unwrap()
    }
    pub fn round_case(&self) -> (WorkId, CaseId) {
        self.debate_case(false)
    }
    pub fn final_case(&self) -> (WorkId, CaseId) {
        self.debate_case(true)
    }
    fn debate_case(&self, final_verdict: bool) -> (WorkId, CaseId) {
        let trigger = EventId::new();
        let debate = DebateId::new();
        let case = CaseId::new();
        let work_id = WorkId::new();
        let state = DebateState {
            debate_id: debate,
            state: if final_verdict {
                DebateStatus::Judging
            } else {
                DebateStatus::RoundRunning
            },
            debaters: [nick("member0"), nick("member1")],
            round: 1,
            total_rounds: 1,
            judge_model: MODEL.into(),
            judge_requested: true,
            state_version: 1,
            terminal_reason: None,
            case_id: None,
            case_ids: vec![],
        };
        let setup = DebateSetup {
            debate_id: debate,
            topic_message_id: MessageId::from_event(trigger),
            topic_text: "topic".into(),
            solutions: state.debaters.clone().map(|nick| DebateSolution {
                nick,
                message_id: MessageId::new(),
                text: "solution".into(),
            }),
            instruction: "instruction".into(),
            rounds: 1,
            judge_model: MODEL.into(),
            judge_swapped: false,
        };
        let transcript = state.debaters.clone().map(|speaker| DebateTurn {
            generation: GenerationIdentity {
                message_id: MessageId::new(),
                work_id: WorkId::new(),
                trigger_event_id: trigger,
                attempt: 1,
            },
            debate_id: debate,
            round: 1,
            speaker,
            outcome: DebateTurnOutcome::Completed {
                content: "argument".into(),
                model: Text::new(MODEL).unwrap(),
                finish_reason: FinishReason::Stop,
                usage: None,
            },
        });
        let retained_turns = transcript.clone();
        // Seed accepted evidence; case opening and work admission use the router.
        self.store
            .commit(
                trigger,
                "fixture".into(),
                trigger.to_string(),
                move |tx, group| {
                    let seq = group.push(
                        tx,
                        squad_core::router::envelope(
                            trigger,
                            Actor::Member(retained_turns[0].speaker.clone()),
                            vec![Recipient::Me],
                            EnvelopePayload::DebateTurn(retained_turns[0].clone()),
                        ),
                    )?;
                    squad_store::contests::save_debate(tx, &state, None, seq)?;
                    group.push(
                        tx,
                        squad_core::router::envelope(
                            trigger,
                            Actor::Member(retained_turns[1].speaker.clone()),
                            vec![Recipient::Me],
                            EnvelopePayload::DebateTurn(retained_turns[1].clone()),
                        ),
                    )?;
                    Ok(group.accepted(trigger))
                },
            )
            .unwrap();
        let bundle = DebateAdjudicationBundle {
            topic: AdjudicationTopic {
                message_id: setup.topic_message_id,
                text: setup.topic_text.clone(),
            },
            solutions: setup.solutions.clone(),
            transcript: transcript.into(),
            instruction: setup.instruction.clone(),
            rubric: FrozenDebateRubric {
                version: "v1".into(),
                criteria: vec![],
                verdict_schema: serde_json::json!({}),
            },
            judge: JudgeProvenance {
                model: MODEL.into(),
                prompt_pack_version: "1.0.0".into(),
                rendered_template_hashes: BTreeMap::new(),
            },
            round_scoring: DebateScoringEvidence {
                round_scores: vec![],
                round_tally: BTreeMap::from([(nick("member0"), 0), (nick("member1"), 0)]),
                unscored_rounds: vec![],
            },
        };
        let opened = CaseOpened {
            case_id: case,
            judge_invocation_id: EventId::new(),
            work_id: Some(work_id),
            judge_model: Text::new(MODEL).unwrap(),
            material: CaseMaterial::Debate {
                setup,
                rubric_version: Text::new("v1").unwrap(),
            },
            debate_id: Some(debate),
            contest_id: Some(debate),
        };
        let binding = ExecutorBinding {
            model: Text::new(MODEL).unwrap(),
            effort: ReasoningEffort::High,
        };
        let mut bindings: BTreeMap<_, _> = self
            .config
            .agents
            .iter()
            .map(|a| (a.nick.to_string(), binding.clone()))
            .collect();
        bindings.insert("judge".into(), binding);
        let mut router = Router::new(
            self.store.clone(),
            self.config.agents.iter().map(|a| a.nick.clone()).collect(),
            bindings,
            20_000_000,
        )
        .unwrap();
        let event = squad_core::router::envelope(
            trigger,
            Actor::System,
            vec![Recipient::Me],
            EnvelopePayload::CaseOpened(opened),
        );
        let case_trigger = event.base.id;
        router.open_debate_case(event, bundle).unwrap();
        let context = ContextId(format!("judge:{case}"));
        let boundary_context = context.clone();
        let boundary = self
            .store
            .call(move |db| {
                let pos = db.query_row(
                    "SELECT MAX(pos) FROM history WHERE context_id=?",
                    [boundary_context.0],
                    |r| r.get(0),
                )?;
                Ok(HistoryPosition::new(pos).unwrap())
            })
            .unwrap();
        let state = router.debate_state(debate).unwrap();
        assert!(state.judge_requested);
        assert_eq!(state.case_id, Some(case));
        let work = Work {
            work_id,
            message_id: MessageId::new(),
            in_reply_to_message_id: None,
            kind: if final_verdict {
                WorkKind::Judge
            } else {
                WorkKind::JudgeRound
            },
            target: Target::Judge,
            context_id: context,
            trigger_event_id: case_trigger,
            history_through: boundary,
            attempt: 1,
            debate_id: Some(debate),
            round: if final_verdict { None } else { Some(1) },
            replay: None,
            case_id: Some(case),
            prompt_ref: Some(PromptRef {
                template_id: Text::new(if final_verdict {
                    "judge.debate"
                } else {
                    "judge.round.md"
                })
                .unwrap(),
                pack_version: Text::new("1.0.0").unwrap(),
                template_hash: Text::new("fixture").unwrap(),
            }),
            max_output_tokens: Some(if final_verdict {
                constants::FINAL_JUDGE_VISIBLE_TOKENS
            } else {
                constants::ROUND_JUDGE_VISIBLE_TOKENS
            }),
            contest_id: Some(debate),
            step_id: None,
        };
        router
            .allocate_work(EventId::new(), debate, state.state_version, vec![work])
            .unwrap();
        (work_id, case)
    }
    pub fn status(&self, id: WorkId) -> WorkStatus {
        self.store
            .call(move |db| Ok(squad_store::work::record(db, id)?.unwrap().status))
            .unwrap()
    }
}
pub fn valid_verdict() -> serde_json::Value {
    let scores = ["member0","member1"].map(|nick| {
        let criteria = ["fit","feasibility","originality","survivedAttacks"].map(|id| serde_json::json!({"criterionId":id,"score":5,"reasoning":"supported by the supplied evidence"}));
        serde_json::json!({"nick":nick,"criteria":criteria})
    });
    serde_json::json!({"winner":"member0","scores":scores,"reasoning":"Equal totals, member0 provides the stronger tie-breaker.","whatToStealFromLoser":[],"tallyOverrideReason":null})
}
