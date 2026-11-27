use squad_protocol::*;
use squad_store::{Store, events};

fn append(store: &Store, count: usize) -> Vec<CommittedEnvelope> {
    let id = EventId::new();
    store
        .commit(id, "fixture".into(), id.to_string(), move |tx, group| {
            for i in 0..count {
                group.push(
                    tx,
                    ProducerEnvelope {
                        base: EnvelopeBase {
                            version: 1,
                            id: EventId::new(),
                            room_id: MainRoom::Main,
                            correlation_id: id,
                            causation_id: None,
                            sender: Actor::System,
                            recipients: vec![Recipient::Me],
                            created_at: group.accepted_at,
                        },
                        content: EnvelopePayload::SystemNotice(SystemNotice {
                            code: Text::new("FIXTURE").unwrap(),
                            text: Text::new(format!("item {i}")).unwrap(),
                            work_id: None,
                            debate_id: None,
                            case_id: None,
                            contest_id: None,
                        }),
                    },
                )?;
            }
            Ok(group.accepted(id))
        })
        .unwrap()
        .events
}
#[test]
fn groups_are_complete_in_both_directions_and_at_high_water() {
    let store = Store::open(":memory:").unwrap();
    append(&store, 3);
    append(&store, 2);
    store
        .call(|db| {
            assert_eq!(events::page(db, None, 1)?.len(), 3);
            assert_eq!(events::page(db, Some(Cursor::new(1).unwrap()), 1)?.len(), 3);
            assert_eq!(
                events::before(db, None, 1)?
                    .iter()
                    .map(|e| e.commit.seq.get())
                    .collect::<Vec<_>>(),
                [4, 5]
            );
            assert_eq!(
                events::before(db, Some(Cursor::new(3).unwrap()), 1)?.len(),
                3
            );
            assert_eq!(events::boundaries(db)?.high_water.unwrap().get(), 5);
            let through = Cursor::new(3).unwrap();
            assert_eq!(
                events::replay(db, None, Some(through), 100).unwrap().len(),
                3
            );
            assert!(
                events::replay(db, None, Some(Cursor::new(2).unwrap()), 100)
                    .unwrap()
                    .is_empty()
            );
            assert!(events::page(db, Some(Cursor::new(5).unwrap()), 100)?.is_empty());
            Ok(())
        })
        .unwrap();
}
#[test]
fn malformed_future_and_cleared_cursors_require_explicit_reset() {
    let store = Store::open(":memory:").unwrap();
    append(&store, 2);
    store
        .call(|db| {
            for value in [
                "0",
                "01",
                "-1",
                "+1",
                "1.0",
                "abc",
                "9223372036854775808",
                "3",
            ] {
                assert!(
                    matches!(
                        events::replay(db, Some(value), None, 1),
                        Err(events::ReplayError::Cursor {
                            code: ErrorCode::CURSOR_INVALID,
                            ..
                        })
                    ),
                    "{value}"
                );
            }
            let tx = db.transaction()?;
            squad_store::clear::content(&tx)?;
            tx.commit()?;
            Ok(())
        })
        .unwrap();
    append(&store, 1);
    store.call(|db| {
        assert!(matches!(events::replay(db,Some("1"),None,1),Err(events::ReplayError::Cursor {code:ErrorCode::CURSOR_EXPIRED,earliest_cursor:Some(c)}) if c.get()==3));
        assert_eq!(events::replay(db,None,None,1).unwrap()[0].commit.seq.get(),3);
        Ok(())
    }).unwrap();
}
#[test]
fn durable_catchup_after_broadcast_lag_never_invents_ephemeral_history() {
    let store = Store::open(":memory:").unwrap();
    append(&store, 2);
    let mut replay = events::FeedReplay::default();
    replay = store
        .call(move |db| {
            assert_eq!(replay.catch_up(db, 1).unwrap().len(), 2);
            Ok(replay)
        })
        .unwrap();
    let final_group = append(&store, 3);
    store
        .call(move |db| {
            let caught = replay.catch_up(db, 1).unwrap();
            assert_eq!(
                caught.iter().map(|e| e.base.id).collect::<Vec<_>>(),
                final_group.iter().map(|e| e.base.id).collect::<Vec<_>>()
            );
            assert!(replay.catch_up(db, 1).unwrap().is_empty());
            Ok(())
        })
        .unwrap();
}

