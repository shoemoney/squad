use serde_json::{Value, json};
use squad_agents::judge::*;
use squad_protocol::*;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;
const ID: &str = "01900000-0000-7000-8000-000000000001";
const OTHER: &str = "01900000-0000-7000-8000-000000000002";
fn work(round: bool, attempt: u32) -> Work {
    serde_json::from_value(json!({"workId":ID,"messageId":ID,"kind":if round {"judge.round"} else {"judge"},"target":"judge","contextId":format!("judge:{ID}"),"triggerEventId":ID,"historyThrough":"10","attempt":attempt,"debateId":ID,"contestId":ID,"round":1,"caseId":ID,"maxOutputTokens":if round {512} else {4096},"promptRef":{"templateId":if round {"judge.round.md"} else {"judge.debate"},"packVersion":"1.0.0","templateHash":"frozen"}})).unwrap()
}
fn debaters() -> [MemberNick; 2] {
    [
        MemberNick::new("alpha").unwrap(),
        MemberNick::new("beta").unwrap(),
    ]
}
fn tally() -> BTreeMap<MemberNick, u32> {
    debaters().into_iter().map(|n| (n, 0)).collect()
}
fn verdict() -> Value {
    json!({"winner":"alpha","scores":(["alpha","beta"].map(|nick|json!({"nick":nick,"criteria":(["fit","feasibility","originality","survivedAttacks"].map(|c|json!({"criterionId":c,"score":5,"reasoning":"reason"})))}))),"reasoning":"Equal totals resolved by fit","whatToStealFromLoser":[],"tallyOverrideReason":null})
}
#[test]
fn round_validation_uses_canonical_schema_and_shared_two_attempt_budget() {
    let valid =
        json!({"scores":{"alpha":{"score":5,"reason":"same"},"beta":{"score":5,"reason":"same"}}});
    assert!(matches!(
        validate_output(
            &work(true, 1),
            &valid.to_string(),
            Some(&debaters()),
            &tally()
        )
        .unwrap(),
        JudgeValidation::Accepted(JudgeOutput::Round(_))
    ));
    for raw in [
        "broken".to_owned(),
        json!({"scores":{"wrong":{"score":11,"reason":"bad"}}}).to_string(),
    ] {
        let JudgeValidation::Repair(repair) =
            validate_output(&work(true, 1), &raw, Some(&debaters()), &tally()).unwrap()
        else {
            panic!()
        };
        assert_eq!(repair.attempt, 2);
        assert_eq!(repair.rejected_output, raw);
        assert!(matches!(
            validate_output(&work(true, 2), &raw, Some(&debaters()), &tally()).unwrap(),
            JudgeValidation::Exhausted {
                unscored_round: true,
                ..
            }
        ));
    }
    assert!(validate_output(&work(true, 3), "broken", Some(&debaters()), &tally()).is_err());
}
#[test]
fn canonical_verdict_validation_rejects_totals_null_ties_and_repairs_override_reason() {
    let valid = verdict();
    let JudgeValidation::Accepted(JudgeOutput::Verdict(accepted)) = validate_output(
        &work(false, 1),
        &valid.to_string(),
        Some(&debaters()),
        &tally(),
    )
    .unwrap() else {
        panic!()
    };
    assert_eq!(accepted.weighted_totals[0].total, 5.0);
    for winner in [json!(null), json!("tie"), json!("unknown")] {
        let mut value = valid.clone();
        value["winner"] = winner;
        assert!(matches!(
            validate_output(
                &work(false, 1),
                &value.to_string(),
                Some(&debaters()),
                &tally()
            )
            .unwrap(),
            JudgeValidation::Repair(_)
        ));
    }
    for key in ["roundTally", "weightedTotals"] {
        let mut value = valid.clone();
        value[key] = json!({});
        assert!(matches!(
            validate_output(
                &work(false, 1),
                &value.to_string(),
                Some(&debaters()),
                &tally()
            )
            .unwrap(),
            JudgeValidation::Repair(_)
        ));
    }
    let mut leader = tally();
    leader.insert(MemberNick::new("beta").unwrap(), 1);
    let JudgeValidation::Repair(repair) = validate_output(
        &work(false, 1),
        &valid.to_string(),
        Some(&debaters()),
        &leader,
    )
    .unwrap() else {
        panic!()
    };
    assert!(repair.validation_errors[0].contains("tallyOverrideReason"));
    let mut fixed = valid;
    fixed["tallyOverrideReason"] = json!("The final solution fixed the earlier defect");
    assert!(matches!(
        validate_output(
            &work(false, 2),
            &fixed.to_string(),
            Some(&debaters()),
            &leader
        )
        .unwrap(),
        JudgeValidation::Accepted(_)
    ));
    assert!(matches!(
        validate_output(&work(false, 3), "bad", Some(&debaters()), &tally()).unwrap(),
        JudgeValidation::Exhausted {
            code: ErrorCode::JUDGE_INVALID_VERDICT,
            unscored_round: false
        }
    ));
}
#[test]
fn standalone_and_debate_followups_remain_free_form() {
    let mut standalone = work(false, 1);
    standalone.debate_id = None;
    standalone.round = None;
    standalone.contest_id = None;
    let mut followup = work(false, 1);
    followup.prompt_ref.as_mut().unwrap().template_id = Text::new("judge.followup").unwrap();
    for work in [standalone, followup] {
        assert_eq!(
            validate_output(&work, "A free-form ruling", None, &BTreeMap::new()).unwrap(),
            JudgeValidation::Accepted(JudgeOutput::Ruling("A free-form ruling".into()))
        );
    }
}
struct Fakes {
    repair: Option<JudgeRepairData>,
    reads: Mutex<Vec<(WorkId, u32)>>,
    values: Mutex<BTreeMap<String, String>>,
}
impl WorkReader for Fakes {
    fn unfinished(&self) -> ReaderFuture<'_, Vec<WorkRecord>> {
        Box::pin(async { panic!("not used") })
    }
    fn get(&self, _: WorkId) -> ReaderFuture<'_, Option<WorkRecord>> {
        Box::pin(async { panic!("not used") })
    }
    fn repair_for_attempt(
        &self,
        id: WorkId,
        attempt: u32,
    ) -> ReaderFuture<'_, Option<JudgeRepairData>> {
        self.reads.lock().unwrap().push((id, attempt));
        Box::pin(async { Ok(self.repair.clone()) })
    }
}
impl HistoryReader for Fakes {
    fn for_work(&self, _: WorkId) -> ReaderFuture<'_, Vec<HistoryEntry>> {
        Box::pin(async { panic!("must not append history to judge request") })
    }
}
impl ExecutorSubmit for Fakes {
    fn submit(&self, _: ExecutorRequest) -> SubmitFuture<'_> {
        Box::pin(async { panic!("preparation must not submit") })
    }
}
impl PromptRenderer for Fakes {
    fn render(
        &self,
        prompt: &PromptRef,
        values: &BTreeMap<String, String>,
    ) -> Result<String, ReaderError> {
        *self.values.lock().unwrap() = values.clone();
        Ok(json!({"system":"JUDGE_SYSTEM","user":format!("EVIDENCE_ONCE {}",values.get("invalid_output_json").cloned().unwrap_or_default()),"provider_user_name":"squad","step":if prompt.template_id.as_str()=="judge.round.md" {"judge.round"} else {"judge.debate"}}).to_string())
    }
}
fn context(fake: Arc<Fakes>) -> AgentContext {
    AgentContext {
        executor: ExecutorKey::Case(ID.parse().unwrap()),
        mailbox: tokio::sync::mpsc::channel(1).1,
        history: fake.clone(),
        work: fake.clone(),
        submit: fake.clone(),
        render: fake,
        cancel: CancellationToken::new(),
    }
}
fn grant(work: Work) -> ExecutionGrant {
    ExecutionGrant {
        binding: ExecutorBinding {
            model: Text::new("vendor/concrete").unwrap(),
            effort: ReasoningEffort::High,
        },
        work,
        owner: ExecutorKey::Case(ID.parse().unwrap()),
        fence: 1,
        epoch: 1,
        lease_until: "2026-11-24T23:59:00Z".parse().unwrap(),
        deadline: None,
    }
}
#[tokio::test]
async fn repair_reads_exact_work_attempt_and_preserves_two_message_framing_and_limits() {
    let fake = Arc::new(Fakes {
        repair: Some(JudgeRepairData {
            attempt: 2,
            rejected_output: "BROKEN \" {{slot}}".into(),
            validation_errors: vec!["invalid schema".into()],
        }),
        reads: Mutex::new(vec![]),
        values: Mutex::new(BTreeMap::new()),
    });
    let context = context(fake.clone());
    let request = prepare_attempt(
        &context,
        &grant(work(true, 2)),
        BTreeMap::new(),
        true,
        16000,
    )
    .await
    .unwrap();
    assert_eq!(*fake.reads.lock().unwrap(), vec![(ID.parse().unwrap(), 2)]);
    assert_eq!(request.messages.len(), 2);
    assert_eq!(request.messages[0].role, "system");
    assert_eq!(request.messages[1].role, "user");
    assert_eq!(request.messages[1].name.as_deref(), Some("squad"));
    assert_eq!(
        request.messages[1].content.matches("EVIDENCE_ONCE").count(),
        1
    );
    assert_eq!(request.messages[1].content.matches("BROKEN").count(), 1);
    assert_eq!(request.visible_ceiling, 512);
    assert_eq!(request.reasoning_effort, ReasoningEffort::Low);
    let body = request.body().unwrap();
    assert_eq!(body["reasoning"]["max_tokens"], 1024);
    assert_eq!(body["max_tokens"], 1536);
    let values = fake.values.lock().unwrap();
    assert_eq!(
        serde_json::from_str::<String>(&values["invalid_output_json"]).unwrap(),
        "BROKEN \" {{slot}}"
    );
}
#[tokio::test]
async fn foreign_owner_stale_repair_and_caller_supplied_repair_are_rejected() {
    let fake = Arc::new(Fakes {
        repair: Some(JudgeRepairData {
            attempt: 1,
            rejected_output: "foreign".into(),
            validation_errors: vec![],
        }),
        reads: Mutex::new(vec![]),
        values: Mutex::new(BTreeMap::new()),
    });
    let context = context(fake.clone());
    let mut grant = grant(work(true, 2));
    assert!(
        prepare_attempt(&context, &grant, BTreeMap::new(), true, 16000)
            .await
            .is_err()
    );
    grant.owner = ExecutorKey::Case(OTHER.parse().unwrap());
    assert!(
        prepare_attempt(&context, &grant, BTreeMap::new(), true, 16000)
            .await
            .is_err()
    );
    grant.owner = context.executor.clone();
    assert!(
        prepare_attempt(
            &context,
            &grant,
            BTreeMap::from([("invalid_output_json".into(), "forged".into())]),
            true,
            16000
        )
        .await
        .is_err()
    );
}

