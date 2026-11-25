mod support;
use squad_protocol::*;
use squad_store::Store;

#[test]
fn accepted_content_receipts_terminal_work_and_active_case_anchors_survive_reopen_until_clear() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("room.db");
    let store = Store::open(&path).unwrap();
    let source = support::event(&store);
    let contest = ContestId::new();
    let case = CaseId::new();
    let work = WorkId::new();
    let id = EventId::new();
    store.commit(id,"fixture".into(),"retained round".into(),move |tx,group| {
        let state=DebateState{debate_id:contest,state:DebateStatus::RoundRunning,debaters:[Nick::new("alice").unwrap(),Nick::new("bob").unwrap()],round:1,total_rounds:2,judge_model:"test/judge".into(),judge_requested:true,state_version:1,terminal_reason:None,case_id:Some(case),case_ids:vec![case]};
        tx.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active,updated_seq) VALUES (?,'debate',1,1,?,1,1)",rusqlite::params![contest.to_string(),serde_json::to_string(&state).unwrap()])?;
        tx.execute("INSERT INTO cases VALUES (?1,?2,1,?3,'test/judge','{\"material\":\"retained\"}',1,NULL,'{}')",rusqlite::params![case.to_string(),format!("judge:{case}"),contest.to_string()])?;
        let context=ContextId(format!("judge:{case}"));
        let boundary=squad_store::history::project(tx,&squad_store::history::attributed(&source,context.clone(),"case.material",HistoryRole::User,None,"retained case material".into()))?.unwrap();
        let executable=Work {work_id:work,message_id:MessageId::new(),in_reply_to_message_id:None,kind:WorkKind::JudgeRound,target:Target::Judge,context_id:context,trigger_event_id:source.base.id,history_through:boundary,attempt:1,debate_id:Some(contest),round:Some(1),replay:None,case_id:Some(case),prompt_ref:Some(PromptRef {template_id:Text::new("judge.round.md").unwrap(),pack_version:Text::new("fixture").unwrap(),template_hash:Text::new("hash").unwrap()}),max_output_tokens:Some(512),contest_id:Some(contest),step_id:None};
        squad_store::work::insert(tx,&executable,&ExecutorBinding {model:Text::new("test/judge").unwrap(),effort:ReasoningEffort::Low},None)?;
        let tally=std::collections::BTreeMap::from([("alice".parse().unwrap(),0),("bob".parse().unwrap(),0)]);
        let seq=group.push(tx,ProducerEnvelope {base:EnvelopeBase {version:1,id:EventId::new(),room_id:MainRoom::Main,correlation_id:id,causation_id:Some(source.base.id),sender:Actor::System,recipients:vec![Recipient::Me],created_at:group.accepted_at},content:EnvelopePayload::DebateRoundScore(DebateRoundScore {debate_id:contest,case_id:case,round:1,scores:None,tally,reason:Some(Text::new("unscored").unwrap())})})?;
        tx.execute("UPDATE work SET status='failed',attempt=2,runnable=0,completed_event_seq=? WHERE work_id=?",rusqlite::params![seq.get(),work.to_string()])?;
        tx.execute("INSERT INTO round_scores VALUES (?1,?2,1,?3,?4,NULL,'{\"alice\":0,\"bob\":0}','unscored')",rusqlite::params![contest.to_string(),case.to_string(),work.to_string(),seq.get()])?;
        squad_store::cases::associate(tx,case,seq)?;
        squad_store::contests::associate(tx,contest,ContestKind::Debate,seq)?;
        Ok(group.accepted(id))
    }).unwrap();
    drop(store);
    let reopened = Store::open(&path).unwrap();
    reopened.call(move |db| {
        assert_eq!(squad_store::work::record(db,work)?.unwrap().status,WorkStatus::Failed);
        assert_eq!(db.query_row("SELECT COUNT(*) FROM receipts",[],|r|r.get::<_,i64>(0))?,2);
        assert_eq!(db.query_row("SELECT content FROM history",[],|r|r.get::<_,String>(0))?,"retained case material");
        assert_eq!(db.query_row("SELECT active FROM contests",[],|r|r.get::<_,i64>(0))?,1);
        assert_eq!(db.query_row("SELECT COUNT(*) FROM round_scores JOIN events ON events.seq=round_scores.event_seq JOIN case_events ON case_events.event_seq=events.seq",[],|r|r.get::<_,i64>(0))?,1);
        assert!(matches!(squad_store::events::at(db,2)?.content,EnvelopePayload::DebateRoundScore(_)));
        let tx=db.transaction()?;squad_store::clear::content(&tx)?;tx.commit()?;
        assert_eq!(db.query_row("SELECT COUNT(*) FROM round_scores",[],|r|r.get::<_,i64>(0))?,0);
        assert!(db.prepare("PRAGMA foreign_key_check")?.query([])?.next()?.is_none());Ok(())
    }).unwrap();
}
