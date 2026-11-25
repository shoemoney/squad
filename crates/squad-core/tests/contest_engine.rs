//! M6.P1.L7.T1-T4: uniform checks over the shared contest engine. Every script
//! is built from the same moves; one contest owns the room at a time; barriers
//! commit atomically and replay without duplicates; only generation moves
//! reach a provider.
mod support;
use squad_core::orchestrator::{census, contests, grill, types::*};
use squad_core::router::Router;
use squad_protocol::*;
use std::collections::BTreeMap;
use support::Fixture;

fn nick(s: &str) -> MemberNick {
    s.parse().unwrap()
}

fn model_move(m: &ContestMove) -> bool {
    matches!(
        m,
        ContestMove::Collect | ContestMove::Fanout | ContestMove::Turns | ContestMove::Judge
    )
}

/// Declared actors, templates, word limits, attempts and deadlines are
/// uniform: generation moves declare all of them, local moves declare none.
fn check_script(name: &str, steps: &[StepSpec]) {
    assert!(!steps.is_empty(), "{name}: empty script");
    for (i, step) in steps.iter().enumerate() {
        let at = format!("{name}[{i}] {:?}", step.move_name);
        if model_move(&step.move_name) {
            let composition = step.composition.as_deref().expect(&at);
            assert!(!composition.is_empty(), "{at}");
            assert!(
                step.attempts >= 1 && step.attempts <= 3,
                "{at}: attempts {}",
                step.attempts
            );
            assert!(
                step.deadline_ms >= 60_000,
                "{at}: deadline {}",
                step.deadline_ms
            );
            if step.move_name != ContestMove::Judge {
                assert!(
                    step.word_limit.is_some_and(|w| w > 0),
                    "{at}: no word limit"
                );
            }
            if step.move_name == ContestMove::Judge {
                assert_eq!(step.actors, vec![Target::Judge], "{at}");
            }
        } else {
            // pick / tally / barrier / report never call a provider.
            assert_eq!(step.attempts, 0, "{at}");
            assert_eq!(step.deadline_ms, 0, "{at}");
            assert!(step.composition.is_none(), "{at}");
            assert!(step.word_limit.is_none(), "{at}");
            assert!(step.actors.is_empty(), "{at}");
        }
    }
    // A fanout/turns step is always followed by a barrier before anything else
    // advances (the atomic commit point).
    for pair in steps.windows(2) {
        if matches!(pair[0].move_name, ContestMove::Fanout | ContestMove::Turns) {
            assert_eq!(pair[1].move_name, ContestMove::Barrier, "{name}: {pair:?}");
        }
    }
    assert_eq!(
        steps.last().unwrap().move_name,
        ContestMove::Report,
        "{name}"
    );
}

fn prompts() -> FrozenPrompts {
    FrozenPrompts {
        pack_version: "test".into(),
        composition_hashes: BTreeMap::new(),
        turn_words: constants::TURN_WORDS,
        final_turn_words: constants::FINAL_TURN_WORDS,
    }
}