#[test]
fn round_repairs_cover_fractional_bounds_reasons_and_extra_fields() {
    let valid =
        json!({"scores":{"alpha":{"score":5,"reason":"ok"},"beta":{"score":5,"reason":"ok"}}});
    let mut values = Vec::new();
    for bad in [json!(0), json!(11), json!(1.5)] {
        let mut value = valid.clone();
        value["scores"]["alpha"]["score"] = bad;
        values.push(value);
    }
    let mut value = valid.clone();
    value["scores"]["alpha"]["reason"] = json!("x".repeat(201));
    values.push(value);
    let mut value = valid.clone();
    value["scores"]["alpha"]["extra"] = json!(true);
    values.push(value);
    let mut value = valid;
    value["roundTally"] = json!({"alpha":1,"beta":0});
    values.push(value);
    for value in values {
        assert!(matches!(
            validate_output(
                &work(true, 1),
                &value.to_string(),
                Some(&debaters()),
                &tally()
            )
            .unwrap(),
            JudgeValidation::Repair(_)
        ));
    }
}
#[tokio::test]
async fn final_request_uses_high_reasoning_and_unsupported_models_omit_parameters() {
    let fake = Arc::new(Fakes {
        repair: None,
        reads: Mutex::new(vec![]),
        values: Mutex::new(BTreeMap::new()),
    });
    let context = context(fake);
    let request = prepare_attempt(
        &context,
        &grant(work(false, 1)),
        BTreeMap::new(),
        true,
        16000,
    )
    .await
    .unwrap();
    assert_eq!(request.visible_ceiling, 4096);
    assert_eq!(request.reasoning_effort, ReasoningEffort::High);
    assert_eq!(request.body().unwrap()["reasoning"]["max_tokens"], 8192);
    assert!(matches!(
        request.policy,
        squad_agents::openrouter::ExecutionPolicy::SingleAttempt(_)
    ));
    let request = prepare_attempt(
        &context,
        &grant(work(true, 1)),
        BTreeMap::new(),
        false,
        1000,
    )
    .await
    .unwrap();
    assert!(request.body().unwrap().get("reasoning").is_none());
}
