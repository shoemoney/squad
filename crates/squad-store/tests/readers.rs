//! M8.P11.L1.T2 / M9.P4.L1.T2 (store half): every step-bound reader refuses guessed work
//! IDs and alternate contexts for every work kind the eleven new modes add.
mod support;
use rusqlite::params;
use squad_protocol::*;
use squad_store::{Store, contest_steps, readers::ScopedReaders};

/// `(wire kind, judge?)` for all fifty-one new work kinds, grouped by mode.
const KINDS: &[(&str, bool)] = &[
    ("forge.draft", false),
    ("forge.rank", false),
    ("forge.fuse", false),
    ("forge.judge", true),
    ("forecast.frame", true),
    ("forecast.estimate", false),
    ("forecast.revise", false),
    ("forecast.cruxes", true),
    ("decide.options", false),
    ("decide.shortlist", true),
    ("decide.criteria", false),
    ("decide.freeze", true),
    ("decide.score", false),
    ("decide.explain", true),
    ("negotiate.map", false),
    ("negotiate.brief", true),
    ("negotiate.package", false),
    ("negotiate.move", false),
    ("negotiate.counter", false),
    ("negotiate.playbook", true),
    ("blow_it_up.attack", false),
    ("blow_it_up.challenge", false),
    ("blow_it_up.rule", true),
    ("readback.interpret", false),
    ("readback.diff", true),
    ("readback.rewrite", true),
    ("debug.hypothesize", false),
    ("debug.merge", true),
    ("debug.test", false),
    ("debug.select", true),
    ("debug.report", true),
    ("trace.extract", false),
    ("trace.merge", true),
    ("trace.verify", false),
    ("trace.assemble", true),
    ("rank.compare", false),
    ("rank.explain", true),
    ("tune.cases", false),
    ("tune.freeze", true),
    ("tune.execute", false),
    ("tune.score", true),
    ("tune.mutate", false),
    ("bass_ackwards_prd.done", false),
    ("bass_ackwards_prd.freeze", true),
    ("bass_ackwards_prd.tree", false),
    ("bass_ackwards_prd.merge", true),
    ("bass_ackwards_prd.review", false),
    ("bass_ackwards_prd.revise", true),
    ("bass_ackwards_prd.must_have", false),
    ("bass_ackwards_prd.finalize", true),
    ("bass_ackwards_prd.apply", true),
];

fn ready<T>(future: ReaderFuture<'_, T>) -> Result<T, ReaderError> {
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    let mut future = std::pin::pin!(future);
    match future.as_mut().poll(&mut context) {
        std::task::Poll::Ready(v) => v,
        std::task::Poll::Pending => panic!("store reader unexpectedly waits"),
    }
}

struct Case {
    store: Store,
    work: Work,
    event: EventId,
    owner: ExecutorKey,
    judge: bool,
    contest: ContestId,
}

fn member(name: &str) -> ExecutorKey {
    ExecutorKey::Member(name.parse().unwrap())
}

