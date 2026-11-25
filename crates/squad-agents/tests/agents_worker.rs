use squad_agents::worker::{self, ExecutionFuture, WorkExecutor};
use squad_protocol::*;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio_util::sync::CancellationToken;

struct Job {
    grant: ExecutionGrant,
    runnable: bool,
    completed: bool,
}
struct Harness {
    jobs: Mutex<Vec<Job>>,
    requests: Mutex<Vec<ExecutorRequest>>,
    executions: Mutex<Vec<WorkId>>,
    release: tokio::sync::Semaphore,
}
impl Harness {
    fn new(count: usize) -> Arc<Self> {
        Arc::new(Self { jobs: Mutex::new((0..count).map(|_| {
            let work = serde_json::from_value(serde_json::json!({"workId":WorkId::new(),"messageId":MessageId::new(),"inReplyToMessageId":MessageId::new(),"kind":"reply","target":"alice","contextId":"alice","triggerEventId":EventId::new(),"historyThrough":"1","attempt":1})).unwrap();
            Job { grant: ExecutionGrant { binding: ExecutorBinding { model: Text::new("fixture/model").unwrap(), effort: ReasoningEffort::Low }, work, owner: ExecutorKey::Member(MemberNick::new("alice").unwrap()), fence: 1, epoch: 1, lease_until: chrono::Utc::now() + chrono::Duration::seconds(60), deadline: None }, runnable: true, completed: false }
        }).collect()), requests: Mutex::new(vec![]), executions: Mutex::new(vec![]), release: tokio::sync::Semaphore::new(0) })
    }
    fn context(self: &Arc<Self>) -> (AgentContext, tokio::sync::mpsc::Sender<Wakeup>) {
        let (tx, mailbox) = tokio::sync::mpsc::channel(1);
        (
            AgentContext {
                executor: ExecutorKey::Member(MemberNick::new("alice").unwrap()),
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
    async fn until_count(&self, count: usize) {
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if self.executions.lock().unwrap().len() >= count {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }
}
impl WorkReader for Harness {
    fn unfinished(&self) -> ReaderFuture<'_, Vec<WorkRecord>> {
        Box::pin(async {
            Ok(self
                .jobs
                .lock()
                .unwrap()
                .iter()
                .filter(|j| !j.completed)
                .map(|j| WorkRecord {
                    work_id: j.grant.work.work_id,
                    message_id: j.grant.work.message_id,
                    in_reply_to_message_id: j.grant.work.in_reply_to_message_id,
                    kind: j.grant.work.kind.clone(),
                    target: j.grant.work.target.clone(),
                    context_id: j.grant.work.context_id.clone(),
                    status: WorkStatus::Queued,
                    attempt: 1,
                    runnable: j.runnable,
                    executable: Some(j.grant.work.clone()),
                    lease_until: None,
                    deadline: None,
                    error: None,
                })
                .collect())
        })
    }
    fn get(&self, id: WorkId) -> ReaderFuture<'_, Option<WorkRecord>> {
        Box::pin(async move {
            Ok(self
                .unfinished()
                .await?
                .into_iter()
                .find(|r| r.work_id == id)
                .map(|mut r| {
                    r.status = WorkStatus::Running;
                    r
                }))
        })
    }
    fn repair_for_attempt(&self, _: WorkId, _: u32) -> ReaderFuture<'_, Option<JudgeRepairData>> {
        Box::pin(async { Ok(None) })
    }
}
impl HistoryReader for Harness {
    fn for_work(&self, _: WorkId) -> ReaderFuture<'_, Vec<HistoryEntry>> {
        Box::pin(async { Ok(vec![]) })
    }
}
impl PromptRenderer for Harness {
    fn render(&self, _: &PromptRef, _: &BTreeMap<String, String>) -> Result<String, ReaderError> {
        Ok(String::new())
    }
}
impl ExecutorSubmit for Harness {
    fn submit(&self, request: ExecutorRequest) -> SubmitFuture<'_> {
        Box::pin(async move {
            self.requests.lock().unwrap().push(request.clone());
            let jobs = self.jobs.lock().unwrap();
            match request {
                ExecutorRequest::Claim { work_id } => {
                    RequestResult::Internal(InternalResult::Claimed(
                        jobs.iter()
                            .find(|j| j.grant.work.work_id == work_id)
                            .unwrap()
                            .grant
                            .clone(),
                    ))
                }
                ExecutorRequest::StartAttempt { work_id, .. } => {
                    RequestResult::Internal(InternalResult::Started(
                        jobs.iter()
                            .find(|j| j.grant.work.work_id == work_id)
                            .unwrap()
                            .grant
                            .clone(),
                    ))
                }
                _ => RequestResult::Accepted {
                    request_id: EventId::new(),
                    event_ids: vec![],
                    cursor: None,
                },
            }
        })
    }
}
impl WorkExecutor for Harness {
    fn execute<'a>(
        &'a self,
        _: &'a AgentContext,
        grant: ExecutionGrant,
        cancel: CancellationToken,
    ) -> ExecutionFuture<'a> {
        Box::pin(async move {
            self.executions.lock().unwrap().push(grant.work.work_id);
            tokio::select! { biased; _ = cancel.cancelled() => return Err(ReaderError::StaleWork), permit = self.release.acquire() => permit.unwrap().forget() }
            self.jobs
                .lock()
                .unwrap()
                .iter_mut()
                .find(|j| j.grant.work.work_id == grant.work.work_id)
                .unwrap()
                .completed = true;
            Ok(())
        })
    }
}
#[tokio::test]
async fn fifo_and_duplicate_wakeups_never_overlap_generations() {
    let h = Harness::new(3);
    let order: Vec<_> = h
        .jobs
        .lock()
        .unwrap()
        .iter()
        .map(|j| j.grant.work.work_id)
        .collect();
    let (ctx, tx) = h.context();
    let cancel = ctx.cancel.clone();
    let task = worker::spawn(ctx, h.clone());
    h.until_count(1).await;
    tx.try_send(Wakeup).unwrap();
    assert!(tx.try_send(Wakeup).is_err());
    assert_eq!(h.executions.lock().unwrap().len(), 1);
    h.release.add_permits(1);
    h.until_count(2).await;
    h.release.add_permits(1);
    h.until_count(3).await;
    h.release.add_permits(1);
    cancel.cancel();
    task.await.unwrap().unwrap();
    assert_eq!(*h.executions.lock().unwrap(), order);
}
#[tokio::test]
async fn held_ordinary_queue_allows_runnable_contest_then_resumes_on_release() {
    let h = Harness::new(2);
    {
        let mut jobs = h.jobs.lock().unwrap();
        jobs[0].runnable = false;
        jobs[1].grant.work.kind = WorkKind::DebateTurn;
        let contest = ContestId::new();
        jobs[1].grant.work.debate_id = Some(contest);
        jobs[1].grant.work.contest_id = Some(contest);
        jobs[1].grant.work.round = Some(0);
    }
    let ids: Vec<_> = h
        .jobs
        .lock()
        .unwrap()
        .iter()
        .map(|j| j.grant.work.work_id)
        .collect();
    let (ctx, tx) = h.context();
    let cancel = ctx.cancel.clone();
    let task = worker::spawn(ctx, h.clone());
    h.until_count(1).await;
    assert_eq!(h.executions.lock().unwrap()[0], ids[1]);
    h.jobs.lock().unwrap()[0].runnable = true;
    h.release.add_permits(1);
    tx.try_send(Wakeup).unwrap();
    h.until_count(2).await;
    assert_eq!(h.executions.lock().unwrap()[1], ids[0]);
    cancel.cancel();
    task.await.unwrap().unwrap();
}
#[tokio::test]
async fn bounded_queue_reports_canonical_depth_and_dispatches_one_at_a_time() {
    let h = Harness::new(constants::AGENT_QUEUE_DEPTH + 3);
    let (ctx, _tx) = h.context();
    let cancel = ctx.cancel.clone();
    let task = worker::spawn(ctx, h.clone());
    h.until_count(1).await;
    assert!(h.requests.lock().unwrap().iter().any(|r| matches!(r, ExecutorRequest::Presence(p) if p.status == PresenceStatus::Thinking && p.queue_depth == constants::AGENT_QUEUE_DEPTH as u32)));
    assert_eq!(h.executions.lock().unwrap().len(), 1);
    cancel.cancel();
    task.await.unwrap().unwrap();
}

#[tokio::test(start_paused = true)]
async fn heartbeats_continue_while_a_generation_is_active() {
    let h = Harness::new(1);
    let (ctx, _tx) = h.context();
    let cancel = ctx.cancel.clone();
    let task = worker::spawn(ctx, h.clone());
    h.until_count(1).await;
    tokio::time::advance(Duration::from_secs(5)).await;
    tokio::task::yield_now().await;
    assert!(h.requests.lock().unwrap().iter().filter(|r| matches!(r, ExecutorRequest::Presence(p) if p.status == PresenceStatus::Thinking)).count() >= 2);
    cancel.cancel();
    task.await.unwrap().unwrap();
    assert!(
        h.requests.lock().unwrap().iter().any(
            |r| matches!(r, ExecutorRequest::Presence(p) if p.status == PresenceStatus::Offline)
        )
    );
}
