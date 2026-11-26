//! Contest plan reservations are sized from what each step can receive, not from
//! the model's context window. Uses the default six-member roster and realistic
//! catalog prices; no provider call is made by any preflight.
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

const ROSTER: [(&str, &str, u64, &str, &str); 6] = [
    (
        "claude",
        "anthropic/claude-opus-5-5",
        1_000_000,
        "0.000015",
        "0.000075",
    ),
    (
        "fable",
        "anthropic/claude-fable-5-2",
        1_000_000,
        "0.000015",
        "0.000075",
    ),
    (
        "openai",
        "openai/gpt-sol-6-1",
        400_000,
        "0.00001",
        "0.00004",
    ),
    (
        "gemini",
        "google/gemini-4-neon",
        1_000_000,
        "0.000002",
        "0.000012",
    ),
    ("grok", "x-ai/grok-5", 256_000, "0.000003", "0.000015"),
    ("muse", "meta/muse-wonder", 262_144, "0.00000125", "0.00001"),
];
#[derive(Debug, Clone, Copy)]
struct Quote {
    estimated: f64,
    reserved: f64,
    calls: u32,
}
struct Fixture {
    _dir: tempfile::TempDir,
    config: SquadConfig,
    fake: FakeOpenRouter,
}
impl Fixture {
    async fn new(daily_max: f64) -> Self {
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
        config.agents = ROSTER
            .iter()
            .map(|(n, slug, ..)| config::AgentConfig {
                nick: n.parse().unwrap(),
                model: (*slug).into(),
                persona: String::new(),
                reasoning_effort: None,
            })
            .collect();
        config.judge.model = ROSTER[0].1.into();
        config.budget.daily_max_usd = daily_max;
        // Always stop at confirmation: the preflight is all these tests read.
        config.budget.debate_confirm_usd = 0.0;
        let fake = FakeOpenRouter::start().await;
        let models = ROSTER
            .iter()
            .map(|(_, id, ctx, p, c)| serde_json::json!({"id":id,"context_length":ctx,"top_provider":{"max_completion_tokens":65536},"supported_parameters":["reasoning"],"pricing":{"prompt":p,"completion":c}}))
            .collect::<Vec<_>>();
        fake.catalog(serde_json::json!({"data":models})).await;
        Mock::given(method("POST"))
            .and(path("/api/v1/chat/completions"))
            .respond_with(streaming("ordinary response", "stop"))
            .mount(&fake.server)
            .await;
        Self {
            _dir: dir,
            config,
            fake,
        }
    }
    async fn open(&self) -> RoomHandle {
        RoomHandle::open_with_client(
            self.config.clone(),
            "main",
            OpenMode::Interactive,
            OpenRouterClient::new(&self.fake.base_url(), "fake-key").unwrap(),
        )
        .await
        .unwrap()
    }
    fn db(&self) -> rusqlite::Connection {
        rusqlite::Connection::open(self.config.data_dir.join("squad.db")).unwrap()
    }
    async fn request(&self, room: &RoomHandle, text: &str) -> Request {
        let state = room.state().await;
        let mut ctx = parser::ParseContext {
            roster: self.config.agents.iter().map(|a| a.nick.clone()).collect(),
            active_contest: state.active_contest.clone(),
            ..Default::default()
        };
        for e in room.page_events(None, 500).await.unwrap() {
            let (message_id, body, reply) = match &e.content {
                EnvelopePayload::UserMessage(m) => (Some(m.input.message_id), m.body.clone(), None),
                EnvelopePayload::MessageFinal(m) => (
                    Some(m.completed.generation.message_id),
                    m.content.clone(),
                    Some(m.in_reply_to_message_id),
                ),
                _ => (None, String::new(), None),
            };
            ctx.records.push(parser::RecordSummary {
                event_id: e.base.id,
                seq: e.commit.seq,
                message_id,
                author: e.base.sender,
                recipients: e.base.recipients,
                kind: Text::new(if reply.is_some() {
                    "message.final"
                } else {
                    "user.message"
                })
                .unwrap(),
                content: body,
                in_reply_to_message_id: reply,
                eligible: true,
                superseded: false,
                cell_ids: vec![],
                contest_id: None,
                case_id: None,
                plan_event_id: None,
                grid_event_id: None,
                state_version: None,
            });
        }
        let meta = parser::SubmissionMeta::default();
        Request::from_parsed(
            EventId::new(),
            parse(text, &meta, &ctx).unwrap_or_else(|e| panic!("{text}: {e:?}")),
            &meta,
            false,
        )
        .unwrap()
    }
    async fn ordinary(&self, room: &RoomHandle, text: &str, replies: i64) {
        let r = room.submit(self.request(room, text).await).await;
        assert!(matches!(r, RequestResult::Accepted { .. }), "{r:?}");
        tokio::time::timeout(Duration::from_secs(15), async {
            while self
                .db()
                .query_row(
                    "SELECT COUNT(*) FROM events WHERE kind='message.final'",
                    [],
                    |r| r.get::<_, i64>(0),
                )
                .unwrap()
                < replies
            {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("ordinary replies");
    }
    /// Submit `command` after `seed` ordinary exchanges and read the frozen preflight.
    async fn quote(
        &self,
        seed: &[(&str, i64)],
        command: &str,
    ) -> Result<Quote, (ErrorCode, String)> {
        let room = self.open().await;
        for (text, replies) in seed {
            self.ordinary(&room, text, *replies).await;
        }
        let r = room.submit(self.request(&room, command).await).await;
        let preflight = match r {
            RequestResult::PendingConfirmation { preflight, .. } => {
                serde_json::to_value(preflight).unwrap()
            }
            RequestResult::Rejected { error, .. } => {
                room.shutdown().await.unwrap();
                return Err((error.code, error.text.to_string()));
            }
            RequestResult::Accepted { .. } => tokio::time::timeout(Duration::from_secs(15), async {
                loop {
                    let raw: Option<String> = self
                        .db()
                        .query_row(
                            "SELECT payload FROM events WHERE kind IN ('debate.preflight','contest.preflight') ORDER BY seq DESC LIMIT 1",
                            [],
                            |r| r.get(0),
                        )
                        .ok();
                    if let Some(raw) = raw {
                        break serde_json::from_str::<serde_json::Value>(&raw).unwrap();
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            })
            .await
            .expect("preflight event"),
            other => panic!("{command}: {other:?}"),
        };
        let estimate = find_estimate(&preflight).unwrap_or_else(|| panic!("{preflight}"));
        let quote = Quote {
            estimated: estimate["estimatedUsd"].as_f64().unwrap(),
            reserved: estimate["reservedUsd"].as_f64().unwrap(),
            calls: estimate["modelCalls"].as_u64().unwrap() as u32,
        };
        room.shutdown().await.unwrap();
        Ok(quote)
    }
}
fn find_estimate(value: &serde_json::Value) -> Option<&serde_json::Value> {
    match value {
        serde_json::Value::Object(map) => map
            .get("estimate")
            .or_else(|| map.values().find_map(find_estimate)),
        _ => None,
    }
}
/// The small contests of the default roster: its three cheapest members (an
/// Opus-class judge, as configured by default, runs every structured step).
/// Mode, seed exchanges (text, replies expected), command.
type Scenario = (&'static str, &'static [(&'static str, i64)], &'static str);
const MODES: [Scenario; 7] = [
    (
        "debate",
        &[("@gemini @grok Which approach?", 2)],
        "@gemini @grok $debate Which approach?",
    ),
    (
        "tournament",
        &[],
        "@gemini @grok @muse $tournament Design a bridge",
    ),
    (
        "grill",
        &[("@gemini Draft a rollout plan", 1)],
        "@gemini @grok @muse $grill",
    ),
    (
        "census",
        &[],
        "@gemini @grok @muse $census Build a database service.",
    ),
    (
        "crosscheck",
        &[],
        "@gemini @grok @muse $crosscheck Maintain service.\n--\nKeep service available.",
    ),
    (
        "survey",
        &[],
        "@gemini @grok @muse $survey Which checks improve reliability?",
    ),
    (
        "probe",
        &[],
        "@gemini @grok @muse @openai $probe Requests stall after deploy; inspect logs and metrics.",
    ),
];
/// (mode, expected reserved USD, tolerance). Probe splits roles at random, so its
/// reservation varies with who explains.
const EXPECTED: [(&str, f64, f64); 7] = [
    ("debate", 7.62, 0.01),
    ("tournament", 24.36, 0.01),
    ("grill", 4.19, 0.01),
    ("census", 19.56, 0.01),
    ("crosscheck", 13.12, 0.01),
    ("survey", 14.53, 0.01),
    ("probe", 9.5, 0.12),
];
#[tokio::test]
async fn default_roster_contests_reserve_from_step_bounds_not_context_windows() {
    for ((mode, seed, command), (name, expected, tolerance)) in MODES.into_iter().zip(EXPECTED) {
        assert_eq!(mode, name);
        let f = Fixture::new(1000.).await;
        let q = f
            .quote(seed, command)
            .await
            .unwrap_or_else(|e| panic!("{mode}: {e:?}"));
        assert!(
            (q.reserved - expected).abs() <= expected * tolerance,
            "{mode}: reserved {} vs {expected}",
            q.reserved
        );
        assert!(
            q.estimated > 0. && q.estimated < q.reserved,
            "{mode}: {q:?}"
        );
    }
}
#[tokio::test]
async fn small_contests_fit_the_default_daily_cap() {
    // Everything but the tournament fits $20; its Opus-class judge alone reserves
    // ~$11 of output ceilings (3 attempts x 2 replays x 2 rounds) before any input.
    for (mode, seed, command) in MODES {
        let f = Fixture::new(20.).await;
        let result = f.quote(seed, command).await;
        if mode == "tournament" {
            assert_eq!(result.unwrap_err().0, ErrorCode::DAILY_MAX_REACHED);
        } else {
            let q = result.unwrap_or_else(|e| panic!("{mode}: {e:?}"));
            assert!(q.reserved < 20., "{mode}: {q:?}");
        }
    }
}
#[tokio::test]
async fn grill_no_longer_refused_for_context_with_the_full_roster() {
    // Before: a six-member grill was rejected with CONTEXT_TOO_LARGE on the
    // 256k-context members because every floor was assumed to hold 32 KiB per turn.
    let f = Fixture::new(1000.).await;
    let q = f
        .quote(
            &[("@claude Draft a rollout plan", 1)],
            "@claude $grill Focus on cost",
        )
        .await
        .expect("six-member grill preflight");
    assert!(q.estimated < q.reserved && q.calls == 22, "{q:?}");
}

#[derive(Clone)]
struct CensusAnswers;
impl Respond for CensusAnswers {
    fn respond(&self, r: &HttpRequest) -> ResponseTemplate {
        let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
        let text = body["messages"][1]["content"].as_str().unwrap();
        if body["response_format"]["json_schema"]["name"] != "census_response" {
            return streaming("ordinary response", "stop");
        }
        let material = mode_data(text);
        let output = if text.contains("Hunt independently") {
            serde_json::json!({"gaps":[{"localId":"one","omission":"No backup process","consequence":"Data may be lost"}]})
        } else if text.contains("Merge the independent findings") {
            serde_json::json!({"ledger":[{"id":"gap","omission":"No backup process","consequence":"Data may be lost","lenses":["failure modes"],"sourceFindIds":material["sourceIds"]}]})
        } else if text.contains("Independently rate") {
            serde_json::json!({"ratings":material["ledger"].as_array().unwrap().iter().map(|g|serde_json::json!({"gapId":g["id"],"rating":"real","reason":"Evidence","citation":serde_json::Value::Null})).collect::<Vec<_>>(),"additions":[]})
        } else if text.contains("Finalize the ledger") {
            let outcomes = material["outcomes"].as_array().unwrap();
            serde_json::json!({"confirmed":outcomes.iter().map(|g|serde_json::json!({"gapId":g["gap"]["id"],"priority":"Prevent permanent loss"})).collect::<Vec<_>>(),"covered":[],"trivial":[],"additions":[]})
        } else {
            return streaming("ordinary response", "stop");
        };
        streaming(&output.to_string(), "stop")
    }
}
fn mode_data(text: &str) -> serde_json::Value {
    for (i, c) in text.char_indices() {
        if c == '{' {
            let mut stream =
                serde_json::Deserializer::from_str(&text[i..]).into_iter::<serde_json::Value>();
            if let Some(Ok(v)) = stream.next()
                && v.get("plan").is_some()
            {
                return v;
            }
        }
    }
    panic!("No mode data in prompt: {text}")
}
fn kind_of(text: &str) -> &'static str {
    if text.contains("Hunt independently") {
        "census.hunt"
    } else if text.contains("Merge the independent findings") {
        "census.merge"
    } else if text.contains("Independently rate") {
        "census.rate"
    } else if text.contains("Finalize the ledger") {
        "census.final"
    } else {
        "other"
    }
}
impl Fixture {
    async fn census_run(&self) -> RoomHandle {
        Mock::given(method("POST"))
            .and(path("/api/v1/chat/completions"))
            .respond_with(CensusAnswers)
            .with_priority(1)
            .mount(&self.fake.server)
            .await;
        self.open().await
    }
    fn plan(&self) -> serde_json::Value {
        let raw: String = self
            .db()
            .query_row(
                "SELECT plan_json FROM contests ORDER BY rowid DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        serde_json::from_str(&raw).unwrap()
    }
    async fn wait_for(&self, check: impl Fn() -> bool) {
        tokio::time::timeout(Duration::from_secs(20), async {
            while !check() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("condition");
    }
    fn reports(&self) -> i64 {
        self.db()
            .query_row(
                "SELECT COUNT(*) FROM events WHERE kind='census.report'",
                [],
                |r| r.get(0),
            )
            .unwrap()
    }
}
fn estimate_bytes(body: &serde_json::Value) -> u64 {
    body["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            m["content"].as_str().unwrap().len() as u64
                + m["name"].as_str().map_or(0, |n| n.len() as u64)
                + 16
        })
        .sum()
}
#[tokio::test]
async fn no_started_request_exceeds_its_frozen_bound_or_attempt_share() {
    let mut f = Fixture::new(200.).await;
    f.config.budget.debate_confirm_usd = 100.;
    let room = f.census_run().await;
    let r = room
        .submit(
            f.request(
                &room,
                "@gemini @grok @muse $census Build a database service.",
            )
            .await,
        )
        .await;
    assert!(matches!(r, RequestResult::Accepted { .. }), "{r:?}");
    f.wait_for(|| f.reports() == 1).await;
    let plan = f.plan();
    let (shares, bounds) = (
        &plan["settings"]["attemptShares"],
        &plan["settings"]["stepInputBounds"],
    );
    let prices: BTreeMap<_, _> = ROSTER
        .iter()
        .map(|(_, slug, _, p, c)| {
            (
                *slug,
                (p.parse::<f64>().unwrap(), c.parse::<f64>().unwrap()),
            )
        })
        .collect();
    let mut seen = BTreeMap::<&str, u32>::new();
    for body in f.fake.captured_bodies().await {
        if body["response_format"]["json_schema"]["name"] != "census_response" {
            continue;
        }
        let kind = kind_of(body["messages"][1]["content"].as_str().unwrap());
        let input = estimate_bytes(&body);
        let (prompt, completion) = prices[body["model"].as_str().unwrap()];
        let worst =
            input as f64 * prompt + body["max_tokens"].as_u64().unwrap() as f64 * completion;
        let share = shares[kind].as_i64().unwrap();
        assert!(
            input <= bounds[kind].as_u64().unwrap(),
            "{kind}: {input} > bound {}",
            bounds[kind]
        );
        assert!(
            worst * 1e6 <= share as f64,
            "{kind}: request worst ${worst} > share {share}"
        );
        *seen.entry(kind).or_default() += 1;
    }
    assert_eq!(
        seen,
        BTreeMap::from([
            ("census.final", 1),
            ("census.hunt", 3),
            ("census.merge", 1),
            ("census.rate", 3)
        ])
    );
    // The plan holds exactly the permitted attempts at their frozen shares.
    let attempts = [
        ("census.hunt", 3 * 2 * 2),
        ("census.rate", 3 * 2 * 2),
        ("census.merge", 2 * 3),
        ("census.final", 2 * 3),
    ];
    let total: i64 = attempts
        .iter()
        .map(|(k, n)| shares[*k].as_i64().unwrap() * n)
        .sum();
    assert_eq!(
        (plan["estimate"]["reservedUsd"].as_f64().unwrap() * 1e6).round() as i64,
        total
    );
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn growing_the_plan_between_passes_requires_a_revised_preflight() {
    let mut f = Fixture::new(200.).await;
    f.config.budget.debate_confirm_usd = 100.;
    let room = f.census_run().await;
    room.submit(
        f.request(
            &room,
            "@gemini @grok @muse $census Build a database service.",
        )
        .await,
    )
    .await;
    f.wait_for(|| f.reports() == 1).await;
    let before = f.plan();
    let (contest, version): (String, u64) = f
        .db()
        .query_row(
            "SELECT contest_id,state_version FROM contests WHERE active=1",
            [],
            |r| Ok((r.get(0)?, r.get::<_, i64>(1)? as u64)),
        )
        .unwrap();
    // A shorter-or-equal replacement stays inside the frozen bounds: same plan id.
    let continue_with = |text: Option<String>, version: u64| Request::CensusContinue {
        request_id: EventId::new(),
        contest_id: contest.parse().unwrap(),
        expected_state_version: version,
        plan_event_id: None,
        replacement_text: text.map(|t| Text::new(t).unwrap()),
    };
    let grown = "Build a database service with backups, failover and audits. ".repeat(300);
    let r = room
        .submit(continue_with(Some(grown.clone()), version))
        .await;
    assert!(matches!(r, RequestResult::Accepted { .. }), "{r:?}");
    f.wait_for(|| f.reports() == 2).await;
    let after = f.plan();
    assert_ne!(before["planId"], after["planId"]);
    assert!(after["settings"]["planBytes"].as_u64().unwrap() as usize >= grown.len());
    assert!(
        after["settings"]["stepInputBounds"]["census.hunt"]
            .as_u64()
            .unwrap()
            > before["settings"]["stepInputBounds"]["census.hunt"]
                .as_u64()
                .unwrap()
                + grown.len() as u64
    );
    // One pass remains: its reservation covers the bigger plan at one pass's attempts.
    assert_eq!(after["settings"]["passes"], 1);
    assert_eq!(after["settings"]["reservedCalls"], 3 * 4 + 6);
    // Every request of the revised pass fits the revised frozen bounds.
    let bounds = &after["settings"]["stepInputBounds"];
    for body in f.fake.captured_bodies().await {
        if body["response_format"]["json_schema"]["name"] != "census_response" {
            continue;
        }
        let kind = kind_of(body["messages"][1]["content"].as_str().unwrap());
        assert!(
            estimate_bytes(&body) <= bounds[kind].as_u64().unwrap(),
            "{kind}"
        );
    }
    room.shutdown().await.unwrap();
}
