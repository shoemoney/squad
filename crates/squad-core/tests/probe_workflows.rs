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
        Self::scenario(6, Scenario::Happy).await
    }
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
        config.budget.debate_confirm_usd = 100.;
        let fake = FakeOpenRouter::start().await;
        let models=config.agents.iter().map(|a|a.model.clone()).chain(std::iter::once(config.judge.model.clone())).map(|id|serde_json::json!({"id":id,"context_length":1000000,"top_provider":{"max_completion_tokens":65536},"supported_parameters":["reasoning"],"pricing":{"prompt":"0.000001","completion":"0.000002"}})).collect::<Vec<_>>();
        fake.catalog(serde_json::json!({"data":models})).await;
        Mock::given(method("POST"))
            .and(path("/api/v1/chat/completions"))
            .respond_with(Answers {
                scenario,
                report_calls: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
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
#[derive(Clone, Copy)]
enum Scenario {
    Happy,
    Distinct,
    SlowReport,
    SlowOneProposal,
    SkippedSlowReport,
    MemberFailure,
    AllFail,
    ReportRepair,
    ReportExhaustion,
    SlowPropose,
    SlowPrediction,
}
#[derive(Clone)]
struct Answers {
    scenario: Scenario,
    report_calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}
impl Respond for Answers {
    fn respond(&self, r: &HttpRequest) -> ResponseTemplate {
        let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
        let user = body["messages"].as_array().unwrap().last().unwrap()["content"]
            .as_str()
            .unwrap();
        let input = user
            .char_indices()
            .filter(|(_, c)| *c == '{')
            .find_map(|(i, _)| {
                serde_json::Deserializer::from_str(&user[i..])
                    .into_iter::<serde_json::Value>()
                    .next()
                    .and_then(Result::ok)
                    .filter(|v| v.get("stage").is_some())
            });
        let Some(input) = input else {
            return streaming("ordinary answer", "stop");
        };
        let stage = input["stage"].as_str().unwrap();
        if matches!(self.scenario, Scenario::AllFail)
            || (matches!(
                self.scenario,
                Scenario::MemberFailure | Scenario::SkippedSlowReport
            ) && body["model"] == "fixture/member0")
        {
            return ResponseTemplate::new(500)
                .set_body_json(serde_json::json!({"error":{"message":"upstream exploded"}}));
        }
        if stage == "report" {
            let count = self
                .report_calls
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if matches!(self.scenario, Scenario::ReportExhaustion)
                || (matches!(self.scenario, Scenario::ReportRepair) && count == 0)
            {
                return streaming("{\"winner\":\"invented\"}", "stop");
            }
        }
        let value = match stage {
            "propose" if user.contains("propose one explanation") => {
                serde_json::json!({"explanation":(["Loose connector","Dry bearing","Resonant cover","A vibrating cable","A noisy regulator","An unstable sensor"][body["model"].as_str().unwrap().chars().last().unwrap().to_digit(10).unwrap() as usize]),"keyAssumption":"The component can move","limitations":["Unverified"]})
            }
            "propose" => {
                serde_json::json!({"check":"Read the light","procedure":"Observe without touching","measurement":"Light color","outcomes":[{"id":"red","description":"Red light"},{"id":"green","description":"Green light"}],"effort":"One minute","safetyNotes":["Do not open the case"]})
            }
            "predict" if matches!(self.scenario, Scenario::Distinct) => {
                let index = body["model"]
                    .as_str()
                    .unwrap()
                    .chars()
                    .last()
                    .unwrap()
                    .to_digit(10)
                    .unwrap();
                serde_json::json!({"predictions":input["checks"].as_array().unwrap().iter().map(|c|{let ids=c["data"]["outcomes"].as_array().unwrap().iter().enumerate().filter(|(i,_)|match index/2 {0=>*i==0,1=>*i==1,_=>true}).map(|(_,v)|v["id"].clone()).collect::<Vec<_>>();serde_json::json!({"checkId":c["id"],"predictable":true,"expectedOutcomeIds":ids,"expectedObservation":"Record indicator color","reason":"Hypothesis predicts these outcomes"})}).collect::<Vec<_>>()})
            }
            "predict" => {
                serde_json::json!({"predictions":input["checks"].as_array().unwrap().iter().map(|c|serde_json::json!({"checkId":c["id"],"predictable":false,"expectedOutcomeIds":[],"expectedObservation":null,"reason":"The light cannot diagnose the connector"})).collect::<Vec<_>>()})
            }
            "report" if matches!(self.scenario, Scenario::Distinct) => {
                distinguishing_report(&input["material"])
            }
            "report" => {
                let m = &input["material"];
                let remaining = m["explanations"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v["id"].clone())
                    .collect::<Vec<_>>();
                let groups = if remaining.len() > 1 {
                    vec![
                        serde_json::json!({"explanations":remaining,"reason":"Available checks cannot separate unknown predictions"}),
                    ]
                } else {
                    vec![]
                };
                serde_json::json!({"decisionTree":{"root":"root","nodes":{"root":{"kind":"leaf","remaining":remaining,"nextAction":"Reopen diagnostic brief; predictions cannot distinguish explanations","reassess":true}}},"predictionMatrix":m["predictionMatrix"],"discardedChecks":m["checks"].as_array().unwrap().iter().map(|c|serde_json::json!({"checkId":c["id"],"reason":"No distinguishing predictions"})).collect::<Vec<_>>(),"inseparableGroups":groups,"limitations":["No checks performed"]})
            }
            _ => panic!("unknown probe input"),
        };
        let response = streaming(&value.to_string(), "stop");
        if (matches!(self.scenario, Scenario::SlowPropose) && stage == "propose")
            || (matches!(self.scenario, Scenario::SlowPrediction) && stage == "predict")
            || (matches!(
                self.scenario,
                Scenario::SlowReport | Scenario::SkippedSlowReport
            ) && stage == "report")
            || (matches!(self.scenario, Scenario::SlowOneProposal)
                && stage == "propose"
                && body["model"] == "fixture/member0")
        {
            response.set_delay(Duration::from_secs(2))
        } else {
            response
        }
    }
}
#[tokio::test]
async fn six_members_ten_calls_and_locked_visibility() {
    let f = Fixture::new().await;
    let room = f.open(OpenMode::Interactive).await;
    assert!(matches!(
        room.submit(request(
            &f.config,
            "@all $probe Intermittent noise, fan ruled out, inspect indicator only"
        ))
        .await,
        RequestResult::Accepted { .. }
    ));
    f.wait(|| terminal(&f)).await;
    assert_eq!(f.count("probe.explanation"), 3);
    assert_eq!(f.count("probe.check"), 3);
    assert_eq!(f.count("probe.prediction"), 3);
    let db = f.db();
    let calls: i64 = db
        .query_row("SELECT COUNT(*) FROM attempts", [], |r| r.get(0))
        .unwrap();
    assert_eq!(calls, 10);
    let status: String = db
        .query_row(
            "SELECT json_extract(state_json,'$.state') FROM contests WHERE kind='probe'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(status, "COMPLETE");
    let mut q=db.prepare("SELECT w.kind,h.content,h.meta,w.target FROM work w JOIN history h ON h.context_id=w.context_id").unwrap();
    for row in q
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })
        .unwrap()
    {
        let (kind, input, meta, target) = row.unwrap();
        let value: serde_json::Value = serde_json::from_str(&input).unwrap();
        assert_eq!(meta, "{\"anonymous\":true}");
        if kind == "probe.explain" || kind == "probe.check" {
            assert!(value.get("explanations").is_none());
            assert!(value.get("checks").is_none());
        }
        if kind == "probe.predict" {
            let original:String=db.query_row("SELECT json_extract(e.payload,'$.data.explanation') FROM probe_roles r JOIN events e ON e.seq=r.initial_event_seq WHERE r.nick=?",[target],|r|r.get(0)).unwrap();
            assert_eq!(value["explanation"]["data"]["explanation"], original);
            assert!(value.get("explanation").is_some());
            assert!(value.get("explanations").is_none());
            assert!(value.get("predictionMatrix").is_none());
            assert_eq!(value["checks"].as_array().unwrap().len(), 3);
        }
    }
    let contest: ContestId = db
        .query_row(
            "SELECT contest_id FROM contests WHERE kind='probe'",
            [],
            |r| r.get::<_, String>(0),
        )
        .unwrap()
        .parse()
        .unwrap();
    let export = room
        .export(ExportScope::Probe, Some(contest))
        .await
        .unwrap();
    assert!(export.contains("Decision tree"));
    assert!(export.contains("Prediction matrix"));
    assert!(export.contains("Inseparable"));
    assert!(export.contains("Procedure"));
    assert!(!export.contains("fixture/member"));
    let captured = f.fake.server.received_requests().await.unwrap();
    let requests = captured
        .iter()
        .filter(|r| r.url.path().ends_with("chat/completions"))
        .collect::<Vec<_>>();
    assert_eq!(requests.len(), 10);
    for r in requests {
        let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
        let messages = body["messages"].to_string();
        assert!(!messages.contains("PERSONA_"));
        assert!(!messages.contains("fixture/member"));
        assert_eq!(body["messages"].as_array().unwrap().len(), 2);
        assert_eq!(body["messages"][1]["name"], "squad");
        if body["model"] == "fixture/judge" {
            assert_eq!(body["reasoning"]["max_tokens"], 4096);
        }
    }
    let events = squad_store::events::page(&db, None, usize::MAX).unwrap();
    let mut groups = std::collections::BTreeMap::<i64, Vec<&CommittedEnvelope>>::new();
    for e in &events {
        groups
            .entry(e.commit.seq.get() - i64::from(e.commit.commit_index))
            .or_default()
            .push(e);
    }
    for group in groups.values() {
        assert_eq!(group.len(), group[0].commit.commit_size as usize);
        for (i, e) in group.iter().enumerate() {
            assert_eq!(e.commit.commit_index, i as u32);
        }
    }
    f.assert_ledger();
    drop(q);
    drop(db);
    room.shutdown().await.unwrap();
    assert_eq!(
        export,
        squad_core::room::export_offline(
            &f.config.data_dir,
            "main",
            ExportScope::Probe,
            Some(contest)
        )
        .unwrap()
    );
    let store = squad_store::Store::open(f.config.data_dir.join("squad.db")).unwrap();
    let (output, work, binding, fence, epoch) = store
        .call(|db| {
            let event = squad_store::events::page(db, None, usize::MAX)?
                .into_iter()
                .find(|e| matches!(e.content, EnvelopePayload::ProbePrediction(_)))
                .unwrap();
            let EnvelopePayload::ProbePrediction(p) = &event.content else {
                panic!()
            };
            let (w, b) = squad_store::work::template(db, p.completed.generation.work_id)?.unwrap();
            let (fence, epoch): (i64, i64) = db.query_row(
                "SELECT lease_fence-1,epoch FROM work WHERE work_id=?",
                [w.work_id.to_string()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            Ok((event.content, w, b, fence as u64, epoch as u64))
        })
        .unwrap();
    let mut bindings = std::collections::BTreeMap::new();
    for member in &f.config.agents {
        bindings.insert(member.nick.to_string(), binding.clone());
    }
    bindings.insert("judge".into(), binding);
    let mut router = squad_core::router::Router::new(
        store,
        f.config.agents.iter().map(|m| m.nick.clone()).collect(),
        bindings,
        20_000_000,
    )
    .unwrap();
    let Target::Member(nick) = &work.target else {
        panic!()
    };
    let owner = ExecutorKey::Member(nick.clone());
    let submit = |output| {
        Request::Internal(InternalRequest::Executor(ExecutorRequest::Result {
            work_id: work.work_id,
            attempt: 1,
            fence,
            epoch,
            output,
        }))
    };
    let duplicate = router
        .process(submit(output.clone()), Some(owner.clone()))
        .unwrap();
    assert!(duplicate.duplicate);
    assert!(duplicate.events.is_empty());
    let mut forged = output;
    if let EnvelopePayload::ProbePrediction(p) = &mut forged {
        p.data.predictions[0].reason = Text::new("Changed after lock").unwrap();
    }
    assert!(router.process(submit(forged), Some(owner)).is_err());
    drop(router);
    let room = f.open(OpenMode::OneShot).await;
    assert_eq!(f.count("probe.prediction"), 3);
    assert_eq!(f.count("probe.report"), 1);
    assert!(matches!(
        room.submit(Request::Clear {
            request_id: EventId::new()
        })
        .await,
        RequestResult::Accepted { .. }
    ));
    for table in [
        "probe_roles",
        "mode_artifacts",
        "contest_steps",
        "contest_private_maps",
        "cases",
    ] {
        let count: i64 = f
            .db()
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }
    let calls: i64 = f
        .db()
        .query_row("SELECT COUNT(*) FROM attempts", [], |r| r.get(0))
        .unwrap();
    assert_eq!(calls, 10);
    f.assert_ledger();
    room.shutdown().await.unwrap();
}

const BRIEF: &str = "@all $probe Intermittent noise, fan ruled out, inspect indicator only";
fn terminal(f: &Fixture) -> bool {
    f.db()
        .query_row(
            "SELECT NOT active FROM contests WHERE kind='probe'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(false)
}
fn status(f: &Fixture) -> String {
    f.db()
        .query_row(
            "SELECT json_extract(state_json,'$.state') FROM contests WHERE kind='probe'",
            [],
            |r| r.get(0),
        )
        .unwrap()
}
#[tokio::test]
async fn member_failure_is_skipped_and_insufficient_evidence_terminates() {
    for scenario in [Scenario::MemberFailure, Scenario::AllFail] {
        let f = Fixture::scenario(6, scenario).await;
        let room = f.open(OpenMode::Interactive).await;
        let result = room.submit(request(&f.config, BRIEF)).await;
        assert!(
            matches!(result, RequestResult::Accepted { .. }),
            "{result:?}"
        );
        f.wait(|| terminal(&f)).await;
        if matches!(scenario, Scenario::AllFail) {
            assert_eq!(status(&f), "FAILED");
            assert_eq!(f.count("probe.report"), 0);
            assert_provider_failure(&f.db(), 500, Some("upstream exploded"));
        } else {
            assert_eq!(status(&f), "COMPLETE");
            let count:i64=f.db().query_row("SELECT COUNT(*) FROM probe_roles WHERE initial_event_seq IS NULL AND role<>'sitting_out'",[],|r|r.get(0)).unwrap();
            assert_eq!(count, 1);
        }
        f.assert_ledger();
        room.shutdown().await.unwrap();
    }
}
#[tokio::test]
async fn report_repair_and_exhaustion_preserve_locked_matrix() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::ERROR)
        .try_init();
    for scenario in [Scenario::ReportRepair, Scenario::ReportExhaustion] {
        let f = Fixture::scenario(6, scenario).await;
        let room = f.open(OpenMode::Interactive).await;
        room.submit(request(&f.config, BRIEF)).await;
        f.wait(|| terminal(&f)).await;
        let count: i64 = f
            .db()
            .query_row(
                "SELECT attempt FROM work WHERE kind='probe.report'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 2);
        assert_eq!(
            status(&f),
            if matches!(scenario, Scenario::ReportRepair) {
                "COMPLETE"
            } else {
                "FAILED"
            }
        );
        assert_eq!(f.count("probe.prediction"), 3);
        f.assert_ledger();
        room.shutdown().await.unwrap();
    }
}
#[tokio::test]
async fn odd_roster_sits_out_last_member() {
    let f = Fixture::scenario(5, Scenario::Happy).await;
    let room = f.open(OpenMode::Interactive).await;
    room.submit(request(&f.config, BRIEF)).await;
    f.wait(|| terminal(&f)).await;
    let db = f.db();
    let out: String = db
        .query_row(
            "SELECT nick FROM probe_roles WHERE role='sitting_out'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(out, "member4");
    let calls: i64 = db
        .query_row("SELECT COUNT(*) FROM attempts", [], |r| r.get(0))
        .unwrap();
    assert_eq!(calls, 7);
    let calls: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM work WHERE target='member4'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(calls, 0);
    assert_eq!(status(&f), "COMPLETE");
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn stop_fences_late_results_and_clear_removes_probe_records() {
    for scenario in [
        Scenario::SlowPropose,
        Scenario::SlowPrediction,
        Scenario::SlowReport,
    ] {
        let f = Fixture::scenario(6, scenario).await;
        let room = f.open(OpenMode::Interactive).await;
        room.submit(request(&f.config, BRIEF)).await;
        let expected = match scenario {
            Scenario::SlowPropose => "PROPOSING",
            Scenario::SlowPrediction => "PREDICTING",
            _ => "REPORTING",
        };
        f.wait(|| {
            status(&f) == expected
                && f.db()
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM work WHERE status='running')",
                        [],
                        |r| r.get(0),
                    )
                    .unwrap()
        })
        .await;
        let result = room
            .submit(Request::Stop {
                request_id: EventId::new(),
                target: None,
            })
            .await;
        assert!(matches!(result, RequestResult::Accepted { .. }));
        assert_eq!(status(&f), "STOPPED");
        let db = f.db();
        let mut q = db
            .prepare("SELECT work_id FROM work WHERE status='cancelled'")
            .unwrap();
        let ids = q
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(!ids.is_empty());
        for id in ids {
            let id: WorkId = id.parse().unwrap();
            let work = squad_store::work::template(&db, id).unwrap().unwrap().0;
            let key = squad_store::work::owner(&work).unwrap();
            assert!(matches!(
                squad_store::work::check_fence(&db, id, &key, 1, 1, 1, chrono::Utc::now()),
                Err(squad_store::StoreError::Domain(
                    ErrorCode::STALE_WORK_ATTEMPT,
                    _
                ))
            ));
        }
        drop(q);
        drop(db);
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(f.count("probe.report"), 0);
        room.submit(Request::SetVars {
            request_id: EventId::new(),
            updates: vec![VariableUpdate {
                name: VariableName::CensusPasses,
                value: 4,
            }],
        })
        .await;
        room.submit(Request::Clear {
            request_id: EventId::new(),
        })
        .await;
        for table in [
            "probe_roles",
            "mode_artifacts",
            "contest_steps",
            "contest_private_maps",
            "cases",
        ] {
            let count: i64 = f
                .db()
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
                .unwrap();
            assert_eq!(count, 0);
        }
        let value: i64 = f
            .db()
            .query_row(
                "SELECT value FROM vars WHERE name='CENSUS_PASSES'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(value, 2);
        f.assert_ledger();
        room.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn restart_at_assignment_artifact_and_both_barriers_keeps_accepted_calls() {
    for scenario in [
        Scenario::SlowPropose,
        Scenario::SlowOneProposal,
        Scenario::SlowPrediction,
        Scenario::SlowReport,
        Scenario::SkippedSlowReport,
    ] {
        let f = Fixture::scenario(6, scenario).await;
        let room = f.open(OpenMode::Interactive).await;
        room.submit(request(&f.config, BRIEF)).await;
        match scenario {
            Scenario::SlowPropose => {
                f.wait(|| {
                    f.db()
                        .query_row("SELECT COUNT(*)>0 FROM work", [], |r| r.get(0))
                        .unwrap()
                })
                .await
            }
            Scenario::SlowOneProposal => {
                f.wait(|| f.count("probe.explanation") + f.count("probe.check") >= 5)
                    .await
            }
            Scenario::SlowPrediction => {
                f.wait(|| f.count("probe.explanation") == 3 && f.count("probe.check") == 3)
                    .await
            }
            _ => f.wait(|| status(&f) == "REPORTING").await,
        }
        let db = f.db();
        let split: String = db
            .query_row(
                "SELECT payload FROM events WHERE kind='probe.setup'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let mut q = db
            .prepare("SELECT work_id FROM work WHERE status='completed'")
            .unwrap();
        let accepted = q
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        drop(q);
        drop(db);
        room.shutdown().await.unwrap();
        let room = f.open(OpenMode::Interactive).await;
        f.wait(|| terminal(&f)).await;
        let db = f.db();
        let restored: String = db
            .query_row(
                "SELECT payload FROM events WHERE kind='probe.setup'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(split, restored);
        for id in accepted {
            let calls: i64 = db
                .query_row("SELECT COUNT(*) FROM attempts WHERE work_id=?", [id], |r| {
                    r.get(0)
                })
                .unwrap();
            assert_eq!(calls, 1, "accepted work called again");
        }
        assert_eq!(status(&f), "COMPLETE");
        assert_eq!(
            f.count("probe.explanation") + f.count("probe.check"),
            if matches!(scenario, Scenario::SkippedSlowReport) {
                5
            } else {
                6
            }
        );
        if matches!(scenario, Scenario::SkippedSlowReport) {
            let state: String = db
                .query_row("SELECT state_json FROM contests", [], |r| r.get(0))
                .unwrap();
            let state: ProbeState = serde_json::from_str(&state).unwrap();
            assert!(state.skipped_members.contains(&nick("member0")));
        }
        f.assert_ledger();
        room.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn daily_max_refuses_before_dispatch() {
    let mut f = Fixture::new().await;
    f.config.budget.daily_max_usd = 0.000001;
    let room = f.open(OpenMode::Interactive).await;
    let result = room.submit(request(&f.config, BRIEF)).await;
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
    let calls: i64 = f
        .db()
        .query_row("SELECT COUNT(*) FROM attempts", [], |r| r.get(0))
        .unwrap();
    let RequestResult::Rejected { error, .. } = &result else {
        unreachable!()
    };
    let text = error.text.as_str();
    assert!(
        text.starts_with("Daily budget admission refused: spent $0.000000 of $"),
        "{text}"
    );
    assert!(
        text.contains("today; this request needs up to $") && text.contains("reserved"),
        "{text}"
    );
    assert_eq!(calls, 0);
    room.shutdown().await.unwrap();
}

#[tokio::test]
async fn real_scoped_readers_reject_guessed_work_and_alternate_contexts() {
    let f = Fixture::new().await;
    let room = f.open(OpenMode::OneShot).await;
    let result = room.submit_commit_only(request(&f.config, BRIEF)).await;
    assert!(matches!(result, RequestResult::Accepted { .. }));
    room.shutdown().await.unwrap();
    let store = squad_store::Store::open(f.config.data_dir.join("squad.db")).unwrap();
    let (work, other, epoch) = store
        .call(|db| {
            db.execute("UPDATE work SET runnable=1", [])?;
            let mut q = db.prepare("SELECT work_id FROM work ORDER BY rowid")?;
            let ids = q
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            let id: WorkId = ids[0].parse().unwrap();
            let other: WorkId = ids[1].parse().unwrap();
            let work = squad_store::work::template(db, id)?.unwrap().0;
            let Target::Member(nick) = &work.target else {
                panic!()
            };
            let key = ExecutorKey::Member(nick.clone());
            let grant =
                squad_store::leases::claim(db, id, &key, chrono::Utc::now(), None)?.unwrap();
            Ok((grant.work, other, grant.epoch))
        })
        .unwrap();
    let Target::Member(nick) = &work.target else {
        panic!()
    };
    let readers = squad_store::readers::ScopedReaders::new(
        store.clone(),
        ExecutorKey::Member(nick.clone()),
        epoch,
    );
    assert_eq!(
        HistoryReader::for_work(&readers, work.work_id)
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(matches!(
        WorkReader::get(&readers, other).await,
        Err(ReaderError::Forbidden)
    ));
    assert!(matches!(
        HistoryReader::for_work(&readers, other).await,
        Err(ReaderError::Forbidden)
    ));
    assert!(matches!(
        WorkReader::repair_for_attempt(&readers, work.work_id, 1).await,
        Err(ReaderError::Forbidden)
    ));
    let id = work.work_id;
    let context = nick.to_string();
    store.call(move|db|{db.execute("UPDATE work SET scheduling_meta=json_set(scheduling_meta,'$.work.contextId',?) WHERE work_id=?",rusqlite::params![context,id.to_string()])?;Ok(())}).unwrap();
    assert!(matches!(
        HistoryReader::for_work(&readers, work.work_id).await,
        Err(ReaderError::Forbidden)
    ));
    store
        .call(|db| {
            db.execute("UPDATE room_meta SET epoch=epoch+1", [])?;
            Ok(())
        })
        .unwrap();
    assert!(matches!(
        WorkReader::get(&readers, work.work_id).await,
        Err(ReaderError::StaleWork)
    ));
}

#[test]
fn blind_redaction_uses_unicode_boundaries_and_longest_phrases() {
    let tokens = vec![
        "alpha".into(),
        "openai".into(),
        "very curious analyst".into(),
        "gpt".into(),
        "fixture/model".into(),
        "éclair".into(),
    ];
    let original =
        "ALPHA, OpenAI / GPT; very curious analyst; fixture/model; ÉCLAIR. alphabet stays.";
    assert_eq!(
        squad_core::router::probe::redact(original, &tokens),
        "[redacted], [redacted] / [redacted]; [redacted]; [redacted]; [redacted]. alphabet stays."
    );
    assert!(original.contains("ALPHA"));
}

fn distinguishing_report(m: &serde_json::Value) -> serde_json::Value {
    let all = m["explanations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].clone())
        .collect::<Vec<_>>();
    let checks = m["checks"].as_array().unwrap();
    let check = &checks[0];
    let rows = m["predictionMatrix"].as_object().unwrap();
    let ids = all
        .iter()
        .map(|id| id.as_str().unwrap())
        .collect::<Vec<_>>();
    let mut groups = vec![];
    for (i, a) in ids.iter().enumerate() {
        for b in &ids[i + 1..] {
            if rows[*a] == rows[*b] {
                groups.push(serde_json::json!({"explanations":[a,b],"reason":"Identical locked predictions"}));
            }
        }
    }
    let compatible = |outcome: &serde_json::Value| {
        all.iter()
            .filter(|id| {
                rows[id.as_str().unwrap()]["predictions"][0]["expectedOutcomeIds"]
                    .as_array()
                    .unwrap()
                    .contains(outcome)
            })
            .cloned()
            .collect::<Vec<_>>()
    };
    let a = &check["data"]["outcomes"][0]["id"];
    let b = &check["data"]["outcomes"][1]["id"];
    serde_json::json!({"decisionTree":{"root":"root","nodes":{"root":{"kind":"check","checkId":check["id"],"measurement":check["data"]["measurement"],"branches":{a.as_str().unwrap():"a",b.as_str().unwrap():"b"},"unexpected":"unexpected"},"a":{"kind":"leaf","remaining":compatible(a),"nextAction":"Review remaining possibilities","reassess":false},"b":{"kind":"leaf","remaining":compatible(b),"nextAction":"Review remaining possibilities","reassess":false},"unexpected":{"kind":"leaf","remaining":all,"nextAction":"Reopen diagnostic brief","reassess":true}}},"predictionMatrix":m["predictionMatrix"],"discardedChecks":checks[1..].iter().map(|c|serde_json::json!({"checkId":c["id"],"reason":"Redundant with first check"})).collect::<Vec<_>>(),"inseparableGroups":groups,"limitations":["Human must perform checks"]})
}
#[tokio::test]
async fn distinguishing_tree_and_unexpected_branch_export_without_truth_declaration() {
    let f = Fixture::scenario(6, Scenario::Distinct).await;
    let room = f.open(OpenMode::Interactive).await;
    room.submit(request(&f.config, BRIEF)).await;
    f.wait(|| terminal(&f)).await;
    assert_eq!(status(&f), "COMPLETE");
    let id: ContestId = f
        .db()
        .query_row("SELECT contest_id FROM contests", [], |r| {
            r.get::<_, String>(0)
        })
        .unwrap()
        .parse()
        .unwrap();
    let export = room.export(ExportScope::Probe, Some(id)).await.unwrap();
    assert!(export.contains("Unexpected result"));
    assert!(export.contains("reopen the diagnostic brief"));
    assert!(export.contains("No explanation is declared true"));
    room.shutdown().await.unwrap();
}

#[tokio::test]
async fn ordinary_private_history_never_enters_any_probe_stage() {
    let f = Fixture::new().await;
    let room = f.open(OpenMode::Interactive).await;
    for i in 0..6 {
        room.submit(request(
            &f.config,
            &format!("@member{i} PRIVATE_{i}_CANARY"),
        ))
        .await;
    }
    f.wait(|| f.count("message.final") == 6).await;
    room.submit(request(&f.config, BRIEF)).await;
    f.wait(|| terminal(&f)).await;
    let captured = f.fake.server.received_requests().await.unwrap();
    let mut calls = 0;
    for r in captured
        .iter()
        .filter(|r| r.url.path().ends_with("chat/completions"))
    {
        let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
        let messages = body["messages"].to_string();
        if messages.contains("Probe ") {
            calls += 1;
            for i in 0..6 {
                assert!(!messages.contains(&format!("PRIVATE_{i}_CANARY")));
            }
            assert!(!messages.contains("PERSONA_"));
        }
    }
    assert_eq!(calls, 10);
    room.shutdown().await.unwrap();
}

#[tokio::test]
async fn preflight_confirmation_cancel_and_expiry_never_dispatch_early() {
    for action in ["confirm", "cancel", "expire"] {
        let mut f = Fixture::new().await;
        f.config.budget.debate_confirm_usd = 0.000001;
        let room = f.open(OpenMode::Interactive).await;
        let result = room.submit(request(&f.config, BRIEF)).await;
        let RequestResult::PendingConfirmation {
            preflight: PendingPreflight::Contest(plan),
            ..
        } = result
        else {
            panic!("expected confirmation: {result:?}")
        };
        assert_eq!(plan.estimate.model_calls, 10);
        assert_eq!(
            f.db()
                .query_row("SELECT COUNT(*) FROM attempts", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(room.state().await.pending_preflights.len(), 1);
        if action == "expire" {
            f.db()
                .execute(
                    "UPDATE contests SET expires_at='2000-01-01T00:00:00+00:00'",
                    [],
                )
                .unwrap();
            room.submit(Request::SetVars {
                request_id: EventId::new(),
                updates: vec![VariableUpdate {
                    name: VariableName::Rounds,
                    value: 3,
                }],
            })
            .await;
        } else {
            let request = if action == "confirm" {
                Request::ContestConfirm {
                    request_id: EventId::new(),
                    contest_id: plan.contest_id,
                    plan_id: plan.plan_id,
                }
            } else {
                Request::ContestCancel {
                    request_id: EventId::new(),
                    contest_id: plan.contest_id,
                    plan_id: plan.plan_id,
                }
            };
            assert!(matches!(
                room.submit(request).await,
                RequestResult::Accepted { .. }
            ));
        }
        f.wait(|| terminal(&f)).await;
        assert_eq!(
            status(&f),
            if action == "confirm" {
                "COMPLETE"
            } else if action == "cancel" {
                "STOPPED"
            } else {
                "FAILED"
            }
        );
        assert!(room.state().await.pending_preflights.is_empty());
        if action != "confirm" {
            assert_eq!(
                f.db()
                    .query_row("SELECT COUNT(*) FROM attempts", [], |r| r.get::<_, i64>(0))
                    .unwrap(),
                0
            );
        }
        assert_eq!(f.count("contest.preflight.result"), 1);
        f.assert_ledger();
        room.shutdown().await.unwrap();
    }
}

#[test]
fn blind_tokens_cover_short_model_families_and_complete_providers() {
    let tokens = squad_core::router::probe::blind_tokens(vec![
        "~x-ai/o3".into(),
        "openai/gpt-6.1".into(),
        "a curious analyst".into(),
        "alpha".into(),
    ]);
    for token in ["x-ai", "o3", "gpt", "openai", "a curious analyst", "alpha"] {
        assert!(tokens.iter().any(|t| t == token));
    }
    for unrelated in ["x", "ai", "6", "1"] {
        assert!(!tokens.iter().any(|t| t == unrelated));
    }
    assert_eq!(
        squad_core::router::probe::redact("O3, X-AI, GPT; a curious analyst.", &tokens),
        "[redacted], [redacted], [redacted]; [redacted]."
    );
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