#[test]
fn debate_census_and_grill_scripts_share_moves_actors_limits_and_failure_rules() {
    for rounds in [1, 2, 5, 20] {
        for (opening, judge) in [(true, true), (true, false), (false, true), (false, false)] {
            let steps = contests::debate_script(rounds, opening, judge, &prompts()).unwrap();
            check_script(&format!("debate r{rounds} o{opening} j{judge}"), &steps);
            let judged = steps
                .iter()
                .filter(|s| s.move_name == ContestMove::Judge)
                .count();
            assert_eq!(judged > 0, judge);
            for s in steps.iter().filter(|s| s.move_name == ContestMove::Fanout) {
                assert_eq!(s.attempts, constants::DEBATE_TURN_ATTEMPTS);
            }
        }
    }
    assert!(contests::debate_script(0, true, true, &prompts()).is_err());
    assert!(contests::debate_script(21, true, true, &prompts()).is_err());

    for passes in 1..=5u8 {
        let participants: Vec<_> = (0..12).map(|i| nick(&format!("member{i}"))).collect();
        let contest = ContestId::new();
        let setup = CensusSetup {
            contest_id: contest,
            plan: CensusPlan {
                contest_id: contest,
                version: 1,
                text: Text::new("plan").unwrap(),
                source_event_ids: vec![],
            },
            participants: participants.clone(),
            passes,
            room_words: constants::ROOM_WORDS,
            plan_words: 1_200,
            models: BTreeMap::new(),
            prompt_refs: BTreeMap::new(),
            redaction_tokens: vec![],
        };
        let steps = census::script(&setup);
        check_script(&format!("census p{passes}"), &steps);
        // Eight steps per pass, and only the hunt and rate fanouts address members.
        assert_eq!(steps.len(), 8 * usize::from(passes));
        for s in steps.iter().filter(|s| s.move_name == ContestMove::Fanout) {
            assert_eq!(s.actors.len(), participants.len());
        }
        let calls: usize = steps
            .iter()
            .filter(|s| model_move(&s.move_name))
            .map(|s| s.actors.len())
            .sum();
        assert_eq!(calls, usize::from(passes) * (2 * participants.len() + 2));
    }

    for rounds in [1, 3, 10] {
        let setup = GrillSetup {
            contest_id: ContestId::new(),
            leader: nick("alpha"),
            grillers: ["beta", "gamma", "delta"].map(nick).into(),
            source_message_id: MessageId::new(),
            initial_plan: "plan".into(),
            focus: String::new(),
            total_rounds: rounds,
        };
        let steps = grill::script(&setup, 400, 1_200).unwrap();
        check_script(&format!("grill r{rounds}"), &steps);
        assert!(steps.iter().all(|s| s.move_name != ContestMove::Judge));
        let calls = steps.iter().filter(|s| model_move(&s.move_name)).count();
        assert_eq!(calls as u32, setup.normal_calls());
    }
}

// ---------------------------------------------------------------- busy contest

fn router(f: &Fixture) -> Router {
    let roster: Vec<_> = f.config.agents.iter().map(|a| a.nick.clone()).collect();
    let binding = ExecutorBinding {
        model: Text::new(support::fake_openrouter::MODEL).unwrap(),
        effort: ReasoningEffort::Medium,
    };
    let mut bindings: BTreeMap<String, ExecutorBinding> = roster
        .iter()
        .map(|n| (n.to_string(), binding.clone()))
        .collect();
    bindings.insert("judge".into(), binding);
    let mut router = Router::new(f.store.clone(), roster, bindings, 20_000_000).unwrap();
    router.engine_config = Some(f.config.clone());
    router
}

fn submit(router: &mut Router, text: &str) -> Result<RequestResult, ErrorCode> {
    let meta = parser::SubmissionMeta::default();
    let parsed = parse(text, &meta, &router.parse_context().unwrap()).map_err(|e| e.code)?;
    let request = Request::from_parsed(EventId::new(), parsed, &meta, false).unwrap();
    match router.process(request, None) {
        Ok(outcome) => Ok(outcome.result),
        Err(squad_store::StoreError::Domain(code, _)) => Err(code),
        Err(other) => panic!("{other:?}"),
    }
}

const MODES: [(&str, &str); 7] = [
    ("debate", "@all $debate Which approach should we take?"),
    (
        "tournament",
        "@member0 @member1 @member2 $tournament Design a bridge",
    ),
    ("grill", "@all $grill How do we ship safely?"),
    (
        "census",
        "@all $census Launch one region, then expand after review.",
    ),
    (
        "crosscheck",
        "@all $crosscheck Launch one region, then expand after review.",
    ),
    ("survey", "@all $survey Which rollout should we use?"),
    (
        "probe",
        "@all $probe Requests stall after deploy; inspect logs and metrics.",
    ),
];

