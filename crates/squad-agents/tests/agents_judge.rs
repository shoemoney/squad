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
        Ok(json!({"system":"JUDGE_SYSTEM","user":format!("EVIDENCE_ONCE {} {}",values.get("invalid_output_json").cloned().unwrap_or_default(),values.get("case_data_json").cloned().unwrap_or_default()),"provider_user_name":"squad","step":match prompt.template_id.as_str() {"judge.round.md" => "judge.round", "judge.followup" => "judge.followup", _ => "judge.debate"}}).to_string())
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

// ---- census, crosscheck, survey and probe judge calls ----
use squad_agents::{registry::*, resolver::Model};

fn new_mode_config() -> config::SquadConfig {
    let root = std::path::PathBuf::from("/nonexistent/squad-judge-fixture");
    let options = config::LoadOptions {
        env_file: None,
        data_dir: None,
        working_dir: root.clone(),
        config_dir: None,
        default_data_dir: root.join("data"),
    };
    let env = BTreeMap::from([("OPENROUTER_API_KEY".to_string(), "fixture-only".to_string())]);
    let mut cfg = config::SquadConfig::load_with_env(&options, &env).unwrap();
    cfg.judge.reasoning_effort = ReasoningEffort::High;
    cfg.judge.reasoning_effort_explicit = true;
    cfg.agents[0].nick = config::Nick::new("alpha").unwrap();
    cfg.agents[0].persona = "PERSONA_CANARY".into();
    cfg
}
fn model(reasoning: bool) -> Model {
    Model {
        id: "vendor/concrete".into(),
        context_length: 200_000,
        output_limit: 65_536,
        supported_parameters: if reasoning {
            vec!["reasoning".into()]
        } else {
            vec![]
        },
        reasoning_efforts: vec![],
        pricing: Value::Null,
    }
}
fn mode_grant(kind: &str, effort: ReasoningEffort) -> ExecutionGrant {
    let judge = matches!(
        kind,
        "census.merge"
            | "census.final"
            | "crosscheck.goal"
            | "crosscheck.normalize"
            | "crosscheck.rule"
            | "survey.extract"
            | "survey.report"
            | "probe.report"
    );
    let (case, step) = (CaseId::new(), StepId::new());
    let contest = ContestId::new();
    let mut work = json!({"workId":WorkId::new(),"messageId":MessageId::new(),"kind":kind,"target":if judge {"judge"} else {"alpha"},"contextId":if judge {format!("judge:{case}")} else {format!("contest:{contest}:{step}:alpha")},"triggerEventId":EventId::new(),"historyThrough":"1","attempt":1,"contestId":contest,"stepId":step,"maxOutputTokens":if judge {4096} else {2048}});
    if judge {
        work["caseId"] = json!(case);
    }
    if kind.starts_with("census") || kind.starts_with("probe") {
        work["promptRef"] =
            json!({"templateId":kind,"packVersion":"1.0.0","templateHash":"frozen"});
    }
    let work: Work = serde_json::from_value(work).unwrap();
    ExecutionGrant {
        owner: if judge {
            ExecutorKey::Case(case)
        } else {
            ExecutorKey::Member("alpha".parse().unwrap())
        },
        binding: ExecutorBinding {
            model: Text::new("vendor/concrete").unwrap(),
            effort,
        },
        work,
        fence: 1,
        epoch: 1,
        lease_until: "2026-11-24T23:59:00Z".parse().unwrap(),
        deadline: None,
    }
}

