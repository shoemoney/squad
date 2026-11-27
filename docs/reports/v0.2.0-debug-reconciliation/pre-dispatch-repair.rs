//! One-off, source-proven repair of an old pre-dispatch DebugMerge reservation.
use squad_core::router::Router;
use squad_protocol::*;
use squad_store::Store;
use std::collections::BTreeMap;
fn main() {
    let path = std::env::args().nth(1).expect("SQLite path");
    let store = Store::open(path).unwrap();
    let (epoch,mut record,roster,bindings) = store.call(|db| {
        let id="01a21adf-ab58-75a3-a719-7a62874fb9af";
        let (kind,status):(String,String)=db.query_row("SELECT kind,status FROM work WHERE work_id=?",[id],|r|Ok((r.get(0)?,r.get(1)?)))?;
        assert_eq!((kind.as_str(),status.as_str()),("debug.merge","cancelled"));
        let (epoch,json):(i64,String)=db.query_row("SELECT epoch,json_object('workId',work_id,'attempt',attempt,'target',target,'contextId',context_id,'providerRequestId',provider_request_id,'model',model,'responseModel',response_model,'day',day,'startedAt',started_at,'endedAt',ended_at,'outcome',outcome,'usage',json(usage_json),'estimatedCostUsd',estimated_cost_usd,'debateId',debate_id,'caseId',case_id,'contestId',contest_id) FROM attempts WHERE work_id=? AND attempt=1 AND unresolved=1",[id],|r|Ok((r.get(0)?,r.get(1)?)))?;
        let record:UsageRecord=serde_json::from_str(&json)?;
        assert_eq!(record.target,Target::Judge);assert!(record.provider_request_id.is_none());assert!(record.usage.is_none());
        assert_eq!(db.query_row("SELECT COUNT(*) FROM attempts",[],|r|r.get::<_,i64>(0))?,12);
        let mut q=db.prepare("SELECT nick,resolved_slug,role FROM models")?;
        let rows=q.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        let roster=rows.iter().filter(|r|r.2=="member").map(|r|r.0.parse().unwrap()).collect();
        let bindings=rows.into_iter().map(|(n,m,_)|(n,ExecutorBinding{model:m.parse().unwrap(),effort:ReasoningEffort::Low})).collect::<BTreeMap<_,_>>();
        Ok((epoch as u64,record,roster,bindings))
    }).unwrap();
    record.usage = Some(Usage {
        input_tokens: 0,
        output_tokens: 0,
        cost_usd: Some(0.),
    });
    record.ended_at = Some(chrono::Utc::now());
    record.outcome = UsageOutcome::Cancelled;
    let owner = ExecutorKey::Case(record.case_id.unwrap());
    let work_id = record.work_id;
    let contest_id = record.contest_id;
    let case_id = record.case_id;
    let mut router = Router::new(store.clone(), roster, bindings, 150_000_000).unwrap();
    let result = router
        .process(
            Request::Internal(InternalRequest::Executor(ExecutorRequest::Account {
                fence: 0,
                epoch,
                record,
            })),
            Some(owner),
        )
        .unwrap();
    assert!(
        matches!(result.result, RequestResult::Accepted { .. }),
        "{:?}",
        result.result
    );
    let id = EventId::new();
    store.commit(id,"operator-evidence".into(),id.to_string(),move |tx,g| {
        g.push(tx,ProducerEnvelope{base:EnvelopeBase{version:1,id,room_id:MainRoom::Main,correlation_id:id,causation_id:None,sender:Actor::System,recipients:vec![Recipient::Me],created_at:g.accepted_at},content:EnvelopePayload::SystemNotice(SystemNotice{code:"BILLING_PRE_DISPATCH_RECONCILED".parse().unwrap(),text:"Old source 30e7407 runtime/judge.rs delegates debug.merge to ordinary JudgeExecutor; its work-kind guard rejects before invoking the provider runner. Exact cancelled work 01a21adf-ab58-75a3-a719-7a62874fb9af has no provider request ID. Reconciled this source-proven undispatched attempt at zero cost through the authenticated accounting writer; other ID-less attempts remain unknown.".parse().unwrap(),work_id:Some(work_id),debate_id:None,case_id,contest_id})})?;
        Ok(g.accepted(id))
    }).unwrap();
    store.call(|db| {
        let (spent,reserved,unknown):(i64,i64,i64)=db.query_row("SELECT SUM(spent_micros),SUM(reserved_micros),SUM(unknown_calls) FROM spend_daily",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
        assert_eq!((spent,reserved,unknown),(151144,0,0));
        assert_eq!(db.query_row("SELECT COUNT(*) FROM attempts",[],|r|r.get::<_,i64>(0))?,12);
        println!("{}",serde_json::json!({"spent_micros":spent,"reserved_micros":reserved,"unknown_calls":unknown,"attempts":12,"basis":"source-proven pre-dispatch rejection; canonical Account transaction with audit notice"}));
        Ok(())
    }).unwrap();
}
