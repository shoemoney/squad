//! M8.P11.L1.T1 / M9.P4.L1.T1: every work kind the eleven new modes add is fenced by
//! `/stop` (cancelled, not runnable, lease fence advanced, claim released) and removed
//! by `/clear`, one contest of each mode at a time, against the real store and router.
use rusqlite::params;
use squad_core::router::Router;
use squad_protocol::*;
use squad_store::{Store, commit::CommitOutcome};
#[path = "support/mode_starts.rs"]
mod mode_starts;

/// `(mode, [(wire kind, judge?)])`. Fifty-one kinds in all.
const MODES: &[(&str, &[(&str, bool)])] = &[
    (
        "forge",
        &[
            ("forge.draft", false),
            ("forge.rank", false),
            ("forge.fuse", false),
            ("forge.judge", true),
        ],
    ),
    (
        "forecast",
        &[
            ("forecast.frame", true),
            ("forecast.estimate", false),
            ("forecast.revise", false),
            ("forecast.cruxes", true),
        ],
    ),
    (
        "decide",
        &[
            ("decide.options", false),
            ("decide.shortlist", true),
            ("decide.criteria", false),
            ("decide.freeze", true),
            ("decide.score", false),
            ("decide.explain", true),
        ],
    ),
    (
        "negotiate",
        &[
            ("negotiate.map", false),
            ("negotiate.brief", true),
            ("negotiate.package", false),
            ("negotiate.move", false),
            ("negotiate.counter", false),
            ("negotiate.playbook", true),
        ],
    ),
    (
        "blow_it_up",
        &[
            ("blow_it_up.attack", false),
            ("blow_it_up.challenge", false),
            ("blow_it_up.rule", true),
        ],
    ),
    (
        "readback",
        &[
            ("readback.interpret", false),
            ("readback.diff", true),
            ("readback.rewrite", true),
        ],
    ),
    (
        "debug",
        &[
            ("debug.hypothesize", false),
            ("debug.merge", true),
            ("debug.test", false),
            ("debug.select", true),
            ("debug.report", true),
        ],
    ),
    (
        "trace",
        &[
            ("trace.extract", false),
            ("trace.merge", true),
            ("trace.verify", false),
            ("trace.assemble", true),
        ],
    ),
    ("rank", &[("rank.compare", false), ("rank.explain", true)]),
    (
        "tune",
        &[
            ("tune.cases", false),
            ("tune.freeze", true),
            ("tune.execute", false),
            ("tune.score", true),
            ("tune.mutate", false),
        ],
    ),
    (
        "bass_ackwards_prd",
        &[
            ("bass_ackwards_prd.done", false),
            ("bass_ackwards_prd.freeze", true),
            ("bass_ackwards_prd.tree", false),
            ("bass_ackwards_prd.merge", true),
            ("bass_ackwards_prd.review", false),
            ("bass_ackwards_prd.revise", true),
            ("bass_ackwards_prd.must_have", false),
            ("bass_ackwards_prd.finalize", true),
            ("bass_ackwards_prd.apply", true),
        ],
    ),
];

fn command(router: &mut Router, raw: &str) -> CommitOutcome {
    let ctx = router.parse_context().unwrap();
    let meta = Default::default();
    let parsed = parse(raw, &meta, &ctx).unwrap();
    let request =
        Request::from_parsed_with_context(EventId::new(), parsed, &meta, &ctx, false).unwrap();
    router.process(request, None).unwrap()
}

