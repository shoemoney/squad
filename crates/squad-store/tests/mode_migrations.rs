use rusqlite::{Connection, params};
use squad_store::schema;

const MIGRATIONS: [&str; 7] = [
    schema::INITIAL,
    include_str!("../migrations/0002_tournament.sql"),
    include_str!("../migrations/0003_census.sql"),
    include_str!("../migrations/0004_crosscheck.sql"),
    include_str!("../migrations/0005_survey.sql"),
    include_str!("../migrations/0006_grill.sql"),
    include_str!("../migrations/0007_probe.sql"),
];

#[test]
fn every_populated_mode_schema_upgrades_without_losing_events_work_or_indexes() {
    let kinds = [
        "user.message",
        "tournament.entry",
        "census.hunt",
        "crosscheck.review",
        "survey.vote",
        "grill.plan",
        "probe.prediction",
    ];
    for version in 1..=MIGRATIONS.len() {
        let mut db = Connection::open_in_memory().unwrap();
        // Construct an older database using the same table-rebuild procedure as migrate.
        db.pragma_update(None, "foreign_keys", false).unwrap();
        for (i, migration) in MIGRATIONS[..version].iter().enumerate() {
            db.execute_batch(migration).unwrap();
            let seq = i as i64 + 1;
            db.execute("INSERT INTO events(seq,id,room_id,epoch,version,kind,correlation_id,sender,recipients,payload,created_at,accepted_at,commit_start_seq,commit_index,commit_size) VALUES(?1,?2,'main',1,1,?3,'request','me','[\"me\"]','{}','2026-11-25T00:00:00Z','2026-11-25T00:00:00Z',?1,0,1)", params![seq,format!("event-{seq}"),kinds[i]]).unwrap();
            if i == 0 {
                db.execute("INSERT INTO work(work_id,kind,target,context_id,trigger_event_seq,trigger_event_id,message_id,in_reply_to_message_id,status,epoch,runnable) VALUES('work','reply','alpha','alpha',1,'event-1','message','source','queued',1,1)", []).unwrap();
            }
        }
        db.pragma_update(None, "user_version", version as i64)
            .unwrap();
        schema::migrate(&mut db).unwrap();
        schema::migrate(&mut db).unwrap();
        let retained: Vec<String> = db
            .prepare("SELECT kind FROM events ORDER BY seq")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(retained, kinds[..version], "upgrading version {version}");
        let work: (String, String, String) = db
            .query_row(
                "SELECT kind,trigger_event_id,status FROM work WHERE work_id='work'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(work, ("reply".into(), "event-1".into(), "queued".into()));
        for index in [
            "tournament_entry_slot",
            "tournament_work_slot",
            "work_round_judge_idx",
        ] {
            assert_eq!(
                db.query_row(
                    "SELECT COUNT(*) FROM sqlite_schema WHERE type='index' AND name=?",
                    [index],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
                1
            );
        }
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| r
                .get::<_, i64>(
                0
            ))
            .unwrap(),
            0
        );
        assert_eq!(
            db.query_row("PRAGMA foreign_keys", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
}

#[test]
fn scaffold_registers_twenty_migrations_and_all_eighteen_kinds() {
    let mut db = Connection::open_in_memory().unwrap();
    schema::migrate(&mut db).unwrap();
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        20
    );
    for kind in [
        "debate",
        "tournament",
        "grill",
        "census",
        "crosscheck",
        "survey",
        "probe",
        "forge",
        "forecast",
        "decide",
        "negotiate",
        "blow_it_up",
        "readback",
        "debug",
        "trace",
        "rank",
        "tune",
        "bass_ackwards_prd",
    ] {
        db.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active) VALUES(?,?,1,1,'{}',0)",params![kind,kind]).unwrap();
    }
    for (name, default, max) in [
        ("NEGOTIATE_ROUNDS", 3, 6),
        ("READBACK_ROUNDS", 2, 4),
        ("DEBUG_ROUNDS", 3, 6),
        ("TUNE_GENERATIONS", 3, 6),
    ] {
        assert_eq!(
            db.query_row("SELECT value FROM vars WHERE name=?", [name], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            default
        );
        assert!(
            db.execute(
                "UPDATE vars SET value=? WHERE name=?",
                params![max + 1, name]
            )
            .is_err()
        );
    }
}
