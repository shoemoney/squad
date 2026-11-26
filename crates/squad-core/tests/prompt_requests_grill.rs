use serde_json::{Value, json};
use squad_core::prompts::{PromptPack, inputs_grill::GrillRecord};
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
fn work(kind: &str, nick: &str, round: u32) -> Work {
    let judge = kind.starts_with("judge");
    serde_json::from_value(json!({"workId":ID,"messageId":ID,"kind":kind,"target":if judge {"judge"} else {nick},"contextId":if judge {format!("judge:{ID}")} else {nick.into()},"triggerEventId":ID,"historyThrough":"10","attempt":1,"debateId":ID,"contestId":ID,"round":round,"caseId":if judge {json!(ID)} else {Value::Null},"maxOutputTokens":if kind=="judge.round" {512} else if judge {4096} else {2048}, "promptRef":{"templateId":if kind=="judge.round" {"judge.round.md"} else {"judge.debate"},"packVersion":"1.0.0","templateHash":"frozen"}})).unwrap()
}

fn frozen() -> GrillPreflight {
    GrillPreflight {
        setup: GrillSetup {
            contest_id: ID.parse().unwrap(),
            leader: "alpha".parse().unwrap(),
            grillers: vec!["beta".parse().unwrap()],
            source_message_id: ID.parse().unwrap(),
            initial_plan: "Plan".into(),
            focus: "FOCUS_CANARY".into(),
            total_rounds: 2,
        },
        answer_words: 321,
        plan_words: 1234,
    }
}
fn grill_work(kind: WorkKind, id: &str, actor: &str) -> Work {
    let mut w = work("grill.question", actor, 1);
    w.debate_id = None;
    w.step_id = Some(ID.parse().unwrap());
    w.kind = kind;
    w.prompt_ref.as_mut().unwrap().template_id = Text::new(id).unwrap();
    w.target = Target::Member(actor.parse().unwrap());
    w.context_id = ContextId(format!("contest:{ID}:{ID}:{actor}"));
    w
}
fn state(status: GrillStatus) -> GrillState {
    let mut s = squad_core::orchestrator::grill::initial_state(&frozen().setup).unwrap();
    s.state = status;
    s
}
fn record() -> GrillRecord {
    GrillRecord {
        asked_questions: vec!["ASKED_EARLIER_CANARY".into()],
        floor: vec!["FLOOR_CANARY".into()],
    }
}

#[test]
fn question_composition_carries_angle_focus_asked_list_floor_and_exact_pass_once() {
    let pack = PromptPack::embedded().unwrap();
    let (frozen, config) = (frozen(), config());
    let w = grill_work(WorkKind::GrillQuestion, "grill.question", "beta");
    let st = state(GrillStatus::Questioning);
    let (id, inputs) = pack
        .grill_inputs_with(&config, &w, &binding(), &frozen, &st, &record())
        .unwrap();
    let r = pack.render_composition(id, &inputs, false).unwrap();
    for canary in ["FOCUS_CANARY", "ASKED_EARLIER_CANARY", "FLOOR_CANARY"] {
        assert_eq!(r.user.matches(canary).count(), 1, "{canary}");
        assert!(!r.system.contains(canary));
    }
    assert!(r.user.contains("Your primary angle for this turn is cost."));
    assert!(r.user.contains("\nPASS\n"));
    // The enforced limit is 200 words; the prompt states 150 because models overshoot (live: 206 and 216).
    assert!(r.user.contains("within 150 words in total"));
    assert!(r.user.contains("at most three"));
    assert!(r.user.contains("plan is version 1"));
    assert!(!r.user.contains("judge.round") && !r.user.contains("verdict"));
    let (_, legacy) = pack
        .grill_inputs(&config, &w, &binding(), &frozen, &st)
        .unwrap();
    let r = pack.render_composition(id, &legacy, false).unwrap();
    assert!(r.user.contains("FOCUS_CANARY") && !r.user.contains("ASKED_EARLIER"));
}

#[test]
fn angles_rotate_across_griller_turns_and_rounds() {
    let pack = PromptPack::embedded().unwrap();
    let (frozen, config) = (frozen(), config());
    let mut seen = vec![];
    for round in 1..=2 {
        let mut st = state(GrillStatus::Questioning);
        st.round = round;
        let mut w = grill_work(WorkKind::GrillQuestion, "grill.question", "beta");
        w.round = Some(round);
        let (id, inputs) = pack
            .grill_inputs(&config, &w, &binding(), &frozen, &st)
            .unwrap();
        let text = pack.render_composition(id, &inputs, false).unwrap().user;
        seen.push(
            text.lines()
                .find(|l| l.contains("primary angle"))
                .unwrap()
                .to_owned(),
        );
    }
    assert_ne!(seen[0], seen[1]);
}

#[test]
fn answer_and_rebuild_compositions_use_limits_versions_and_floor() {
    let pack = PromptPack::embedded().unwrap();
    let (frozen, config) = (frozen(), config());
    let w = grill_work(WorkKind::GrillAnswer, "grill.answer", "alpha");
    let st = state(GrillStatus::Answering);
    let (id, inputs) = pack
        .grill_inputs_with(&config, &w, &binding(), &frozen, &st, &record())
        .unwrap();
    let r = pack.render_composition(id, &inputs, false).unwrap();
    assert!(r.user.contains("within 321 words"));
    assert!(r.user.contains("Plan changes"));
    // Live: Opus read the limit as per-question prose and wrote 506 words against 400.
    assert!(r.user.contains("one shared budget"));
    assert!(r.user.contains("for all questions together"));
    assert!(r.user.contains("each \"answer\" in at most 42 words"));
    assert!(r.user.contains("each\n\"planChanges\" in at most 32 words"));
    assert_eq!(r.user.matches("FLOOR_CANARY").count(), 1);
    assert!(!r.user.contains("ASKED_EARLIER_CANARY"));
    let w = grill_work(WorkKind::GrillRebuild, "grill.rebuild", "alpha");
    let st = state(GrillStatus::Rebuilding);
    let (id, inputs) = pack
        .grill_inputs_with(&config, &w, &binding(), &frozen, &st, &record())
        .unwrap();
    let r = pack.render_composition(id, &inputs, false).unwrap();
    // Live: the stated length is half the enforced limit because models write ~1.6x what they are told.
    assert!(r.user.contains("within 617 words"));
    assert!(!r.user.contains("1234"));
    assert!(r.user.contains("as version 2"));
    assert!(r.user.contains("question-and-answer appendix"));
    assert_eq!(r.user.matches("FLOOR_CANARY").count(), 1);
}

#[test]
fn grill_compositions_never_register_a_judge_and_reject_misassigned_actors() {
    let pack = PromptPack::embedded().unwrap();
    for id in ["grill.question", "grill.answer", "grill.rebuild"] {
        let c = pack.composition(id).unwrap();
        assert_eq!(c.system, ["room.system"]);
        assert!(!c.user.iter().any(|t| t.contains("judge")));
    }
    let (frozen, config) = (frozen(), config());
    let mut w = grill_work(WorkKind::GrillAnswer, "grill.answer", "beta");
    assert!(
        pack.grill_inputs(
            &config,
            &w,
            &binding(),
            &frozen,
            &state(GrillStatus::Answering)
        )
        .is_err()
    );
    w = grill_work(WorkKind::GrillRebuild, "grill.rebuild", "alpha");
    assert!(
        pack.grill_inputs(
            &config,
            &w,
            &binding(),
            &frozen,
            &state(GrillStatus::Questioning)
        )
        .is_err()
    );
}
