#[path = "support/m6_room.rs"]
mod live;
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

// ---- M3.P2.L2.T3 / T5 / T6 / T7 / T8 / T11 / T12 ----

fn insert_contest(
    launcher: &Launcher,
    kind: &str,
    state: serde_json::Value,
    active: bool,
) -> ContestId {
    let id: ContestId = state["contestId"]
        .as_str()
        .or_else(|| state["debateId"].as_str())
        .unwrap()
        .parse()
        .unwrap();
    let kind = kind.to_owned();
    store(launcher)
        .call(move |db| {
            db.execute(
                "INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active) VALUES (?,?,1,?,?,?)",
                rusqlite::params![
                    id.to_string(),
                    kind,
                    state["stateVersion"].as_i64().unwrap_or(1),
                    state.to_string(),
                    active
                ],
            )?;
            Ok(())
        })
        .unwrap();
    id
}

fn seed_scores(launcher: &Launcher) {
    store(launcher)
        .call(|db| {
            db.execute(
                "INSERT INTO scores(nick,wins,losses,trophies) VALUES ('a',2,1,3),('b',1,2,0)",
                [],
            )?;
            Ok(())
        })
        .unwrap();
}

#[tokio::test]
async fn repeated_verdict_observation_and_unscored_rounds_never_change_wins_or_losses() {
    let root = tempfile::tempdir().unwrap();
    let launcher = Launcher::default();
    let (room, _) = open(root.path(), &launcher, OpenMode::OneShot).await;
    seed_scores(&launcher);
    let before = room.state().await.scoreboard;
    // Committed verdicts, late scores and unscored markers are observed, never re-applied.
    commit_each(&launcher, debate_fixture());
    for _ in 0..3 {
        let state = room.state().await;
        assert_eq!(state.scoreboard, before);
        assert_eq!(state.trophies[&nick("a")], 3);
        assert_eq!(
            state.debates.len(),
            0,
            "no contest row means no debate projection"
        );
        room.page_events(None, 500).await.unwrap();
        room.export(ExportScope::Debate, Some(CONTEST.parse().unwrap()))
            .await
            .unwrap();
    }
    assert_eq!(room.state().await.scoreboard, before);
    room.shutdown().await.unwrap();
    let (room, _) = open(root.path(), &Launcher::default(), OpenMode::OneShot).await;
    assert_eq!(room.state().await.scoreboard, before);
    room.shutdown().await.unwrap();
}

#[test]
fn daily_spend_is_projected_per_local_day_with_reservations_unknowns_and_effective_max() {
    let store = squad_store::Store::open(":memory:").unwrap();
    store
        .call(|db| {
            // Two days: a billed failure and a cancellation shaped into spend, plus unknown costs.
            db.execute(
                "INSERT INTO spend_daily VALUES ('2026-11-24',3500000,1000000,1)",
                [],
            )?;
            db.execute(
                "INSERT INTO spend_daily VALUES ('2026-11-25',250000,0,0)",
                [],
            )?;
            Ok(())
        })
        .unwrap();
    let project = |day: &'static str, max: i64| {
        store
            .call(move |db| squad_core::state::snapshot(db, &[], &BTreeMap::new(), day, max))
            .unwrap()
            .daily_spend
    };
    let yesterday = project("2026-11-24", 20_000_000);
    assert_eq!(
        (
            yesterday.spent_usd,
            yesterday.reserved_usd,
            yesterday.unknown_calls
        ),
        (3.5, 1.0, 1)
    );
    assert!(yesterday.has_unreconciled_charges);
    // A new local day starts from its own row; yesterday's spend and unknowns never carry over.
    let today = project("2026-11-25", 20_000_000);
    assert_eq!(
        (today.spent_usd, today.reserved_usd, today.unknown_calls),
        (0.25, 0., 0)
    );
    assert!(!today.has_unreconciled_charges);
    assert_eq!(today.day, "2026-11-25");
    // A day without rows is zero, and the effective max is whatever the caller resolved.
    let empty = project("2026-11-26", 5_000_000);
    assert_eq!((empty.spent_usd, empty.limit_usd), (0., 5.));
    // Repeated reads are idempotent.
    assert_eq!(project("2026-11-25", 20_000_000), today);
}