/// A running, leased, step-bound work of `wire` with one frozen artifact and one history row.
fn case(wire: &str, judge: bool) -> Case {
    let store = Store::open(":memory:").unwrap();
    let source = support::event(&store);
    let event = source.base.id;
    let contest = ContestId::new();
    let step = StepId::new();
    let case_id = CaseId::new();
    let kind: WorkKind = serde_json::from_value(serde_json::json!(wire)).unwrap();
    let kind_name = wire;
    let composition = kind.new_mode_composition();
    if wire != "tune.execute" {
        assert_eq!(kind.new_mode_judge(), judge, "{wire}");
    }
    let (target, context, owner) = if judge {
        (
            Target::Judge,
            ContextId(format!("judge:{case_id}")),
            ExecutorKey::Case(case_id),
        )
    } else {
        (
            Target::Member("a".parse().unwrap()),
            ContextId(format!("contest:{contest}:{step}:a")),
            member("a"),
        )
    };
    let work = Work {
        work_id: WorkId::new(),
        message_id: MessageId::new(),
        in_reply_to_message_id: None,
        kind,
        target: target.clone(),
        context_id: context.clone(),
        trigger_event_id: event,
        history_through: HistoryPosition::new(1).unwrap(),
        attempt: 1,
        debate_id: None,
        round: None,
        replay: None,
        case_id: judge.then_some(case_id),
        prompt_ref: (kind_name != "tune.execute").then(|| PromptRef {
            template_id: Text::new(composition.unwrap()).unwrap(),
            pack_version: Text::new("1.0.0").unwrap(),
            template_hash: Text::new("fixture").unwrap(),
        }),
        max_output_tokens: (kind_name == "tune.execute")
            .then_some(constants::TUNE_EXEC_VISIBLE_TOKENS),
        contest_id: Some(contest),
        step_id: Some(step),
    };
    let w = work.clone();
    let seq = source.commit.seq;
    let name = serde_json::to_value(&w.kind).unwrap();
    let lease_owner = serde_json::to_string(&owner).unwrap();
    store.call(move |db| {
        db.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active) VALUES(?,?,1,1,'{}',0)",
            params![contest.to_string(), w.kind.new_mode_kind().unwrap().as_str()])?;
        if judge {
            db.execute("INSERT INTO cases VALUES(?,?,1,?,'test/model','{}',1,NULL,'{}')",
                params![case_id.to_string(), w.context_id.0, contest.to_string()])?;
        }
        let frozen = ContestStep {
            contest_id: contest, step_id: step, ordinal: 1,
            script_version: Text::new("v1").unwrap(),
            move_name: if judge { ContestMove::Judge } else { ContestMove::Fanout },
            inputs: vec![event],
            assignments: vec![ContestAssignment {
                target: w.target.clone(), context_id: w.context_id.clone(),
                payload_refs: vec![event], work_id: Some(w.work_id),
            }],
        };
        contest_steps::insert(db, &frozen, seq)?;
        db.execute("INSERT INTO history(pos,context_id,event_seq,origin_event_id,projection_key,role,content,meta) VALUES(1,?,1,?,'fixture','user','input','{\"anonymous\":true}')",
            params![w.context_id.0, event.to_string()])?;
        squad_store::work::insert(db, &w, &ExecutorBinding { model: Text::new("test/model").unwrap(), effort: ReasoningEffort::Low }, None)?;
        contest_steps::attach_work(db, step, &w)?;
        db.execute("UPDATE work SET status='running',attempt=1,history_through=1,lease_owner=?,lease_until='2099-01-01T00:00:00Z' WHERE work_id=?",
            params![lease_owner, w.work_id.to_string()])?;
        let (audience, recipients) = if judge { ("case", "[]") } else { ("members", "[\"a\"]") };
        db.execute("INSERT INTO contest_artifacts VALUES(1,?,?,?,?,0,'system',?,?,'{\"body\":\"private\"}')",
            params![contest.to_string(), step.to_string(), w.kind.new_mode_kind().unwrap().as_str(), name.as_str().unwrap(), audience, recipients])?;
        Ok(())
    }).unwrap();
    Case {
        store,
        work,
        event,
        owner,
        judge,
        contest,
    }
}

fn readers(c: &Case, owner: ExecutorKey) -> ScopedReaders {
    ScopedReaders::new(c.store.clone(), owner, 1)
}

/// Every reader entry point, as `(name, result-is-ok)`, for one executor and work id.
fn probe(r: &ScopedReaders, id: WorkId, event: EventId) -> Vec<(&'static str, ReaderResult)> {
    vec![
        (
            "artifact_for_work",
            ReaderResult::of(r.artifact_for_work(id, event).map(|_| ())),
        ),
        (
            "for_work",
            ReaderResult::of(ready(r.for_work(id)).map(|_| ())),
        ),
        (
            "prompt_values_for_work",
            ReaderResult::of(ready(r.prompt_values_for_work(id)).map(|_| ())),
        ),
        ("get", ReaderResult::of(ready(r.get(id)).map(|_| ()))),
        (
            "repair_for_attempt",
            ReaderResult::of(ready(r.repair_for_attempt(id, 1)).map(|_| ())),
        ),
    ]
}
#[derive(Debug, PartialEq)]
enum ReaderResult {
    Ok,
    Err(ReaderError),
}
impl ReaderResult {
    fn of(r: Result<(), ReaderError>) -> Self {
        r.map_or_else(Self::Err, |()| Self::Ok)
    }
}

