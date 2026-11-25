//! Step, pass and execution-identity attribution of new-mode provider calls.
use squad_agents::accounting::{AttemptAccounting, AttemptAttribution};
use squad_protocol::*;

fn grant(kind: &str, target: &str, context: String, step: Option<StepId>) -> ExecutionGrant {
    let contest = ContestId::new();
    let judge = target == "judge";
    let mut work = serde_json::json!({"workId":WorkId::new(),"messageId":MessageId::new(),"kind":kind,"target":target,"contextId":context.replace("{contest}", &contest.to_string()),"triggerEventId":EventId::new(),"historyThrough":"1","attempt":2,"contestId":contest});
    if let Some(step) = step {
        work["stepId"] = serde_json::json!(step);
    }
    if judge {
        work["caseId"] = serde_json::json!(CaseId::new());
    }
    ExecutionGrant {
        binding: ExecutorBinding {
            model: Text::new("fixture/model").unwrap(),
            effort: ReasoningEffort::Medium,
        },
        work: serde_json::from_value(work).unwrap(),
        owner: ExecutorKey::Member("alpha".parse().unwrap()),
        fence: 3,
        epoch: 4,
        lease_until: chrono::Utc::now() + chrono::Duration::seconds(60),
        deadline: None,
    }
}

#[test]
fn each_new_mode_call_is_attributed_to_its_contest_step_pass_execution_work_and_attempt() {
    let cases = [
        ("census.hunt", "alpha", Some(2)),
        ("census.merge", "judge", Some(2)),
        ("crosscheck.inventory", "alpha", None),
        ("crosscheck.audit", "alpha", None),
        ("survey.extract", "judge", None),
        ("probe.predict", "alpha", None),
    ];
    let mut seen = std::collections::BTreeSet::new();
    for (kind, target, pass) in cases {
        let step = StepId::new();
        let context = if target == "judge" {
            "judge:{contest}".to_owned()
        } else {
            format!("contest:{{contest}}:{step}:{target}")
        };
        let grant = grant(kind, target, context, Some(step));
        let account = AttemptAccounting::new(&grant, chrono::Utc::now(), "2026-11-25".into());
        let attribution = account.attribution(&grant, pass);
        assert_eq!(attribution.contest_id, grant.work.contest_id);
        assert_eq!(attribution.step_id, Some(step));
        assert_eq!(attribution.pass, pass);
        assert_eq!(attribution.execution, grant.work.context_id);
        assert_eq!(attribution.work_id, grant.work.work_id);
        assert_eq!(attribution.attempt, 2);
        assert!(attribution.matches(&account.record), "{kind}");
        assert!(seen.insert((attribution.work_id, attribution.attempt)));
    }
}

#[test]
fn attribution_rejects_a_record_for_another_attempt_step_context_or_contest() {
    let step = StepId::new();
    let grant = grant(
        "census.hunt",
        "alpha",
        format!("contest:{{contest}}:{step}:alpha"),
        Some(step),
    );
    let account = AttemptAccounting::new(&grant, chrono::Utc::now(), "2026-11-25".into());
    let attribution = AttemptAttribution::from_grant(&grant, Some(1));
    for mutate in [
        |r: &mut UsageRecord| r.attempt += 1,
        |r: &mut UsageRecord| r.work_id = WorkId::new(),
        |r: &mut UsageRecord| r.contest_id = Some(ContestId::new()),
        |r: &mut UsageRecord| r.context_id = ContextId("contest:other".into()),
        |r: &mut UsageRecord| r.target = Target::Judge,
    ] {
        let mut record = account.record.clone();
        mutate(&mut record);
        assert!(!attribution.matches(&record));
    }
}