#[tokio::test]
async fn active_debate_tournament_and_grill_project_without_touching_trophies() {
    let root = tempfile::tempdir().unwrap();
    let launcher = Launcher::default();
    let (room, _) = open(root.path(), &launcher, OpenMode::OneShot).await;
    seed_scores(&launcher);
    let debate = serde_json::json!({"debateId":CONTEST,"state":"ROUND_RUNNING","debaters":["a","b"],"round":2,"totalRounds":3,"judgeModel":"judge/model","judgeRequested":true,"stateVersion":4,"terminalReason":null,"caseId":CASE,"caseIds":[CASE]});
    insert_contest(&launcher, "debate", debate, false);
    let tournament = serde_json::json!({"contestId":uuid(500),"state":"ANSWERING","stateVersion":2,"round":2,"replay":0,"survivors":["a","b"],"eliminated":[],"terminalReason":null});
    let id = insert_contest(&launcher, "tournament", tournament.clone(), true);
    let first = room.try_state().await.unwrap();
    assert_eq!(first.debates.len(), 1);
    assert_eq!(first.debates[0].round, 2);
    let Some(ContestState::Tournament(active)) = &first.active_contest else {
        panic!("{:?}", first.active_contest)
    };
    assert_eq!((active.contest_id, active.round), (id, 2));
    // Repeated observation adds no trophies; scores and trophies keep their persisted values.
    for _ in 0..3 {
        let again = room.state().await;
        assert_eq!(again.trophies, first.trophies);
        assert_eq!(again.scoreboard, first.scoreboard);
    }
    assert_eq!(first.trophies[&nick("a")], 3);
    // Replace the tournament with a grill; the active projection follows the one active contest.
    store(&launcher)
        .call(|db| Ok(db.execute("UPDATE contests SET active=0 WHERE kind='tournament'", [])?))
        .unwrap();
    let grill = serde_json::json!({"contestId":uuid(600),"state":"QUESTIONING","stateVersion":2,"round":1,"totalRounds":3,"leader":"a","grillerOrder":["b"],"turn":1,"planVersion":1,"questionsAsked":1,"passed":0,"terminalReason":null});
    insert_contest(&launcher, "grill", grill, true);
    let state = room.state().await;
    assert!(
        matches!(state.active_contest, Some(ContestState::Grill(_))),
        "{:?}",
        state.active_contest
    );
    assert_eq!(state.trophies, first.trophies);
    // Clearing resets the room's contest content but nothing else awards or duplicates trophies.
    accepted(
        room.submit(Request::Clear {
            request_id: EventId::new(),
        })
        .await,
    );
    let cleared = room.state().await;
    assert!(cleared.active_contest.is_none() && cleared.trophies.is_empty());
    room.shutdown().await.unwrap();
}

