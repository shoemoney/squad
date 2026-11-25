mod support;
use squad_protocol::*;
use squad_store::Store;

fn round_fixture() -> (Store, WorkId, ExecutorKey) {
    let store = Store::open(":memory:").unwrap();
    let source = support::event(&store);
    let case = CaseId::new();
    let contest = ContestId::new();
    let id = WorkId::new();
    store.call(move |db| {
        let tx=db.transaction()?;
        tx.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active) VALUES (?,'debate',1,1,'{}',1)",[contest.to_string()])?;
        tx.execute("INSERT INTO cases VALUES (?1,?2,1,?3,'test/judge','{}',1,NULL,'{}')",rusqlite::params![case.to_string(),format!("judge:{case}"),contest.to_string()])?;
        let context=ContextId(format!("judge:{case}"));
        let boundary=squad_store::history::project(&tx,&squad_store::history::attributed(&source,context.clone(),"case.material",HistoryRole::User,None,"round material".into()))?.unwrap();
        let work=Work {work_id:id,message_id:MessageId::new(),in_reply_to_message_id:None,kind:WorkKind::JudgeRound,target:Target::Judge,context_id:context,trigger_event_id:source.base.id,history_through:boundary,attempt:1,debate_id:Some(contest),round:Some(1),replay:None,case_id:Some(case),prompt_ref:Some(PromptRef {template_id:Text::new("judge.round.md").unwrap(),pack_version:Text::new("fixture").unwrap(),template_hash:Text::new("fixture-hash").unwrap()}),max_output_tokens:Some(512),contest_id:Some(contest),step_id:None};
        squad_store::work::insert(&tx,&work,&ExecutorBinding {model:Text::new("test/judge").unwrap(),effort:ReasoningEffort::Low},None)?;
        tx.commit()?;Ok(())
    }).unwrap();
    (store, id, ExecutorKey::Case(case))
}
#[test]
fn interrupted_round_calls_keep_two_attempt_limit_and_unknown_accounting() {
    let (store, id, owner) = round_fixture();
    store
        .call(move |db| {
            for ordinal in 1..=2 {
                let tx = db.transaction()?;
                let grant =
                    squad_store::leases::claim(&tx, id, &owner, chrono::Utc::now())?.unwrap();
                assert_eq!(grant.work.attempt, ordinal);
                squad_store::accounting::start(
                    &tx,
                    &grant,
                    chrono::Utc::now(),
                    "2026-11-24",
                    20_000_000,
                )?;
                // A repeated start acknowledgement cannot count a second call.
                squad_store::accounting::start(
                    &tx,
                    &grant,
                    chrono::Utc::now(),
                    "2026-11-24",
                    20_000_000,
                )?;
                tx.commit()?;
                let tx = db.transaction()?;
                squad_store::leases::recover(&tx, true, chrono::Utc::now())?;
                tx.commit()?;
                assert!(
                    squad_store::work::check_fence(
                        db,
                        id,
                        &owner,
                        ordinal,
                        grant.fence,
                        grant.epoch,
                        chrono::Utc::now()
                    )
                    .is_err()
                );
            }
            let tx = db.transaction()?;
            assert!(squad_store::leases::claim(&tx, id, &owner, chrono::Utc::now())?.is_none());
            assert_eq!(squad_store::work::record(&tx, id)?.unwrap().attempt, 2);
            assert_eq!(
                squad_store::accounting::daily(&tx, "2026-11-24")?.unknown_calls,
                2
            );
            assert_eq!(
                tx.query_row("SELECT COUNT(*) FROM attempts", [], |r| r.get::<_, i64>(0))?,
                2
            );
            tx.commit()?;
            Ok(())
        })
        .unwrap();
}
#[test]
fn expired_unstarted_lease_reuses_ordinal_but_retains_deadline_and_frozen_boundary() {
    let (store, id, owner) = round_fixture();
    store
        .call(move |db| {
            let now = chrono::Utc::now();
            let deadline = now + chrono::Duration::minutes(10);
            db.execute(
                "UPDATE work SET execution_deadline=? WHERE work_id=?",
                rusqlite::params![deadline.to_rfc3339(), id.to_string()],
            )?;
            let tx = db.transaction()?;
            let first = squad_store::leases::claim(&tx, id, &owner, now)?.unwrap();
            tx.commit()?;
            let after = now + chrono::Duration::minutes(2);
            let tx = db.transaction()?;
            assert_eq!(squad_store::leases::recover(&tx, false, after)?, vec![id]);
            tx.commit()?;
            let tx = db.transaction()?;
            let next = squad_store::leases::claim(&tx, id, &owner, after)?.unwrap();
            assert_eq!(next.work.attempt, 1);
            assert!(next.fence > first.fence);
            assert_eq!(next.work.history_through, first.work.history_through);
            assert_eq!(next.deadline, Some(deadline));
            tx.commit()?;
            let tx = db.transaction()?;
            squad_store::leases::recover(&tx, false, deadline)?;
            assert!(squad_store::leases::claim(&tx, id, &owner, deadline)?.is_none());
            tx.commit()?;
            Ok(())
        })
        .unwrap();
}

#[test]
fn an_unstarted_repair_claim_does_not_inherit_the_previous_calls_started_flag() {
    let (store, id, owner) = round_fixture();
    store
        .call(move |db| {
            let tx = db.transaction()?;
            let first = squad_store::leases::claim(&tx, id, &owner, chrono::Utc::now())?.unwrap();
            squad_store::accounting::start(
                &tx,
                &first,
                chrono::Utc::now(),
                "2026-11-24",
                20_000_000,
            )?;
            // The owning judge policy has queued a repair following validation failure.
            tx.execute(
                "UPDATE work SET status='queued' WHERE work_id=?",
                [id.to_string()],
            )?;
            let second = squad_store::leases::claim(&tx, id, &owner, chrono::Utc::now())?.unwrap();
            assert_eq!(second.work.attempt, 2);
            tx.commit()?;
            let tx = db.transaction()?;
            squad_store::leases::recover(&tx, true, chrono::Utc::now())?;
            let recovered =
                squad_store::leases::claim(&tx, id, &owner, chrono::Utc::now())?.unwrap();
            assert_eq!(recovered.work.attempt, 2);
            assert!(recovered.fence > second.fence);
            assert_eq!(
                squad_store::accounting::daily(&tx, "2026-11-24")?.unknown_calls,
                1
            );
            tx.commit()?;
            Ok(())
        })
        .unwrap();
}
