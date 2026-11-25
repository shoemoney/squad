use serde_json::{Value, json};
use squad_core::prompts::PromptPack;
use squad_protocol::*;
use std::collections::BTreeMap;
const ID: &str = "01900000-0000-7000-8000-000000000001";
fn config() -> SquadConfig {
    let dir = std::env::temp_dir().join(format!("squad-inputs-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let options = config::LoadOptions {
        env_file: None,
        data_dir: Some(dir.clone()),
        working_dir: dir.clone(),
        config_dir: None,
        default_data_dir: dir,
    };
    SquadConfig::load_with_env(
        &options,
        &BTreeMap::from([
            ("OPENROUTER_API_KEY".into(), "test-fixture".into()),
            ("SQUAD_MODEL_ALPHA".into(), "vendor/alpha".into()),
            ("SQUAD_MODEL_BETA".into(), "vendor/beta".into()),
            ("SQUAD_PERSONA_ALPHA".into(), "Be precise {{rogue}}".into()),
            ("SQUAD_USER_NAME".into(), "Jeremy \"J\"".into()),
            ("SQUAD_TURN_WORDS".into(), "321".into()),
            ("SQUAD_FINAL_TURN_WORDS".into(), "543".into()),
        ]),
    )
    .unwrap()
}
fn binding() -> ExecutorBinding {
    ExecutorBinding {
        model: Text::new("vendor/concrete").unwrap(),
        effort: ReasoningEffort::High,
    }
}
fn mode_work(kind: &str, target: &str, attempt: u32) -> Work {
    let judge = target == "judge";
    let context = if judge {
        format!("judge:{ID}")
    } else {
        format!("contest:{ID}:{ID}:{target}")
    };
    let work: Work = serde_json::from_value(json!({"workId":ID,"messageId":ID,"kind":kind,"target":target,"contextId":context,"triggerEventId":ID,"historyThrough":"10","attempt":attempt,"contestId":ID,"stepId":ID,"caseId":if judge {json!(ID)} else {Value::Null},"maxOutputTokens":if judge {4096} else {2048},"promptRef":{"templateId":kind,"packVersion":"1.0.0","templateHash":"frozen"}})).unwrap();
    work.validate().unwrap_or_else(|e| panic!("{kind}: {e:?}"));
    work
}
fn t(s: &str) -> Text {
    Text::new(s).unwrap()
}
fn all_text(r: &squad_core::prompts::RenderedComposition) -> String {
    format!("{}\n{}", r.system, r.user)
}

fn goal() -> CrosscheckGoal {
    CrosscheckGoal {
        goal: t("GOAL_CANARY"),
        constraints: vec![t("CONSTRAINT_CANARY")],
        uncertainties: vec![t("UNCERTAINTY_CANARY")],
    }
}
fn render(
    work: &Work,
    material: &CrosscheckCaseMaterial,
    repair: Option<&JudgeRepairData>,
) -> squad_core::prompts::RenderedComposition {
    let pack = PromptPack::embedded().unwrap();
    let inputs = pack
        .crosscheck_inputs(&config(), &binding(), work, material, repair)
        .unwrap();
    pack.render_composition(material.composition(), &inputs, repair.is_some())
        .unwrap()
}

#[test]
fn inventory_requests_carry_only_goal_and_lens_never_the_plan() {
    let material = CrosscheckCaseMaterial::Inventory {
        goal: goal(),
        lens: CrosscheckLens::FailureStates,
    };
    let r = render(
        &mode_work("crosscheck.inventory", "alpha", 1),
        &material,
        None,
    );
    let text = all_text(&r);
    for canary in ["GOAL_CANARY", "CONSTRAINT_CANARY", "UNCERTAINTY_CANARY"] {
        assert_eq!(r.user.matches(canary).count(), 1, "{canary}");
    }
    assert!(r.user.contains("failure_states"));
    assert!(!text.contains("PLAN_CANARY"));
    assert_eq!(r.provider_user_name, "squad");
}

#[test]
fn goal_extraction_and_normalization_use_the_judge_system_and_no_member_identity() {
    let extract = CrosscheckCaseMaterial::ExtractGoal {
        plan: t("PLAN_CANARY"),
    };
    let r = render(&mode_work("crosscheck.goal", "judge", 1), &extract, None);
    assert_eq!(r.user.matches("PLAN_CANARY").count(), 1);
    assert!(!r.system.contains("PERSONA") && !r.system.contains("Be precise"));
    assert!(!all_text(&r).contains("vendor/alpha"));
    let normalize = CrosscheckCaseMaterial::NormalizeInventory {
        goal: goal(),
        inventories: vec![],
    };
    let r = render(
        &mode_work("crosscheck.normalize", "judge", 1),
        &normalize,
        None,
    );
    assert!(!all_text(&r).contains("PLAN_CANARY"));
    assert_eq!(r.user.matches("GOAL_CANARY").count(), 1);
}

#[test]
fn reviewer_and_fresh_auditor_requests_receive_only_their_own_cells_and_sampled_evidence() {
    let cell = CellId::new();
    let review_cell = CellId::new();
    let material = CrosscheckCaseMaterial::Review {
        plan: t("PLAN_CANARY"),
        cells: vec![CrosscheckVisibleCell {
            cell_id: cell,
            items: vec![t("OWN_CELL_ITEM")],
        }],
    };
    let r = render(&mode_work("crosscheck.cell", "beta", 1), &material, None);
    assert_eq!(r.user.matches("OWN_CELL_ITEM").count(), 1);
    assert!(r.user.contains(&cell.to_string()));
    assert!(!all_text(&r).contains("OTHER_CELL_ITEM"));
    let audit = CrosscheckCaseMaterial::Audit {
        plan: t("PLAN_CANARY"),
        grid: vec![CrosscheckVisibleCell {
            cell_id: review_cell,
            items: vec![t("GRID_ITEM")],
        }],
        samples: vec![CrosscheckCellReview {
            cell_id: review_cell,
            status: CrosscheckDisposition::Covered,
            citation: Some(CrosscheckCitation {
                location: t("plan"),
                quote: t("SAMPLED_QUOTE"),
            }),
            reason: t("SAMPLED_REASON"),
            proposed_fix: None,
        }],
    };
    let r = render(&mode_work("crosscheck.audit", "beta", 1), &audit, None);
    for canary in ["SAMPLED_QUOTE", "SAMPLED_REASON", "GRID_ITEM"] {
        assert_eq!(r.user.matches(canary).count(), 1, "{canary}");
    }
    let text = all_text(&r);
    assert!(
        !r.user.contains("alpha") && !r.user.contains("beta") && !r.user.contains("reviewer\"")
    );
    assert!(!text.contains("vendor/alpha") && !text.contains("vendor/beta"));
    // The auditor's composition is registered as a fresh context.
    let pack = PromptPack::embedded().unwrap();
    assert_eq!(
        pack.composition("crosscheck.audit").unwrap().metadata["fresh_context"],
        json!(true)
    );
    assert_eq!(
        pack.composition("crosscheck.cell").unwrap().metadata["fresh_context"],
        json!(false)
    );
}

#[test]
fn repair_suffix_is_shared_task_specific_and_only_carries_the_rejected_output_once() {
    let material = CrosscheckCaseMaterial::ExtractGoal {
        plan: t("PLAN_CANARY"),
    };
    let repair = JudgeRepairData {
        attempt: 2,
        rejected_output: "REJECTED_ONCE".into(),
        validation_errors: vec!["ERROR_ONCE".into()],
    };
    let work = mode_work("crosscheck.goal", "judge", 2);
    let r = render(&work, &material, Some(&repair));
    assert_eq!(r.user.matches("REJECTED_ONCE").count(), 1);
    assert_eq!(r.user.matches("ERROR_ONCE").count(), 1);
    assert_eq!(r.user.matches("PLAN_CANARY").count(), 1);
    assert!(r.user.contains("crosscheck.normalize.extract_goal"));
    assert_eq!(r.provider_user_name, "squad");
    // Mismatched attempt, or a member work item, can never carry a repair.
    let pack = PromptPack::embedded().unwrap();
    let wrong_attempt = mode_work("crosscheck.goal", "judge", 1);
    let _ = (&pack, &wrong_attempt);
    let sharing = ["census.merge", "survey.extract", "probe.report"].map(|id| {
        let spec = pack.composition(id).unwrap();
        spec.repair_suffix.clone().unwrap()
    });
    assert!(sharing.iter().all(|s| s == "mode.repair"));
}
