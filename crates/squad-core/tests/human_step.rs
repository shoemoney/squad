use rusqlite::params;
use squad_core::router::{Router, human};
use squad_protocol::*;
use squad_store::{Store, commit::CommitOutcome};
fn event(store: &Store) -> CommittedEnvelope {
    let id = EventId::new();
    store
        .commit(id, "fixture".into(), id.to_string(), move |tx, g| {
            g.push(
                tx,
                ProducerEnvelope {
                    base: EnvelopeBase {
                        version: 1,
                        id,
                        room_id: MainRoom::Main,
                        correlation_id: id,
                        causation_id: None,
                        sender: Actor::System,
                        recipients: vec![Recipient::Me],
                        created_at: g.accepted_at,
                    },
                    content: EnvelopePayload::SystemNotice(SystemNotice {
                        code: "FIXTURE".parse().unwrap(),
                        text: "fixture".parse().unwrap(),
                        work_id: None,
                        debate_id: None,
                        case_id: None,
                        contest_id: None,
                    }),
                },
            )?;
            Ok(g.accepted(id))
        })
        .unwrap()
        .events
        .remove(0)
}
fn fixture(path: &std::path::Path, kind: ContestKind) -> (Store, Router, parser::PendingHuman) {
    let store = Store::open(path).unwrap();
    let seq = event(&store).commit.seq;
    let pending = parser::PendingHuman {
        contest_id: ContestId::new(),
        kind,
        state_version: 2,
        step_id: StepId::new(),
        prompt: "Choose what you mean".into(),
        items: vec!["first".into(), "second".into()],
        choices: vec![vec!["a".into(), "b".into()], vec!["a".into()]],
        outcome_labels: vec!["O1".into(), "O2".into()],
        answers: Default::default(),
    };
    let p = pending.clone();
    store.call(move|db|{
 let state=serde_json::json!({"contestId":p.contest_id,"stateVersion":1,"state":if p.kind==ContestKind::Debugging {"MERGING"} else {"INTERPRETING"},"stepId":p.step_id,"round":if p.kind==ContestKind::BassAckwardsPrd {0}else{1},"artifactEventIds":[],"terminalReason":null});
 db.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active,updated_seq) VALUES(?,?,1,1,?,1,?)",params![p.contest_id.to_string(),p.kind.as_str(),state.to_string(),seq.get()])?;
 db.execute("INSERT INTO contest_steps(step_id,contest_id,ordinal,script_version,move,state_json,inputs_json,updated_seq) VALUES(?,?,1,'v1','judge','{}','[]',?)",params![p.step_id.to_string(),p.contest_id.to_string(),seq.get()])?;Ok(())}).unwrap();
    let held = pending.contest_id;
    store.call(move|db| {
        db.execute("INSERT INTO catalog VALUES ('test/model',131072,65536,'{\"prompt\":\"0\",\"completion\":\"0\"}','[]','2026-11-25T00:00:00Z')",[])?;
        let trigger:String=db.query_row("SELECT id FROM events WHERE seq=1",[],|r|r.get(0))?;
        let work=Work{work_id:WorkId::new(),message_id:MessageId::new(),in_reply_to_message_id:Some(MessageId::new()),kind:WorkKind::Reply,target:Target::Member("a".parse().unwrap()),context_id:ContextId("a".into()),trigger_event_id:trigger.parse().unwrap(),history_through:HistoryPosition::new(1).unwrap(),attempt:1,debate_id:None,round:None,replay:None,case_id:None,prompt_ref:None,max_output_tokens:None,contest_id:None,step_id:None};
        squad_store::work::insert(db,&work,&ExecutorBinding{model:"test/model".parse().unwrap(),effort:ReasoningEffort::Low},Some(held))?;
        Ok(())
    }).unwrap();
    let p = pending.clone();
    let id = EventId::new();
    store
        .commit(id, "engine".into(), id.to_string(), move |tx, g| {
            human::request(tx, g, id, &p)?;
            Ok(g.accepted(id))
        })
        .unwrap();
    let router = router(&store);
    (store, router, pending)
}
fn router(store: &Store) -> Router {
    let roster = ["a", "b", "c"].map(|n| Nick::new(n).unwrap()).to_vec();
    let bindings = ["a", "b", "c", "judge"]
        .map(|n| {
            (
                n.into(),
                ExecutorBinding {
                    model: "test/model".parse().unwrap(),
                    effort: ReasoningEffort::Low,
                },
            )
        })
        .into();
    Router::new(store.clone(), roster, bindings, 1000000).unwrap()
}
fn answer(router: &mut Router, raw: &str) -> CommitOutcome {
    let ctx = router.parse_context().unwrap();
    let meta = Default::default();
    let parsed = parse(raw, &meta, &ctx).unwrap();
    let request =
        Request::from_parsed_with_context(EventId::new(), parsed, &meta, &ctx, false).unwrap();
    router.process(request, None).unwrap()
}
#[test]
fn partial_answers_restart_continue_and_stale_versions() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("room.db");
    let (store, mut r, p) = fixture(&path, ContestKind::Readback);
    assert!(
        parse(
            "/answer 1z",
            &Default::default(),
            &r.parse_context().unwrap()
        )
        .is_err()
    );
    assert!(matches!(
        answer(&mut r, "1b").result,
        RequestResult::Accepted { .. }
    ));
    assert_eq!(
        r.parse_context().unwrap().pending_human.unwrap().answers[&1],
        "b"
    );
    let stale = Request::HumanAnswer {
        request_id: EventId::new(),
        contest_id: p.contest_id,
        expected_state_version: 2,
        text: Some("2a".into()),
    };
    assert!(matches!(
        r.process(stale, None).unwrap().result,
        RequestResult::Rejected { .. }
    ));
    assert!(matches!(
        answer(&mut r, "@a ordinary message").result,
        RequestResult::Accepted { .. }
    ));
    assert!(r.parse_context().unwrap().pending_human.is_some());
    let runnable=store.call(|db|Ok(db.query_row("SELECT COUNT(*) FROM work WHERE kind='reply' AND runnable=1 AND held_by_contest_id IS NULL",[],|r|r.get::<_,i64>(0))?)).unwrap();
    assert_eq!(runnable, 2);
    answer(&mut r, "/who");
    drop(r);
    drop(store);
    let store = Store::open(&path).unwrap();
    squad_core::router::recovery::recover_on_open_mode(
        &store,
        OpenMode::Interactive,
        chrono::Utc::now() + chrono::Duration::days(365),
    )
    .unwrap();
    let mut r = router(&store);
    let restored = r.parse_context().unwrap().pending_human.unwrap();
    assert_eq!(restored.answers[&1], "b");
    assert_eq!(restored.prompt, p.prompt);
    assert!(matches!(
        answer(&mut r, "/continue").result,
        RequestResult::Accepted { .. }
    ));
    assert!(r.parse_context().unwrap().pending_human.is_none());
    let status = store
        .call(|db| {
            Ok(db.query_row(
                "SELECT json_extract(state_json,'$.state') FROM contests WHERE active=1",
                [],
                |r| r.get::<_, String>(0),
            )?)
        })
        .unwrap();
    assert_eq!(status, "WAITING_FOR_PARTICIPANTS");
}
#[test]
fn debug_answer_is_literal_and_stop_closes_pending_step() {
    let dir = tempfile::tempdir().unwrap();
    let (_store, mut r, _) = fixture(&dir.path().join("room.db"), ContestKind::Debugging);
    assert!(matches!(
        answer(&mut r, "/answer O2 @all $ROUNDS=99 /stop").result,
        RequestResult::Accepted { .. }
    ));
    assert!(r.parse_context().unwrap().pending_human.is_none());
    let (_store, mut r, _) = fixture(&dir.path().join("other.db"), ContestKind::Readback);
    assert!(matches!(
        answer(&mut r, "/stop").result,
        RequestResult::Accepted { .. }
    ));
    assert!(r.parse_context().unwrap().pending_human.is_none());
}

#[test]
fn prd_notes_stay_pending_until_continue_and_survive_literal_commands() {
    let root = tempfile::tempdir().unwrap();
    let (_store, mut r, _) = fixture(&root.path().join("prd.db"), ContestKind::BassAckwardsPrd);
    assert!(matches!(
        answer(&mut r, "/answer keep @a and $ROUNDS=99 literal").result,
        RequestResult::Accepted { .. }
    ));
    let pending = r.parse_context().unwrap().pending_human.unwrap();
    assert_eq!(pending.answers[&1], "keep @a and $ROUNDS=99 literal");
    assert!(matches!(
        answer(&mut r, "/answer second note").result,
        RequestResult::Accepted { .. }
    ));
    assert_eq!(
        r.parse_context()
            .unwrap()
            .pending_human
            .unwrap()
            .answers
            .len(),
        2
    );
    assert!(parse("/answer", &Default::default(), &r.parse_context().unwrap()).is_err());
    assert!(matches!(
        answer(&mut r, "/continue").result,
        RequestResult::Accepted { .. }
    ));
    assert!(r.parse_context().unwrap().pending_human.is_none());
}