#[test]
fn late_round_scores_and_state_events_replay_without_recomputing_the_tally() {
    use std::collections::BTreeMap;
    let store = Store::open(":memory:").unwrap();
    let debate = ContestId::new();
    let case = CaseId::new();
    let alice: MemberNick = "alice".parse().unwrap();
    let bob: MemberNick = "bob".parse().unwrap();
    let tally = BTreeMap::from([(alice.clone(), 4), (bob.clone(), 5)]);
    let expected = tally.clone();
    let id = EventId::new();
    store
        .commit(
            id,
            "fixture".into(),
            "late round".into(),
            move |tx, group| {
                for payload in [
                    EnvelopePayload::DebateState(DebateState {
                        debate_id: debate,
                        state: DebateStatus::RoundRunning,
                        debaters: [alice, bob],
                        round: 2,
                        total_rounds: 2,
                        judge_model: "test/judge".into(),
                        judge_requested: true,
                        state_version: 3,
                        terminal_reason: None,
                        case_id: Some(case),
                        case_ids: vec![case],
                    }),
                    EnvelopePayload::DebateRoundScore(DebateRoundScore {
                        debate_id: debate,
                        case_id: case,
                        round: 1,
                        scores: None,
                        tally,
                        reason: Some(Text::new("unscored").unwrap()),
                    }),
                ] {
                    group.push(
                        tx,
                        ProducerEnvelope {
                            base: EnvelopeBase {
                                version: 1,
                                id: EventId::new(),
                                room_id: MainRoom::Main,
                                correlation_id: id,
                                causation_id: None,
                                sender: Actor::System,
                                recipients: vec![Recipient::Me],
                                created_at: group.accepted_at,
                            },
                            content: payload,
                        },
                    )?;
                }
                Ok(group.accepted(id))
            },
        )
        .unwrap();
    store
        .call(move |db| {
            let events = events::page(db, None, 1)?;
            assert_eq!(events.len(), 2);
            let EnvelopePayload::DebateState(state) = &events[0].content else {
                panic!("state")
            };
            assert_eq!(state.round, 2);
            let EnvelopePayload::DebateRoundScore(score) = &events[1].content else {
                panic!("score")
            };
            assert_eq!(score.round, 1);
            assert_eq!(score.tally, expected);
            assert!(score.scores.is_none());
            assert_eq!(
                db.query_row("SELECT COUNT(*) FROM round_scores", [], |r| r
                    .get::<_, i64>(0))?,
                0,
                "replay must not run projections"
            );
            Ok(())
        })
        .unwrap();
}

#[test]
fn generic_controls_artifacts_reports_and_budget_replay_without_private_maps() {
    let store = Store::open(":memory:").unwrap();
    let contest = ContestId::new();
    let step = StepId::new();
    let message = MessageId::new();
    let id = EventId::new();
    let nicks: Vec<MemberNick> = vec!["alice".parse().unwrap(), "bob".parse().unwrap()];
    store.commit(id,"fixture".into(),"generic replay".into(),move |tx,group| {
        tx.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active) VALUES (?,'debate',1,1,'{}',0)",[contest.to_string()])?;
        tx.execute("INSERT INTO contest_private_maps VALUES (?,'labels','{\"privateMarker\":\"do not leak\"}')",[contest.to_string()])?;
        let payloads=vec![
            EnvelopePayload::ContestCollecting(ContestCollecting {contest_id:contest,kind:ContestKind::Debate,question_message_id:message,question:"question".into(),addressed:nicks.clone()}),
            EnvelopePayload::ContestPickRequested(ContestPickRequested {contest_id:contest,kind:ContestKind::Debate,question_message_id:message,eligible:nicks.clone(),required_picks:2,banner:"Pick debaters".into()}),
            EnvelopePayload::ContestPicked(ContestPicked {contest_id:contest,kind:ContestKind::Debate,nicks,source_message_ids:vec![message,MessageId::new()]}),
            EnvelopePayload::ContestStep(ContestStep {contest_id:contest,step_id:step,ordinal:1,script_version:Text::new("fixture.v1").unwrap(),move_name:ContestMove::Report,inputs:vec![],assignments:vec![]}),
            EnvelopePayload::ContestArtifact(ContestArtifact {kind: None, round: None, actor: None, audience: None, body: None, contest_id:contest,step_id:step,artifact_kind:Text::new("fixture").unwrap(),work_id:None,source_refs:vec![],data:serde_json::json!({"public":"artifact"})}),
            EnvelopePayload::ContestReport(ContestReport {contest_id:contest,step_id:step,artifact_event_ids:vec![],skipped_work_ids:vec![],data:serde_json::json!({"public":"report"})}),
            EnvelopePayload::BudgetState(BudgetStateEvent {spent_usd:0.5,reserved_usd:0.1,limit_usd:10.0,estimated_usd:0.,unresolved_attempts:1,day:"2026-11-24".into(),unknown_calls:1}),
        ];
        for payload in payloads {group.push(tx,ProducerEnvelope {base:EnvelopeBase {version:1,id:EventId::new(),room_id:MainRoom::Main,correlation_id:id,causation_id:None,sender:Actor::System,recipients:vec![Recipient::Me],created_at:group.accepted_at},content:payload})?;}
        Ok(group.accepted(id))
    }).unwrap();
    store
        .call(|db| {
            let events = events::page(db, None, 1)?;
            assert_eq!(events.len(), 7);
            let wire = serde_json::to_string(&events)?;
            assert!(!wire.contains("privateMarker"));
            assert!(!wire.contains("do not leak"));
            assert!(matches!(
                events[5].content,
                EnvelopePayload::ContestReport(_)
            ));
            assert!(matches!(events[6].content, EnvelopePayload::BudgetState(_)));
            Ok(())
        })
        .unwrap();
}
