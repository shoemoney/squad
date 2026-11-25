//! M1.P3.L3.T6 / M1.P3.L4.T6 / M1.P3.L4.T8 / M1.P4.L3.T4: real-store checks for
//! delayed/duplicate projections, fresh judge cases over retained round scores,
//! export read boundaries and the storage ceiling.
use rusqlite::params;
use squad_protocol::*;
use squad_store::{Store, accounting as a, export, history, storage};

fn nick(s: &str) -> MemberNick {
    s.parse().unwrap()
}

fn debate_state(contest: ContestId, case: Option<CaseId>) -> DebateState {
    DebateState {
        debate_id: contest,
        state: DebateStatus::Judging,
        debaters: [nick("alice"), nick("bob")],
        round: 2,
        total_rounds: 2,
        judge_model: "test/model".into(),
        judge_requested: true,
        state_version: 1,
        terminal_reason: None,
        case_id: case,
        case_ids: case.into_iter().collect(),
    }
}

fn notice(id: EventId, text: &str, contest: Option<ContestId>) -> ProducerEnvelope {
    ProducerEnvelope {
        base: EnvelopeBase {
            version: 1,
            id,
            room_id: MainRoom::Main,
            correlation_id: id,
            causation_id: None,
            sender: Actor::System,
            recipients: vec![Recipient::Me],
            created_at: chrono::Utc::now(),
        },
        content: EnvelopePayload::SystemNotice(SystemNotice {
            code: Text::new("FIXTURE").unwrap(),
            text: Text::new(text).unwrap(),
            work_id: None,
            debate_id: None,
            case_id: None,
            contest_id: contest,
        }),
    }
}

fn open_case(db: &rusqlite::Connection, contest: ContestId, case: CaseId, seq: Cursor, q: &str) {
    let opened = CaseOpened {
        case_id: case,
        judge_invocation_id: EventId::new(),
        work_id: None,
        judge_model: Text::new("test/model").unwrap(),
        material: CaseMaterial::Standalone {
            messages: vec![],
            question: Text::new(q).unwrap(),
        },
        debate_id: Some(contest),
        contest_id: Some(contest),
    };
    squad_store::cases::open(db, &opened, seq).unwrap();
    db.execute(
        "UPDATE cases SET contest_id=? WHERE case_id=?",
        params![contest.to_string(), case.to_string()],
    )
    .unwrap();
}

struct Seeded {
    store: Store,
    contest: ContestId,
    first_case: CaseId,
    second_case: CaseId,
    source: EventId,
    contest_event: EventId,
    room_event: EventId,
}

fn seeded() -> Seeded {
    let store = Store::open(":memory:").unwrap();
    let contest = ContestId::new();
    let first_case = CaseId::new();
    let second_case = CaseId::new();
    let source = EventId::new();
    let contest_event = EventId::new();
    let room_event = EventId::new();
    store
        .commit(source, "fixture".into(), "seed".into(), move |tx, group| {
            let state = debate_state(contest, Some(first_case));
            let seq = group.push(tx, notice(source, "source evidence", None))?;
            // The debate state is the contest's first event; it creates the contest row.
            let state_seq = group.push(tx, squad_core_state(&state, source))?;
            squad_store::contests::save_debate(tx, &state, None, state_seq)?;
            squad_store::contests::associate(tx, contest, ContestKind::Debate, state_seq)?;
            let contest_seq =
                group.push(tx, notice(contest_event, "contest-visible", Some(contest)))?;
            squad_store::contests::associate(tx, contest, ContestKind::Debate, contest_seq)?;
            group.push(tx, notice(room_event, "room-only", None))?;
            open_case(tx, contest, first_case, seq, "first case");
            Ok(group.accepted(source))
        })
        .unwrap();
    // A fresh explicit-judge case on the same contest, opened later.
    store
        .commit(
            EventId::new(),
            "fixture".into(),
            "second".into(),
            move |tx, group| {
                let id = EventId::new();
                let seq = group.push(tx, notice(id, "explicit judge request", Some(contest)))?;
                open_case(tx, contest, second_case, seq, "second case");
                Ok(group.accepted(id))
            },
        )
        .unwrap();
    Seeded {
        store,
        contest,
        first_case,
        second_case,
        source,
        contest_event,
        room_event,
    }
}

