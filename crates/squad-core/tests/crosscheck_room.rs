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
    async fn new() -> Self {
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
            .respond_with(CrosscheckAnswers)
            .with_priority(10)
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
#[derive(Clone)]
struct CrosscheckAnswers;
impl Respond for CrosscheckAnswers {
    fn respond(&self, r: &HttpRequest) -> ResponseTemplate {
        use serde_json::json;
        let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
        let prompt = body["messages"][1]["content"].as_str().unwrap();
        let data: serde_json::Value = prompt
            .lines()
            .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
            .find(|v| v.get("task").is_some())
            .unwrap();
        let result = match data["task"].as_str().unwrap() {
            "extract_goal" => {
                json!({"goal":"Keep the service available","constraints":[],"uncertainties":[]})
            }
            "inventory" => {
                json!({"items":(0..3).map(|i|json!({"localId":format!("local{i}"),"item":format!("{} case {i}",data["lens"]),"rationale":"An explicit obligation"})).collect::<Vec<_>>()})
            }
            "normalize_inventory" => {
                let items=data["inventories"].as_array().unwrap().iter().enumerate().flat_map(|(i,inv)|inv[2]["items"].as_array().unwrap().iter().enumerate().map(move |(j,item)|json!({"id":format!("n{i}-{j}"),"lens":inv[1],"item":item["item"],"sourceItemIds":[{"inventoryEventId":inv[0],"localId":item["localId"]}]}))).collect::<Vec<_>>();
                let typed =
                    serde_json::from_value::<Vec<CrosscheckNormalizedItem>>(json!(items)).unwrap();
                let keys = squad_core::orchestrator::crosscheck::candidates(&typed)
                    .unwrap()
                    .into_iter()
                    .map(|(k, _)| k)
                    .collect::<Vec<_>>();
                json!({"items":items,"priorityOrder":keys})
            }
            "review" => {
                json!({"cells":data["cells"].as_array().unwrap().iter().map(|c|json!({"cellId":c["cellId"],"status":"gap","citation":null,"reason":"Missing handling","proposedFix":"Add a concrete control"})).collect::<Vec<_>>()})
            }
            "audit" => {
                json!({"audits":data["samples"].as_array().unwrap().iter().map(|c|json!({"cellId":c["cellId"],"verdict":"upheld","reason":"Supported","citation":null})).collect::<Vec<_>>(),"missedCase":{"case":"An additional scenario","reason":"Independent test","proposedFix":"Add coverage"}})
            }
            "rule" => {
                json!({"cells":data["grid"]["cells"].as_array().unwrap().iter().map(|c|json!({"cellId":c["cellId"],"status":"gap","citation":null,"reason":"Missing handling","proposedFix":"Add a concrete control"})).collect::<Vec<_>>(),"additions":data["audits"].as_array().unwrap().iter().map(|a|json!({"auditWorkId":a["workId"],"decision":{"kind":"rejected"},"reason":"Already represented"})).collect::<Vec<_>>(),"residualRisks":[]})
            }
            other => panic!("unknown {other}"),
        };
        structured_judge(result)
    }
}
#[tokio::test]
async fn crosscheck_full_room_16_calls_and_replay() {
    let _ = tracing_subscriber::fmt().try_init();
    let f = Fixture::new().await;
    let room = f.open(OpenMode::Interactive).await;
    let result=room.submit(request(&f.config,"@all $crosscheck SECRET_PLAN_CANARY Maintain service.\n--\nKeep the service available; no extra staff.")).await;
    assert!(
        matches!(result, RequestResult::Accepted { .. }),
        "{result:?}"
    );
    f.wait(|| f.count("crosscheck.report") == 1).await;
    assert_eq!(f.count("crosscheck.inventory"), 6);
    assert_eq!(f.count("crosscheck.review"), 6);
    assert_eq!(f.count("crosscheck.audit"), 2);
    assert_eq!(
        f.db()
            .query_row("SELECT COUNT(*) FROM crosscheck_cells", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        153
    );
    let bodies = f.fake.captured_bodies().await;
    assert_eq!(bodies.len(), 16);
    for b in &bodies {
        let prompt = b["messages"][1]["content"].as_str().unwrap();
        if prompt.contains("Build an independent inventory") {
            assert!(!prompt.contains("SECRET_PLAN_CANARY"));
        }
        if prompt.contains("independent fresh audit") {
            assert!(!prompt.contains("originators"));
        }
    }
    room.shutdown().await.unwrap();
    let room = f.open(OpenMode::Interactive).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(f.fake.captured_bodies().await.len(), 16);
    f.assert_ledger();
    let billing: (i64, i64) = f
        .db()
        .query_row(
            "SELECT COALESCE(SUM(spent_micros),0),COALESCE(SUM(unknown_calls),0) FROM spend_daily",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    room.submit(Request::SetVars {
        request_id: EventId::new(),
        updates: vec![VariableUpdate {
            name: VariableName::CensusPasses,
            value: 4,
        }],
    })
    .await;
    let cleared = room
        .submit(Request::Clear {
            request_id: EventId::new(),
        })
        .await;
    assert!(
        matches!(cleared, RequestResult::Accepted { .. }),
        "{cleared:?}"
    );
    for table in [
        "crosscheck_cells",
        "crosscheck_audit_samples",
        "contest_private_maps",
        "contest_steps",
    ] {
        assert_eq!(
            f.db()
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    let retained: (i64, i64) = f
        .db()
        .query_row(
            "SELECT COALESCE(SUM(spent_micros),0),COALESCE(SUM(unknown_calls),0) FROM spend_daily",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(retained, billing);
    assert_eq!(
        f.db()
            .query_row(
                "SELECT value FROM vars WHERE name='CENSUS_PASSES'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        2
    );
    f.assert_ledger();
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn crosscheck_extraction_is_seventeenth_call_and_exports_acceptance() {
    let f = Fixture::new().await;
    let room = f.open(OpenMode::Interactive).await;
    let result = room
        .submit(request(
            &f.config,
            "@all $crosscheck Maintain service with the existing staff.",
        ))
        .await;
    assert!(
        matches!(result, RequestResult::Accepted { .. }),
        "{result:?}"
    );
    f.wait(|| f.count("crosscheck.report") == 1).await;
    assert_eq!(f.fake.captured_bodies().await.len(), 17);
    assert_eq!(f.count("crosscheck.goal"), 1);
    let db = f.db();
    let (id, raw): (String, String) = db
        .query_row("SELECT contest_id,state_json FROM contests", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    let state: CrosscheckState = serde_json::from_str(&raw).unwrap();
    let reference = |kind: &str| {
        db.query_row(
            "SELECT id FROM events WHERE kind=? ORDER BY seq DESC LIMIT 1",
            [kind],
            |r| r.get::<_, String>(0),
        )
        .unwrap()
        .parse::<EventId>()
        .unwrap()
    };
    let cell = db
        .query_row("SELECT cell_id FROM crosscheck_cells LIMIT 1", [], |r| {
            r.get::<_, String>(0)
        })
        .unwrap()
        .parse()
        .unwrap();
    let accepted = room
        .submit(Request::CrosscheckAccept {
            request_id: EventId::new(),
            contest_id: id.parse().unwrap(),
            expected_state_version: state.state_version,
            plan_event_id: reference("crosscheck.setup"),
            grid_event_id: reference("crosscheck.grid"),
            cell_ids: vec![cell],
            reason: Text::new("I accept this specific residual risk").unwrap(),
        })
        .await;
    assert!(
        matches!(accepted, RequestResult::Accepted { .. }),
        "{accepted:?}"
    );
    assert_eq!(f.count("crosscheck.acceptance"), 1);
    let text = room
        .export(ExportScope::Crosscheck, Some(state.contest_id))
        .await
        .unwrap();
    assert!(text.contains("Explicit human risk acceptance"));
    assert!(text.contains("Bounded stop: false"));
    assert!(!text.contains("tie_break_seed"));
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn crosscheck_failed_reviewer_retains_open_cells() {
    use wiremock::matchers::{body_partial_json, body_string_contains};
    let f = Fixture::new().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .and(body_partial_json(
            serde_json::json!({"model":"fixture/member0"}),
        ))
        .and(body_string_contains("review your assigned cells"))
        .respond_with(
            ResponseTemplate::new(500)
                .set_body_json(serde_json::json!({"error":{"message":"upstream exploded"}})),
        )
        .mount(&f.fake.server)
        .await;
    let room = f.open(OpenMode::Interactive).await;
    room.submit(request(
        &f.config,
        "@all $crosscheck Maintain service.\n--\nKeep service available.",
    ))
    .await;
    f.wait(|| f.count("crosscheck.report") == 1).await;
    assert_eq!(f.count("crosscheck.review"), 5);
    assert_provider_failure(&f.db(), 500, Some("upstream exploded"));
    let raw: String = f
        .db()
        .query_row(
            "SELECT payload FROM events WHERE kind='crosscheck.report'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let report: CrosscheckReport = serde_json::from_str(&raw).unwrap();
    assert_eq!(report.ledger.len(), 153);
    assert_eq!(report.failed_reviewers, vec![nick("member0")]);
    assert!(!report.bounded_stop);
    let pending: i64 = f
        .db()
        .query_row(
            "SELECT COUNT(*) FROM crosscheck_cells WHERE reviewer='member0' AND status='pending'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(pending > 0);
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn crosscheck_daily_max_refuses_before_provider_calls() {
    let mut f = Fixture::new().await;
    f.config.budget.daily_max_usd = 0.;
    let room = f.open(OpenMode::Interactive).await;
    let result = room
        .submit(request(
            &f.config,
            "@all $crosscheck Maintain service.\n--\nKeep service available.",
        ))
        .await;
    assert!(
        matches!(
            result,
            RequestResult::Rejected {
                error: ErrorPayload {
                    code: ErrorCode::DAILY_MAX_REACHED,
                    ..
                },
                ..
            }
        ),
        "{result:?}"
    );
    assert!(f.fake.captured_bodies().await.is_empty());
    room.shutdown().await.unwrap();
}
#[derive(Clone)]
struct RepairOnce(std::sync::Arc<std::sync::atomic::AtomicUsize>);
impl Respond for RepairOnce {
    fn respond(&self, r: &HttpRequest) -> ResponseTemplate {
        if self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
            structured_judge(serde_json::json!({"items":[],"priorityOrder":[]}))
        } else {
            CrosscheckAnswers.respond(r)
        }
    }
}
#[tokio::test]
async fn crosscheck_judge_repairs_once_and_exhaustion_fails() {
    use wiremock::matchers::body_string_contains;
    for repair in [true, false] {
        let f = Fixture::new().await;
        let matcher = Mock::given(method("POST"))
            .and(path("/api/v1/chat/completions"))
            .and(body_string_contains("normalize independent inventories"));
        if repair {
            matcher
                .respond_with(RepairOnce(Default::default()))
                .mount(&f.fake.server)
                .await;
        } else {
            matcher
                .respond_with(structured_judge(serde_json::json!({"bad":true})))
                .mount(&f.fake.server)
                .await;
        }
        let room = f.open(OpenMode::Interactive).await;
        room.submit(request(
            &f.config,
            "@all $crosscheck Maintain service.\n--\nKeep service available.",
        ))
        .await;
        f.wait(|| {
            f.db()
                .query_row("SELECT active=0 FROM contests", [], |r| r.get::<_, bool>(0))
                .unwrap_or(false)
        })
        .await;
        assert_eq!(f.count("crosscheck.report"), i64::from(repair));
        assert_eq!(
            f.db()
                .query_row("SELECT COUNT(*) FROM judge_repairs", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        room.shutdown().await.unwrap();
    }
}
#[tokio::test]
async fn crosscheck_stop_fences_late_judge_and_clear_removes_mode_records() {
    use wiremock::matchers::body_string_contains;
    let f = Fixture::new().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .and(body_string_contains("normalize independent inventories"))
        .respond_with(
            structured_judge(serde_json::json!({"items":[],"priorityOrder":[]}))
                .set_delay(Duration::from_secs(2)),
        )
        .mount(&f.fake.server)
        .await;
    let room = f.open(OpenMode::Interactive).await;
    room.submit(request(
        &f.config,
        "@all $crosscheck Maintain service.\n--\nKeep service available.",
    ))
    .await;
    f.wait(|| f.count("crosscheck.inventory") == 6).await;
    let result = room
        .submit(Request::Stop {
            request_id: EventId::new(),
            target: None,
        })
        .await;
    assert!(
        matches!(result, RequestResult::Accepted { .. }),
        "{result:?}"
    );
    assert_eq!(f.count("crosscheck.report"), 0);
    let state: String = f
        .db()
        .query_row(
            "SELECT json_extract(state_json,'$.status') FROM contests",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(state, "STOPPED");
    let result = room
        .submit(Request::Clear {
            request_id: EventId::new(),
        })
        .await;
    assert!(
        matches!(result, RequestResult::Accepted { .. }),
        "{result:?}"
    );
    assert_eq!(f.count("crosscheck.inventory"), 0);
    assert_eq!(
        f.db()
            .query_row("SELECT COUNT(*) FROM crosscheck_cells", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn crosscheck_restart_preserves_samples_and_never_repeats_accepted_steps() {
    use wiremock::matchers::body_string_contains;
    #[derive(Clone)]
    struct SlowAudit;
    impl Respond for SlowAudit {
        fn respond(&self, r: &HttpRequest) -> ResponseTemplate {
            CrosscheckAnswers
                .respond(r)
                .set_delay(Duration::from_millis(600))
        }
    }
    let f = Fixture::new().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .and(body_string_contains("independent fresh audit"))
        .respond_with(SlowAudit)
        .mount(&f.fake.server)
        .await;
    let room = f.open(OpenMode::Interactive).await;
    room.submit(request(
        &f.config,
        "@all $crosscheck Maintain service.\n--\nKeep service available.",
    ))
    .await;
    f.wait(|| {
        f.db()
            .query_row("SELECT COUNT(*) FROM crosscheck_audit_samples", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap()
            == 2
    })
    .await;
    let frozen: String = f
        .db()
        .query_row(
            "SELECT group_concat(auditor || sample_json) FROM crosscheck_audit_samples",
            [],
            |r| r.get(0),
        )
        .unwrap();
    room.shutdown().await.unwrap();
    let room = f.open(OpenMode::Interactive).await;
    f.wait(|| f.count("crosscheck.report") == 1).await;
    let recovered: String = f
        .db()
        .query_row(
            "SELECT group_concat(auditor || sample_json) FROM crosscheck_audit_samples",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(frozen, recovered);
    assert_eq!(f.count("crosscheck.inventory"), 6);
    assert_eq!(f.count("crosscheck.review"), 6);
    let requests = f.fake.captured_bodies().await;
    assert_eq!(
        requests
            .iter()
            .filter(|b| b.to_string().contains("Build an independent inventory"))
            .count(),
        6
    );
    assert_eq!(
        requests
            .iter()
            .filter(|b| b.to_string().contains("review your assigned cells"))
            .count(),
        6
    );
    f.assert_ledger();
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn crosscheck_real_readers_enforce_fresh_anonymous_audit_and_work_ownership() {
    use squad_store::readers::ScopedReaders;
    use wiremock::matchers::body_string_contains;
    #[derive(Clone)]
    struct SlowAudit;
    impl Respond for SlowAudit {
        fn respond(&self, r: &HttpRequest) -> ResponseTemplate {
            CrosscheckAnswers
                .respond(r)
                .set_delay(Duration::from_millis(1000))
        }
    }
    let f = Fixture::new().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .and(body_string_contains("independent fresh audit"))
        .respond_with(SlowAudit)
        .mount(&f.fake.server)
        .await;
    let room = f.open(OpenMode::Interactive).await;
    room.submit(request(
        &f.config,
        "@all $crosscheck PRIVATE_PLAN_SENTINEL Maintain service.\n--\nKeep service available.",
    ))
    .await;
    f.wait(||f.db().query_row("SELECT COUNT(*) FROM work w JOIN contests c ON c.contest_id=w.contest_id WHERE w.status='running' AND json_extract(c.state_json,'$.status')='AUDITING'",[],|r|r.get::<_,i64>(0)).unwrap()==2).await;
    let (id,member):(String,String)=f.db().query_row("SELECT w.work_id,w.target FROM work w JOIN contests c ON c.contest_id=w.contest_id WHERE w.status='running' AND json_extract(c.state_json,'$.status')='AUDITING' LIMIT 1",[],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
    let store = squad_store::Store::open(f.config.data_dir.join("squad.db")).unwrap();
    let readers = ScopedReaders::new(store, ExecutorKey::Member(nick(&member)), 1);
    let id: WorkId = id.parse().unwrap();
    let history = readers.for_work(id).await.unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(
        history[0].meta,
        HistoryMetadata::Anonymous { anonymous: True }
    );
    assert_eq!(history[0].name.as_deref(), Some("squad"));
    assert!(!history[0].content.contains("originators"));
    assert!(!history[0].content.contains("reviewer"));
    let earlier: String = f
        .db()
        .query_row(
            "SELECT work_id FROM work WHERE target=? AND status='completed' ORDER BY rowid LIMIT 1",
            [&member],
            |r| r.get(0),
        )
        .unwrap();
    assert!(matches!(
        readers.for_work(earlier.parse().unwrap()).await,
        Err(ReaderError::NotRunnable)
    ));
    let foreign: String = f
        .db()
        .query_row(
            "SELECT work_id FROM work WHERE target<>? LIMIT 1",
            [&member],
            |r| r.get(0),
        )
        .unwrap();
    assert!(matches!(
        readers
            .prompt_values_for_work(foreign.parse().unwrap())
            .await,
        Err(ReaderError::Forbidden)
    ));
    assert!(readers.get(WorkId::new()).await.unwrap().is_none());
    room.submit(Request::Stop {
        request_id: EventId::new(),
        target: None,
    })
    .await;
    assert!(readers.prompt_values_for_work(id).await.is_err());
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn crosscheck_confirmation_and_no_wait_use_shared_lifecycle() {
    let mut f = Fixture::new().await;
    f.config.budget.debate_confirm_usd = 0.;
    let room = f.open(OpenMode::OneShot).await;
    let result = room
        .submit_commit_only(request(
            &f.config,
            "@all $crosscheck Maintain service.\n--\nKeep service available.",
        ))
        .await;
    let RequestResult::PendingConfirmation {
        preflight: PendingPreflight::Contest(plan),
        ..
    } = result
    else {
        panic!("{result:?}")
    };
    assert!(f.fake.captured_bodies().await.is_empty());
    let confirmation_id = EventId::new();
    let result = room
        .submit_commit_only(Request::ContestConfirm {
            request_id: confirmation_id,
            contest_id: plan.contest_id,
            plan_id: plan.plan_id,
        })
        .await;
    assert!(
        matches!(result, RequestResult::Accepted { .. }),
        "{result:?}"
    );
    assert!(f.fake.captured_bodies().await.is_empty());
    let events = squad_store::events::page(&f.db(), None, 500).unwrap();
    assert!(events.iter().any(|event|event.base.causation_id==Some(confirmation_id) && matches!(&event.content,EnvelopePayload::ContestPreflightResult(p) if p.confirmation==Some(Confirmation::Explicit))));
    room.shutdown().await.unwrap();
    let room = f.open(OpenMode::Interactive).await;
    f.wait(|| f.count("crosscheck.report") == 1).await;
    assert_eq!(f.fake.captured_bodies().await.len(), 16);
    assert_eq!(f.count("crosscheck.acceptance"), 0);
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn crosscheck_inventory_does_not_read_a_plan_already_in_ordinary_history() {
    struct Ordinary;
    impl wiremock::Match for Ordinary {
        fn matches(&self, r: &HttpRequest) -> bool {
            let b: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
            b.get("response_format").is_none()
        }
    }
    let f = Fixture::new().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .and(Ordinary)
        .respond_with(streaming("PRIVATE_HISTORY_PLAN_CANARY", "stop"))
        .mount(&f.fake.server)
        .await;
    let room = f.open(OpenMode::Interactive).await;
    room.submit(request(&f.config, "@all PRIVATE_HISTORY_PLAN_CANARY"))
        .await;
    f.wait(|| f.count("message.final") == 6).await;
    let seq: i64 = f
        .db()
        .query_row(
            "SELECT MIN(seq) FROM events WHERE kind='message.final'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let event = squad_store::events::at(&f.db(), seq).unwrap();
    let EnvelopePayload::MessageFinal(reply) = event.content else {
        panic!("ordinary final")
    };
    let context = parser::ParseContext {
        roster: f.config.agents.iter().map(|a| a.nick.clone()).collect(),
        records: vec![parser::RecordSummary {
            event_id: event.base.id,
            message_id: Some(reply.completed.generation.message_id),
            seq: event.commit.seq,
            kind: Text::new("message.final").unwrap(),
            author: event.base.sender,
            recipients: event.base.recipients,
            content: reply.content,
            in_reply_to_message_id: Some(reply.in_reply_to_message_id),
            contest_id: None,
            case_id: None,
            plan_event_id: None,
            grid_event_id: None,
            cell_ids: vec![],
            state_version: None,
            superseded: false,
            eligible: true,
        }],
        ..Default::default()
    };
    let meta = parser::SubmissionMeta {
        quote: vec![event.base.id],
        ..Default::default()
    };
    let parsed = parse("@all $crosscheck Keep service available.", &meta, &context).unwrap();
    let command = Request::from_parsed(EventId::new(), parsed, &meta, false).unwrap();
    let result = room.submit(command).await;
    assert!(
        matches!(result, RequestResult::Accepted { .. }),
        "{result:?}"
    );
    f.wait(|| f.count("crosscheck.report") == 1).await;
    let bodies = f.fake.captured_bodies().await;
    for body in bodies
        .iter()
        .filter(|b| b.to_string().contains("Build an independent inventory"))
    {
        assert!(!body.to_string().contains("PRIVATE_HISTORY_PLAN_CANARY"));
    }
    let originals:i64=f.db().query_row("SELECT COUNT(*) FROM history WHERE content LIKE '%PRIVATE_HISTORY_PLAN_CANARY%' AND context_id NOT LIKE 'contest:%' AND context_id NOT LIKE 'judge:%'",[],|r|r.get(0)).unwrap();
    assert!(originals >= 12);
    room.shutdown().await.unwrap();
}

#[tokio::test]
async fn crosscheck_stop_preserves_accepted_partial_reviews_in_export() {
    use wiremock::matchers::{body_partial_json, body_string_contains};
    struct SlowReview;
    impl Respond for SlowReview {
        fn respond(&self, request: &HttpRequest) -> ResponseTemplate {
            CrosscheckAnswers
                .respond(request)
                .set_delay(Duration::from_secs(3))
        }
    }
    let f = Fixture::new().await;
    Mock::given(method("POST"))
        .and(body_partial_json(
            serde_json::json!({"model":"fixture/member5"}),
        ))
        .and(body_string_contains("review your assigned cells"))
        .respond_with(SlowReview)
        .mount(&f.fake.server)
        .await;
    let room = f.open(OpenMode::Interactive).await;
    room.submit(request(
        &f.config,
        "@all $crosscheck Maintain service.\n--\nKeep service available.",
    ))
    .await;
    f.wait(|| {
        f.db()
            .query_row(
                "SELECT COUNT(*) FROM mode_artifacts WHERE artifact_kind='crosscheck.cell'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .unwrap()
            == 5
    })
    .await;
    let result = room
        .submit(Request::Stop {
            request_id: EventId::new(),
            target: None,
        })
        .await;
    assert!(
        matches!(result, RequestResult::Accepted { .. }),
        "{result:?}"
    );
    assert_eq!(f.count("crosscheck.review"), 0);
    assert_eq!(f.count("crosscheck.report"), 0);
    let state: CrosscheckState = serde_json::from_str(
        &f.db()
            .query_row("SELECT state_json FROM contests", [], |r| {
                r.get::<_, String>(0)
            })
            .unwrap(),
    )
    .unwrap();
    let text = room
        .export(ExportScope::Crosscheck, Some(state.contest_id))
        .await
        .unwrap();
    assert!(text.contains("### Review · member0"), "{text}");
    assert!(text.contains("Proposed fix"), "{text}");
    room.shutdown().await.unwrap();
}

fn assert_provider_failure(db: &rusqlite::Connection, status: i64, message: Option<&str>) {
    let rows: Vec<String> = db
        .prepare("SELECT error_json FROM work WHERE json_extract(error_json,'$.providerStatus')=?")
        .unwrap()
        .query_map([status], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert!(
        !rows.is_empty(),
        "no work row carries providerStatus {status}"
    );
    for raw in &rows {
        assert!(
            !raw.contains("Bearer") && !raw.contains("fake-key"),
            "{raw}"
        );
        if let Some(message) = message {
            assert!(raw.contains(message), "{raw}");
        }
    }
}
