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
#[test]
fn every_mode_competes_for_the_same_active_claim() {
    let db = db();
    let kinds = [
        ContestKind::Debate,
        ContestKind::Tournament,
        ContestKind::Grill,
        ContestKind::Census,
        ContestKind::Crosscheck,
        ContestKind::Survey,
        ContestKind::Probe,
    ];
    for first in kinds {
        let id = ContestId::new().to_string();
        db.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active) VALUES (?,?,1,1,'{}',1)",params![id,first.as_str()]).unwrap();
        for second in kinds {
            assert!(db.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active) VALUES (?,?,1,1,'{}',1)",params![ContestId::new().to_string(),second.as_str()]).is_err());
        }
        db.execute("DELETE FROM contests WHERE contest_id=?", [id])
            .unwrap();
    }
}
#[test]
fn crashes_at_assignment_artifact_barrier_and_next_work_leave_no_partial_commit() {
    use squad_store::{StoreError, commit, contest_steps, history, work};
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("recovery.db");
    let initial = db();
    let mut s = state();
    s.status = CrosscheckStatus::BuildingGrid;
    s.grid_version = 0;
    squad_store::crosscheck::save_state(&initial, &s, None, Cursor::new(1).unwrap()).unwrap();
    let seed = CellId::new();
    contest_steps::private_map(
        &initial,
        s.contest_id,
        "seed",
        &serde_json::json!({"seed":seed}),
    )
    .unwrap();
    initial
        .execute("VACUUM INTO ?", [path.to_str().unwrap()])
        .unwrap();
    drop(initial);
    for boundary in ["assignment", "artifact", "barrier", "next_work"] {
        let mut db = Connection::open(&path).unwrap();
        squad_store::schema::migrate(&mut db).unwrap();
        let operation = EventId::new();
        let step_id = StepId::new();
        let step = ContestStep {
            contest_id: s.contest_id,
            step_id,
            ordinal: 3,
            script_version: Text::new("crosscheck-v1").unwrap(),
            move_name: ContestMove::Judge,
            inputs: vec![],
            assignments: vec![],
        };
        let carrier = |content| ProducerEnvelope {
            base: EnvelopeBase {
                version: 1,
                id: EventId::new(),
                room_id: MainRoom::Main,
                correlation_id: operation,
                causation_id: Some(operation),
                sender: Actor::System,
                recipients: vec![Recipient::Me],
                created_at: chrono::Utc::now(),
            },
            content,
        };
        let result = commit::commit(
            &mut db,
            operation,
            "contest-engine",
            "boundary-fault",
            |tx, group| {
                let seq = group.push(tx, carrier(EnvelopePayload::ContestStep(step.clone())))?;
                contest_steps::insert(tx, &step, seq)?;
                contest_steps::private_map(
                    tx,
                    s.contest_id,
                    "assignment",
                    &serde_json::json!({"step":step_id}),
                )?;
                if boundary == "assignment" {
                    return Err(StoreError::Unavailable);
                }
                let grid = CrosscheckGrid {
                    contest_id: s.contest_id,
                    plan_id: s.plan_id,
                    plan_version: 1,
                    grid_version: 1,
                    items: vec![],
                    cells: vec![CrosscheckCell {
                        cell_id: CellId::new(),
                        key: "single:a".into(),
                        items: vec![Text::new("case").unwrap()],
                        originators: vec!["member0".parse().unwrap()],
                        reviewer: Some("member1".parse().unwrap()),
                    }],
                    omitted: vec![],
                };
                let seq = group.push(tx, carrier(EnvelopePayload::CrosscheckGrid(grid.clone())))?;
                squad_store::crosscheck::save_grid(tx, &grid, seq)?;
                if boundary == "artifact" {
                    return Err(StoreError::Unavailable);
                }
                let report = ContestReport {
                    contest_id: s.contest_id,
                    step_id,
                    artifact_event_ids: vec![],
                    skipped_work_ids: vec![],
                    data: serde_json::json!({}),
                };
                let seq = group.push(tx, carrier(EnvelopePayload::ContestReport(report)))?;
                contest_steps::barrier(tx, step_id, seq)?;
                if boundary == "barrier" {
                    return Err(StoreError::Unavailable);
                }
                let next_id = StepId::new();
                let nick: MemberNick = "member1".parse().unwrap();
                let target = Target::Member(nick.clone());
                let context = ContextId(format!("contest:{}:{}:{}", s.contest_id, next_id, nick));
                let work_id = WorkId::new();
                let next = ContestStep {
                    contest_id: s.contest_id,
                    step_id: next_id,
                    ordinal: 4,
                    script_version: Text::new("crosscheck-v1").unwrap(),
                    move_name: ContestMove::Fanout,
                    inputs: vec![],
                    assignments: vec![ContestAssignment {
                        target: target.clone(),
                        context_id: context.clone(),
                        payload_refs: vec![],
                        work_id: Some(work_id),
                    }],
                };
                let seq = group.push(tx, carrier(EnvelopePayload::ContestStep(next.clone())))?;
                contest_steps::insert(tx, &next, seq)?;
                let raw: String =
                    tx.query_row("SELECT id FROM events WHERE seq=1", [], |r| r.get(0))?;
                let origin: EventId = raw.parse().unwrap();
                let through = history::project(
                    tx,
                    &HistoryProjection {
                        context_id: context.clone(),
                        origin_event_id: origin,
                        projection_key: Text::new("crosscheck.input").unwrap(),
                        role: HistoryRole::User,
                        name: Some("squad".into()),
                        content: "authorized cells".into(),
                        meta: HistoryMetadata::Anonymous { anonymous: True },
                    },
                )?
                .unwrap();
                let item = Work {
                    work_id,
                    message_id: MessageId::new(),
                    in_reply_to_message_id: None,
                    kind: WorkKind::CrosscheckCell,
                    target,
                    context_id: context,
                    trigger_event_id: origin,
                    history_through: through,
                    attempt: 1,
                    debate_id: None,
                    round: None,
                    replay: None,
                    case_id: None,
                    prompt_ref: None,
                    max_output_tokens: Some(2048),
                    contest_id: Some(s.contest_id),
                    step_id: Some(next_id),
                };
                work::insert(
                    tx,
                    &item,
                    &ExecutorBinding {
                        model: Text::new("fixture/member1").unwrap(),
                        effort: ReasoningEffort::Medium,
                    },
                    None,
                )?;
                contest_steps::attach_work(tx, next_id, &item)?;
                Err(StoreError::Unavailable)
            },
        );
        assert!(
            matches!(result, Err(StoreError::Unavailable)),
            "{boundary}: {result:?}"
        );
        drop(db);
        let mut recovered = Connection::open(&path).unwrap();
        squad_store::schema::migrate(&mut recovered).unwrap();
        assert_eq!(
            squad_store::crosscheck::state(&recovered, s.contest_id).unwrap(),
            s
        );
        for table in [
            "contest_steps",
            "contest_assignments",
            "crosscheck_cells",
            "work",
            "receipts",
        ] {
            assert_eq!(
                recovered
                    .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r
                        .get::<_, i64>(0))
                    .unwrap(),
                0,
                "{boundary}: {table}"
            );
        }
        assert_eq!(
            recovered
                .query_row("SELECT COUNT(*) FROM events", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            3
        );
        let retained: String = recovered
            .query_row(
                "SELECT map_json FROM contest_private_maps WHERE scope_key='seed'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&retained).unwrap()["seed"],
            serde_json::json!(seed)
        );
    }
}
