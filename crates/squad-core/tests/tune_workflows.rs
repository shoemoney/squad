#[path = "../../squad-agents/tests/support/fake_openrouter.rs"]
mod fake_openrouter;
use fake_openrouter::*;
use squad_agents::openrouter::OpenRouterClient;
use squad_core::room::RoomHandle;
use squad_protocol::*;
use std::{collections::BTreeMap, time::Duration};
use wiremock::{
    Mock, Request as HttpRequest, Respond, ResponseTemplate,
    matchers::{method, path},
};

struct Fixture {
    _dir: tempfile::TempDir,
    config: SquadConfig,
    fake: FakeOpenRouter,
}
impl Fixture {
    async fn scenario(count: usize, scenario: Scenario) -> Self {
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
        config.agents = (0..count)
            .map(|i| config::AgentConfig {
                nick: nick(&format!("member{i}")),
                model: format!("fixture/member{i}"),
                persona: format!("PERSONA_{i}"),
                reasoning_effort: None,
            })
            .collect();
        config.judge.model = "fixture/judge".into();
        config.budget.daily_max_usd = 20.;
        config.budget.debate_confirm_usd = 0.;
        let fake = FakeOpenRouter::start().await;
        let models=config.agents.iter().map(|a|a.model.clone()).chain(std::iter::once(config.judge.model.clone())).map(|id|serde_json::json!({"id":id,"context_length":1000000,"top_provider":{"max_completion_tokens":65536},"supported_parameters":["reasoning"],"pricing":{"prompt":"0.000001","completion":"0.000002"}})).collect::<Vec<_>>();
        fake.catalog(serde_json::json!({"data":models})).await;
        Mock::given(method("POST"))
            .and(path("/api/v1/chat/completions"))
            .respond_with(Answers {
                scenario,
                calls: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
                executions: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            })
            .mount(&fake.server)
            .await;
        Self {
            _dir: dir,
            config,
            fake,
        }
    }
    async fn open(&self, mode: OpenMode) -> RoomHandle {
        RoomHandle::open_with_client(
            self.config.clone(),
            "main",
            mode,
            OpenRouterClient::new(&self.fake.base_url(), "fake-key").unwrap(),
        )
        .await
        .unwrap()
    }
    async fn admit(&self, room: &RoomHandle, named: bool) -> RequestResult {
        self.admit_with_confirmation(room, named, true).await
    }
    async fn admit_with_confirmation(
        &self,
        room: &RoomHandle,
        named: bool,
        preconfirmed: bool,
    ) -> RequestResult {
        let ctx = ParseContext {
            roster: self.config.agents.iter().map(|a| a.nick.clone()).collect(),
            ..Default::default()
        };
        let meta = SubmissionMeta::default();
        let raw = format!(
            "$TUNE_GENERATIONS=1 @all $tune PROMPT answer clearly\n--\nprovide helpful concise answers{}",
            if named { "\n--\n@member0" } else { "" }
        );
        let parsed = parse(&raw, &meta, &ctx).unwrap();
        room.submit(
            squad_protocol::Request::from_parsed(EventId::new(), parsed, &meta, preconfirmed)
                .unwrap(),
        )
        .await
    }
    fn db(&self) -> rusqlite::Connection {
        rusqlite::Connection::open(self.config.data_dir.join("squad.db")).unwrap()
    }
    fn count(&self, kind: &str) -> i64 {
        self.db()
            .query_row("SELECT COUNT(*) FROM events WHERE kind=?", [kind], |r| {
                r.get(0)
            })
            .unwrap()
    }
    fn assert_ledger(&self) {
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
    async fn wait(&self, check: impl Fn() -> bool) {
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
fn request(config: &SquadConfig, text: &str) -> squad_protocol::Request {
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

#[derive(Clone, Copy, PartialEq)]
enum Scenario {
    Happy,
    Perfect,
    NoImprovement,
    HoldoutReject,
    LeakRepair,
    BadMember,
    Insufficient,
    FreezeFail,
    ScoreFail,
    ExecuteFail,
    HoldoutMissing,
    OneHoldoutMissing,
    OneBaselineMissing,
    Incomplete,
    EmptyGeneration,
    Slow,
    Pause(&'static str),
}
#[derive(Clone)]
struct Answers {
    scenario: Scenario,
    calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    executions: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}
fn streaming(text: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).insert_header("content-type","text/event-stream").set_body_string(format!("data: {}\n\ndata: {}\n\ndata: [DONE]\n\n",serde_json::json!({"id":"fixture-gen","model":"fixture/model","choices":[{"index":0,"delta":{"content":text}}]}),serde_json::json!({"id":"fixture-gen","choices":[{"index":0,"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":30,"completion_tokens":30,"total_tokens":60,"cost":0.0001}})))
}
fn input_from(body: &serde_json::Value) -> Option<serde_json::Value> {
    let user = body["messages"].as_array()?.last()?["content"].as_str()?;
    user.char_indices()
        .filter(|(_, c)| *c == '{')
        .find_map(|(i, _)| {
            serde_json::Deserializer::from_str(&user[i..])
                .into_iter::<serde_json::Value>()
                .next()
                .and_then(Result::ok)
                .filter(|v| v.get("step").is_some())
        })
}
impl Respond for Answers {
    fn respond(&self, r: &HttpRequest) -> ResponseTemplate {
        use serde_json::json;
        let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
        let Some(input) = input_from(&body) else {
            if body["messages"].as_array().unwrap().len() == 2
                && body["messages"][0]["content"]
                    .as_str()
                    .unwrap_or("")
                    .starts_with("PROMPT")
            {
                let execution = self
                    .executions
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let mutation = body["messages"][0]["content"]
                    .as_str()
                    .unwrap()
                    .contains("improved");
                if self.scenario == Scenario::ExecuteFail
                    || (self.scenario == Scenario::HoldoutMissing && execution >= 12)
                    || (self.scenario == Scenario::OneHoldoutMissing
                        && (12..14).contains(&execution))
                    || (self.scenario == Scenario::OneBaselineMissing && execution < 2)
                    || (self.scenario == Scenario::Incomplete && mutation)
                {
                    return ResponseTemplate::new(500);
                }
                let improved = body["messages"][0]["content"]
                    .as_str()
                    .unwrap()
                    .contains("improved");
                let response = streaming(if improved {
                    "IMPROVED output"
                } else {
                    "BASELINE output"
                });
                return if matches!(self.scenario, Scenario::Pause("execute"))
                    || (self.scenario == Scenario::Pause("holdout_execute") && execution >= 12)
                {
                    response.set_delay(Duration::from_millis(300))
                } else {
                    response
                };
            }
            let response = streaming("startup answer");
            return if self.scenario == Scenario::Pause("reply") {
                response.set_delay(Duration::from_secs(2))
            } else {
                response
            };
        };
        let stage = input["step"].as_str().unwrap();
        let answer = match stage {
            "cases" => {
                if self.scenario == Scenario::Insufficient
                    || (self.scenario == Scenario::BadMember && body["model"] == "fixture/member0")
                {
                    return streaming("{}");
                }
                json!({"cases":[{"localId":"a","kind":"typical","input":"SECRET_CASE one two three four five six seven","expect":["SECRET_EXPECT useful answer"],"mustNot":["SECRET_FORBIDDEN fabrication"]},{"localId":"b","kind":"edge","input":"SECRET_EDGE input","expect":["SECRET_EXPECT concise"],"mustNot":[]}],"criteria":[{"localId":"r","name":"Correctness","description":"SECRET_RUBRIC correct response to every requirement"},{"localId":"s","name":"Conciseness","description":"SECRET_RUBRIC avoid padding"}]})
            }
            "freeze" => {
                if self.scenario == Scenario::FreezeFail {
                    return streaming("{}");
                }
                let proposals = input["proposals"].as_array().unwrap();
                let selected: Vec<_> = proposals
                    .iter()
                    .take(3)
                    .flat_map(|p| {
                        [
                            format!("{}:a", p["label"].as_str().unwrap()),
                            format!("{}:b", p["label"].as_str().unwrap()),
                        ]
                    })
                    .collect();
                let dropped:Vec<_>=proposals.iter().skip(3).flat_map(|p|[json!({"caseRef":format!("{}:a",p["label"].as_str().unwrap()),"why":"duplicate"}),json!({"caseRef":format!("{}:b",p["label"].as_str().unwrap()),"why":"duplicate"})]).collect();
                json!({"cases":selected,"dropped":dropped,"rubric":[{"name":"Correctness","description":"SECRET_RUBRIC correct response to every requirement","sourceRefs":["P1:r"]},{"name":"Conciseness","description":"SECRET_RUBRIC avoid padding","sourceRefs":["P1:s"]}]})
            }
            "mutate" => {
                if self.scenario == Scenario::EmptyGeneration {
                    return streaming("{}");
                }
                json!({"prompt":"PROMPT improved and concise; handle uncertainty.","changes":[{"what":"Add precision","why":"Avoid incomplete answers","targets":[input["digest"]["cases"][0]["caseLabel"].clone()]}]})
            }
            "score" => {
                if self.scenario == Scenario::ScoreFail {
                    return streaming("{}");
                }
                let n = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let rows:Vec<_>=input["cases"].as_array().unwrap().iter().map(|c|{
                    let outputs:Vec<_>=c["outputs"].as_array().unwrap().iter().map(|o|{
                        let improved=o["output"].as_str().unwrap().contains("IMPROVED");let holdout=c["caseLabel"].as_str().unwrap().starts_with('H');
                        let score=if self.scenario==Scenario::Perfect || (improved&&self.scenario!=Scenario::NoImprovement&&!(holdout&&self.scenario==Scenario::HoldoutReject)){4}else{2};
                        json!({"label":o["label"],"scores":[{"criterion":"Correctness","score":score},{"criterion":"Conciseness","score":score}],"note":if self.scenario==Scenario::LeakRepair&&n==0{"SECRET_RUBRIC correct response to every requirement"}else{"Partially addresses requested behavior"}})
                    }).collect();json!({"caseLabel":c["caseLabel"],"outputs":outputs})
                }).collect();
                json!({"cases":rows})
            }
            _ => panic!("unknown stage {stage}"),
        };
        let response = streaming(&answer.to_string());
        if matches!(self.scenario,Scenario::Pause(p) if p==stage)
            || (self.scenario == Scenario::Pause("holdout_score")
                && stage == "score"
                && input["cases"][0]["caseLabel"]
                    .as_str()
                    .unwrap()
                    .starts_with('H'))
        {
            response.set_delay(Duration::from_millis(300))
        } else if self.scenario == Scenario::Slow && stage == "cases" {
            response.set_delay(Duration::from_secs(2))
        } else {
            response
        }
    }
}
async fn run(scenario: Scenario) -> (Fixture, RoomHandle, ContestId) {
    let f = Fixture::scenario(6, scenario).await;
    let room = f.open(OpenMode::Interactive).await;
    let receipt = f.admit(&room, false).await;
    assert!(
        matches!(receipt, RequestResult::Accepted { .. }),
        "{receipt:?}"
    );
    let id: String = f
        .db()
        .query_row(
            "SELECT contest_id FROM contests WHERE kind='tune'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    f.wait(|| {
        f.db()
            .query_row("SELECT active=0 FROM contests WHERE kind='tune'", [], |r| {
                r.get::<_, bool>(0)
            })
            .unwrap_or(false)
    })
    .await;
    (f, room, id.parse().unwrap())
}
#[tokio::test]
async fn complete_blind_tune_reuses_outputs_and_balances_ledger() {
    let (f, room, id) = run(Scenario::Happy).await;
    let run = squad_store::tune::load(&f.db(), id).unwrap();
    assert_eq!(run.report.as_ref().unwrap().reason, "tuned", "{run:#?}");
    assert_eq!(run.candidates.len(), 4);
    assert_eq!(run.sheets.len(), 3);
    assert_eq!(run.setup.generations, 1);
    let requests = f.fake.server.received_requests().await.unwrap();
    let mut pairs = std::collections::BTreeSet::new();
    for r in requests.iter().filter(|r| r.method == "POST") {
        let b: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
        if let Some(input) = input_from(&b) {
            let text = b.to_string();
            match input["step"].as_str().unwrap() {
                "mutate" => {
                    for forbidden in [
                        "SECRET_CASE",
                        "SECRET_EXPECT",
                        "SECRET_FORBIDDEN",
                        "SECRET_RUBRIC",
                        "BASELINE output",
                        "IMPROVED output",
                        "H1",
                        "PERSONA_",
                    ] {
                        assert!(!text.contains(forbidden), "leak {forbidden}");
                    }
                }
                "score" => {
                    for forbidden in ["PROMPT", "member0", "PERSONA_", "generation", "champion"] {
                        assert!(
                            !input.to_string().contains(forbidden),
                            "score leak {forbidden}"
                        );
                    }
                }
                _ => {}
            }
        } else if b["messages"][0]["content"]
            .as_str()
            .unwrap_or("")
            .starts_with("PROMPT")
        {
            assert_eq!(b["messages"].as_array().unwrap().len(), 2);
            // 1,024 visible tokens plus the judge binding's High reasoning allowance
            // (the fixture models support reasoning).
            assert_eq!(b["max_tokens"], 1024 + 8192);
            assert!(b.get("response_format").is_none());
            // Cases may have equal input text; the durable pair key, not input
            // text, is what must never repeat.
        }
    }
    for e in squad_store::tune::executions(&f.db(), id).unwrap() {
        assert!(pairs.insert((e.candidate_id, e.case_label)));
    }
    let calls:i64=f.db().query_row("SELECT COUNT(*) FROM attempts a JOIN work w USING(work_id) WHERE w.kind='tune.execute'",[],|r|r.get(0)).unwrap();
    assert_eq!(calls, pairs.len() as i64);
    assert!(calls <= 21);
    let export = room.export(ExportScope::Tune, Some(id)).await.unwrap();
    assert!(export.contains("Tuned:"));
    assert!(export.contains("Reusable suite"));
    assert!(export.contains("SECRET_CASE"));
    assert!(export.contains("maps"));
    f.assert_ledger();
    let outstanding: (i64, i64) = f.db().query_row(
        "SELECT COALESCE(SUM(reserved_micros),0),COALESCE(SUM(unknown_calls),0) FROM spend_daily",
        [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert_eq!(outstanding, (0, 0));
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn early_ends_and_failures_are_explicit() {
    for (scenario, reason) in [
        (Scenario::Perfect, "nothing_to_fix"),
        (Scenario::NoImprovement, "no_improvement"),
        (Scenario::HoldoutReject, "original_kept"),
        (Scenario::Insufficient, "insufficient_cases"),
        (Scenario::FreezeFail, "judge_failed"),
        (Scenario::ScoreFail, "judge_failed"),
        (Scenario::ExecuteFail, "insufficient_cases"),
        (Scenario::HoldoutMissing, "holdout_unavailable"),
        (Scenario::Incomplete, "no_improvement"),
        (Scenario::EmptyGeneration, "no_improvement"),
    ] {
        let (f, room, id) = run(scenario).await;
        let run = squad_store::tune::load(&f.db(), id).unwrap();
        assert_eq!(run.report.as_ref().unwrap().reason, reason);
        if scenario == Scenario::HoldoutMissing {
            let report = squad_core::export::tune::report(&run);
            for c in &run.suite.as_ref().unwrap().holdout {
                assert!(report.contains(&format!("Dropped held-out case {}", c.label)));
            }
        }

        if matches!(scenario, Scenario::Perfect | Scenario::NoImprovement) {
            assert!(run.holdout.is_none());
        }
        f.assert_ledger();
        room.shutdown().await.unwrap();
    }
}
#[tokio::test]
async fn invalid_notes_repair_once_and_skipped_member_keeps_quorum() {
    for scenario in [Scenario::LeakRepair, Scenario::BadMember] {
        let (f, room, id) = run(scenario).await;
        let run = squad_store::tune::load(&f.db(), id).unwrap();
        assert_eq!(run.report.as_ref().unwrap().reason, "tuned");
        if scenario == Scenario::LeakRepair {
            let repairs: i64 = f
                .db()
                .query_row("SELECT COUNT(*) FROM mode_repairs", [], |r| r.get(0))
                .unwrap();
            assert_eq!(repairs, 1);
        } else {
            assert_eq!(run.proposals.len(), 5);
            assert_eq!(run.skipped.len(), 1);
            assert!(
                squad_core::export::tune::report(&run)
                    .contains("Skipped member @member0 at tune.cases")
            );
        }
        room.shutdown().await.unwrap();
    }
}
#[tokio::test]
async fn named_executor_is_removed_and_stop_fences_work() {
    let f = Fixture::scenario(4, Scenario::Slow).await;
    let room = f.open(OpenMode::Interactive).await;
    let receipt = f.admit(&room, true).await;
    assert!(
        matches!(receipt, RequestResult::Accepted { .. }),
        "{receipt:?}"
    );
    f.wait(|| f.count("work.state") > 0).await;
    let id: String = f
        .db()
        .query_row(
            "SELECT contest_id FROM contests WHERE kind='tune'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let run = squad_store::tune::load(&f.db(), id.parse().unwrap()).unwrap();
    assert_eq!(run.setup.participants.len(), 3);
    assert!(!run.setup.participants.contains(&nick("member0")));
    assert_eq!(run.setup.executor.model.as_str(), "fixture/member0");
    let held = room
        .submit(request(&f.config, "@member1 follow up after tune"))
        .await;
    assert!(matches!(held, RequestResult::Accepted { .. }));
    assert!(f.db().query_row("SELECT EXISTS(SELECT 1 FROM work WHERE kind='reply' AND held_by_contest_id=? AND runnable=0)",[id.clone()],|r|r.get::<_,bool>(0)).unwrap());
    let _ = room.submit(request(&f.config, "/stop")).await;
    f.wait(|| {
        f.db()
            .query_row("SELECT active=0 FROM contests", [], |r| r.get::<_, bool>(0))
            .unwrap()
    })
    .await;
    assert_eq!(
        squad_core::router::tune::load_state(&f.db(), id.parse().unwrap())
            .unwrap()
            .state,
        TuneStatus::Stopped
    );
    room.shutdown().await.unwrap();
    f.assert_ledger();
}

#[tokio::test]
async fn restart_keeps_seed_suite_and_accepted_execution_pairs() {
    for (phase, state) in [
        ("cases", "BUILDING_CASES"),
        ("freeze", "FREEZING"),
        ("execute", "EXECUTING"),
        ("score", "SCORING"),
        ("mutate", "MUTATING"),
        ("holdout_execute", "HOLDOUT_EXECUTING"),
        ("holdout_score", "HOLDOUT_SCORING"),
    ] {
        let f = Fixture::scenario(6, Scenario::Pause(phase)).await;
        let room = f.open(OpenMode::Interactive).await;
        f.admit(&room, false).await;
        f.wait(|| {
            f.db()
                .query_row(
                    "SELECT json_extract(state_json,'$.state')=? FROM contests",
                    [state],
                    |r| r.get::<_, bool>(0),
                )
                .unwrap_or(false)
        })
        .await;
        let id: String = f
            .db()
            .query_row("SELECT contest_id FROM contests", [], |r| r.get(0))
            .unwrap();
        let id: ContestId = id.parse().unwrap();
        let before = squad_store::tune::load(&f.db(), id).unwrap();
        let accepted = squad_store::tune::executions(&f.db(), id).unwrap();
        let completed: Vec<String> = f
            .db()
            .prepare("SELECT work_id FROM work WHERE status='completed' AND kind LIKE 'tune.%'")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        room.shutdown().await.unwrap();
        let room = f.open(OpenMode::Interactive).await;
        f.wait(|| {
            f.db()
                .query_row("SELECT active=0 FROM contests", [], |r| r.get::<_, bool>(0))
                .unwrap_or(false)
        })
        .await;
        let after = squad_store::tune::load(&f.db(), id).unwrap();
        assert_eq!(before.setup, after.setup);
        for sheet in &before.sheets {
            let restored = after
                .sheets
                .iter()
                .find(|s| s.generation == sheet.generation && s.holdout == sheet.holdout)
                .unwrap();
            assert_eq!(
                serde_json::to_value(sheet).unwrap(),
                serde_json::to_value(restored).unwrap()
            );
        }
        for work in completed {
            let attempts: i64 = f
                .db()
                .query_row(
                    "SELECT COUNT(*) FROM attempts WHERE work_id=?",
                    [work],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(attempts, 1, "accepted work was repeated");
        }
        if let Some(suite) = before.suite {
            assert_eq!(suite, after.suite.unwrap());
        }
        for e in accepted {
            assert_eq!(
                squad_store::tune::execution(&f.db(), id, &e.candidate_id, &e.case_label).unwrap(),
                Some(e)
            );
        }
        f.assert_ledger();
        room.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn stop_at_each_paid_phase_fences_results_and_releases_unstarted_reservations() {
    for (phase, state) in [
        ("cases", "BUILDING_CASES"),
        ("freeze", "FREEZING"),
        ("execute", "EXECUTING"),
        ("score", "SCORING"),
        ("mutate", "MUTATING"),
        ("holdout_execute", "HOLDOUT_EXECUTING"),
        ("holdout_score", "HOLDOUT_SCORING"),
    ] {
        let f = Fixture::scenario(6, Scenario::Pause(phase)).await;
        let room = f.open(OpenMode::Interactive).await;
        f.admit(&room, false).await;
        f.wait(|| {
            f.db()
                .query_row(
                    "SELECT json_extract(state_json,'$.state')=? FROM contests",
                    [state],
                    |r| r.get::<_, bool>(0),
                )
                .unwrap_or(false)
        })
        .await;
        assert!(matches!(
            room.submit(request(&f.config, "/stop")).await,
            RequestResult::Accepted { .. }
        ));
        room.shutdown().await.unwrap();
        let id: String = f
            .db()
            .query_row("SELECT contest_id FROM contests", [], |r| r.get(0))
            .unwrap();
        let id: ContestId = id.parse().unwrap();
        let state = squad_core::router::tune::load_state(&f.db(), id).unwrap();
        assert_eq!(state.state, TuneStatus::Stopped);
        let accepted = squad_store::tune::executions(&f.db(), id).unwrap();
        let run = squad_store::tune::load(&f.db(), id).unwrap();
        let room = f.open(OpenMode::Interactive).await;
        assert_eq!(
            squad_core::router::tune::load_state(&f.db(), id).unwrap(),
            state
        );
        assert_eq!(
            squad_store::tune::executions(&f.db(), id).unwrap(),
            accepted
        );
        assert_eq!(
            squad_store::tune::load(&f.db(), id).unwrap().setup,
            run.setup
        );
        assert!(!f.db().query_row("SELECT EXISTS(SELECT 1 FROM work WHERE kind LIKE 'tune.%' AND status IN ('queued','running'))", [], |r| r.get::<_, bool>(0)).unwrap());
        // A cancelled in-flight call may still be billed. Keep unknown-attempt
        // holds, but release every unstarted reservation and conserve the ledger.
        assert!(
            !f.db()
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM reservations WHERE unresolved=0)",
                    [],
                    |r| r.get::<_, bool>(0)
                )
                .unwrap()
        );
        f.assert_ledger();
        room.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn report_commit_is_atomic_and_terminal_restart_makes_no_provider_call() {
    let (f, room, id) = run(Scenario::Happy).await;
    let before = room.export(ExportScope::Tune, Some(id)).await.unwrap();
    room.shutdown().await.unwrap();
    let calls = f
        .fake
        .server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.method == "POST")
        .count();
    // REPORTING and COMPLETE share the final transaction. There is no durable
    // gap in which a report exists while its terminal state is missing.
    let reports: i64 = f
        .db()
        .query_row(
            "SELECT COUNT(*) FROM events WHERE kind='contest.report' AND json_type(payload,'$.data.tuneArchive')='object'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(reports, 1);
    let state = squad_core::router::tune::load_state(&f.db(), id).unwrap();
    assert_eq!(state.state, TuneStatus::Complete);
    let room = f.open(OpenMode::Interactive).await;
    assert_eq!(
        room.export(ExportScope::Tune, Some(id)).await.unwrap(),
        before
    );
    assert_eq!(
        squad_core::router::tune::load_state(&f.db(), id).unwrap(),
        state
    );
    room.shutdown().await.unwrap();
    assert_eq!(
        f.fake
            .server
            .received_requests()
            .await
            .unwrap()
            .into_iter()
            .filter(|r| r.method == "POST")
            .count(),
        calls
    );
    f.assert_ledger();
}

#[tokio::test]
async fn named_executor_completes_without_proposing_or_mutating() {
    let f = Fixture::scenario(4, Scenario::Happy).await;
    let room = f.open(OpenMode::Interactive).await;
    assert!(matches!(
        f.admit(&room, true).await,
        RequestResult::Accepted { .. }
    ));
    f.wait(|| {
        f.db()
            .query_row("SELECT active=0 FROM contests", [], |r| r.get::<_, bool>(0))
            .unwrap_or(false)
    })
    .await;
    let id: String = f
        .db()
        .query_row("SELECT contest_id FROM contests", [], |r| r.get(0))
        .unwrap();
    let run = squad_store::tune::load(&f.db(), id.parse().unwrap()).unwrap();
    assert_eq!(run.report.as_ref().unwrap().reason, "tuned");
    assert_eq!(run.proposals.len(), 3);
    assert!(!run.proposals.contains_key(&nick("member0")));
    assert!(
        run.candidates
            .iter()
            .all(|c| c.author.as_ref() != Some(&nick("member0")))
    );
    for r in f
        .fake
        .server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.method == "POST")
    {
        let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
        if input_from(&body).is_some() {
            assert_ne!(body["model"], "fixture/member0");
        } else if body["messages"][0]["content"]
            .as_str()
            .unwrap_or("")
            .starts_with("PROMPT")
        {
            assert_eq!(body["model"], "fixture/member0");
        }
    }
    f.assert_ledger();
    room.shutdown().await.unwrap();
}

#[tokio::test]
async fn preflight_restart_preserves_setup_and_can_confirm_or_stop_without_calls() {
    for confirm in [false, true] {
        let f = Fixture::scenario(6, Scenario::Happy).await;
        let room = f.open(OpenMode::Interactive).await;
        let RequestResult::PendingConfirmation {
            preflight: PendingPreflight::Contest(plan),
            ..
        } = f.admit_with_confirmation(&room, false, false).await
        else {
            panic!("missing preflight")
        };
        assert_eq!(
            squad_core::router::tune::load_state(&f.db(), plan.contest_id)
                .unwrap()
                .state,
            TuneStatus::Preflight
        );
        let before = squad_store::tune::load(&f.db(), plan.contest_id).unwrap();
        room.shutdown().await.unwrap();
        let room = f.open(OpenMode::Interactive).await;
        assert_eq!(
            squad_store::tune::load(&f.db(), plan.contest_id)
                .unwrap()
                .setup,
            before.setup
        );
        assert!(
            f.fake
                .server
                .received_requests()
                .await
                .unwrap()
                .iter()
                .all(|r| r.method != "POST")
        );
        let request = if confirm {
            squad_protocol::Request::ContestConfirm {
                request_id: EventId::new(),
                contest_id: plan.contest_id,
                plan_id: plan.plan_id,
            }
        } else {
            request(&f.config, "/stop")
        };
        assert!(matches!(
            room.submit(request).await,
            RequestResult::Accepted { .. }
        ));
        f.wait(|| {
            f.db()
                .query_row("SELECT active=0 FROM contests", [], |r| r.get::<_, bool>(0))
                .unwrap_or(false)
        })
        .await;
        assert_eq!(
            squad_core::router::tune::load_state(&f.db(), plan.contest_id)
                .unwrap()
                .state,
            if confirm {
                TuneStatus::Complete
            } else {
                TuneStatus::Stopped
            }
        );
        assert_eq!(
            squad_store::tune::load(&f.db(), plan.contest_id)
                .unwrap()
                .setup,
            before.setup
        );
        f.assert_ledger();
        room.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn one_shot_default_launcher_drives_tune_to_report() {
    let f = Fixture::scenario(6, Scenario::Happy).await;
    let room = f.open(OpenMode::OneShot).await;
    assert!(matches!(
        f.admit(&room, false).await,
        RequestResult::Accepted { .. }
    ));
    f.wait(|| {
        f.db()
            .query_row("SELECT active=0 FROM contests", [], |r| r.get::<_, bool>(0))
            .unwrap_or(false)
    })
    .await;
    let id: String = f
        .db()
        .query_row("SELECT contest_id FROM contests", [], |r| r.get(0))
        .unwrap();
    let run = squad_store::tune::load(&f.db(), id.parse().unwrap()).unwrap();
    assert_eq!(run.report.unwrap().reason, "tuned");
    assert!(
        room.export(ExportScope::Tune, Some(id.parse().unwrap()))
            .await
            .unwrap()
            .contains("Reusable suite")
    );
    f.assert_ledger();
    room.shutdown().await.unwrap();
}

#[tokio::test]
async fn missing_case_is_dropped_consistently_without_discarding_the_pass() {
    for scenario in [Scenario::OneBaselineMissing, Scenario::OneHoldoutMissing] {
        let (f, room, id) = run(scenario).await;
        let run = squad_store::tune::load(&f.db(), id).unwrap();
        assert_eq!(run.report.as_ref().unwrap().reason, "tuned");
        let suite = run.suite.as_ref().unwrap();
        if scenario == Scenario::OneBaselineMissing {
            assert_eq!(suite.train.len(), 2);
            assert!(
                suite
                    .dropped
                    .iter()
                    .any(|c| c.why == "baseline execution missing")
            );
            assert!(squad_core::export::tune::report(&run).contains("baseline execution missing"));
        } else {
            let holdout = run.holdout.as_ref().unwrap();
            assert_eq!(holdout.dropped, ["H1"]);
            let sheet = &run.sheets.iter().find(|s| s.holdout).unwrap().sheet;
            assert_eq!(sheet.cases.len(), 2);
            assert!(sheet.cases.iter().all(|c| c.case_label != "H1"));
            assert!(
                sheet
                    .cases
                    .iter()
                    .all(|c| c.outputs.len() == holdout.finalists.len())
            );
            assert!(squad_core::export::tune::report(&run).contains("Dropped held-out case H1"));
        }
        f.assert_ledger();
        room.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn waiting_for_prior_member_work_survives_restart_and_can_stop() {
    for resume in [false, true] {
        let f = Fixture::scenario(6, Scenario::Pause("reply")).await;
        let room = f.open(OpenMode::Interactive).await;
        assert!(matches!(
            room.submit(request(&f.config, "@member0 earlier question"))
                .await,
            RequestResult::Accepted { .. }
        ));
        f.wait(|| {
            f.db()
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM work WHERE kind='reply' AND status='running')",
                    [],
                    |r| r.get::<_, bool>(0),
                )
                .unwrap()
        })
        .await;
        assert!(matches!(
            f.admit(&room, false).await,
            RequestResult::Accepted { .. }
        ));
        let id: String = f
            .db()
            .query_row("SELECT contest_id FROM contests", [], |r| r.get(0))
            .unwrap();
        let id: ContestId = id.parse().unwrap();
        assert_eq!(
            squad_core::router::tune::load_state(&f.db(), id)
                .unwrap()
                .state,
            TuneStatus::WaitingForParticipants
        );
        let setup = squad_store::tune::load(&f.db(), id).unwrap().setup;
        let calls: i64 = f.db().query_row("SELECT COUNT(*) FROM attempts a JOIN work w USING(work_id) WHERE w.kind LIKE 'tune.%'", [], |r| r.get(0)).unwrap();
        assert_eq!(calls, 0);
        room.shutdown().await.unwrap();
        let room = f.open(OpenMode::Interactive).await;
        if !resume {
            assert!(matches!(
                room.submit(request(&f.config, "/stop")).await,
                RequestResult::Accepted { .. }
            ));
        }
        f.wait(|| {
            f.db()
                .query_row("SELECT active=0 FROM contests", [], |r| r.get::<_, bool>(0))
                .unwrap()
        })
        .await;
        assert_eq!(
            squad_core::router::tune::load_state(&f.db(), id)
                .unwrap()
                .state,
            if resume {
                TuneStatus::Complete
            } else {
                TuneStatus::Stopped
            }
        );
        assert_eq!(squad_store::tune::load(&f.db(), id).unwrap().setup, setup);
        f.assert_ledger();
        room.shutdown().await.unwrap();
    }
}
