use rusqlite::{Connection, params};
use squad_protocol::*;
use squad_store::{Store, grill, schema};
fn fixture() -> (Store, GrillSetup) {
    let store = Store::open(":memory:").unwrap();
    let setup = GrillSetup {
        contest_id: ContestId::new(),
        leader: "alpha".parse().unwrap(),
        grillers: vec!["beta".parse().unwrap()],
        source_message_id: MessageId::new(),
        initial_plan: "INITIAL_PLAN".into(),
        focus: "Focus".into(),
        total_rounds: 2,
    };
    let s = setup.clone();
    store.call(move|db|{
        let state=GrillState{contest_id:s.contest_id,state:GrillStatus::Questioning,state_version:1,round:1,total_rounds:2,leader:s.leader.clone(),griller_order:s.grillers.clone(),turn:0,plan_version:1,questions_asked:0,passed:0,terminal_reason:None};
        db.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active) VALUES (?,'grill',1,1,?,1)",params![s.contest_id.to_string(),serde_json::to_string(&state)?])?;
        db.execute("INSERT INTO contest_private_maps VALUES (?,'grill.settings',?)",params![s.contest_id.to_string(),serde_json::to_string(&GrillPreflight{setup:s,answer_words:400,plan_words:1200})?])?;
        Ok(())
    }).unwrap();
    (store, setup)
}
fn commit(store: &Store, payload: EnvelopePayload) -> CommittedEnvelope {
    let id = EventId::new();
    let sender = match &payload {
        EnvelopePayload::GrillQuestion(v) => Actor::Member(v.griller.clone()),
        EnvelopePayload::GrillAnswer(v) => Actor::Member(v.leader.clone()),
        EnvelopePayload::GrillPlan(v) if v.version > 1 => Actor::Member(v.leader.clone()),
        _ => Actor::System,
    };
    store
        .commit(id, "fixture".into(), id.to_string(), move |tx, group| {
            let seq = group.push(
                tx,
                ProducerEnvelope {
                    base: EnvelopeBase {
                        version: 1,
                        id,
                        room_id: MainRoom::Main,
                        correlation_id: id,
                        causation_id: None,
                        sender,
                        recipients: vec![
                            Recipient::Me,
                            Recipient::Member("alpha".parse().unwrap()),
                            Recipient::Member("beta".parse().unwrap()),
                        ],
                        created_at: group.accepted_at,
                    },
                    content: payload,
                },
            )?;
            grill::retain(tx, &squad_store::events::at(tx, seq.get())?)?;
            Ok(group.accepted(id))
        })
        .unwrap()
        .events
        .remove(0)
}
#[test]
fn retained_floor_is_exact_and_next_round_excludes_earlier_answers() {
    let (store, s) = fixture();
    let id = s.contest_id;
    commit(&store, EnvelopePayload::GrillSetup(s.clone()));
    commit(
        &store,
        EnvelopePayload::GrillPlan(GrillPlan {
            contest_id: id,
            round: 0,
            version: 1,
            leader: s.leader.clone(),
            text: s.initial_plan.clone(),
        }),
    );
    let question = commit(
        &store,
        EnvelopePayload::GrillQuestion(GrillQuestion {
            contest_id: id,
            round: 1,
            turn: 0,
            griller: s.grillers[0].clone(),
            leader: s.leader.clone(),
            plan_version: 1,
            angle: "cost".into(),
            questions: vec!["EARLIER_QUESTION?".into()],
        }),
    );
    store.call(move|db|{db.execute("UPDATE contests SET state_json=json_set(state_json,'$.state','ANSWERING') WHERE contest_id=?",[id.to_string()])?;Ok(())}).unwrap();
    commit(
        &store,
        EnvelopePayload::GrillAnswer(GrillAnswer {
            contest_id: id,
            round: 1,
            turn: 0,
            leader: s.leader.clone(),
            question_event_id: question.base.id,
            answers: vec![GrillAnswerItem {
                question_index: 0,
                answer: "EARLIER_ANSWER_CANARY".into(),
                plan_changes: "Change budget".into(),
            }],
        }),
    );
    store.call(move|db|{db.execute("UPDATE contests SET state_json=json_set(state_json,'$.state','REBUILDING') WHERE contest_id=?",[id.to_string()])?;Ok(())}).unwrap();
    commit(
        &store,
        EnvelopePayload::GrillPlan(GrillPlan {
            contest_id: id,
            round: 1,
            version: 2,
            leader: s.leader,
            text: "LATEST_PLAN".into(),
        }),
    );
    store.call(move|db|{
        assert_eq!(db.query_row("SELECT COUNT(DISTINCT context_id) FROM history",[],|r|r.get::<_,i64>(0))?,2);
        assert_eq!(db.query_row("SELECT COUNT(*) FROM history WHERE context_id='outsider'",[],|r|r.get::<_,i64>(0))?,0);
        for n in ["alpha","beta"]{assert_eq!(db.query_row("SELECT COUNT(*) FROM history WHERE context_id=?",[n],|r|r.get::<_,i64>(0))?,5);}
        db.execute("UPDATE contests SET state_json=json_set(state_json,'$.state','QUESTIONING','$.round',2,'$.planVersion',2) WHERE contest_id=?",[id.to_string()])?;
        let projection=grill::projection(db,id,ContextId("fresh".into()))?;
        let text=projection.iter().map(|p|p.content.as_str()).collect::<Vec<_>>().join("\n");
        assert!(text.contains("LATEST_PLAN"));assert!(text.contains("EARLIER_QUESTION"));
        assert!(!text.contains("EARLIER_ANSWER_CANARY"));assert!(!text.contains("INITIAL_PLAN"));
        assert_eq!(db.query_row("SELECT COUNT(*) FROM grill_plans",[],|r|r.get::<_,i64>(0))?,2);
        let tx=db.transaction()?;squad_store::clear::content(&tx)?;tx.commit()?;
        assert_eq!(db.query_row("SELECT COUNT(*) FROM grill_plans",[],|r|r.get::<_,i64>(0))?,0);
        assert_eq!(db.query_row("SELECT COUNT(*) FROM grill_turns",[],|r|r.get::<_,i64>(0))?,0);
        Ok(())
    }).unwrap();
}
#[test]
fn migration_preserves_old_events_and_references() {
    let mut db = Connection::open_in_memory().unwrap();
    db.execute_batch(schema::INITIAL).unwrap();
    db.pragma_update(None, "user_version", 1).unwrap();
    db.execute("INSERT INTO events VALUES (1,'old','main',1,1,'system.notice','old',NULL,'system','[\"me\"]','{}','2026-01-01','2026-01-01',1,0,1)",[]).unwrap();
    db.execute(
        "INSERT INTO history VALUES (1,'alpha',1,'old','test','user',NULL,'CANARY','{}',0)",
        [],
    )
    .unwrap();
    schema::migrate(&mut db).unwrap();
    schema::migrate(&mut db).unwrap();
    assert_eq!(
        db.query_row("SELECT content FROM history WHERE event_seq=1", [], |r| {
            r.get::<_, String>(0)
        })
        .unwrap(),
        "CANARY"
    );
    assert_eq!(
        db.query_row("PRAGMA foreign_keys", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert!(
        !db.prepare("PRAGMA foreign_key_check")
            .unwrap()
            .exists([])
            .unwrap()
    );
    let sql: String = db
        .query_row("SELECT sql FROM sqlite_master WHERE name='work'", [], |r| {
            r.get(0)
        })
        .unwrap();
    for kind in ["grill.question", "grill.answer", "grill.rebuild"] {
        assert!(sql.contains(kind));
    }
}