#[test]
fn table_lists_every_new_work_kind_once() {
    assert_eq!(KINDS.len(), 51);
    let mut modes = std::collections::BTreeSet::new();
    for (wire, _) in KINDS {
        let kind: WorkKind = serde_json::from_value(serde_json::json!(wire)).unwrap();
        modes.insert(kind.new_mode_kind().expect(wire).as_str());
    }
    assert_eq!(modes.len(), 11, "{modes:?}");
    let unique: std::collections::BTreeSet<_> = KINDS.iter().map(|(w, _)| *w).collect();
    assert_eq!(unique.len(), KINDS.len());
}

#[test]
fn the_assigned_executor_reads_its_own_new_mode_step() {
    for (wire, judge) in KINDS {
        let c = case(wire, *judge);
        let r = readers(&c, c.owner.clone());
        assert_eq!(
            r.artifact_for_work(c.work.work_id, c.event).unwrap()["body"],
            "private",
            "{wire}"
        );
        assert_eq!(
            ready(r.for_work(c.work.work_id)).unwrap().len(),
            1,
            "{wire}"
        );
        assert!(
            ready(r.prompt_values_for_work(c.work.work_id)).is_ok(),
            "{wire}"
        );
        assert_eq!(
            ready(r.get(c.work.work_id)).unwrap().unwrap().work_id,
            c.work.work_id,
            "{wire}"
        );
        assert_eq!(ready(r.unfinished()).unwrap().len(), 1, "{wire}");
    }
}

#[test]
fn guessed_work_ids_are_unknown_to_every_reader() {
    for (wire, judge) in KINDS {
        let c = case(wire, *judge);
        let r = readers(&c, c.owner.clone());
        let guess = WorkId::new();
        for (reader, result) in probe(&r, guess, c.event) {
            match reader {
                "get" => assert_eq!(result, ReaderResult::Ok, "{wire} {reader}"),
                _ => assert_eq!(
                    result,
                    ReaderResult::Err(ReaderError::NotFound),
                    "{wire} {reader}"
                ),
            }
        }
        assert!(ready(r.get(guess)).unwrap().is_none(), "{wire}");
    }
}

#[test]
fn another_member_judge_or_case_cannot_read_a_step_bound_work() {
    for (wire, judge) in KINDS {
        let c = case(wire, *judge);
        let intruders = [
            member("b"),
            ExecutorKey::Case(CaseId::new()),
            if c.judge {
                member("a")
            } else {
                ExecutorKey::Case(CaseId::new())
            },
        ];
        for intruder in intruders {
            let r = readers(&c, intruder.clone());
            for (reader, result) in probe(&r, c.work.work_id, c.event) {
                assert_eq!(
                    result,
                    ReaderResult::Err(ReaderError::Forbidden),
                    "{wire} {reader} {intruder:?}"
                );
            }
            assert!(
                ready(r.unfinished()).unwrap().is_empty(),
                "{wire} {intruder:?} saw another executor's work in unfinished()"
            );
        }
    }
}

#[test]
fn alternate_context_ids_and_tampered_assignments_are_refused() {
    for (wire, judge) in KINDS {
        // A work row cannot be moved into a sibling step's or case's context at all.
        let c = case(wire, *judge);
        let id = c.work.work_id;
        let other_context = if *judge {
            format!("judge:{}", CaseId::new())
        } else {
            format!("contest:{}:{}:a", c.contest, StepId::new())
        };
        let moved = c.store.call(move |db| {
            db.execute("PRAGMA foreign_keys=OFF", [])?;
            db.execute(
                "UPDATE work SET context_id=? WHERE work_id=?",
                params![other_context, id.to_string()],
            )?;
            Ok(())
        });
        assert!(moved.is_err(), "{wire}: the store accepted a moved context");
        let r = readers(&c, c.owner.clone());
        assert!(r.artifact_for_work(id, c.event).is_ok(), "{wire} unchanged");

        // The frozen assignment is retargeted: the work no longer matches its slot.
        let c = case(wire, *judge);
        let id = c.work.work_id;
        c.store
            .call(move |db| {
                db.execute("PRAGMA foreign_keys=OFF", [])?;
                db.execute(
                    "UPDATE contest_assignments SET assignment_json=json_set(assignment_json,'$.workId',?) WHERE work_id=?",
                    params![WorkId::new().to_string(), id.to_string()],
                )?;
                Ok(())
            })
            .unwrap();
        let r = readers(&c, c.owner.clone());
        for (reader, result) in probe(&r, id, c.event) {
            assert_eq!(
                result,
                ReaderResult::Err(ReaderError::Forbidden),
                "{wire} {reader} retargeted assignment"
            );
        }
    }
}

