use squad_protocol::*;
use squad_store::{Store, bass_ackwards_prd::*};
#[test]
fn migration_records_round_trip_and_cascade_on_clear() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("room.db");
    let store = Store::open(&path).unwrap();
    let id = ContestId::new();
    store.call(move|db|{db.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active) VALUES(?,'bass_ackwards_prd',1,1,'{}',0)",[id.to_string()])?;put(db,id,"lens",&serde_json::json!({"rotation":["a","b","c"]}))?;put(db,id,"version:1",&serde_json::json!({"key":"permanent"}))?;Ok(())}).unwrap();
    drop(store);
    let store = Store::open(&path).unwrap();
    store
        .call(move |db| {
            let v: serde_json::Value = get(db, id, "version:1")?;
            assert_eq!(v["key"], "permanent");
            assert!(optional::<serde_json::Value>(db, id, "missing")?.is_none());
            db.execute("DELETE FROM contests WHERE contest_id=?", [id.to_string()])?;
            let n: i64 =
                db.query_row("SELECT COUNT(*) FROM bass_ackwards_prd_records", [], |r| {
                    r.get(0)
                })?;
            assert_eq!(n, 0);
            Ok(())
        })
        .unwrap();
}

fn ready<T>(future: ReaderFuture<'_, T>) -> Result<T, ReaderError> {
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    let mut future = std::pin::pin!(future);
    match future.as_mut().poll(&mut context) {
        std::task::Poll::Ready(v) => v,
        std::task::Poll::Pending => panic!("store reader unexpectedly waits"),
    }
}
#[test]
fn fresh_step_history_rejects_sibling_work_and_stale_epochs() {
    let store = Store::open(":memory:").unwrap();
    let contest = ContestId::new();
    let step = StepId::new();
    let ids = [WorkId::new(), WorkId::new()];
    let event = EventId::new();
    store.commit(event,"fixture".into(),event.to_string(),move|tx,g|{
 let seq=g.push(tx,ProducerEnvelope{base:EnvelopeBase{version:1,id:event,room_id:MainRoom::Main,correlation_id:event,causation_id:None,sender:Actor::System,recipients:vec![Recipient::Me],created_at:g.accepted_at},content:EnvelopePayload::SystemNotice(SystemNotice{code:"FIXTURE".parse().unwrap(),text:"fixture".parse().unwrap(),work_id:None,debate_id:None,case_id:None,contest_id:None})})?;
 save(tx,&BassAckwardsPrdState{contest_id:contest,state_version:1,state:BassAckwardsPrdStatus::DefiningDone,step_id:Some(step),round:0,artifact_event_ids:vec![],terminal_reason:None},None,seq)?;
 let assignments=["alpha","beta"].into_iter().enumerate().map(|(i,n)|ContestAssignment{target:Target::Member(n.parse().unwrap()),context_id:ContextId(format!("contest:{contest}:{step}:{n}")),payload_refs:vec![],work_id:Some(ids[i])}).collect::<Vec<_>>();
 squad_store::contest_steps::insert(tx,&ContestStep{contest_id:contest,step_id:step,ordinal:1,script_version:"v1".parse().unwrap(),move_name:ContestMove::Fanout,inputs:vec![],assignments:assignments.clone()},seq)?;
 tx.execute("INSERT INTO catalog VALUES('test/model',1000000,65536,'{\"prompt\":\"0\",\"completion\":\"0\"}','[]','2026-11-26T00:00:00Z')",[])?;
 for (i,a) in assignments.into_iter().enumerate(){let pos=squad_store::history::project(tx,&HistoryProjection{context_id:a.context_id.clone(),origin_event_id:event,projection_key:"bass_ackwards_prd.step".parse().unwrap(),role:HistoryRole::User,name:Some("squad".into()),content:format!("private input {i}"),meta:HistoryMetadata::Anonymous{anonymous:True}})?.unwrap();let w=Work{work_id:ids[i],message_id:MessageId::new(),in_reply_to_message_id:None,kind:WorkKind::BassAckwardsPrdDone,target:a.target.clone(),context_id:a.context_id,trigger_event_id:event,history_through:pos,attempt:1,debate_id:None,round:None,replay:None,case_id:None,prompt_ref:Some(PromptRef{pack_version:"v1".parse().unwrap(),template_id:"bass_ackwards_prd.done".parse().unwrap(),template_hash:"hash".parse().unwrap()}),max_output_tokens:Some(2048),contest_id:Some(contest),step_id:Some(step)};squad_store::work::insert(tx,&w,&ExecutorBinding{model:"test/model".parse().unwrap(),effort:ReasoningEffort::Low},None)?;squad_store::contest_steps::attach_work(tx,step,&w)?;squad_store::leases::claim(tx,w.work_id,&ExecutorKey::Member(if i==0{"alpha"}else{"beta"}.parse().unwrap()),chrono::Utc::now(),None)?;}
 Ok(g.accepted(event))}).unwrap();
    let reader = squad_store::readers::ScopedReaders::new(
        store.clone(),
        ExecutorKey::Member("alpha".parse().unwrap()),
        1,
    );
    assert_eq!(
        ready(reader.for_work(ids[0])).unwrap()[0].content,
        "private input 0"
    );
    assert_eq!(
        ready(reader.for_work(ids[1])).unwrap_err(),
        ReaderError::Forbidden
    );
    let stale = squad_store::readers::ScopedReaders::new(
        store,
        ExecutorKey::Member("alpha".parse().unwrap()),
        2,
    );
    assert_eq!(
        ready(stale.for_work(ids[0])).unwrap_err(),
        ReaderError::StaleWork
    );
}
