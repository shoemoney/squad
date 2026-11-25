#![allow(dead_code)]
#[path = "support/fake_openrouter.rs"]
mod fake;
use squad_agents::{
    accounting::AttemptAccounting, agents::member::MemberExecutor, context::*, openrouter::*,
    worker,
};
use squad_protocol::*;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio_util::sync::CancellationToken;

struct Fixture {
    grant: Mutex<ExecutionGrant>,
    history: Mutex<Vec<HistoryEntry>>,
    requests: Mutex<Vec<ExecutorRequest>>,
    status: Mutex<WorkStatus>,
    reject_result: bool,
}
impl Fixture {
    fn new(nick: &str, text: &str) -> Arc<Self> {
        let work: Work = serde_json::from_value(serde_json::json!({
            "workId": WorkId::new(), "messageId": MessageId::new(), "inReplyToMessageId": MessageId::new(),
            "kind":"reply", "target":nick,"contextId":nick,"triggerEventId":EventId::new(),"historyThrough":"1","attempt":1
        })).unwrap();
        Arc::new(Self {
            grant: Mutex::new(ExecutionGrant {
                binding: ExecutorBinding {
                    model: Text::new(fake::MODEL).unwrap(),
                    effort: ReasoningEffort::Low,
                },
                owner: ExecutorKey::Member(MemberNick::new(nick).unwrap()),
                work,
                fence: 1,
                epoch: 1,
                lease_until: chrono::Utc::now() + chrono::Duration::seconds(60),
                deadline: None,
            }),
            history: Mutex::new(vec![history(1, Actor::Me, text)]),
            requests: Mutex::new(vec![]),
            status: Mutex::new(WorkStatus::Queued),
            reject_result: false,
        })
    }
    fn context(self: &Arc<Self>) -> (AgentContext, tokio::sync::mpsc::Sender<Wakeup>) {
        let (tx, mailbox) = tokio::sync::mpsc::channel(1);
        (
            AgentContext {
                executor: self.grant.lock().unwrap().owner.clone(),
                mailbox,
                history: self.clone(),
                work: self.clone(),
                submit: self.clone(),
                render: self.clone(),
                cancel: CancellationToken::new(),
            },
            tx,
        )
    }
}
fn history(position: u64, author: Actor, content: &str) -> HistoryEntry {
    serde_json::from_value(serde_json::json!({"position":position.to_string(),"role":"user","name":"FORGED", "content":content,
        "meta":{"originEventId": EventId::new(),"originSeq":position.to_string(),"author":author,"projectionKey":"fixture"}})).unwrap()
}
impl HistoryReader for Fixture {
    fn for_work(&self, id: WorkId) -> ReaderFuture<'_, Vec<HistoryEntry>> {
        Box::pin(async move {
            if id != self.grant.lock().unwrap().work.work_id {
                return Err(ReaderError::Forbidden);
            }
            Ok(self.history.lock().unwrap().clone())
        })
    }
}
impl WorkReader for Fixture {
    fn prompt_values_for_work(&self, _: WorkId) -> ReaderFuture<'_, BTreeMap<String, String>> {
        Box::pin(async {
            Ok(BTreeMap::from([
                ("opponent".into(), "bob".into()),
                ("rounds".into(), "3".into()),
            ]))
        })
    }

    fn unfinished(&self) -> ReaderFuture<'_, Vec<WorkRecord>> {
        Box::pin(async {
            let grant = self.grant.lock().unwrap();
            let status = self.status.lock().unwrap().clone();
            Ok(
                if matches!(status, WorkStatus::Completed | WorkStatus::Cancelled) {
                    vec![]
                } else {
                    vec![WorkRecord {
                        work_id: grant.work.work_id,
                        message_id: grant.work.message_id,
                        in_reply_to_message_id: grant.work.in_reply_to_message_id,
                        kind: grant.work.kind.clone(),
                        target: grant.work.target.clone(),
                        context_id: grant.work.context_id.clone(),
                        status,
                        attempt: grant.work.attempt,
                        runnable: true,
                        executable: Some(grant.work.clone()),
                        lease_until: None,
                        deadline: None,
                        error: None,
                    }]
                },
            )
        })
    }
    fn get(&self, _: WorkId) -> ReaderFuture<'_, Option<WorkRecord>> {
        Box::pin(async { Ok(self.unfinished().await?.into_iter().next()) })
    }
    fn repair_for_attempt(&self, _: WorkId, _: u32) -> ReaderFuture<'_, Option<JudgeRepairData>> {
        Box::pin(async { Ok(None) })
    }
}
impl PromptRenderer for Fixture {
    fn render(
        &self,
        _: &PromptRef,
        values: &BTreeMap<String, String>,
    ) -> Result<String, ReaderError> {
        Ok(serde_json::to_string(values).unwrap())
    }
}
impl ExecutorSubmit for Fixture {
    fn submit(&self, request: ExecutorRequest) -> SubmitFuture<'_> {
        Box::pin(async move {
            self.requests.lock().unwrap().push(request.clone());
            let mut grant = self.grant.lock().unwrap();
            match request {
                ExecutorRequest::Claim { .. } => {
                    RequestResult::Internal(InternalResult::Claimed(grant.clone()))
                }
                ExecutorRequest::StartAttempt { attempt, fence, .. } if fence == grant.fence => {
                    grant.work.attempt = attempt;
                    *self.status.lock().unwrap() = WorkStatus::Running;
                    RequestResult::Internal(InternalResult::Started(grant.clone()))
                }
                ExecutorRequest::RenewLease { .. } => {
                    RequestResult::Internal(InternalResult::LeaseRenewed {
                        work_id: grant.work.work_id,
                        fence: grant.fence,
                        lease_until: grant.lease_until,
                    })
                }
                ExecutorRequest::Result { attempt, fence, .. }
                    if attempt != grant.work.attempt
                        || fence != grant.fence
                        || self.reject_result
                        || *self.status.lock().unwrap() == WorkStatus::Cancelled =>
                {
                    RequestResult::Internal(InternalResult::NoWork)
                }
                ExecutorRequest::Result { .. } => {
                    *self.status.lock().unwrap() = WorkStatus::Completed;
                    accepted()
                }
                ExecutorRequest::Failed { .. } => {
                    *self.status.lock().unwrap() = WorkStatus::Failed;
                    accepted()
                }
                _ => accepted(),
            }
        })
    }
}
fn accepted() -> RequestResult {
    RequestResult::Accepted {
        request_id: EventId::new(),
        event_ids: vec![],
        cursor: None,
    }
}
fn prompt() -> MemberPrompt {
    MemberPrompt {
        system: PromptRef {
            template_id: Text::new("room.system.md").unwrap(),
            pack_version: Text::new("fixture").unwrap(),
            template_hash: Text::new("fixture").unwrap(),
        },
        roster: vec![
            MemberNick::new("alice").unwrap(),
            MemberNick::new("bob").unwrap(),
        ],
        user_name: "Jeremy".into(),
        persona: "precise".into(),
    }
}
fn executor(server: &fake::FakeOpenRouter) -> Arc<MemberExecutor> {
    Arc::new(MemberExecutor {
        client: OpenRouterClient::new(&server.base_url(), "fixture-key").unwrap(),
        request: ChatRequest {
            model: fake::MODEL.into(),
            messages: vec![],
            visible_ceiling: 100,
            published_output_limit: 8192,
            supports_reasoning: true,
            reasoning_effort: ReasoningEffort::Low,
            reasoning_explicit: false,
            response_format: None,
            policy: ExecutionPolicy::SingleAttempt(DeadlineClass::Ordinary),
        },
        prompt: prompt(),
        context_length: 32768,
    })
}
async fn wait_completed(fixture: &Fixture) {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if *fixture.status.lock().unwrap() == WorkStatus::Completed {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn concurrent_members_have_no_foreign_history_or_reasoning_in_ingress() {
    let server = fake::FakeOpenRouter::start().await;
    server
        .completions(vec![
            fake::streaming("answer-a", "stop"),
            fake::streaming("answer-b", "stop"),
        ])
        .await;
    let alice = Fixture::new("alice", "ALICE_PRIVATE_CANARY");
    let bob = Fixture::new("bob", "BOB_PRIVATE_CANARY");
    let (ac, _at) = alice.context();
    let a_cancel = ac.cancel.clone();
    let (bc, _bt) = bob.context();
    let b_cancel = bc.cancel.clone();
    let a = worker::spawn(ac, executor(&server));
    let b = worker::spawn(bc, executor(&server));
    tokio::join!(wait_completed(&alice), wait_completed(&bob));
    a_cancel.cancel();
    b_cancel.cancel();
    a.await.unwrap().unwrap();
    b.await.unwrap().unwrap();
    let bodies = server.captured_bodies().await;
    assert_eq!(bodies.len(), 2);
    for body in bodies {
        let text = body.to_string();
        assert_ne!(
            text.contains("ALICE_PRIVATE_CANARY"),
            text.contains("BOB_PRIVATE_CANARY")
        );
        assert!(text.contains("Jeremy"));
        assert!(text.contains("precise"));
        assert_eq!(body["model"], fake::MODEL);
    }
    for fixture in [&alice, &bob] {
        let requests = fixture.requests.lock().unwrap().clone();
        assert_eq!(
            requests
                .iter()
                .filter(|r| matches!(r, ExecutorRequest::Result { .. }))
                .count(),
            1
        );
        assert!(!format!("{requests:?}").contains("PRIVATE REASONING"));
        assert!(
            requests
                .iter()
                .any(|r| matches!(r, ExecutorRequest::Account { .. }))
        );
    }
}
#[tokio::test]
async fn context_preserves_roles_quotes_and_removes_whole_old_exchange() {
    let f = Fixture::new("alice", "current");
    *f.history.lock().unwrap() = vec![
        history(1, Actor::Me, &"old".repeat(900)),
        history(
            2,
            Actor::Member(MemberNick::new("alice").unwrap()),
            "old answer",
        ),
        history(3, Actor::Me, "current [bob quote: preserved]"),
    ];
    f.grant.lock().unwrap().work.history_through =
        serde_json::from_value(serde_json::json!("3")).unwrap();
    let (ctx, _) = f.context();
    let grant = f.grant.lock().unwrap().clone();
    let messages = member_messages(&ctx, &grant, &prompt(), 2048, 100)
        .await
        .unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[1].name.as_deref(), Some("me"));
    assert!(messages[1].content.contains("bob quote"));
    assert!(!messages.iter().any(|m| m.content.contains("old answer")));
}
#[tokio::test]
async fn boundary_and_oversized_mandatory_material_fail_closed() {
    let f = Fixture::new("alice", "current");
    let (ctx, _) = f.context();
    f.history
        .lock()
        .unwrap()
        .push(history(2, Actor::Me, "future"));
    let grant = f.grant.lock().unwrap().clone();
    assert!(read_history(&ctx, &grant).await.is_err());
    assert!(matches!(
        judge_messages("system".into(), "case".repeat(1000), 4096, 1000),
        Err(ContextError::TooLarge { .. })
    ));
}
#[tokio::test]
async fn accounting_reconciles_same_attempt_and_keeps_unknown_cost() {
    let f = Fixture::new("alice", "current");
    let grant = f.grant.lock().unwrap().clone();
    let mut account = AttemptAccounting::new(&grant, chrono::Utc::now(), "2026-11-24".into());
    account.usage(&squad_agents::openrouter::Usage {
        prompt_tokens: Some(10),
        completion_tokens: Some(20),
        ..Default::default()
    });
    assert_eq!(account.record.usage.as_ref().unwrap().cost_usd, None);
    let server = fake::FakeOpenRouter::start().await;
    server.generation("known", serde_json::json!({"tokens_prompt":10,"tokens_completion":20,"total_cost":0.01,"model":"fixture/response"})).await;
    let client = OpenRouterClient::new(&server.base_url(), "fixture-key").unwrap();
    account.metadata(Some("known".into()), None);
    account.reconcile(&client).await.unwrap();
    account.reconcile(&client).await.unwrap();
    account.submit(f.as_ref()).await.unwrap();
    account.submit(f.as_ref()).await.unwrap();
    assert_eq!(account.record.usage.as_ref().unwrap().cost_usd, Some(0.01));
    assert_eq!(account.record.attempt, 1);
    assert_eq!(account.record.model.as_str(), fake::MODEL);
    assert_eq!(
        account.record.response_model.unwrap().as_str(),
        "fixture/response"
    );
    assert_eq!(server.server.received_requests().await.unwrap().len(), 1);
}
#[tokio::test]
async fn cancelled_queued_worker_makes_no_provider_call() {
    let server = fake::FakeOpenRouter::start().await;
    let f = Fixture::new("alice", "current");
    let (ctx, _) = f.context();
    ctx.cancel.cancel();
    worker::spawn(ctx, executor(&server))
        .await
        .unwrap()
        .unwrap();
    assert!(server.captured_bodies().await.is_empty());
}
#[tokio::test]
async fn result_rejection_is_never_treated_as_acceptance() {
    let f = Fixture::new("alice", "current");
    let (ctx, _) = f.context();
    let grant = f.grant.lock().unwrap().clone();
    *f.status.lock().unwrap() = WorkStatus::Cancelled;
    assert!(worker::start_attempt(&ctx, &grant, 1).await.is_ok());
    let mut stale = grant;
    stale.fence += 1;
    assert!(worker::start_attempt(&ctx, &stale, 2).await.is_err());
    assert!(
        squad_agents::accounting::acknowledged(RequestResult::Internal(InternalResult::NoWork))
            .is_err()
    );
}
#[test]
fn accounting_rejects_invalid_costs_without_fabricating_zero() {
    let f = Fixture::new("alice", "current");
    let grant = f.grant.lock().unwrap().clone();
    let mut a = AttemptAccounting::new(&grant, chrono::Utc::now(), "2026-11-24".into());
    a.usage(&squad_agents::openrouter::Usage {
        prompt_tokens: Some(1),
        completion_tokens: Some(2),
        cost: Some(f64::NAN),
        ..Default::default()
    });
    assert_eq!(a.record.usage.as_ref().unwrap().cost_usd, None);
    a.record.estimated_cost_usd = Some(-1.0);
    assert!(a.record.validate().is_err());
}

#[tokio::test]
async fn ordinary_retry_has_two_durable_attempts_and_replaces_partial_output() {
    let server = fake::FakeOpenRouter::start().await;
    server
        .completions(vec![
            fake::raw_sse(format!("data: {}\n\n", serde_json::json!({"id":"gen-first", "model":fake::MODEL, "choices":[{"index":0,"delta":{"content":"PARTIAL_OLD"}}]}))),
            fake::streaming("recovered", "stop"),
        ])
        .await;
    let f = Fixture::new("alice", "current");
    let (ctx, _tx) = f.context();
    let cancel = ctx.cancel.clone();
    let task = worker::spawn(ctx, executor(&server));
    wait_completed(&f).await;
    cancel.cancel();
    task.await.unwrap().unwrap();
    let requests = f.requests.lock().unwrap().clone();
    let starts: Vec<_> = requests
        .iter()
        .filter_map(|r| {
            if let ExecutorRequest::StartAttempt { attempt, .. } = r {
                Some(*attempt)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(starts, vec![1, 2]);
    assert!(requests.iter().any(|r| matches!(r, ExecutorRequest::Delta { delta, .. } if delta.generation.attempt == 1 && delta.text == "PARTIAL_OLD")));
    assert!(requests.iter().any(|r| matches!(r, ExecutorRequest::Result { attempt: 2, output: EnvelopePayload::AgentReply(reply), .. } if reply.content == "recovered")));

    let attempts: std::collections::BTreeSet<_> = requests
        .iter()
        .filter_map(|r| {
            if let ExecutorRequest::Account { record, .. } = r {
                Some(record.attempt)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(attempts, std::collections::BTreeSet::from([1, 2]));
    assert!(
        requests
            .iter()
            .any(|r| matches!(r, ExecutorRequest::Result { attempt: 2, .. }))
    );
    drop(requests);
    assert_eq!(server.captured_bodies().await.len(), 2);
}
#[tokio::test]
async fn cancellation_interrupts_active_http_and_retains_unknown_charge() {
    let server = fake::FakeOpenRouter::start().await;
    server
        .completions(vec![
            fake::streaming("late", "stop").set_delay(Duration::from_secs(30)),
        ])
        .await;
    let f = Fixture::new("alice", "current");
    let (ctx, _tx) = f.context();
    let cancel = ctx.cancel.clone();
    let task = worker::spawn(ctx, executor(&server));
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if !server.captured_bodies().await.is_empty() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    cancel.cancel();
    tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let requests = f.requests.lock().unwrap().clone();
    assert!(
        !requests
            .iter()
            .any(|r| matches!(r, ExecutorRequest::Result { .. }))
    );
    assert!(requests.iter().any(|r| matches!(r, ExecutorRequest::Account { record, .. } if record.outcome == UsageOutcome::Cancelled && record.usage.is_none())));
}
#[tokio::test]
async fn unscored_notices_and_author_labels_are_never_system_instructions() {
    let f = Fixture::new("alice", "current");
    *f.history.lock().unwrap() = vec![
        history(1, Actor::System, "unscored: no evidence"),
        history(
            2,
            Actor::Member(MemberNick::new("bob").unwrap()),
            "authorized opponent",
        ),
        history(3, Actor::Me, "current"),
    ];
    f.grant.lock().unwrap().work.history_through =
        serde_json::from_value(serde_json::json!("3")).unwrap();
    let (ctx, _) = f.context();
    let grant = f.grant.lock().unwrap().clone();
    let messages = member_messages(&ctx, &grant, &prompt(), 32768, 2000)
        .await
        .unwrap();
    assert_eq!(messages.iter().filter(|m| m.role == "system").count(), 1);
    assert_eq!(messages[1].role, "user");
    assert_eq!(messages[1].name.as_deref(), Some("squad"));
    assert!(messages[1].content.starts_with("[squad notice]"));
    assert_eq!(messages[2].role, "user");
    assert_eq!(messages[2].name.as_deref(), Some("bob"));
}
#[tokio::test]
async fn contest_material_is_mandatory_and_cannot_be_dropped_for_budget() {
    let f = Fixture::new("alice", "current");
    *f.history.lock().unwrap() = vec![
        history(1, Actor::Me, &"evidence".repeat(1000)),
        history(2, Actor::Me, "current"),
    ];
    let mut grant = f.grant.lock().unwrap().clone();
    grant.work.contest_id = Some(ContestId::new());
    grant.work.history_through = serde_json::from_value(serde_json::json!("2")).unwrap();
    let (ctx, _) = f.context();
    assert!(matches!(
        member_messages(&ctx, &grant, &prompt(), 4096, 1000).await,
        Err(ContextError::TooLarge { .. })
    ));
}
#[test]
fn judge_evidence_and_repair_are_budgeted_once_in_one_user_message() {
    let messages = judge_messages(
        "system fixture".into(),
        "case evidence, own scores, repair errors and rejected output".into(),
        8192,
        1536,
    )
    .unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[1].role, "user");
    assert_eq!(
        messages
            .iter()
            .filter(|m| m.content.contains("case evidence"))
            .count(),
        1
    );
    assert_eq!(input_budget(32768, 1536), 32768 - 1536 - 1639);
}

#[tokio::test]
async fn case_bound_history_never_contains_a_different_case_or_member() {
    let a = Fixture::new("alice", "CASE_A_ONLY");
    let b = Fixture::new("alice", "CASE_B_ONLY");
    for f in [&a, &b] {
        let mut grant = f.grant.lock().unwrap();
        let case = CaseId::new();
        grant.owner = ExecutorKey::Case(case);
        grant.work.kind = WorkKind::Judge;
        grant.work.case_id = Some(case);
        grant.work.target = Target::Judge;
        grant.work.context_id = ContextId(format!("judge:{case}"));
    }
    let (ac, _) = a.context();
    let (bc, _) = b.context();
    let ag = a.grant.lock().unwrap().clone();
    let bg = b.grant.lock().unwrap().clone();
    let ah = read_history(&ac, &ag).await.unwrap();
    let bh = read_history(&bc, &bg).await.unwrap();
    assert_eq!(ah.len(), 1);
    assert_eq!(bh.len(), 1);
    assert!(ah[0].content.contains("CASE_A_ONLY"));
    assert!(!ah[0].content.contains("CASE_B_ONLY"));
    assert!(read_history(&ac, &bg).await.is_err());
    let server = fake::FakeOpenRouter::start().await;
    server
        .completions(vec![
            fake::streaming("ruling A", "stop"),
            fake::streaming("ruling B", "stop"),
        ])
        .await;
    for (ctx, grant, evidence) in [(&ac, &ag, &ah), (&bc, &bg, &bh)] {
        let mut request = executor(&server).request.clone();
        request.messages = judge_messages(
            "judge-only system".into(),
            evidence[0].content.clone(),
            32768,
            8192,
        )
        .unwrap();
        squad_agents::agents::stream_attempt(
            ctx,
            grant,
            &OpenRouterClient::new(&server.base_url(), "fixture-key").unwrap(),
            request,
            CancellationToken::new(),
            false,
        )
        .await
        .unwrap();
    }
    for body in server.captured_bodies().await {
        let text = body.to_string();
        assert_ne!(text.contains("CASE_A_ONLY"), text.contains("CASE_B_ONLY"));
        assert_eq!(body["messages"].as_array().unwrap().len(), 2);
        assert!(!text.contains("precise"));
        assert!(!text.contains("ALICE_PRIVATE"));
    }
}
#[test]
fn partial_usage_does_not_invent_token_totals_or_drop_a_known_cost() {
    let f = Fixture::new("alice", "current");
    let grant = f.grant.lock().unwrap().clone();
    let mut a = AttemptAccounting::new(&grant, chrono::Utc::now(), "2026-11-24".into());
    a.usage(&squad_agents::openrouter::Usage {
        cost: Some(0.002),
        ..Default::default()
    });
    assert!(a.record.usage.is_none());
    a.usage(&squad_agents::openrouter::Usage {
        prompt_tokens: Some(1),
        completion_tokens: Some(2),
        ..Default::default()
    });
    assert_eq!(a.record.usage.as_ref().unwrap().cost_usd, Some(0.002));
    let restored = AttemptAccounting::from_record(a.record.clone(), 2, 1).unwrap();
    assert_eq!(restored.record, a.record);
}

#[tokio::test]
async fn debate_instruction_uses_frozen_work_metadata_through_renderer() {
    let f = Fixture::new("alice", "authorized topic and opening");
    let (ctx, _) = f.context();
    let mut grant = f.grant.lock().unwrap().clone();
    let debate = ContestId::new();
    grant.work.kind = WorkKind::DebateTurn;
    grant.work.debate_id = Some(debate);
    grant.work.contest_id = Some(debate);
    grant.work.round = Some(0);
    grant.work.prompt_ref = Some(PromptRef {
        template_id: Text::new("debate.round0.md").unwrap(),
        pack_version: Text::new("fixture").unwrap(),
        template_hash: Text::new("fixture").unwrap(),
    });
    let messages = member_messages(&ctx, &grant, &prompt(), 32768, 1536)
        .await
        .unwrap();
    assert_eq!(messages.last().unwrap().name.as_deref(), Some("squad"));
    assert!(messages.last().unwrap().content.contains("opponent"));
    assert!(messages.last().unwrap().content.contains("bob"));
    assert_eq!(
        messages
            .iter()
            .filter(|m| m.content.contains("authorized topic"))
            .count(),
        1
    );
}
#[tokio::test]
async fn permanent_member_failure_is_reported_without_engine_transition_or_retry() {
    let server = fake::FakeOpenRouter::start().await;
    server
        .completions(vec![wiremock::ResponseTemplate::new(400)])
        .await;
    let f = Fixture::new("alice", "current");
    let (ctx, _tx) = f.context();
    let cancel = ctx.cancel.clone();
    let task = worker::spawn(ctx, executor(&server));
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if *f.status.lock().unwrap() == WorkStatus::Failed {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    cancel.cancel();
    task.await.unwrap().unwrap();
    assert_eq!(server.captured_bodies().await.len(), 1);
    let requests = f.requests.lock().unwrap().clone();
    assert!(requests.iter().any(|r| matches!(r, ExecutorRequest::Failed { error, .. } if error.code == ErrorCode::PROVIDER_ERROR && error.retry_action.is_some())));
    assert!(
        !requests
            .iter()
            .any(|r| matches!(r, ExecutorRequest::Result { .. }))
    );
}
#[test]
fn round_score_accounting_retains_case_round_work_and_attempt_identity() {
    let f = Fixture::new("alice", "current");
    let mut grant = f.grant.lock().unwrap().clone();
    let debate = ContestId::new();
    let case = CaseId::new();
    grant.work.kind = WorkKind::JudgeRound;
    grant.work.debate_id = Some(debate);
    grant.work.contest_id = Some(debate);
    grant.work.case_id = Some(case);
    grant.work.round = Some(2);
    grant.work.target = Target::Judge;
    grant.owner = ExecutorKey::Case(case);
    grant.work.context_id = ContextId(format!("judge:{case}"));
    grant.work.attempt = 2;
    let mut account = AttemptAccounting::new(&grant, chrono::Utc::now(), "2026-11-24".into());
    account.finish(UsageOutcome::Cancelled, chrono::Utc::now());
    let restored = AttemptAccounting::from_record(account.record, 2, 1).unwrap();
    assert_eq!(restored.record.work_id, grant.work.work_id);
    assert_eq!(restored.record.attempt, 2);
    assert_eq!(restored.record.debate_id, Some(debate));
    assert_eq!(restored.record.case_id, Some(case));
    assert_eq!(restored.record.target, Target::Judge);
    assert_eq!(restored.record.usage, None);
    assert_eq!(restored.record.outcome, UsageOutcome::Cancelled);
}

#[tokio::test]
async fn regeneration_uses_original_reply_anchor_and_preserves_frozen_quote_text() {
    let server = fake::FakeOpenRouter::start().await;
    server
        .completions(vec![fake::streaming("replacement", "stop")])
        .await;
    let f = Fixture::new(
        "alice",
        "current; author-labelled quote of superseded answer is retained",
    );
    let anchor = f.grant.lock().unwrap().work.in_reply_to_message_id.unwrap();
    f.grant.lock().unwrap().work.trigger_event_id = EventId::new();
    let (ctx, _tx) = f.context();
    let cancel = ctx.cancel.clone();
    let task = worker::spawn(ctx, executor(&server));
    wait_completed(&f).await;
    cancel.cancel();
    task.await.unwrap().unwrap();
    let requests = f.requests.lock().unwrap().clone();
    assert!(requests.iter().any(|r| matches!(r, ExecutorRequest::Result { output: EnvelopePayload::AgentReply(reply), .. } if reply.in_reply_to_message_id == anchor && reply.content == "replacement")));
    drop(requests);
    assert!(
        server.captured_bodies().await[0]
            .to_string()
            .contains("author-labelled quote of superseded answer")
    );
}