#[test]
fn artifact_reader_refuses_events_outside_the_frozen_assignment() {
    for (wire, judge) in KINDS {
        let c = case(wire, *judge);
        let r = readers(&c, c.owner.clone());
        let stranger = support::event(&c.store).base.id;
        assert!(
            r.artifact_for_work(c.work.work_id, stranger).is_err(),
            "{wire}: an event the step never froze was readable"
        );
        assert!(
            r.artifact_for_work(c.work.work_id, EventId::new()).is_err(),
            "{wire}: a guessed event id was readable"
        );
    }
}

#[test]
fn a_stale_epoch_or_lease_or_finished_work_is_not_readable() {
    for (wire, judge) in KINDS {
        let c = case(wire, *judge);
        let id = c.work.work_id;
        // The room was cleared: a capability minted for the old epoch is dead.
        let old = ScopedReaders::new(c.store.clone(), c.owner.clone(), 2);
        for (reader, result) in probe(&old, id, c.event) {
            assert_eq!(
                result,
                ReaderResult::Err(ReaderError::StaleWork),
                "{wire} {reader} epoch"
            );
        }
        let r = readers(&c, c.owner.clone());
        c.store
            .call(move |db| {
                db.execute(
                    "UPDATE work SET lease_until='2000-01-01T00:00:00Z' WHERE work_id=?",
                    [id.to_string()],
                )?;
                Ok(())
            })
            .unwrap();
        assert_eq!(
            ReaderResult::of(r.artifact_for_work(id, c.event).map(|_| ())),
            ReaderResult::Err(ReaderError::StaleWork),
            "{wire} expired lease (artifact)"
        );
        assert_eq!(
            ReaderResult::of(ready(r.for_work(id)).map(|_| ())),
            ReaderResult::Err(ReaderError::StaleWork),
            "{wire} expired lease (history)"
        );
        c.store.call(move |db| {
            db.execute("UPDATE work SET status='completed',lease_until='2099-01-01T00:00:00Z' WHERE work_id=?", [id.to_string()])?;
            Ok(())
        }).unwrap();
        for reader in ["artifact_for_work", "for_work", "prompt_values_for_work"] {
            let result = probe(&r, id, c.event)
                .into_iter()
                .find(|(n, _)| *n == reader)
                .unwrap()
                .1;
            assert_eq!(
                result,
                ReaderResult::Err(ReaderError::NotRunnable),
                "{wire} {reader} finished"
            );
        }
    }
}

#[test]
fn repair_diagnostics_are_attempt_bound_and_private_for_every_new_kind() {
    for (wire, judge) in KINDS {
        let c = case(wire, *judge);
        let (w, id) = (c.work.clone(), c.work.work_id);
        c.store
            .call(move |db| {
                squad_store::judge_repairs::save_mode(
                    db,
                    &w,
                    &JudgeRepairData {
                        attempt: 2,
                        rejected_output: "private rejected output".into(),
                        validation_errors: vec!["bad reference".into()],
                    },
                )?;
                db.execute(
                    "UPDATE work SET attempt=2 WHERE work_id=?",
                    [id.to_string()],
                )?;
                Ok(())
            })
            .unwrap();
        let r = readers(&c, c.owner.clone());
        assert_eq!(
            ready(r.repair_for_attempt(id, 2))
                .unwrap()
                .unwrap()
                .rejected_output,
            "private rejected output",
            "{wire}"
        );
        // A future attempt, attempt zero and a stranger are all refused.
        for attempt in [0, 3] {
            assert!(
                ready(r.repair_for_attempt(id, attempt)).is_err(),
                "{wire} attempt {attempt}"
            );
        }
        assert!(
            ready(readers(&c, member("b")).repair_for_attempt(id, 2)).is_err(),
            "{wire} stranger"
        );
    }
}