/// Start `mode` for real, then give its contest one running, leased work per kind.
fn seed(
    router: &mut Router,
    store: &Store,
    mode: &str,
    kinds: &'static [(&'static str, bool)],
) -> ContestId {
    let (_, text) = mode_starts::STARTS
        .iter()
        .find(|(m, _)| *m == mode)
        .unwrap();
    assert!(matches!(
        mode_starts::start(router, text).unwrap(),
        RequestResult::Accepted { .. }
    ));
    let contest: ContestId = store
        .call(|db| {
            Ok(
                db.query_row("SELECT contest_id FROM contests WHERE active=1", [], |r| {
                    r.get::<_, String>(0)
                })?,
            )
        })
        .unwrap()
        .parse()
        .unwrap();
    let step = StepId::new();
    store.call(move |db| {
        let trigger: String = db.query_row("SELECT id FROM events ORDER BY seq LIMIT 1", [], |r| r.get(0))?;
        db.execute("INSERT INTO contest_steps(step_id,contest_id,ordinal,script_version,move,state_json,inputs_json,updated_seq) VALUES(?,?,99,'v1','judge','{}','[]',1)",
            params![step.to_string(), contest.to_string()])?;
        let template = Work {
            work_id: WorkId::new(), message_id: MessageId::new(),
            in_reply_to_message_id: Some(MessageId::new()), kind: WorkKind::Reply,
            target: Target::Member("alice".parse().unwrap()), context_id: ContextId("alice".into()),
            trigger_event_id: trigger.parse().unwrap(), history_through: HistoryPosition::new(1).unwrap(),
            attempt: 1, debate_id: None, round: None, replay: None, case_id: None, prompt_ref: None,
            max_output_tokens: None, contest_id: None, step_id: None,
        };
        squad_store::work::insert(db, &template, &ExecutorBinding { model: "test/model".parse().unwrap(), effort: ReasoningEffort::Low }, Some(contest))?;
        let raw: String = db.query_row("SELECT scheduling_meta FROM work WHERE work_id=?", [template.work_id.to_string()], |r| r.get(0))?;
        let original: serde_json::Value = serde_json::from_str(&raw)?;
        db.execute("DELETE FROM work WHERE work_id=?", [template.work_id.to_string()])?;
        for (wire, judge) in kinds.iter().copied() {
            let (id, case) = (WorkId::new(), CaseId::new());
            let mut meta = original.clone();
            meta["work"]["workId"] = serde_json::json!(id);
            meta["work"]["kind"] = serde_json::json!(wire);
            meta["work"]["contestId"] = serde_json::json!(contest);
            meta["work"]["stepId"] = serde_json::json!(step);
            meta["work"]["inReplyToMessageId"] = serde_json::Value::Null;
            meta["work"]["target"] = serde_json::json!(if judge { "judge" } else { "alice" });
            let context = if judge { format!("judge:{case}") } else { format!("contest:{contest}:{step}:alice") };
            meta["work"]["contextId"] = serde_json::json!(context);
            if judge {
                meta["work"]["caseId"] = serde_json::json!(case);
                db.execute("INSERT INTO cases VALUES(?,?,1,?,'test/model','{}',1,NULL,'{}')",
                    params![case.to_string(), context, contest.to_string()])?;
            }
            let owner = if judge { ExecutorKey::Case(case) } else { ExecutorKey::Member("alice".parse().unwrap()) };
            db.execute("INSERT INTO work(work_id,kind,target,context_id,trigger_event_seq,trigger_event_id,message_id,status,epoch,runnable,contest_id,scheduling_meta,lease_fence,step_id,max_output_tokens,history_through,lease_owner,lease_until,attempt,case_id) VALUES (?,?,?,?,1,?,?,'running',1,1,?,?,7,?,1024,1,?,'2099-01-01T00:00:00Z',1,?)",
                params![id.to_string(), wire, if judge { "judge" } else { "alice" }, context, trigger, MessageId::new().to_string(), contest.to_string(), meta.to_string(), step.to_string(), serde_json::to_string(&owner)?, judge.then(|| case.to_string())])?;
        }
        Ok(())
    }).unwrap();
    contest
}
#[test]
fn the_table_lists_every_new_work_kind_and_owner() {
    let mut count = 0;
    for (mode, kinds) in MODES {
        for (wire, judge) in *kinds {
            let kind: WorkKind = serde_json::from_value(serde_json::json!(wire)).unwrap();
            assert_eq!(kind.new_mode_kind().unwrap().as_str(), *mode, "{wire}");
            if *wire != "tune.execute" {
                assert_eq!(kind.new_mode_judge(), *judge, "{wire}");
            }
            count += 1;
        }
    }
    assert_eq!(count, 51);
}

#[test]
fn stop_fences_every_new_work_kind_of_each_mode_and_releases_its_claim() {
    for (mode, kinds) in MODES {
        let root = tempfile::tempdir().unwrap();
        let (store, mut r) = mode_starts::room(root.path());
        let contest = seed(&mut r, &store, mode, kinds);
        assert!(
            matches!(
                command(&mut r, "/stop").result,
                RequestResult::Accepted { .. }
            ),
            "{mode}"
        );
        let expected = kinds.len() as i64;
        store.call(move |db| {
            let (total, fenced): (i64, i64) = db.query_row(
                "SELECT COUNT(*),SUM(status='cancelled' AND runnable=0 AND lease_fence=8) FROM work WHERE contest_id=? AND lease_fence>=7",
                [contest.to_string()], |r| Ok((r.get(0)?, r.get(1)?)))?;
            assert_eq!((total, fenced), (expected, expected), "{mode}: /stop left work runnable or unfenced");
            let live: i64 = db.query_row(
                "SELECT COUNT(*) FROM work WHERE contest_id=? AND (runnable=1 OR status IN ('queued','running'))",
                [contest.to_string()], |r| r.get(0))?;
            assert_eq!(live, 0, "{mode}: work the mode itself queued survived /stop");
            let active: i64 = db.query_row("SELECT COUNT(*) FROM contests WHERE active=1", [], |r| r.get(0))?;
            assert_eq!(active, 0, "{mode}: /stop kept the claim");
            Ok(())
        }).unwrap();
        // A fenced attempt can no longer be claimed, renewed or answered.
        let stale = store
            .call(|db| {
                let mut q = db.prepare(
                    "SELECT COUNT(*) FROM work WHERE status IN ('queued','running') AND runnable=1",
                )?;
                Ok(q.query_row([], |r| r.get::<_, i64>(0))?)
            })
            .unwrap();
        assert_eq!(stale, 0, "{mode}");
        // The next contest may now claim the room.
        assert!(r.parse_context().unwrap().pending_human.is_none());
    }
}

#[test]
fn clear_removes_every_new_work_kind_contest_and_private_row() {
    for (mode, kinds) in MODES {
        let root = tempfile::tempdir().unwrap();
        let (store, mut r) = mode_starts::room(root.path());
        let contest = seed(&mut r, &store, mode, kinds);
        store.call(move |db| {
            // Private per-contest material must not survive a clear either.
            db.execute("INSERT INTO contest_private_maps(contest_id,scope_key,map_json) VALUES(?,'fixture.seed','{\"seed\":42}')",
                [contest.to_string()])?;
            let tx = db.transaction()?;
            squad_store::clear::content(&tx)?;
            tx.commit()?;
            for table in ["work", "contests", "contest_steps", "contest_assignments", "contest_private_maps", "cases"] {
                let n: i64 = db.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))?;
                assert_eq!(n, 0, "{mode}: /clear left rows in {table}");
            }
            Ok(())
        }).unwrap();
    }
}