fn squad_core_state(state: &DebateState, correlation: EventId) -> ProducerEnvelope {
    ProducerEnvelope {
        base: EnvelopeBase {
            version: 1,
            id: EventId::new(),
            room_id: MainRoom::Main,
            correlation_id: correlation,
            causation_id: None,
            sender: Actor::System,
            recipients: vec![Recipient::Me],
            created_at: chrono::Utc::now(),
        },
        content: EnvelopePayload::DebateState(state.clone()),
    }
}

fn event_ids(events: &[CommittedEnvelope]) -> Vec<EventId> {
    events.iter().map(|e| e.base.id).collect()
}

#[test]
fn duplicate_source_projections_collapse_and_fresh_cases_get_their_own_single_copy() {
    let s = seeded();
    let (source, first, second) = (s.source, s.first_case, s.second_case);
    s.store
        .call(move |db| {
            let origin = squad_store::events::by_id(db, source)?.unwrap();
            let project = |case: CaseId, key: &str, text: &str| {
                history::project(
                    db,
                    &history::attributed(
                        &origin,
                        ContextId(format!("judge:{case}")),
                        key,
                        HistoryRole::User,
                        None,
                        text.into(),
                    ),
                )
            };
            let pos = project(first, "case.source", "evidence")?.expect("first delivery");
            // Replaying the same source into the same case is a no-op and keeps its position.
            assert_eq!(project(first, "case.source", "evidence")?, None);
            assert_eq!(project(first, "case.source", "changed text")?, None);
            // A distinct projection key of the same source is a different artifact.
            assert!(project(first, "case.retained-scores", "scores")?.is_some());
            // The fresh case receives its own copy exactly once.
            let other = project(second, "case.source", "evidence")?.expect("second delivery");
            assert!(other > pos);
            assert_eq!(project(second, "case.source", "evidence")?, None);
            let read = |case: CaseId| {
                history::read(
                    db,
                    &ContextId(format!("judge:{case}")),
                    HistoryPosition::new(1_000).unwrap(),
                )
            };
            assert_eq!(read(first)?.len(), 2);
            let copies = read(second)?;
            assert_eq!(copies.len(), 1);
            assert_eq!(copies[0].content, "evidence");
            let rows: i64 = db.query_row(
                "SELECT COUNT(*) FROM history WHERE origin_event_id=?",
                [source.to_string()],
                |r| r.get(0),
            )?;
            assert_eq!(rows, 3);
            // Case artifacts associate once per event even if committed again.
            let seq = squad_store::events::by_id(db, source)?.unwrap().commit.seq;
            squad_store::cases::associate(db, first, seq)?;
            squad_store::cases::associate(db, first, seq)?;
            let linked: i64 = db.query_row(
                "SELECT COUNT(*) FROM case_events WHERE case_id=? AND event_seq=?",
                params![first.to_string(), seq.get()],
                |r| r.get(0),
            )?;
            assert_eq!(linked, 1);
            Ok(())
        })
        .unwrap();
}

#[test]
fn export_readers_return_consistent_boundaries_without_private_material() {
    let s = seeded();
    let (contest, case, source) = (s.contest, s.first_case, s.source);
    let work = WorkId::new();
    s.store
        .call(move |db| {
            db.execute(
                "INSERT INTO contest_private_maps VALUES (?,'labels','{\"secret\":\"PRIVATE-LABEL-MAP\"}')",
                [contest.to_string()],
            )?;
            // Work row for the repair FK, then rejected judge output.
            db.execute("INSERT INTO work(work_id,kind,target,context_id,trigger_event_seq,trigger_event_id,message_id,status,attempt,epoch,runnable,contest_id,case_id,max_output_tokens) VALUES (?1,'judge','judge',?2,1,?3,?4,'queued',1,1,1,?5,?6,4096)",
                params![work.to_string(),format!("judge:{case}"),source.to_string(),MessageId::new().to_string(),contest.to_string(),case.to_string()])?;
            squad_store::judge_repairs::save(
                db,
                work,
                case,
                &JudgeRepairData {
                    attempt: 1,
                    rejected_output: "REJECTED-JUDGE-OUTPUT".into(),
                    validation_errors: vec!["bad".into()],
                },
            )?;
            Ok(())
        })
        .unwrap();
    let (room, scoped) = s
        .store
        .call(move |db| Ok((export::room(db)?, export::contest(db, contest)?)))
        .unwrap();
    assert_eq!(room.len(), 5);
    assert!(room.windows(2).all(|w| w[0].commit.seq < w[1].commit.seq));
    // The contest boundary contains exactly the associated events, in order.
    let scoped_ids = event_ids(&scoped);
    assert_eq!(scoped.len(), 2);
    assert!(scoped_ids.contains(&s.contest_event));
    assert!(!scoped_ids.contains(&s.room_event));
    assert!(scoped.windows(2).all(|w| w[0].commit.seq < w[1].commit.seq));
    // An unknown contest yields an empty boundary, not an error or a room dump.
    let none = s
        .store
        .call(|db| export::contest(db, ContestId::new()))
        .unwrap();
    assert!(none.is_empty());
    // Neither boundary can expose router-private maps or repair content.
    for text in [
        serde_json::to_string(&room).unwrap(),
        serde_json::to_string(&scoped).unwrap(),
    ] {
        for needle in [
            "PRIVATE-LABEL-MAP",
            "REJECTED-JUDGE-OUTPUT",
            "contest_private_maps",
        ] {
            assert!(!text.contains(needle), "{needle} leaked");
        }
    }
}

