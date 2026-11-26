use super::*;
use std::{
    io::Write,
    sync::{Arc, Mutex},
};

#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<u8>>>);
impl Write for Capture {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Capture {
    type Writer = Self;
    fn make_writer(&'a self) -> Self {
        self.clone()
    }
}

#[test]
fn step_attempt_barrier_and_report_traces_exclude_bodies() {
    let capture = Capture::default();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_ansi(false)
        .without_time()
        .with_writer(capture.clone())
        .finish();
    let contest = ContestId::new();
    let step = StepId::new();
    let work_id = WorkId::new();
    let request_id = EventId::new();
    let work = Work {
        work_id,
        message_id: MessageId::new(),
        in_reply_to_message_id: None,
        kind: WorkKind::SurveyReport,
        target: Target::Judge,
        context_id: ContextId("case:fixture".into()),
        trigger_event_id: request_id,
        history_through: HistoryPosition::new(1).unwrap(),
        attempt: 2,
        debate_id: None,
        round: None,
        replay: None,
        case_id: None,
        prompt_ref: None,
        max_output_tokens: Some(4096),
        contest_id: Some(contest),
        step_id: Some(step),
    };
    let grant = ExecutionGrant {
        work,
        owner: ExecutorKey::Case(CaseId::new()),
        binding: ExecutorBinding {
            model: Text::new("fixture/judge").unwrap(),
            effort: ReasoningEffort::Medium,
        },
        fence: 1,
        epoch: 1,
        lease_until: "2026-11-26T00:00:00Z".parse().unwrap(),
        deadline: None,
    };
    let event: CommittedEnvelope = serde_json::from_value(serde_json::json!({
        "version":1,"id":EventId::new(),"roomId":"main","correlationId":request_id,
        "sender":"system","recipients":["me"],"createdAt":"2026-11-26T00:00:00Z",
        "seq":"1","acceptedAt":"2026-11-26T00:00:00Z","commitIndex":0,"commitSize":1,
        "kind":"contest.report","payload":{"contestId":contest,"stepId":step,
        "artifactEventIds":[],"skippedWorkIds":[],"data":{"body":"PRIVATE BODY","reasoning":"PRIVATE REASONING"}}
    })).unwrap();
    let outcome = CommitOutcome {
        result: RequestResult::Internal(InternalResult::Started(grant)),
        events: vec![event],
        duplicate: false,
    };
    tracing::subscriber::with_default(subscriber, || trace_outcome(request_id, &outcome));
    let text = String::from_utf8(capture.0.lock().unwrap().clone()).unwrap();
    for id in [
        contest.to_string(),
        step.to_string(),
        work_id.to_string(),
        request_id.to_string(),
    ] {
        assert!(text.contains(&id), "missing {id}: {text}");
    }
    assert!(text.contains("attempt=2"));
    assert!(text.contains("SurveyReport"));
    assert!(
        text.contains("Router commit accepted") && text.contains("Executor grant acknowledged")
    );
    assert!(!text.contains("PRIVATE BODY") && !text.contains("PRIVATE REASONING"));
}