#[tokio::test]
async fn one_active_contest_blocks_every_other_mode_in_every_state() {
    for (first, first_text) in MODES {
        let f = Fixture::new(6, 20_000_000).await;
        f.store
            .call(|db| {
                db.execute("UPDATE catalog SET context_length=2000000", [])?;
                Ok(())
            })
            .unwrap();
        let mut router = router(&f);
        let result = submit(&mut router, first_text);
        assert!(
            matches!(
                result,
                Ok(RequestResult::Accepted { .. } | RequestResult::PendingConfirmation { .. })
            ),
            "{first}: {result:?}"
        );
        let active: i64 = f
            .store
            .call(|db| {
                Ok(
                    db.query_row("SELECT COUNT(*) FROM contests WHERE active=1", [], |r| {
                        r.get(0)
                    })?,
                )
            })
            .unwrap();
        assert_eq!(active, 1, "{first} did not claim the room");
        for (second, second_text) in MODES {
            let code = match submit(&mut router, second_text) {
                Err(code) => code,
                Ok(RequestResult::Rejected { error, .. }) => error.code,
                Ok(other) => panic!("{first} then {second} was admitted: {other:?}"),
            };
            assert!(
                matches!(code, ErrorCode::DEBATE_BUSY | ErrorCode::CONTEST_BUSY),
                "{first} then {second}: {code:?}"
            );
        }
        // Still exactly one owner afterwards: refused claims leave no trace.
        let (active, contests): (i64, i64) = f
            .store
            .call(|db| {
                Ok((
                    db.query_row("SELECT COUNT(*) FROM contests WHERE active=1", [], |r| {
                        r.get(0)
                    })?,
                    db.query_row("SELECT COUNT(*) FROM contests", [], |r| r.get(0))?,
                ))
            })
            .unwrap();
        assert_eq!((active, contests), (1, 1), "{first}");
        if first == "census" {
            // Between passes the census waits for the human; it still owns the room.
            f.store
                .call(|db| {
                    db.execute(
                        "UPDATE contests SET state_json=json_set(state_json,'$.status','WAITING_FOR_PLAN') WHERE kind='census'",
                        [],
                    )?;
                    Ok(())
                })
                .unwrap();
            for (second, second_text) in MODES {
                let code = match submit(&mut router, second_text) {
                    Err(code) => code,
                    Ok(RequestResult::Rejected { error, .. }) => error.code,
                    Ok(other) => {
                        panic!("between-pass census then {second} was admitted: {other:?}")
                    }
                };
                assert!(
                    matches!(code, ErrorCode::DEBATE_BUSY | ErrorCode::CONTEST_BUSY),
                    "between-pass census then {second}: {code:?}"
                );
            }
        }
        // Stop releases the claim for the next mode.
        router
            .process(
                Request::Stop {
                    request_id: EventId::new(),
                    target: None,
                },
                None,
            )
            .unwrap();
    }
}

// ------------------------------------------------- real room: barriers, replay, accounting

mod room {
    use super::support::fake_openrouter::*;
    use squad_agents::openrouter::OpenRouterClient;
    use squad_core::room::RoomHandle;
    use squad_protocol::*;
    use std::{collections::BTreeMap, time::Duration};
    use wiremock::{
        Mock, Request as HttpRequest, Respond, ResponseTemplate,
        matchers::{method, path},
    };