#[tokio::test]
async fn collecting_answers_and_awaiting_pick_survive_restart_without_expiring() {
    let root = tempfile::tempdir().unwrap();
    let launcher = Launcher::default();
    let (room, _) = open(root.path(), &launcher, OpenMode::OneShot).await;
    let question = MessageId::new();
    let answer = MessageId::new();
    let contest = ContestId::new();
    let collecting = serde_json::json!({"contestId":contest,"kind":"debate","state":"COLLECTING_ANSWERS","stateVersion":1,"questionMessageId":question,"addressed":["a","b"],"answers":[],"requiredPicks":2});
    insert_contest(&launcher, "debate", collecting, true);
    let before = room.state().await;
    let Some(ContestState::Selection(state)) = &before.active_contest else {
        panic!("{:?}", before.active_contest)
    };
    assert_eq!(state.state, ContestSelectionStatus::CollectingAnswers);
    assert_eq!(state.addressed, vec![nick("a"), nick("b")]);
    // The collection later reaches the pick; answers are retained as committed references.
    let awaiting = serde_json::json!({"contestId":contest,"kind":"debate","state":"AWAITING_PICK","stateVersion":2,"questionMessageId":question,"addressed":["a","b"],"answers":[{"nick":"a","messageId":answer,"text":"frozen"}],"requiredPicks":2});
    store(&launcher)
        .call(move |db| {
            Ok(db.execute(
                "UPDATE contests SET state_json=?,state_version=2",
                [awaiting.to_string()],
            )?)
        })
        .unwrap();
    let picking = room.state().await;
    let Some(ContestState::Selection(state)) = &picking.active_contest else {
        panic!()
    };
    assert_eq!(state.state, ContestSelectionStatus::AwaitingPick);
    assert_eq!(state.question_message_id, question);
    assert_eq!(state.answers[0].message_id, answer);
    assert!(room.state().await.pending_preflights.is_empty());
    room.shutdown().await.unwrap();
    // No clock expiry applies to a pending pick: it is identical after any number of restarts.
    for _ in 0..2 {
        let (room, _) = open(root.path(), &Launcher::default(), OpenMode::OneShot).await;
        assert_eq!(room.state().await.active_contest, picking.active_contest);
        room.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn four_new_modes_project_their_committed_state_without_recomputation() {
    let modes = [
        (
            "census",
            serde_json::json!({"contestId":uuid(800),"stateVersion":3,"status":"HUNTING","pass":2,"planVersion":1,"stepId":uuid(80),"artifactEventIds":[uuid(1)],"unfinishedWorkIds":[],"terminalReason":null}),
        ),
        (
            "crosscheck",
            serde_json::json!({"contestId":uuid(801),"planId":uuid(81),"planVersion":1,"gridVersion":2,"stateVersion":4,"status":"REVIEWING","stepId":uuid(82),"terminalReason":null}),
        ),
        (
            "survey",
            serde_json::json!({"contestId":uuid(802),"state":"VOTING","stateVersion":3,"stepId":uuid(83),"artifactEventIds":[uuid(2)],"terminalReason":null}),
        ),
        (
            "probe",
            serde_json::json!({"contestId":uuid(803),"state":"PROPOSING","stateVersion":2,"stepId":uuid(84),"artifactEventIds":[],"skippedWorkIds":[],"skippedMembers":["b"],"terminalReason":null}),
        ),
    ];
    for (kind, state) in modes {
        let root = tempfile::tempdir().unwrap();
        let launcher = Launcher::default();
        let (room, _) = open(root.path(), &launcher, OpenMode::OneShot).await;
        seed_scores(&launcher);
        let reports = commit_each(&launcher, new_modes_fixture());
        let scoreboard = room.state().await.scoreboard;
        let id = insert_contest(&launcher, kind, state.clone(), true);
        let first = room.state().await;
        let active = first.active_contest.clone().expect(kind);
        let projected = serde_json::to_value(&active).unwrap().to_string();
        assert!(projected.contains(&id.to_string()), "{kind}: {projected}");
        match (kind, &active) {
            ("census", ContestState::Census(s)) => assert_eq!((s.pass, s.plan_version), (2, 1)),
            ("survey", ContestState::Survey(s)) => assert_eq!(s.artifact_event_ids.len(), 1),
            ("probe", ContestState::Probe(s)) => assert_eq!(s.skipped_members, vec![nick("b")]),
            ("crosscheck", ContestState::Generic(g)) => {
                assert_eq!((g.kind, g.state_version), (ContestKind::Crosscheck, 4))
            }
            other => panic!("{other:?}"),
        }
        // Repeated observation and recovery change neither scores, trophies nor committed reports.
        for _ in 0..2 {
            let state = room.state().await;
            assert_eq!(state.active_contest, first.active_contest);
            assert_eq!(state.scoreboard, scoreboard);
            assert_eq!(state.trophies, first.trophies);
        }
        room.shutdown().await.unwrap();
        let (room, _) = open(root.path(), &Launcher::default(), OpenMode::OneShot).await;
        let after = room.state().await;
        assert_eq!(after.active_contest, first.active_contest);
        assert_eq!(after.scoreboard, scoreboard);
        assert_eq!(page_all(&room, 500).await[..reports.len()], reports[..]);
        // A terminal run leaves no active projection but never alters the report.
        room.shutdown().await.unwrap();
    }
}

#[derive(Clone)]
struct FailingRoundScores;
impl wiremock::Respond for FailingRoundScores {
    fn respond(&self, r: &wiremock::Request) -> wiremock::ResponseTemplate {
        use live::fake_openrouter::*;
        let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
        if body["model"] == "fixture/judge" {
            if body["messages"].to_string().contains("Score round ") {
                failed_round_score()
            } else {
                let scores=["member0","member1"].map(|nick|serde_json::json!({"nick":nick,"criteria":(["fit","feasibility","originality","survivedAttacks"].map(|id|serde_json::json!({"criterionId":id,"score":5,"reasoning":"Supported"})))}));
                structured_judge(
                    serde_json::json!({"winner":"member0","scores":scores,"reasoning":"Tie breaker","whatToStealFromLoser":[],"tallyOverrideReason":null}),
                )
            }
        } else {
            streaming("A concrete proposal", "stop")
        }
    }
}

#[tokio::test]
async fn unscored_rounds_never_increment_wins_or_losses_only_the_verdict_does() {
    let f = live::Fixture::new().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/api/v1/chat/completions"))
        .respond_with(FailingRoundScores)
        .with_priority(10)
        .mount(&f.fake.server)
        .await;
    let room = f.open(OpenMode::Interactive).await;
    room.submit(f.request("@member0 @member1 $debate $ROUNDS=2 @judge Compare designs"))
        .await;
    f.wait(|| f.scalar("SELECT COUNT(*) FROM events WHERE kind='verdict'") == 1)
        .await;
    let events = room.page_events(None, 500).await.unwrap();
    let unscored = events
        .iter()
        .filter(
            |e| matches!(&e.content, EnvelopePayload::DebateRoundScore(s) if s.scores.is_none()),
        )
        .count();
    assert!(
        unscored >= 1,
        "failed round judging commits unscored markers"
    );
    let state = room.state().await;
    let wins: u64 = state.scoreboard.iter().map(|s| s.wins).sum();
    let losses: u64 = state.scoreboard.iter().map(|s| s.losses).sum();
    assert_eq!(
        (wins, losses),
        (1, 1),
        "exactly one verdict was applied: {:?}",
        state.scoreboard
    );
    // Observation neither recomputes nor re-applies.
    assert_eq!(room.state().await.scoreboard, state.scoreboard);
    room.shutdown().await.unwrap();
}