#[test]
fn storage_ceiling_refuses_new_work_but_not_reads_exports_or_reconciliation() {
    let s = seeded();
    let contest = s.contest;
    s.store
        .call(move |db| {
            // A tiny ceiling is exceeded by any real database.
            match storage::ensure_capacity(db, 1) {
                Err(squad_store::StoreError::Domain(ErrorCode::STORAGE_LIMIT, text)) => {
                    assert!(text.contains("/clear") && text.contains("export"));
                }
                other => panic!("expected STORAGE_LIMIT, got {other:?}"),
            }
            assert!(storage::ensure_capacity(db, storage::DEFAULT_CAPACITY).is_ok());
            // Reads and export still work above the ceiling.
            assert_eq!(export::room(db)?.len(), 5);
            assert_eq!(export::contest(db, contest)?.len(), 2);
            assert_eq!(
                history::read(
                    db,
                    &ContextId("alice".into()),
                    HistoryPosition::new(1).unwrap()
                )?
                .len(),
                0
            );
            // Spending reconciliation of already-accepted work is never gated by storage.
            let work = WorkId::new();
            let grant = ExecutionGrant {
                binding: ExecutorBinding {
                    model: Text::new("test/model").unwrap(),
                    effort: ReasoningEffort::Low,
                },
                work: Work {
                    work_id: work,
                    message_id: MessageId::new(),
                    in_reply_to_message_id: Some(MessageId::new()),
                    kind: WorkKind::Reply,
                    target: Target::Member(nick("alice")),
                    context_id: ContextId("alice".into()),
                    trigger_event_id: EventId::new(),
                    history_through: HistoryPosition::new(1).unwrap(),
                    attempt: 1,
                    debate_id: None,
                    round: None,
                    replay: None,
                    case_id: None,
                    prompt_ref: None,
                    max_output_tokens: None,
                    contest_id: None,
                    step_id: None,
                },
                owner: ExecutorKey::Member(nick("alice")),
                fence: 1,
                epoch: 1,
                lease_until: chrono::Utc::now() + chrono::Duration::seconds(30),
                deadline: None,
            };
            let tx = db.transaction()?;
            a::reserve(&tx, "r", Some(work), "2026-11-25", 100_000, 20_000_000)?;
            a::start(&tx, &grant, chrono::Utc::now(), "2026-11-25", 20_000_000)?;
            let record = UsageRecord {
                work_id: work,
                attempt: 1,
                target: grant.work.target.clone(),
                context_id: grant.work.context_id.clone(),
                provider_request_id: None,
                model: grant.binding.model.clone(),
                started_at: chrono::Utc::now(),
                ended_at: Some(chrono::Utc::now()),
                outcome: UsageOutcome::Completed,
                usage: Some(Usage {
                    input_tokens: 10,
                    output_tokens: 2,
                    cost_usd: Some(0.00005),
                }),
                estimated_cost_usd: None,
                debate_id: None,
                case_id: None,
                contest_id: None,
                response_model: None,
                day: "2026-11-25".into(),
            };
            a::reconcile(&tx, &record)?;
            assert_eq!(a::daily(&tx, "2026-11-25")?.spent_micros, 50);
            tx.commit()?;
            Ok(())
        })
        .unwrap();
}