    pub struct Fixture {
        pub _dir: tempfile::TempDir,
        pub config: SquadConfig,
        pub fake: FakeOpenRouter,
    }
    impl Fixture {
        pub async fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let options = config::LoadOptions {
                env_file: None,
                data_dir: Some(dir.path().join("data")),
                working_dir: dir.path().into(),
                config_dir: None,
                default_data_dir: dir.path().join("data"),
            };
            let mut config = SquadConfig::load_with_env(
                &options,
                &BTreeMap::from([("OPENROUTER_API_KEY".into(), "fake-key".into())]),
            )
            .unwrap();
            config.agents = (0..6)
                .map(|i| config::AgentConfig {
                    nick: nick(&format!("member{i}")),
                    model: format!("fixture/member{i}"),
                    persona: format!("PERSONA_{i}"),
                    reasoning_effort: None,
                })
                .collect();
            config.judge.model = "fixture/judge".into();
            config.budget.daily_max_usd = 20.;
            config.budget.debate_confirm_usd = 10.;
            let fake = FakeOpenRouter::start().await;
            let models=config.agents.iter().map(|a|a.model.clone()).chain(std::iter::once(config.judge.model.clone())).map(|id|serde_json::json!({"id":id,"context_length":1000000,"top_provider":{"max_completion_tokens":65536},"supported_parameters":["reasoning"],"pricing":{"prompt":"0.000001","completion":"0.000002"}})).collect::<Vec<_>>();
            fake.catalog(serde_json::json!({"data":models})).await;
            Mock::given(method("POST"))
                .and(path("/api/v1/chat/completions"))
                .respond_with(Answers)
                .mount(&fake.server)
                .await;
            Self {
                _dir: dir,
                config,
                fake,
            }
        }
        pub async fn open(&self, mode: OpenMode) -> RoomHandle {
            RoomHandle::open_with_client(
                self.config.clone(),
                "main",
                mode,
                OpenRouterClient::new(&self.fake.base_url(), "fake-key").unwrap(),
            )
            .await
            .unwrap()
        }
        pub fn db(&self) -> rusqlite::Connection {
            rusqlite::Connection::open(self.config.data_dir.join("squad.db")).unwrap()
        }
        pub fn count(&self, kind: &str) -> i64 {
            self.db()
                .query_row("SELECT COUNT(*) FROM events WHERE kind=?", [kind], |r| {
                    r.get(0)
                })
                .unwrap()
        }
        pub fn assert_ledger(&self) {
            let mut db = self.db();
            let tx = db.transaction().unwrap();
            let (spent,reserved,unknown):(i64,i64,i64)=tx.query_row("SELECT COALESCE(SUM(spent_micros),0),COALESCE(SUM(reserved_micros),0),COALESCE(SUM(unknown_calls),0) FROM spend_daily",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
            let actual: i64 = tx
                .query_row(
                    "SELECT COALESCE(SUM(reported_cost_micros),0) FROM attempts",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            let held: i64 = tx
                .query_row(
                    "SELECT COALESCE(SUM(amount_micros),0) FROM reservations",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            let pending: i64 = tx
                .query_row(
                    "SELECT COUNT(*) FROM attempts WHERE unresolved=1",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!((spent, reserved, unknown), (actual, held, pending));
            assert!(spent >= 0 && reserved >= 0);
        }
        pub async fn wait(&self, check: impl Fn() -> bool) {
            tokio::time::timeout(Duration::from_secs(15), async {
                while !check() {
                    self.assert_ledger();
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            })
            .await
            .unwrap_or_else(|_| {
                let db = self.db();
                let mut q = db
                    .prepare("SELECT kind,status,attempt,error_json FROM work")
                    .unwrap();
                let rows = q
                    .query_map([], |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, i64>(2)?,
                            r.get::<_, Option<String>>(3)?,
                        ))
                    })
                    .unwrap()
                    .collect::<Result<Vec<_>, _>>()
                    .unwrap();
                panic!("timeout: {rows:?}")
            });
        }
    }
    fn nick(s: &str) -> MemberNick {
        s.parse().unwrap()
    }
    pub fn request(config: &SquadConfig, text: &str) -> squad_protocol::Request {
        let ctx = parser::ParseContext {
            roster: config.agents.iter().map(|a| a.nick.clone()).collect(),
            ..Default::default()
        };
        let meta = parser::SubmissionMeta::default();
        squad_protocol::Request::from_parsed(
            EventId::new(),
            parse(text, &meta, &ctx).unwrap(),
            &meta,
            false,
        )
        .unwrap()
    }

    #[derive(Clone)]
    struct Answers;
    impl Respond for Answers {
        fn respond(&self, r: &HttpRequest) -> ResponseTemplate {
            let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
            if body["model"] == "fixture/judge"
                && body["messages"][0]["content"]
                    .as_str()
                    .unwrap()
                    .contains("summoned judge")
            {
                let user = body["messages"][1]["content"].as_str().unwrap();
                let data = user
                    .split("Tournament prompt and anonymized answers, supplied as evidence:\n")
                    .nth(1)
                    .unwrap()
                    .trim_start();
                let mut de = serde_json::Deserializer::from_str(data);
                let data: serde_json::Value = serde::Deserialize::deserialize(&mut de).unwrap();
                structured_judge(
                    serde_json::json!({"ranking":data["entries"].as_array().unwrap().iter().map(|e|serde_json::json!({"label":e["label"],"score":8,"critique":"Supported proposal"})).collect::<Vec<_>>()}),
                )
            } else {
                let jitter = {
                    use std::hash::{Hash, Hasher};
                    let mut h = std::collections::hash_map::DefaultHasher::new();
                    r.body.hash(&mut h);
                    std::time::SystemTime::now().hash(&mut h);
                    h.finish() % 150
                };
                streaming(
                    &format!(
                        "ANSWER_{}: I am MEMBER0 from fixture/member0. PERSONA_0",
                        (0..6)
                            .find(|i| body["messages"][0]["content"]
                                .as_str()
                                .unwrap()
                                .contains(&format!("PERSONA_{i}")))
                            .unwrap()
                    ),
                    "stop",
                )
                .set_delay(Duration::from_millis(jitter))
            }
        }
    }
}

/// Run a tournament to its champion, with member latency jittered so the
/// completion order of the parallel entrant calls differs from run to run.
async fn tournament() -> (room::Fixture, squad_core::room::RoomHandle) {
    let f = room::Fixture::new().await;
    let handle = f.open(OpenMode::Interactive).await;
    let result = handle
        .submit(room::request(
            &f.config,
            "@member0 @member1 @member2 $tournament Design a bridge",
        ))
        .await;
    assert!(
        matches!(result, RequestResult::Accepted { .. }),
        "{result:?}"
    );
    f.wait(|| f.count("tournament.champion") == 1).await;
    (f, handle)
}

fn kinds(f: &room::Fixture) -> BTreeMap<String, i64> {
    let db = f.db();
    let mut q = db
        .prepare("SELECT kind, COUNT(*) FROM events WHERE kind NOT IN ('budget.state','usage.record','work.state') GROUP BY kind")
        .unwrap();
    q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

#[tokio::test]
async fn barriers_commit_atomically_deliver_exactly_and_replay_without_duplicates() {
    let (f, handle) = tournament().await;
    let before = kinds(&f);
    assert_eq!(before["tournament.entry"], 5);
    assert_eq!(before["tournament.ranking"], 2);
    assert_eq!(before["tournament.result"], 5);
    assert_eq!(before["tournament.champion"], 1);
    let db = f.db();
    // Every commit group is whole: sizes and indexes are consistent.
    let torn: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM (SELECT commit_start_seq FROM events GROUP BY commit_start_seq HAVING COUNT(*)<>MAX(commit_size) OR MAX(commit_index)<>COUNT(*)-1 OR MIN(commit_index)<>0)",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(torn, 0, "a commit group was torn");
    // Exact destinations: an entry is projected only into its author's own context,
    // results only into participants; outsiders get nothing.
    let leaked: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM history WHERE projection_key LIKE 'tournament.%' AND context_id IN ('member3','member4','member5')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(leaked, 0);
    let foreign_entries: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM history h JOIN events e ON e.id=h.origin_event_id WHERE h.projection_key='tournament.entry' AND h.context_id<>e.sender",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(foreign_entries, 0);
    let duplicates = |db: &rusqlite::Connection| -> i64 {
        db.query_row(
            "SELECT COUNT(*) FROM (SELECT 1 FROM history GROUP BY context_id,origin_event_id,projection_key HAVING COUNT(*)>1)",
            [],
            |r| r.get(0),
        )
        .unwrap()
    };
    assert_eq!(duplicates(&db), 0);
    let total: i64 = db
        .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
        .unwrap();
    let artifacts: i64 = db
        .query_row("SELECT COUNT(*) FROM mode_artifacts", [], |r| r.get(0))
        .unwrap_or(0);
    drop(db);

    // Replay: restart the room. Nothing already committed is committed again.
    handle.shutdown().await.unwrap();
    drop(handle);
    let handle = f.open(OpenMode::Interactive).await;
    tokio::time::sleep(std::time::Duration::from_millis(600)).await;
    assert_eq!(kinds(&f), before);
    let db = f.db();
    assert_eq!(
        db.query_row::<i64, _, _>("SELECT COUNT(*) FROM events", [], |r| r.get(0))
            .unwrap(),
        total
    );
    assert_eq!(
        db.query_row::<i64, _, _>("SELECT COUNT(*) FROM mode_artifacts", [], |r| r.get(0))
            .unwrap_or(0),
        artifacts
    );
    assert_eq!(duplicates(&db), 0);
    f.assert_ledger();
    handle.shutdown().await.unwrap();
}

#[tokio::test]
async fn only_generation_moves_reach_the_provider_and_accounting_matches_calls() {
    let (f, handle) = tournament().await;
    let db = f.db();
    // 3 entrants then 2 survivors: 5 member calls + 2 blind rankings = 7 provider calls.
    let requests = f.fake.captured_bodies().await.len() as i64;
    let attempts: i64 = db
        .query_row("SELECT COUNT(*) FROM attempts", [], |r| r.get(0))
        .unwrap();
    let work: i64 = db
        .query_row("SELECT COUNT(*) FROM work", [], |r| r.get(0))
        .unwrap();
    assert_eq!((requests, attempts, work), (7, 7, 7));
    let (members, judges): (i64, i64) = (
        db.query_row(
            "SELECT COUNT(*) FROM attempts WHERE target LIKE 'member%'",
            [],
            |r| r.get(0),
        )
        .unwrap(),
        db.query_row(
            "SELECT COUNT(*) FROM attempts WHERE target='judge'",
            [],
            |r| r.get(0),
        )
        .unwrap(),
    );
    assert_eq!((members, judges), (5, 2));
    // Local moves (barrier, tally, report) never own an assignment or a call,
    // and every call belongs to a generation work kind.
    let mut q = db
        .prepare("SELECT s.move, COUNT(x.work_id) FROM contest_steps s LEFT JOIN contest_assignments x ON x.step_id=s.step_id GROUP BY s.move")
        .unwrap();
    let rows: Vec<(String, i64)> = q
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let moves: Vec<&str> = rows.iter().map(|r| r.0.as_str()).collect();
    for local in ["barrier", "tally", "report"] {
        assert!(moves.contains(&local), "{local} step missing: {moves:?}");
    }
    for (movement, assigned) in &rows {
        if matches!(movement.as_str(), "pick" | "tally" | "barrier" | "report") {
            assert_eq!(*assigned, 0, "{movement} owns provider work");
        }
    }
    let mut q = db
        .prepare(
            "SELECT DISTINCT w.kind FROM work w JOIN attempts a ON a.work_id=w.work_id ORDER BY 1",
        )
        .unwrap();
    let kinds: Vec<String> = q
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(
        kinds
            .iter()
            .all(|k| k == "reply" || k.starts_with("judge") || k.starts_with("tournament")),
        "non-generation work was called: {kinds:?}"
    );
    let orphans: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM attempts a LEFT JOIN work w ON w.work_id=a.work_id WHERE w.work_id IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(orphans, 0);
    // Spending reconciles with the calls that were made.
    f.assert_ledger();
    handle.shutdown().await.unwrap();
}
