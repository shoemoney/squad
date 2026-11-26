use squad_agents::agents::{StreamOutput, executor_output};
use squad_core::router::executor::{ExecutorCallFailure, executor_call, executor_call_failure};
use squad_protocol::*;
fn grant() -> ExecutionGrant {
    let contest = ContestId::new();
    let step = StepId::new();
    let case = CaseId::new();
    ExecutionGrant {
        binding: ExecutorBinding {
            model: Text::new("frozen/executor").unwrap(),
            effort: ReasoningEffort::High,
        },
        owner: ExecutorKey::Case(case),
        fence: 1,
        epoch: 1,
        lease_until: chrono::Utc::now(),
        deadline: None,
        work: Work {
            work_id: WorkId::new(),
            message_id: MessageId::new(),
            in_reply_to_message_id: None,
            kind: WorkKind::TuneExecute,
            target: Target::Judge,
            context_id: ContextId(format!("judge:{case}")),
            trigger_event_id: EventId::new(),
            history_through: HistoryPosition::new(1).unwrap(),
            attempt: 1,
            debate_id: None,
            round: None,
            replay: None,
            case_id: Some(case),
            prompt_ref: None,
            max_output_tokens: Some(1024),
            contest_id: Some(contest),
            step_id: Some(step),
        },
    }
}
#[test]
fn both_bare_forms_are_byte_exact_and_use_only_the_frozen_executor() {
    let grant = grant();
    let prompt = "  literal @alice $ROUNDS\ncurly ‘quote’ ";
    let input = "case\n{{nested}} @bob";
    let request = executor_call(&grant, prompt, input, 4096).unwrap();
    let body = request.body().unwrap();
    assert_eq!(body["model"], "frozen/executor");
    assert_eq!(body["max_tokens"], 1024);
    assert_eq!(
        body["messages"],
        serde_json::json!([{"role":"system","content":prompt},{"role":"user","content":input}])
    );
    assert!(body.get("response_format").is_none());
    assert!(body.get("reasoning").is_none());
    let prompt = "prefix\n{{input}}\nSuffix @alice";
    let body = executor_call(&grant, prompt, input, 4096)
        .unwrap()
        .body()
        .unwrap();
    assert_eq!(
        body["messages"],
        serde_json::json!([{"role":"user","content":prompt.replacen("{{input}}",input,1)}])
    );
    assert!(executor_call(&grant, "{{input}} {{input}}", input, 4096).is_err());
}
#[test]
fn executor_retries_plain_then_becomes_missing_and_keeps_truncated_text() {
    use squad_core::orchestrator::{
        rounds::failure_policy,
        types::{AttemptDecision, FailureClass},
    };
    assert_eq!(
        executor_call_failure(1),
        ExecutorCallFailure::Retry { next_attempt: 2 }
    );
    assert_eq!(executor_call_failure(2), ExecutorCallFailure::Missing);
    for failure in [
        FailureClass::Transport,
        FailureClass::Timeout,
        FailureClass::InvalidOutput,
    ] {
        assert!(matches!(
            failure_policy(WorkId::new(), &WorkKind::TuneExecute, 1, 2, false, failure),
            AttemptDecision::Retry {
                repair: false,
                next_attempt: 2,
                ..
            }
        ));
    }
    assert!(matches!(
        failure_policy(
            WorkId::new(),
            &WorkKind::TuneExecute,
            2,
            2,
            false,
            FailureClass::Timeout
        ),
        AttemptDecision::Forfeit(_)
    ));
    let output = executor_output(StreamOutput {
        content: "exact output @alice".into(),
        finish_reason: FinishReason::Length,
        usage: None,
    })
    .unwrap();
    assert!(output.truncated);
    assert_eq!(output.text, "exact output @alice");
    assert!(
        executor_output(StreamOutput {
            content: " \n".into(),
            finish_reason: FinishReason::Stop,
            usage: None
        })
        .is_err()
    );
}

#[test]
fn reservation_prices_exactly_two_plain_executor_attempts_per_call() {
    use squad_core::router::bounds::{Step, price};
    let store = squad_store::Store::open(":memory:").unwrap();
    store.call(|db|{
  db.execute("INSERT INTO catalog VALUES('frozen/executor',131072,65536,'{\"prompt\":\"0\",\"completion\":\"0.0009765625\"}','[]','2026-11-26T00:00:00Z')",[])?;
  let bindings=std::collections::BTreeMap::from([("judge".into(),ExecutorBinding{model:Text::new("frozen/executor").unwrap(),effort:ReasoningEffort::High})]);
  let mut step=Step{kind:WorkKind::TuneExecute,target:"judge".into(),alternatives:vec![],template:0,schema:0,data:20,visible:1024,calls:3,attempts:2};
  let priced=price(db,&bindings,&[step.clone()])?;assert_eq!(priced.calls,3);assert_eq!(priced.estimated_usd,3.0);assert_eq!(priced.reserved_micros,6_000_000);
  step.attempts=3;assert!(price(db,&bindings,&[step]).is_err());Ok(())
 }).unwrap();
}
