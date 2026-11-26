mod support;
use rusqlite::params;
use squad_protocol::*;
use squad_store::{Store, contest_steps, readers::ScopedReaders};
fn fixture() -> (Store, Work, EventId) {
    let store = Store::open(":memory:").unwrap();
    let source = support::event(&store);
    let event = source.base.id;
    let contest = ContestId::new();
    let step = StepId::new();
    let nick: Nick = "a".parse().unwrap();
    let work = Work {
        work_id: WorkId::new(),
        message_id: MessageId::new(),
        in_reply_to_message_id: None,
        kind: WorkKind::ForgeDraft,
        target: Target::Member(nick.clone()),
        context_id: ContextId(format!("contest:{contest}:{step}:{nick}")),
        trigger_event_id: event,
        history_through: HistoryPosition::new(1).unwrap(),
        attempt: 1,
        debate_id: None,
        round: None,
        replay: None,
        case_id: None,
        prompt_ref: Some(PromptRef {
            template_id: Text::new("forge.draft").unwrap(),
            pack_version: Text::new("1.0.0").unwrap(),
            template_hash: Text::new("fixture").unwrap(),
        }),
        max_output_tokens: None,
        contest_id: Some(contest),
        step_id: Some(step),
    };
    let w = work.clone();
    store.call(move|db|{
 db.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active) VALUES(?,'forge',1,1,'{}',0)",[contest.to_string()])?;
 let frozen=ContestStep{contest_id:contest,step_id:step,ordinal:1,script_version:Text::new("v1").unwrap(),move_name:ContestMove::Fanout,inputs:vec![event],assignments:vec![ContestAssignment{target:w.target.clone(),context_id:w.context_id.clone(),payload_refs:vec![event],work_id:Some(w.work_id)}]};
 contest_steps::insert(db,&frozen,source.commit.seq)?;
 db.execute("INSERT INTO history(pos,context_id,event_seq,origin_event_id,projection_key,role,content,meta) VALUES(1,?,1,?,'fixture','user','input','{}')",params![w.context_id.0,event.to_string()])?;
 let plan=ContestPreflight{contest_id:contest,kind:ContestKind::Forge,plan_id:PlanId::new(),expires_at:chrono::Utc::now(),estimate:DebateEstimate{estimated_usd:1.0,reserved_usd:2.0,remaining_usd:10.0,model_calls:1},participants:vec!["a".parse().unwrap()],confirmation_required:false,warning:None,settings:serde_json::json!({"reservedAttempts":2,"attemptShares":{"forge.draft":1_000_000}})};
 db.execute("UPDATE contests SET plan_json=? WHERE contest_id=?",params![serde_json::to_string(&plan)?,contest.to_string()])?;
 squad_store::work::insert(db,&w,&ExecutorBinding{model:Text::new("test/model").unwrap(),effort:ReasoningEffort::Low},None)?;
 contest_steps::attach_work(db,step,&w)?;
 assert_eq!(db.query_row("SELECT json_extract(scheduling_meta,'$.attemptReservationMicros') FROM work WHERE work_id=?",[w.work_id.to_string()],|r|r.get::<_,i64>(0))?,1_000_000);
 db.execute("INSERT INTO contest_artifacts VALUES(1,?,?,'forge','forge.draft',0,'system','members','[\"a\"]','{\"body\":\"private\"}')",params![contest.to_string(),step.to_string()])?;
 Ok(())}).unwrap();
    (store, work, event)
}
#[test]
fn artifact_reads_require_both_work_assignment_and_audience() {
    let (store, work, event) = fixture();
    let reader = ScopedReaders::new(store.clone(), ExecutorKey::Member("a".parse().unwrap()), 1);
    assert_eq!(
        reader.artifact_for_work(work.work_id, event).unwrap()["body"],
        "private"
    );
    let other = ScopedReaders::new(store.clone(), ExecutorKey::Member("b".parse().unwrap()), 1);
    assert!(other.artifact_for_work(work.work_id, event).is_err());
    for audience in ["human", "case"] {
        store
            .call(move |db| {
                db.execute(
                    "UPDATE contest_artifacts SET audience=?,recipients='[]'",
                    [audience],
                )?;
                Ok(())
            })
            .unwrap();
        assert!(reader.artifact_for_work(work.work_id, event).is_err());
    }
    store
        .call(|db| {
            db.execute(
                "UPDATE contest_artifacts SET audience='members',recipients='[\"b\"]'",
                [],
            )?;
            Ok(())
        })
        .unwrap();
    assert!(reader.artifact_for_work(work.work_id, event).is_err());
}
#[test]
fn member_repair_diagnostics_are_private_and_attempt_bound() {
    let (store, work, _) = fixture();
    let w = work.clone();
    store
        .call(move |db| {
            squad_store::judge_repairs::save_mode(
                db,
                &w,
                &JudgeRepairData {
                    attempt: 2,
                    rejected_output: "private rejected output".into(),
                    validation_errors: vec!["bad reference".into()],
                },
            )?;
            db.execute(
                "UPDATE work SET attempt=2 WHERE work_id=?",
                [w.work_id.to_string()],
            )?;
            Ok(())
        })
        .unwrap();
    let a = ScopedReaders::new(store.clone(), ExecutorKey::Member("a".parse().unwrap()), 1);
    assert_eq!(
        ready(a.repair_for_attempt(work.work_id, 2))
            .unwrap()
            .unwrap()
            .rejected_output,
        "private rejected output"
    );
    assert!(ready(a.repair_for_attempt(work.work_id, 3)).is_err());
    let b = ScopedReaders::new(store, ExecutorKey::Member("b".parse().unwrap()), 1);
    assert!(ready(b.repair_for_attempt(work.work_id, 2)).is_err());
}

fn ready<T>(mut future: ReaderFuture<'_, T>) -> Result<T, ReaderError> {
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    match future.as_mut().poll(&mut cx) {
        std::task::Poll::Ready(value) => value,
        std::task::Poll::Pending => panic!("store reader must be ready"),
    }
}
