#[path = "room_support.rs"]
mod support;
use squad_protocol::*;
use std::collections::BTreeMap;
use support::*;
#[tokio::test]
async fn queue_roster_models_variables_and_wire_names_are_authoritative() {
    let root = tempfile::tempdir().unwrap();
    let launcher = Launcher::default();
    let (room, _) = open(root.path(), &launcher, OpenMode::OneShot).await;
    accepted(
        room.submit_commit_only(message("@a queued", EventId::new()))
            .await,
    );
    let state = room.state().await;
    assert_eq!(
        state
            .participants
            .iter()
            .map(|p| p.nick.as_str())
            .collect::<Vec<_>>(),
        vec!["a", "b"]
    );
    assert_eq!(state.participants[0].queue_depth, 1);
    assert_eq!(state.participants[1].queue_depth, 0);
    assert_eq!(state.models.len(), 3);
    assert_eq!(state.models[0].resolved_slug.as_str(), "test/model");
    accepted(
        room.submit(Request::SetVars {
            request_id: EventId::new(),
            updates: vec![
                VariableUpdate {
                    name: VariableName::CensusPasses,
                    value: 4,
                },
                VariableUpdate {
                    name: VariableName::GrillRounds,
                    value: 3,
                },
            ],
        })
        .await,
    );
    let state = room.state().await;
    assert_eq!(state.vars.rounds, 10);
    assert_eq!(state.vars.grill_rounds, 3);
    assert_eq!(state.vars.census_passes, 4);
    let json = serde_json::to_value(state).unwrap();
    assert_eq!(json["vars"]["CENSUS_PASSES"], 4);
    assert!(json.get("dailySpend").is_some());
    assert!(json.get("daily_spend").is_none());
    room.shutdown().await.unwrap();
    let server = server().await;
    let mut cfg = config(root.path());
    cfg.defaults.rounds = 6;
    cfg.defaults.grill_rounds = 8;
    let room = squad_core::RoomHandle::open_with_launcher(
        cfg,
        "main",
        OpenMode::OneShot,
        client(&server),
        &Launcher::default(),
    )
    .await
    .unwrap();
    let vars = room.state().await.vars;
    assert_eq!(
        (vars.rounds, vars.grill_rounds, vars.census_passes),
        (10, 3, 4)
    );
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn scores_trophies_and_unknown_daily_spend_do_not_change_on_observation() {
    let root = tempfile::tempdir().unwrap();
    let launcher = Launcher::default();
    let (room, _) = open(root.path(), &launcher, OpenMode::OneShot).await;
    store(&launcher)
        .call(|db| {
            db.execute(
                "INSERT INTO scores(nick,wins,losses,trophies) VALUES ('a',2,1,3)",
                [],
            )?;
            db.execute(
                "INSERT INTO spend_daily VALUES (?,1250000,750000,2)",
                [chrono::Local::now().format("%Y-%m-%d").to_string()],
            )?;
            db.execute(
                "INSERT INTO spend_daily VALUES ('2000-01-01',99000000,0,0)",
                [],
            )?;
            Ok(())
        })
        .unwrap();
    for _ in 0..3 {
        let state = room.state().await;
        assert_eq!(state.scoreboard[0].wins, 2);
        assert_eq!(state.trophies[&nick("a")], 3);
        assert_eq!(state.daily_spend.spent_usd, 1.25);
        assert_eq!(state.daily_spend.reserved_usd, 0.75);
        assert_eq!(state.daily_spend.unknown_calls, 2);
        assert!(state.daily_spend.has_unreconciled_charges);
        assert_eq!(state.daily_spend.limit_usd, 20.);
    }
    let old = room.state().await.daily_spend;
    accepted(
        room.submit(Request::Clear {
            request_id: EventId::new(),
        })
        .await,
    );
    let state = room.state().await;
    assert_eq!(state.daily_spend, old);
    assert!(state.scoreboard.is_empty());
    assert!(state.trophies.is_empty());
    assert!(state.active_contest.is_none());
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn committed_contest_state_is_preserved_across_observation_and_restart() {
    let root = tempfile::tempdir().unwrap();
    let launcher = Launcher::default();
    let (room, _) = open(root.path(), &launcher, OpenMode::OneShot).await;
    let state = ContestState::Generic(GenericContestState {
        contest_id: ContestId::new(),
        kind: ContestKind::Census,
        state_version: 3,
        status: Text::new("WAITING_FOR_PARTICIPANTS").unwrap(),
        step_id: Some(StepId::new()),
        artifact_event_ids: vec![EventId::new()],
        terminal_reason: None,
    });
    let saved = state.clone();
    store(&launcher).call(move |db| {
        let ContestState::Generic(inner)=saved else {panic!()};
        db.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active) VALUES (?,'census',1,3,?,1)",rusqlite::params![inner.contest_id.to_string(),serde_json::to_string(&inner)?])?; Ok(())
    }).unwrap();
    assert_eq!(room.state().await.active_contest, Some(state.clone()));
    assert_eq!(room.state().await.active_contest, Some(state.clone()));
    room.shutdown().await.unwrap();
    let (room, _) = open(root.path(), &Launcher::default(), OpenMode::OneShot).await;
    assert_eq!(room.state().await.active_contest, Some(state));
    room.shutdown().await.unwrap();
}
#[test]
fn presence_keeps_shared_streaming_identity_and_uses_durable_queue_depth() {
    let store = squad_store::Store::open(":memory:").unwrap();
    let presence = Presence {
        nick: nick("a"),
        status: PresenceStatus::Streaming,
        work_id: None,
        queue_depth: 99,
        error_code: None,
        observed_at: chrono::Utc::now(),
    };
    let observed = presence.observed_at;
    let state = store
        .call(move |db| {
            squad_core::state::snapshot(
                db,
                &[nick("a")],
                &BTreeMap::from([(nick("a"), presence)]),
                "2026-11-24",
                20_000_000,
            )
        })
        .unwrap();
    assert_eq!(state.participants[0].status, PresenceStatus::Streaming);
    assert_eq!(state.participants[0].observed_at, observed);
    assert_eq!(state.participants[0].queue_depth, 0);
}
#[tokio::test]
async fn pending_pick_and_unexpired_preflight_survive_restart_without_mutation() {
    let root = tempfile::tempdir().unwrap();
    let launcher = Launcher::default();
    let (room, _) = open(root.path(), &launcher, OpenMode::OneShot).await;
    let contest = ContestSelectionState {
        contest_id: ContestId::new(),
        kind: ContestKind::Grill,
        state: ContestSelectionStatus::AwaitingPick,
        state_version: 2,
        question_message_id: MessageId::new(),
        addressed: vec![nick("a"), nick("b")],
        answers: vec![ContestAnswer {
            nick: nick("a"),
            message_id: MessageId::new(),
            text: "frozen answer".into(),
        }],
        required_picks: 1,
    };
    let plan = ContestPreflight {
        contest_id: contest.contest_id,
        kind: ContestKind::Grill,
        plan_id: PlanId::new(),
        expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        estimate: DebateEstimate {
            estimated_usd: 1.,
            reserved_usd: 1.,
            remaining_usd: 20.,
            model_calls: 3,
        },
        participants: vec![nick("a"), nick("b")],
        confirmation_required: true,
        warning: None,
        settings: serde_json::json!({}),
    };
    let saved = contest.clone();
    let preflight = plan.clone();
    store(&launcher).call(move |db| {
        db.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active,plan_id,plan_json,expires_at) VALUES (?,'grill',1,2,?,1,?,?,?)",rusqlite::params![saved.contest_id.to_string(),serde_json::to_string(&saved)?,preflight.plan_id.to_string(),serde_json::to_string(&preflight)?,preflight.expires_at.to_rfc3339()])?; Ok(())
    }).unwrap();
    let before = room.state().await;
    assert_eq!(
        before.active_contest,
        Some(ContestState::Selection(contest))
    );
    assert_eq!(
        before.pending_preflights,
        vec![PendingPreflight::Contest(plan)]
    );
    room.shutdown().await.unwrap();
    let (room, _) = open(root.path(), &Launcher::default(), OpenMode::OneShot).await;
    let after = room.state().await;
    assert_eq!(after.active_contest, before.active_contest);
    assert_eq!(after.pending_preflights, before.pending_preflights);
    assert!(after.scoreboard.is_empty());
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn new_room_uses_independent_round_defaults_only_once() {
    let root = tempfile::tempdir().unwrap();
    let server = server().await;
    let launcher = Launcher::default();
    let mut cfg = config(root.path());
    cfg.defaults.rounds = 6;
    cfg.defaults.grill_rounds = 8;
    std::fs::create_dir_all(&cfg.data_dir).unwrap();
    std::fs::File::create(cfg.data_dir.join("squad.db")).unwrap();
    let room = squad_core::RoomHandle::open_with_launcher(
        cfg,
        "main",
        OpenMode::OneShot,
        client(&server),
        &launcher,
    )
    .await
    .unwrap();
    let vars = room.state().await.vars;
    assert_eq!(
        (vars.rounds, vars.grill_rounds, vars.census_passes),
        (6, 8, 2)
    );
    accepted(
        room.submit(Request::SetVars {
            request_id: EventId::new(),
            updates: vec![VariableUpdate {
                name: VariableName::GrillRounds,
                value: 2,
            }],
        })
        .await,
    );
    let vars = room.state().await.vars;
    assert_eq!(
        (vars.rounds, vars.grill_rounds, vars.census_passes),
        (6, 2, 2)
    );
    room.shutdown().await.unwrap();
}
