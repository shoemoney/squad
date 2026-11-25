use rusqlite::Connection;
use squad_store::schema::migrate;
#[test]
fn initial_schema_is_atomic_complete_and_reopenable() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("squad.db");
    let mut db = Connection::open(&path).unwrap();
    migrate(&mut db).unwrap();
    migrate(&mut db).unwrap();
    drop(db);
    let mut db = Connection::open(&path).unwrap();
    migrate(&mut db).unwrap();
    let names = db
        .prepare("SELECT name FROM sqlite_master WHERE type='table'")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    for table in [
        "room_meta",
        "events",
        "history",
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
        "receipts",
        "vars",
        "spend_daily",
        "attempts",
        "reservations",
        "scores",
        "catalog",
        "models",
        "superseded",
    ] {
        assert!(names.iter().any(|n| n == table), "{table}");
    }
    assert!(
        !names
            .iter()
            .any(|n| n.starts_with("tournament_") || n.starts_with("grill_"))
    );
    assert_eq!(
        db.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    assert_eq!(
        db.query_row("PRAGMA foreign_keys", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.query_row("PRAGMA journal_mode", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "wal"
    );
    let violations: i64 = db
        .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(violations, 0);
}
#[test]
fn active_claims_json_shapes_and_future_work_are_constrained() {
    let mut db = Connection::open_in_memory().unwrap();
    migrate(&mut db).unwrap();
    db.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active) VALUES('one','debate',1,0,'{}',1)",[]).unwrap();
    assert!(db.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active) VALUES('two','grill',1,0,'{}',1)",[]).is_err());
    assert!(db.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active) VALUES('two','unknown',1,0,'{}',0)",[]).is_err());
    assert!(db.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active) VALUES('two','debate',1,0,'[]',0)",[]).is_err());
    let work_sql: String = db
        .query_row("SELECT sql FROM sqlite_master WHERE name='work'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert!(work_sql.contains("judge.round"));
    assert!(work_sql.contains("census.hunt"));
    assert!(
        db.query_row(
            "SELECT sql FROM sqlite_master WHERE name='work_round_judge_idx'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap()
        .contains("UNIQUE")
    );
}
#[test]
fn money_conversion_is_once_per_call_and_reservations_round_up() {
    use squad_store::accounting::*;
    assert_eq!(reported_micros(0.0000005), Ok(1));
    assert_eq!(reported_micros(0.0000004), Ok(0));
    assert_eq!(reservation_micros(0.0000004), Ok(1));
    assert_eq!(reported_micros(20.0), Ok(20_000_000));
    for n in [-1.0, f64::NAN, f64::INFINITY, 1e30] {
        assert!(reported_micros(n).is_err());
        assert!(reservation_micros(n).is_err());
    }
}
#[test]
fn cached_model_roles_and_provenance_are_validated() {
    use squad_protocol::*;
    let mut db = rusqlite::Connection::open_in_memory().unwrap();
    squad_store::schema::migrate(&mut db).unwrap();
    let mut model = Model {
        nick: Actor::Judge,
        requested_slug: Text::new("vendor/alias").unwrap(),
        resolved_slug: Text::new("vendor/concrete").unwrap(),
        roster_order: None,
        context_length: 32000,
        pricing: serde_json::json!({"prompt":"0.000001"}),
        resolved_at: "2026-11-24T12:00:00Z".parse().unwrap(),
    };
    squad_store::models::save_model(&db, &model).unwrap();
    let role: (String, Option<u32>, String) = db
        .query_row(
            "SELECT role,roster_order,resolved_slug FROM models WHERE nick='judge'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(role, ("judge".into(), None, "vendor/concrete".into()));
    model.nick = Actor::Member(Nick::new("a").unwrap());
    assert!(squad_store::models::save_model(&db, &model).is_err());
    model.roster_order = Some(0);
    squad_store::models::save_model(&db, &model).unwrap();
    model.nick = Actor::Me;
    assert!(squad_store::models::save_model(&db, &model).is_err());
}
#[test]
fn judge_work_context_visible_ceiling_and_unique_round_are_enforced() {
    let mut db = Connection::open_in_memory().unwrap();
    migrate(&mut db).unwrap();
    db.execute_batch("INSERT INTO events(seq,id,room_id,epoch,version,kind,correlation_id,sender,recipients,payload,created_at,accepted_at,commit_start_seq,commit_index,commit_size) VALUES(1,'event','main',1,1,'case.opened','event','system','[\"me\"]','{}','time','time',1,0,1);
 INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active) VALUES('debate','debate',1,0,'{}',1);
 INSERT INTO cases(case_id,context_id,epoch,contest_id,judge_model,material_json,opened_event_seq,state_json) VALUES('case','judge:case',1,'debate','model','{}',1,'{}');").unwrap();
    let insert = |id: &str, context: &str, ceiling: u32| {
        db.execute("INSERT INTO work(work_id,kind,target,context_id,trigger_event_seq,trigger_event_id,message_id,status,epoch,contest_id,debate_id,round,case_id,prompt_ref,max_output_tokens) VALUES(?1,'judge.round','judge',?2,1,'event',?1,'queued',1,'debate','debate',1,'case','{}',?3)",rusqlite::params![id,context,ceiling])
    };
    assert!(insert("bad-context", "judge:foreign", 512).is_err());
    assert!(insert("bad-ceiling", "judge:case", 1536).is_err());
    assert!(insert("good", "judge:case", 512).is_ok());
    assert!(insert("duplicate-round", "judge:case", 512).is_err());
    assert!(db.execute("INSERT INTO round_scores(debate_id,case_id,round,work_id,event_seq,scores_json,tally_json,reason) VALUES('debate','case',1,'good',1,NULL,'{}',NULL)",[]).is_err());
    db.execute("INSERT INTO round_scores(debate_id,case_id,round,work_id,event_seq,scores_json,tally_json,reason) VALUES('debate','case',1,'good',1,NULL,'{}','unscored')",[]).unwrap();
}
#[test]
fn contest_references_are_kind_checked() {
    let mut db = Connection::open_in_memory().unwrap();
    migrate(&mut db).unwrap();
    let id = squad_protocol::ContestId::new();
    db.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active) VALUES(?1,'debate',1,0,'{}',0)",[id.to_string()]).unwrap();
    assert!(
        squad_store::schema::validate_contest_kind(&db, id, squad_protocol::ContestKind::Debate)
            .unwrap()
    );
    assert!(
        !squad_store::schema::validate_contest_kind(&db, id, squad_protocol::ContestKind::Grill)
            .unwrap()
    );
}
#[test]
fn local_day_keys_are_canonical() {
    for day in ["2026-02-30", "2026-2-01", "2026-11-24T00:00:00Z", ""] {
        assert!(!squad_protocol::valid_day(day));
    }
    assert!(squad_protocol::valid_day("2026-11-24"));
}

#[test]
fn roster_reorder_and_removal_are_atomic() {
    use squad_protocol::*;
    let mut db = Connection::open_in_memory().unwrap();
    migrate(&mut db).unwrap();
    let model = |nick: &str, order| Model {
        nick: Actor::Member(Nick::new(nick).unwrap()),
        requested_slug: Text::new("v/m").unwrap(),
        resolved_slug: Text::new("v/m").unwrap(),
        roster_order: Some(order),
        context_length: 100,
        pricing: serde_json::json!({}),
        resolved_at: "2026-11-24T12:00:00Z".parse().unwrap(),
    };
    let tx = db.transaction().unwrap();
    squad_store::models::save_roster(&tx, &[model("a", 0), model("b", 1)]).unwrap();
    tx.commit().unwrap();
    assert!(squad_store::models::save_model(&db, &model("b", 0)).is_err());
    let tx = db.transaction().unwrap();
    squad_store::models::save_roster(&tx, &[model("b", 0), model("a", 1)]).unwrap();
    tx.commit().unwrap();
    assert_eq!(
        db.query_row("SELECT roster_order FROM models WHERE nick='b'", [], |r| {
            r.get::<_, u32>(0)
        })
        .unwrap(),
        0
    );
    let tx = db.transaction().unwrap();
    squad_store::models::save_roster(&tx, &[model("b", 0)]).unwrap();
    tx.commit().unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM models", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        1
    );
}

#[test]
fn every_opened_connection_enforces_durability_and_foreign_keys() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("squad.db");
    let mut writer = squad_store::open(&path).unwrap();
    migrate(&mut writer).unwrap();
    let reader = squad_store::open(&path).unwrap();
    for connection in [&writer, &reader] {
        assert_eq!(
            connection
                .query_row("PRAGMA foreign_keys", [], |r| r.get::<_, u32>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            connection
                .query_row("PRAGMA synchronous", [], |r| r.get::<_, u32>(0))
                .unwrap(),
            2
        );
        assert_eq!(
            connection
                .query_row("PRAGMA journal_mode", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "wal"
        );
    }
}

#[test]
fn failed_roster_write_restores_previous_members_and_judge() {
    use squad_protocol::*;
    let mut db = Connection::open_in_memory().unwrap();
    migrate(&mut db).unwrap();
    let mut model = Model {
        nick: Actor::Judge,
        roster_order: None,
        requested_slug: Text::new("v/m").unwrap(),
        resolved_slug: Text::new("v/m").unwrap(),
        context_length: 100,
        pricing: serde_json::json!({}),
        resolved_at: "2026-11-24T12:00:00Z".parse().unwrap(),
    };
    squad_store::models::save_model(&db, &model).unwrap();
    model.nick = Actor::Member(Nick::new("a").unwrap());
    model.roster_order = Some(0);
    squad_store::models::save_model(&db, &model).unwrap();
    db.execute_batch("CREATE TRIGGER fail_roster BEFORE INSERT ON models WHEN NEW.nick='b' BEGIN SELECT RAISE(ABORT, 'fixture failure'); END;").unwrap();
    model.nick = Actor::Member(Nick::new("b").unwrap());
    let tx = db.transaction().unwrap();
    assert!(squad_store::models::save_roster(&tx, &[model.clone()]).is_err());
    tx.commit().unwrap();
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM models WHERE nick IN ('a','judge')",
            [],
            |r| r.get::<_, u32>(0)
        )
        .unwrap(),
        2
    );
    model.context_length = 0;
    assert!(matches!(
        squad_store::models::save_model(&db, &model),
        Err(squad_store::models::ModelError::InvalidModel)
    ));
}
