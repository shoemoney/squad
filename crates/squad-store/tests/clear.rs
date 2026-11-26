mod support;
use rusqlite::params;
use squad_protocol::*;
use squad_store::{Store, clear};

#[test]
fn populated_clear_is_foreign_key_safe_and_retains_only_accounting_and_counters() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("room.db");
    let store = Store::open(&path).unwrap();
    let source = support::event(&store);
    let source_id = source.base.id;
    let case = CaseId::new();
    let contest = ContestId::new();
    let work = WorkId::new();
    let step = StepId::new();
    store.call(move |db| {
        let tx=db.transaction()?;
        tx.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active,updated_seq) VALUES (?,'debate',1,1,'{}',1,1)",[contest.to_string()])?;
        tx.execute("INSERT INTO contest_events VALUES (?,1)",[contest.to_string()])?;
        tx.execute("INSERT INTO cases VALUES (?1,?2,1,?3,'test/judge','{}',1,NULL,'{}')",params![case.to_string(),format!("judge:{case}"),contest.to_string()])?;
        tx.execute("INSERT INTO case_events VALUES (?,1)",[case.to_string()])?;
        tx.execute("INSERT INTO contest_steps(step_id,contest_id,ordinal,script_version,move,state_json,inputs_json,updated_seq) VALUES (?1,?2,1,'v1','fanout','{}','[]',1)",params![step.to_string(),contest.to_string()])?;
        tx.execute("INSERT INTO work(work_id,kind,target,context_id,trigger_event_seq,trigger_event_id,message_id,status,attempt,epoch,runnable,contest_id,debate_id,case_id,round,max_output_tokens,step_id) VALUES (?1,'judge.round','judge',?2,1,?3,?4,'queued',2,1,1,?5,?5,?6,1,512,?7)",params![work.to_string(),format!("judge:{case}"),source_id.to_string(),MessageId::new().to_string(),contest.to_string(),case.to_string(),step.to_string()])?;
        tx.execute("INSERT INTO contest_assignments VALUES (?1,'judge',?2,'[]','{}',?3,'pending',NULL)",params![step.to_string(),format!("judge:{case}"),work.to_string()])?;
        tx.execute("INSERT INTO round_scores VALUES (?1,?2,1,?3,1,NULL,'{}','unscored')",params![contest.to_string(),case.to_string(),work.to_string()])?;
        tx.execute("INSERT INTO judge_repairs VALUES (?1,1,?2,'rejected secret','[\"invalid\"]')",params![work.to_string(),case.to_string()])?;
        tx.execute("INSERT INTO contest_answers VALUES (?,'alice',?,NULL,'pending')",params![contest.to_string(),work.to_string()])?;
        tx.execute("INSERT INTO contest_picks VALUES (?,1,'[]',1,NULL,NULL)",[contest.to_string()])?;
        tx.execute("INSERT INTO mode_artifacts VALUES (1,?,?,'fixture',?,'[]')",params![contest.to_string(),step.to_string(),work.to_string()])?;
        tx.execute("INSERT INTO contest_private_maps VALUES (?,'labels','{\"secret\":\"mapping\"}')",[contest.to_string()])?;
        tx.execute("INSERT INTO superseded VALUES (?1,?2,'alice',?3,?4,?5,1)",params![source_id.to_string(),MessageId::new().to_string(),MessageId::new().to_string(),work.to_string(),MessageId::new().to_string()])?;
        tx.execute("INSERT INTO history VALUES (1,?1,1,?2,'fixture','user',NULL,'case secret','{}',0)",params![format!("judge:{case}"),source_id.to_string()])?;
        tx.execute("INSERT INTO deferred_history VALUES (?1,'alice',?2,'fixture','{}')",params![contest.to_string(),source_id.to_string()])?;
        tx.execute_batch("UPDATE room_meta SET next_history_pos=2; UPDATE vars SET value=3,updated_seq=1; INSERT INTO scores VALUES ('alice',2,1,3,1); INSERT INTO spend_daily VALUES ('2026-11-24',123,50,1);")?;
        tx.execute("INSERT INTO attempts(work_id,attempt,epoch,target,context_id,model,day,started_at,outcome,unresolved) VALUES (?1,1,1,'judge',?2,'test/judge','2026-11-24','2026-11-24T00:00:00Z','unknown',1)",params![work.to_string(),format!("judge:{case}")])?;
        tx.execute("INSERT INTO reservations(reservation_id,epoch,work_id,attempt,amount_usd,unresolved,day,amount_micros) VALUES ('started',1,?,1,0.00003,1,'2026-11-24',30)",[work.to_string()])?;
        tx.execute("INSERT INTO reservations(reservation_id,epoch,amount_usd,unresolved,day,amount_micros) VALUES ('unstarted',1,0.00002,0,'2026-11-24',20)",[])?;
        tx.commit()?;Ok(())
    }).unwrap();
    // Reopening preserves all conversational content and room-lifetime receipts.
    drop(store);
    let store = Store::open(&path).unwrap();
    store
        .call(|db| {
            assert_eq!(
                db.query_row("SELECT COUNT(*) FROM case_events", [], |r| r
                    .get::<_, i64>(0))?,
                1
            );
            assert_eq!(
                db.query_row("SELECT COUNT(*) FROM receipts", [], |r| r.get::<_, i64>(0))?,
                1
            );
            Ok(())
        })
        .unwrap();
    let reset = EventId::new();
    let clear = move |tx: &rusqlite::Transaction<'_>,
                      group: &mut squad_store::commit::CommitGroup| {
        clear::content_with_defaults(
            tx,
            clear::Defaults {
                rounds: 7,
                grill_rounds: 4,
                census_passes: 3,
 negotiate_rounds:3,readback_rounds:2,debug_rounds:3,
            },
        )?;
        group.epoch += 1;
        group.push(
            tx,
            ProducerEnvelope {
                base: EnvelopeBase {
                    version: 1,
                    id: reset,
                    room_id: MainRoom::Main,
                    correlation_id: reset,
                    causation_id: None,
                    sender: Actor::System,
                    recipients: vec![Recipient::Me],
                    created_at: group.accepted_at,
                },
                content: EnvelopePayload::RoomCleared(RoomCleared {
                    epoch: 2,
                    previous_through: Cursor::new(1).ok(),
                }),
            },
        )?;
        Ok(group.accepted(reset))
    };
    store
        .commit(reset, "me".into(), "clear".into(), clear)
        .unwrap();
    assert!(
        store
            .commit(reset, "me".into(), "clear".into(), |_, _| panic!(
                "duplicate clear must use receipt"
            ))
            .unwrap()
            .duplicate
    );
    store.vacuum_after_clear().unwrap();
    store
        .call(|db| {
            for table in [
                "history",
                "deferred_history",
                "contests",
                "contest_events",
                "contest_answers",
                "contest_picks",
                "cases",
                "case_events",
                "work",
                "judge_repairs",
                "round_scores",
                "contest_steps",
                "contest_assignments",
                "mode_artifacts",
                "contest_private_maps",
                "superseded",
                "scores",
            ] {
                assert_eq!(
                    db.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r
                        .get::<_, i64>(0))?,
                    0,
                    "{table}"
                );
            }
            assert_eq!(
                db.query_row(
                    "SELECT epoch,next_event_seq,next_history_pos,replay_floor FROM room_meta",
                    [],
                    |r| Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, i64>(2)?,
                        r.get::<_, i64>(3)?
                    ))
                )?,
                (2, 3, 2, 2)
            );
            assert_eq!(
                db.query_row("SELECT COUNT(*) FROM events", [], |r| r.get::<_, i64>(0))?,
                1
            );
            assert_eq!(
                db.query_row("SELECT COUNT(*) FROM receipts", [], |r| r.get::<_, i64>(0))?,
                1
            );
            assert_eq!(
                db.query_row("SELECT value FROM vars WHERE name='ROUNDS'", [], |r| r
                    .get::<_, i64>(0))?,
                7
            );
            assert_eq!(
                db.query_row(
                    "SELECT value FROM vars WHERE name='GRILL_ROUNDS'",
                    [],
                    |r| r.get::<_, i64>(0)
                )?,
                4
            );
            assert_eq!(
                db.query_row(
                    "SELECT value FROM vars WHERE name='CENSUS_PASSES'",
                    [],
                    |r| r.get::<_, i64>(0)
                )?,
                3
            );
            let spend = squad_store::accounting::daily(db, "2026-11-24")?;
            assert_eq!(
                (
                    spend.spent_micros,
                    spend.reserved_micros,
                    spend.unknown_calls
                ),
                (123, 30, 1)
            );
            assert_eq!(
                db.query_row("SELECT COUNT(*) FROM attempts", [], |r| r.get::<_, i64>(0))?,
                1
            );
            assert_eq!(
                db.query_row("PRAGMA freelist_count", [], |r| r.get::<_, i64>(0))?,
                0
            );
            assert!(
                db.prepare("PRAGMA foreign_key_check")?
                    .query([])?
                    .next()?
                    .is_none()
            );
            Ok(())
        })
        .unwrap();
}

#[test]
fn failed_post_commit_vacuum_retries_on_the_next_writer_job() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("room.db");
    let store = Store::open(&path).unwrap();
    store.call(|db| {
        db.busy_timeout(std::time::Duration::ZERO)?;
        db.execute_batch("CREATE TABLE vacuum_fixture(content BLOB); INSERT INTO vacuum_fixture VALUES (zeroblob(1000000)); DROP TABLE vacuum_fixture;")?;
        assert!(db.query_row("PRAGMA freelist_count",[],|r|r.get::<_,i64>(0))?>0);
        Ok(())
    }).unwrap();
    let blocker = squad_store::open(&path).unwrap();
    blocker.execute_batch("BEGIN IMMEDIATE").unwrap();
    assert!(store.vacuum_after_clear().is_err());
    blocker.execute_batch("ROLLBACK").unwrap();
    store
        .call(|db| {
            assert_eq!(
                db.query_row("PRAGMA freelist_count", [], |r| r.get::<_, i64>(0))?,
                0
            );
            Ok(())
        })
        .unwrap();
}
