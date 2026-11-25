use serde_json::json;
use squad_protocol::*;
#[test]
fn strict_hunts_and_citations() {
    let good = json!({"gaps":[{"localId":"g1","omission":"backups","consequence":"loss"}]});
    let hunt: CensusHuntOutput = validation::decode(&good).unwrap();
    hunt.validate().unwrap();
    let mut duplicate = hunt.clone();
    duplicate.gaps.push(hunt.gaps[0].clone());
    assert!(duplicate.validate().is_err());
    let mut extra = good;
    extra["finderCount"] = json!(4);
    assert!(validation::decode::<CensusHuntOutput>(&extra).is_err());
    let gap = CensusGap {
        id: Text::new("g1").unwrap(),
        omission: Text::new("backups").unwrap(),
        consequence: Text::new("loss").unwrap(),
        lenses: vec![Text::new("failure modes").unwrap()],
        source_find_ids: vec![Text::new("find1").unwrap()],
    };
    let rating = json!({"ratings":[{"gapId":"g1","rating":"covered","reason":"addressed","citation":{"location":"line 1","quote":"back up daily"}}],"additions":[]});
    let r: CensusRatingOutput = validation::decode(&rating).unwrap();
    assert!(
        r.validate_against(std::slice::from_ref(&gap), "back up daily")
            .is_ok()
    );
    assert!(r.validate_against(&[gap], "no backups").is_err());
    assert!(
        validation::decode::<CensusGapRating>(
            &json!({"gapId":"g1","rating":"real","reason":"missing"})
        )
        .is_err()
    );
}
#[test]
fn source_coverage_and_chao1() {
    let merge: CensusMergeOutput = validation::decode(&json!({"ledger":[{"id":"g1","omission":"backup","consequence":"loss","lenses":["failure modes"],"sourceFindIds":["f1","f2"]}]})).unwrap();
    assert!(merge.validate_against(&["f1".into(), "f2".into()]).is_ok());
    assert!(merge.validate_against(&["f1".into()]).is_err());
    assert_eq!(census_chao1(3, 2), (2.25, "doubletons"));
    assert_eq!(census_chao1(3, 0), (3.0, "zero_doubletons"));
    assert_eq!(census_chao1(0, 0).0, 0.0);
}
#[test]
fn model_cannot_supply_tallies_or_exceed_eight_finds() {
    let finds = (0..9)
        .map(|i| json!({"localId":format!("g{i}"),"omission":"missing","consequence":"loss"}))
        .collect::<Vec<_>>();
    assert!(validation::decode::<CensusHuntOutput>(&json!({"gaps":finds})).is_err());
    assert!(validation::decode::<CensusFinalOutput>(&json!({"confirmed":[],"covered":[],"trivial":[],"additions":[],"f1":0,"unseen":0,"estimateLow":0})).is_err());
}
#[test]
fn census_environment_seed_has_default_bounds_and_empty_override_validation() {
    let dir = tempfile::tempdir().unwrap();
    let options = config::LoadOptions {
        env_file: None,
        data_dir: Some(dir.path().join("data")),
        working_dir: dir.path().into(),
        config_dir: None,
        default_data_dir: dir.path().join("data"),
    };
    for (value, valid) in [
        ("1", true),
        ("5", true),
        ("0", false),
        ("6", false),
        ("", false),
    ] {
        let env = std::collections::BTreeMap::from([
            ("OPENROUTER_API_KEY".into(), "fixture".into()),
            ("SQUAD_CENSUS_PASSES".into(), value.into()),
        ]);
        let result = SquadConfig::load_with_env(&options, &env);
        assert_eq!(result.is_ok(), valid, "{value}");
        if let Ok(c) = result {
            assert_eq!(c.defaults.census_passes, value.parse::<u8>().unwrap());
        }
    }
}
#[test]
fn census_work_requires_frozen_step_context_and_attempt_budget() {
    let contest = ContestId::new();
    let step = StepId::new();
    let nick: MemberNick = "member0".parse().unwrap();
    let mut work = Work {
        work_id: WorkId::new(),
        message_id: MessageId::new(),
        in_reply_to_message_id: None,
        kind: WorkKind::CensusHunt,
        target: Target::Member(nick.clone()),
        context_id: ContextId(format!("contest:{contest}:{step}:{nick}")),
        trigger_event_id: EventId::new(),
        history_through: HistoryPosition::new(1).unwrap(),
        attempt: 1,
        debate_id: None,
        round: None,
        replay: None,
        case_id: None,
        prompt_ref: Some(PromptRef {
            template_id: Text::new("census.hunt").unwrap(),
            pack_version: Text::new("1.0.0").unwrap(),
            template_hash: Text::new("fixture").unwrap(),
        }),
        max_output_tokens: Some(2048),
        contest_id: Some(contest),
        step_id: Some(step),
    };
    work.validate().unwrap();
    work.step_id = None;
    assert!(work.validate().is_err());
    work.step_id = Some(step);
    work.attempt = 3;
    assert!(work.validate().is_err());
    work.attempt = 1;
    work.context_id = ContextId(nick.to_string());
    assert!(work.validate().is_err());
}
