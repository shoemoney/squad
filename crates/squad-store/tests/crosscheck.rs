use rusqlite::{Connection, params};
use squad_protocol::*;
fn db() -> Connection {
    let mut db = Connection::open_in_memory().unwrap();
    squad_store::schema::migrate(&mut db).unwrap();
    db.execute("INSERT INTO room_meta VALUES ('main',2,1,4,1,NULL)", [])
        .unwrap();
    for seq in 1..=3 {
        db.execute("INSERT INTO events(seq,id,room_id,epoch,version,kind,correlation_id,sender,recipients,payload,created_at,accepted_at,commit_start_seq,commit_index,commit_size) VALUES (?,?,'main',1,1,'system.notice',?,'system','[\"me\"]','{}','2026-11-25','2026-11-25',?,0,1)",params![seq,EventId::new().to_string(),EventId::new().to_string(),seq]).unwrap();
    }
    db
}
fn state() -> CrosscheckState {
    CrosscheckState {
        contest_id: ContestId::new(),
        plan_id: PlanId::new(),
        plan_version: 1,
        grid_version: 1,
        state_version: 1,
        status: CrosscheckStatus::Complete,
        step_id: None,
        terminal_reason: None,
    }
}
#[test]
fn acceptance_is_human_only_version_bound_and_never_creates_unknown_cells() {
    let mut db = db();
    let s = state();
    squad_store::crosscheck::save_state(&db, &s, None, Cursor::new(1).unwrap()).unwrap();
    let cell = CellId::new();
    db.execute("INSERT INTO crosscheck_cells(contest_id,cell_id,grid_event_seq,items_json,originators_json,status,evidence_json) VALUES (?,?,1,'[]','[]','gap','{}')",params![s.contest_id.to_string(),cell.to_string()]).unwrap();
    let a = CrosscheckAcceptance {
        contest_id: s.contest_id,
        plan_id: s.plan_id,
        plan_version: 1,
        grid_version: 1,
        expected_state_version: 1,
        cell_ids: vec![cell],
        residual_risk_ids: vec![],
        reason: Text::new("Explicitly accept this risk").unwrap(),
    };
    for actor in [
        Actor::Judge,
        Actor::System,
        Actor::Member("member0".parse().unwrap()),
    ] {
        assert!(squad_store::crosscheck::accept(&db, &actor, &a, Cursor::new(2).unwrap()).is_err());
    }
    for stale in 0..4 {
        let mut a = a.clone();
        match stale {
            0 => a.plan_version += 1,
            1 => a.grid_version += 1,
            2 => a.expected_state_version += 1,
            _ => a.plan_id = PlanId::new(),
        };
        assert!(
            squad_store::crosscheck::accept(&db, &Actor::Me, &a, Cursor::new(2).unwrap()).is_err()
        );
    }
    let tx = db.transaction().unwrap();
    squad_store::crosscheck::accept(&tx, &Actor::Me, &a, Cursor::new(2).unwrap()).unwrap();
    tx.commit().unwrap();
    assert_eq!(
        db.query_row("SELECT status FROM crosscheck_cells", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "accepted"
    );
    assert!(
        db.execute("UPDATE crosscheck_cells SET acceptance_event_seq=NULL", [])
            .is_err()
    );
    assert!(
        db.execute("UPDATE crosscheck_cells SET origin_event_seq=3", [])
            .is_err()
    );
}
#[test]
fn grid_rejects_self_review_and_keeps_no_eligible_reviewer_open() {
    let db = db();
    let mut s = state();
    s.status = CrosscheckStatus::BuildingGrid;
    s.grid_version = 0;
    squad_store::crosscheck::save_state(&db, &s, None, Cursor::new(1).unwrap()).unwrap();
    let nick: MemberNick = "member0".parse().unwrap();
    let mut g = CrosscheckGrid {
        contest_id: s.contest_id,
        plan_id: s.plan_id,
        plan_version: 1,
        grid_version: 1,
        items: vec![],
        cells: vec![CrosscheckCell {
            cell_id: CellId::new(),
            key: "single:a".into(),
            items: vec![Text::new("scenario").unwrap()],
            originators: vec![nick.clone()],
            reviewer: Some(nick),
        }],
        omitted: vec![],
    };
    assert!(squad_store::crosscheck::save_grid(&db, &g, Cursor::new(2).unwrap()).is_err());
    g.cells[0].reviewer = None;
    squad_store::crosscheck::save_grid(&db, &g, Cursor::new(2).unwrap()).unwrap();
    assert_eq!(
        db.query_row("SELECT status FROM crosscheck_cells", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "pending"
    );
}
#[test]
fn upgrade_preserves_released_events_and_foreign_keys() {
    let mut db = Connection::open_in_memory().unwrap();
    db.execute_batch(squad_store::schema::INITIAL).unwrap();
    db.pragma_update(None, "user_version", 1).unwrap();
    let id = EventId::new().to_string();
    db.execute("INSERT INTO events(seq,id,room_id,epoch,version,kind,correlation_id,sender,recipients,payload,created_at,accepted_at,commit_start_seq,commit_index,commit_size) VALUES (1,?,'main',1,1,'system.notice',?,'system','[\"me\"]','{}','2026-11-25','2026-11-25',1,0,1)",params![id,id]).unwrap();
    db.execute("INSERT INTO history(pos,context_id,event_seq,origin_event_id,projection_key,role,name,content,meta) VALUES (1,'member0',1,?,'ordinary.input','user',NULL,'Preserve this source','{\"anonymous\":true}')",[&id]).unwrap();
    squad_store::schema::migrate(&mut db).unwrap();
    squad_store::schema::migrate(&mut db).unwrap();
    assert_eq!(
        db.query_row("SELECT id FROM events", [], |r| r.get::<_, String>(0))
            .unwrap(),
        id
    );
    assert_eq!(
        db.query_row("SELECT content FROM history", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "Preserve this source"
    );
    assert!(
        !db.prepare("PRAGMA foreign_key_check")
            .unwrap()
            .exists([])
            .unwrap()
    );
    assert_eq!(
        db.query_row("PRAGMA foreign_keys", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
}
