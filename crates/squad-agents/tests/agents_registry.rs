use squad_agents::{registry::*, resolver::Model};
use squad_protocol::config::{LoadOptions, ReasoningEffort, SquadConfig};
use std::{collections::BTreeMap, path::PathBuf};

fn config(pairs: &[(&str, &str)]) -> SquadConfig {
    let root = PathBuf::from("/nonexistent/squad-registry-fixture");
    let options = LoadOptions {
        env_file: None,
        data_dir: None,
        working_dir: root.clone(),
        config_dir: None,
        default_data_dir: root.join("data"),
    };
    let mut env = BTreeMap::from([("OPENROUTER_API_KEY".into(), "fixture-only".into())]);
    env.extend(pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())));
    SquadConfig::load_with_env(&options, &env).unwrap()
}
fn model(reasoning: bool, limit: u32) -> Model {
    Model {
        id: "arbitrary/future-release".into(),
        context_length: 65536,
        output_limit: limit,
        supported_parameters: if reasoning {
            vec!["reasoning".into()]
        } else {
            vec![]
        },
        reasoning_efforts: vec![],
        pricing: serde_json::Value::Null,
    }
}
#[test]
fn defaults_and_arbitrary_repeated_models_keep_personas_and_order() {
    let cfg = config(&[]);
    let registry = Registry::new(&cfg).unwrap();
    assert_eq!(registry.members().len(), 6);
    for (member, (nick, slug)) in registry.members().iter().zip(DEFAULT_MODELS) {
        assert_eq!(member.nick.as_str(), nick);
        assert_eq!(member.model, slug);
    }
    let cfg = config(&[
        ("SQUAD_MODEL_A", "arbitrary/future-release"),
        ("SQUAD_MODEL_B", "arbitrary/future-release"),
        ("SQUAD_PERSONA_A", "skeptic"),
        ("SQUAD_PERSONA_B", "builder"),
    ]);
    assert_eq!(Registry::new(&cfg).unwrap().members().len(), 2);
    assert_eq!(cfg.agents[0].persona, "skeptic");
    assert_eq!(cfg.agents[1].persona, "builder");
}
#[test]
fn roster_bounds_are_checked_even_for_programmatic_configs() {
    let mut cfg = config(&[("SQUAD_MODEL_A", "test/model")]);
    cfg.agents.clear();
    assert!(Registry::new(&cfg).is_err());
    let member = config(&[("SQUAD_MODEL_A", "test/model")]).agents.remove(0);
    for i in 0..12 {
        let mut m = member.clone();
        m.nick = squad_protocol::config::Nick::new(&format!("member{i}")).unwrap();
        cfg.agents.push(m);
    }
    assert!(Registry::new(&cfg).is_ok());
    cfg.agents.push(member);
    assert!(Registry::new(&cfg).is_err());
}
#[test]
fn precedence_budgets_and_complete_output_reservations() {
    for (effort, budget) in [
        (ReasoningEffort::Low, 1024),
        (ReasoningEffort::Medium, 4096),
        (ReasoningEffort::High, 8192),
    ] {
        let mut cfg = config(&[("SQUAD_MODEL_A", "test/model")]);
        cfg.defaults.reasoning_effort = ReasoningEffort::High;
        cfg.agents[0].reasoning_effort = Some(effort);
        let r = Registry::new(&cfg).unwrap();
        let request = r
            .request(Some(&cfg.agents[0]), &model(true, 32768), TaskKind::Reply)
            .unwrap();
        let body = request.body().unwrap();
        assert_eq!(body["max_tokens"], 2048 + budget);
        assert_eq!(body["reasoning"]["max_tokens"], budget);
        assert_eq!(
            r.request(
                Some(&cfg.agents[0]),
                &model(true, budget + 100),
                TaskKind::Reply
            )
            .unwrap()
            .body()
            .unwrap()["max_tokens"],
            budget + 100
        );
        assert!(
            r.request(Some(&cfg.agents[0]), &model(true, budget), TaskKind::Reply)
                .is_err()
        );
    }
}
#[test]
fn implicit_unsupported_defaults_omit_and_explicit_values_fail() {
    let mut cfg = config(&[("SQUAD_MODEL_A", "test/model")]);
    let r = Registry::new(&cfg).unwrap();
    let body = r
        .request(Some(&cfg.agents[0]), &model(false, 1024), TaskKind::Reply)
        .unwrap()
        .body()
        .unwrap();
    assert!(body.get("reasoning").is_none());
    assert_eq!(body["max_tokens"], 1024);
    cfg.defaults.reasoning_effort_explicit = true;
    assert!(
        Registry::new(&cfg)
            .unwrap()
            .request(Some(&cfg.agents[0]), &model(false, 4096), TaskKind::Reply)
            .is_err()
    );
    cfg.defaults.reasoning_effort_explicit = false;
    cfg.agents[0].reasoning_effort = Some(ReasoningEffort::Low);
    assert!(
        Registry::new(&cfg)
            .unwrap()
            .request(Some(&cfg.agents[0]), &model(false, 4096), TaskKind::Reply)
            .is_err()
    );
}
#[test]
fn judges_have_task_specific_effort_and_visible_caps() {
    let mut cfg = config(&[]);
    cfg.judge.reasoning_effort = ReasoningEffort::High;
    cfg.judge.reasoning_effort_explicit = true;
    let r = Registry::new(&cfg).unwrap();
    let m = model(true, 65536);
    let round = r
        .request(None, &m, TaskKind::RoundJudge)
        .unwrap()
        .body()
        .unwrap();
    assert_eq!(round["reasoning"]["effort"], "low");
    assert_eq!(round["max_tokens"], 512 + 1024);
    for kind in [
        TaskKind::CensusJudge,
        TaskKind::CrosscheckJudge,
        TaskKind::SurveyJudge,
        TaskKind::ProbeJudge,
    ] {
        let body = r.request(None, &m, kind).unwrap().body().unwrap();
        assert_eq!(body["reasoning"]["effort"], "medium");
        assert_eq!(body["reasoning"]["max_tokens"], 4096);
        assert_eq!(body["max_tokens"], 8192);
        assert!(
            r.request(None, &model(false, 4096), kind)
                .unwrap()
                .body()
                .unwrap()
                .get("reasoning")
                .is_none()
        );
    }
    assert_eq!(
        r.request(None, &m, TaskKind::Judge)
            .unwrap()
            .body()
            .unwrap()["reasoning"]["effort"],
        "high"
    );
}