#[test]
fn all_four_new_modes_judge_with_medium_effort_and_the_4096_ceiling_whatever_the_binding_says() {
    let cfg = new_mode_config();
    let registry = Registry::new(&cfg).unwrap();
    for kind in [
        "census.merge",
        "census.final",
        "crosscheck.goal",
        "crosscheck.normalize",
        "crosscheck.rule",
        "survey.extract",
        "survey.report",
        "probe.report",
    ] {
        for bound in [ReasoningEffort::Low, ReasoningEffort::High] {
            let grant = mode_grant(kind, bound);
            let request = registry
                .work_request(None, &model(true), &grant)
                .unwrap_or_else(|e| panic!("{kind}: {e:?}"));
            assert_eq!(request.visible_ceiling, 4096, "{kind}");
            let body = request.body().unwrap();
            assert_eq!(body["reasoning"]["max_tokens"], 4096, "{kind} {bound:?}");
            assert_eq!(body["max_tokens"], 8192, "{kind}: visible + reasoning");
            assert!(
                matches!(
                    request.policy,
                    squad_agents::openrouter::ExecutionPolicy::SingleAttempt(
                        squad_agents::openrouter::DeadlineClass::FinalJudge
                    )
                ),
                "{kind}"
            );
        }
        // No supported reasoning: the parameter is omitted, never invented.
        let unsupported = registry
            .work_request(
                None,
                &model(false),
                &mode_grant(kind, ReasoningEffort::High),
            )
            .unwrap();
        assert!(unsupported.body().unwrap().get("reasoning").is_none());
    }
}

#[test]
fn new_mode_member_calls_keep_the_ordinary_ceiling_and_the_binding_effort() {
    let cfg = new_mode_config();
    let registry = Registry::new(&cfg).unwrap();
    for kind in [
        "census.hunt",
        "census.rate",
        "crosscheck.inventory",
        "crosscheck.cell",
        "crosscheck.audit",
        "survey.vote",
        "survey.revote",
        "probe.explain",
        "probe.check",
        "probe.predict",
    ] {
        let grant = mode_grant(kind, ReasoningEffort::Low);
        let request = registry
            .work_request(Some(&cfg.agents[0]), &model(true), &grant)
            .unwrap_or_else(|e| panic!("{kind}: {e:?}"));
        assert_eq!(request.visible_ceiling, 2048, "{kind}");
        assert_eq!(request.body().unwrap()["reasoning"]["max_tokens"], 1024);
    }
}

#[test]
fn judge_work_requests_never_carry_member_personas_or_a_member_identity() {
    let cfg = new_mode_config();
    let registry = Registry::new(&cfg).unwrap();
    let grant = mode_grant("census.merge", ReasoningEffort::Medium);
    // A member registration cannot be attached to case-owned judge work.
    assert!(
        registry
            .work_request(Some(&cfg.agents[0]), &model(true), &grant)
            .is_err()
    );
    let request = registry.work_request(None, &model(true), &grant).unwrap();
    assert!(!format!("{:?}", request.body().unwrap()).contains("PERSONA_CANARY"));
}

#[tokio::test]
async fn followup_requests_use_only_the_retained_case_material_in_one_user_message() {
    let fake = Arc::new(Fakes {
        repair: None,
        reads: Mutex::new(vec![]),
        values: Mutex::new(BTreeMap::new()),
    });
    let context = context(fake.clone());
    let mut followup = work(false, 1);
    followup.prompt_ref.as_mut().unwrap().template_id = Text::new("judge.followup").unwrap();
    let case_material = r#"{"ruling":"RULING_ONCE","followUps":["Q1"]}"#.to_owned();
    let request = prepare_attempt(
        &context,
        &grant(followup),
        BTreeMap::from([("case_data_json".into(), case_material)]),
        true,
        16000,
    )
    .await
    .unwrap();
    assert_eq!(request.messages.len(), 2);
    assert_eq!(request.messages[0].role, "system");
    assert_eq!(request.messages[1].role, "user");
    assert_eq!(request.messages[1].name.as_deref(), Some("squad"));
    assert_eq!(
        request.messages[1].content.matches("RULING_ONCE").count(),
        1
    );
    // The fake history reader panics if anything tried to append history.
    assert!(
        !request
            .messages
            .iter()
            .any(|m| m.content.contains("FABLE_HISTORY_CANARY"))
    );
    assert_eq!(request.reasoning_effort, ReasoningEffort::High);
    assert_eq!(request.visible_ceiling, 4096);
    // A case-bound executor never serves another case's work.
    let mut foreign = grant(work(false, 1));
    foreign.owner = ExecutorKey::Case(OTHER.parse().unwrap());
    assert!(
        prepare_attempt(&context, &foreign, BTreeMap::new(), true, 16000)
            .await
            .is_err()
    );
}
