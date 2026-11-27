//! M8.P11.L1.T2 / M9.P4.L1.T2 (core half): the blind-text redaction fixtures, run over
//! one blind projection of each of the eleven new modes, plus the Tune executor
//! exemption end to end.
//!
//! Each mode is started through the real router on a hostile roster (case variants,
//! provider and family tokens, multi-word and punctuated personas, overlapping tokens).
//! The frozen admission data the mode's projection reads is then loaded from the store
//! and the mode's own projection is applied to a hostile text. Nothing identifying may
//! survive, unrelated words must, the projection is deterministic, and the source text
//! is never touched.
#[path = "support/mode_starts.rs"]
mod mode_starts;
use serde_json::json;
use squad_core::{orchestrator, router};
use squad_protocol::*;

mod tune_harness {
    #![allow(dead_code, unused)]
    // The Tune workflow file supplies the fixture and scripted provider for the executor
    // test. Its own tests already run in their own binary, so here they are plain,
    // unregistered functions.
    mod tokio {
        pub use ::tokio::*;
        pub use ::tracing::instrument as test;
    }
    include!("tune_workflows.rs");

    /// Run Tune to completion with identity tokens inside the candidate prompt and the
    /// task, and return every request body the provider received.
    pub async fn bodies_for_identity_prompt(prompt: &str, task: &str) -> Vec<serde_json::Value> {
        let f = Fixture::scenario(6, Scenario::Happy).await;
        let room = f.open(OpenMode::Interactive).await;
        let ctx = ParseContext {
            roster: f.config.agents.iter().map(|a| a.nick.clone()).collect(),
            ..Default::default()
        };
        let meta = SubmissionMeta::default();
        let raw = format!("$TUNE_GENERATIONS=1 @all $tune {prompt}\n--\n{task}");
        let parsed = parse(&raw, &meta, &ctx).unwrap();
        let result = room
            .submit(
                squad_protocol::Request::from_parsed(EventId::new(), parsed, &meta, true).unwrap(),
            )
            .await;
        assert!(
            matches!(result, RequestResult::Accepted { .. }),
            "{result:?}"
        );
        f.wait(|| {
            f.db()
                .query_row("SELECT active=0 FROM contests", [], |r| r.get::<_, bool>(0))
                .unwrap_or(false)
        })
        .await;
        let bodies = f.fake.captured_bodies().await;
        room.shutdown().await.unwrap();
        bodies
    }
}

/// `(nick, model slug, persona)`: provider and family tokens in the slugs, a persona that
/// overlaps another persona and a nick, and a persona with punctuation inside it.
fn roster() -> Vec<(String, String, String)> {
    [
        ("ada", "openai/gpt-5.6-luna", "Careful Staff Engineer"),
        ("staff", "anthropic/claude-fable-5.2", "Staff Engineer"),
        ("bob", "x-ai/grok-5.1-fast", "Dr. O'Neil, devil's advocate"),
        ("carol", "google/gemini-3-pro", "calm moderator"),
    ]
    .map(|(n, m, p)| (n.to_string(), m.to_string(), p.to_string()))
    .to_vec()
}
const JUDGE: &str = "mistralai/mistral-large-3";

const HOSTILE: &str = "ADA wrote this, and ada (Ada!) agreed; STAFF said so, staff too. \
The careful staff engineer (CAREFUL STAFF ENGINEER) agreed. Dr. O'Neil, devil's advocate, \
and CALM MODERATOR spoke. Models: OpenAI/GPT-5.6-Luna, openai, Anthropic/Claude-Fable-5.2, \
claude, FABLE, X-AI/Grok-5.1-Fast, x-ai, GROK, Google/Gemini-3-Pro, gemini, GPT, luna, \
Mistralai/Mistral-Large-3, MISTRAL. Unrelated: adaptation, malice, stafford, plain words stay.";

/// Nothing here may appear, in any case, on word boundaries.
const FORBIDDEN: &[&str] = &[
    "ada",
    "staff",
    "bob",
    "carol",
    "Careful Staff Engineer",
    "Staff Engineer",
    "Dr. O'Neil, devil's advocate",
    "calm moderator",
    "openai/gpt-5.6-luna",
    "anthropic/claude-fable-5.2",
    "x-ai/grok-5.1-fast",
    "google/gemini-3-pro",
    "mistralai/mistral-large-3",
    "openai",
    "anthropic",
    "x-ai",
    "google",
    "mistralai",
    "gpt",
    "luna",
    "claude",
    "fable",
    "grok",
    "gemini",
    "mistral",
];
/// These must come through untouched, so a projection cannot pass by deleting everything.
const KEPT: &[&str] = &["adaptation", "malice", "stafford", "plain words stay"];

fn word(phrase: &str) -> regex::Regex {
    regex::Regex::new(&format!(r"(?i)\b{}\b", regex::escape(phrase))).unwrap()
}

fn assert_blind(mode: &str, out: &str) {
    for phrase in FORBIDDEN {
        assert!(
            !word(phrase).is_match(out),
            "{mode}: {phrase:?} survived the blind projection:\n{out}"
        );
    }
    for kept in KEPT {
        assert!(out.contains(kept), "{mode}: {kept:?} was damaged:\n{out}");
    }
    assert!(out.contains("[redacted]"), "{mode}: nothing was redacted");
    // Overlapping tokens: the longest persona goes as one phrase, never as fragments.
    assert!(
        out.contains("The [redacted] ([redacted]) agreed."),
        "{mode}: overlapping persona and nick tokens left fragments:\n{out}"
    );
}

fn started(mode: &str) -> (squad_store::Store, ContestId) {
    let dir = tempfile::tempdir().unwrap();
    let (store, mut router) = mode_starts::room_with(dir.path(), &roster(), JUDGE);
    let (_, text) = mode_starts::STARTS
        .iter()
        .find(|(m, _)| *m == mode)
        .unwrap();
    assert!(
        matches!(
            mode_starts::start(&mut router, text).unwrap(),
            RequestResult::Accepted { .. }
        ),
        "{mode}"
    );
    let contest = store
        .call(|db| {
            Ok(db.query_row("SELECT contest_id FROM contests", [], |r| {
                r.get::<_, String>(0)
            })?)
        })
        .unwrap()
        .parse()
        .unwrap();
    (store, contest)
}

/// Tokens frozen into the contest's private maps at admission.
fn frozen_tokens(db: &rusqlite::Connection, contest: ContestId) -> Vec<String> {
    let mut query = db
        .prepare("SELECT map_json FROM contest_private_maps WHERE contest_id=?")
        .unwrap();
    for raw in query
        .query_map([contest.to_string()], |r| r.get::<_, String>(0))
        .unwrap()
    {
        let value: serde_json::Value = serde_json::from_str(&raw.unwrap()).unwrap();
        if let Some(tokens) = value["tokens"].as_array() {
            return tokens
                .iter()
                .map(|t| t.as_str().unwrap().to_string())
                .collect();
        }
    }
    panic!("no frozen redaction tokens for {contest}");
}

fn project(mode: &str) -> String {
    let (store, c) = started(mode);
    let mode = mode.to_string();
    store
        .call(move |db| Ok(projection(&mode, db, c, HOSTILE)))
        .unwrap()
}

fn projection(mode: &str, db: &rusqlite::Connection, c: ContestId, text: &str) -> String {
    match mode {
        "forge" => orchestrator::forge::blind(text, &squad_store::forge::setup(db, c).unwrap()),
        "forecast" => {
            orchestrator::forecast::blind(text, &squad_store::forecast::setup(db, c).unwrap())
        }
        "negotiate" => {
            orchestrator::negotiate::blind(text, &squad_store::negotiate::setup(db, c).unwrap())
        }
        "bass_ackwards_prd" => {
            let setup = squad_store::bass_ackwards_prd::setup(db, c).unwrap();
            let mut v = json!({"statement": text});
            orchestrator::bass_ackwards_prd::blind(&mut v, &setup);
            v["statement"].as_str().unwrap().to_string()
        }
        "decide" => {
            let mut records = db
                .prepare("SELECT body FROM decide_records WHERE contest_id=?")
                .unwrap();
            let setup = records
                .query_map([c.to_string()], |r| r.get::<_, String>(0))
                .unwrap()
                .find_map(|raw| serde_json::from_str::<decide::DecideSetup>(&raw.unwrap()).ok())
                .expect("decide setup record");
            router::decide::blind(json!({"decision": text}), &setup)["decision"]
                .as_str()
                .unwrap()
                .to_string()
        }
        "blow_it_up" => {
            let raw: String = db
                .query_row(
                    "SELECT plan_json FROM contests WHERE contest_id=?",
                    [c.to_string()],
                    |r| r.get(0),
                )
                .unwrap();
            let plan: ContestPreflight = serde_json::from_str(&raw).unwrap();
            router::blow_it_up::blind(json!({"reason": text}), &plan)["reason"]
                .as_str()
                .unwrap()
                .to_string()
        }
        "tune" => {
            let run = squad_store::tune::load(db, c).unwrap();
            let mut v = json!({"input": text});
            router::tune::redact_value(&mut v, &run.redaction);
            v["input"].as_str().unwrap().to_string()
        }
        "readback" => orchestrator::readback::blind(text, &frozen_tokens(db, c)).0,
        "debug" => orchestrator::debugging::blind(text, &frozen_tokens(db, c)).0,
        "rank" => orchestrator::blind::redact(text, &frozen_tokens(db, c)),
        "trace" => {
            let v = orchestrator::trace::blind_input(
                json!({"extractions": {"a": {"claims": [{"claim": text, "scopeNote": text}]}}}),
                &frozen_tokens(db, c),
            );
            let claim = &v["extractions"]["a"]["claims"][0];
            assert_eq!(
                claim["claim"], claim["scopeNote"],
                "trace prose fields differ"
            );
            claim["claim"].as_str().unwrap().to_string()
        }
        other => panic!("no projection for {other}"),
    }
}
use squad_protocol::decide;

macro_rules! blind_mode {
    ($test:ident, $mode:literal) => {
        #[test]
        fn $test() {
            let out = project($mode);
            assert_blind($mode, &out);
            // Deterministic, and the human-visible original is the untouched constant.
            assert_eq!(
                out,
                project($mode),
                "{}: projection is not deterministic",
                $mode
            );
            assert!(HOSTILE.contains("ADA wrote this") && !HOSTILE.contains("[redacted]"));
        }
    };
}
blind_mode!(forge_blind_projection_redacts_the_hostile_roster, "forge");
blind_mode!(
    forecast_blind_projection_redacts_the_hostile_roster,
    "forecast"
);
blind_mode!(decide_blind_projection_redacts_the_hostile_roster, "decide");
blind_mode!(
    negotiate_blind_projection_redacts_the_hostile_roster,
    "negotiate"
);
blind_mode!(
    blow_it_up_blind_projection_redacts_the_hostile_roster,
    "blow_it_up"
);
blind_mode!(
    readback_blind_projection_redacts_the_hostile_roster,
    "readback"
);
blind_mode!(debug_blind_projection_redacts_the_hostile_roster, "debug");
blind_mode!(trace_blind_projection_redacts_the_hostile_roster, "trace");
blind_mode!(rank_blind_projection_redacts_the_hostile_roster, "rank");
blind_mode!(tune_blind_projection_redacts_the_hostile_roster, "tune");
blind_mode!(
    prd_blind_projection_redacts_the_hostile_roster,
    "bass_ackwards_prd"
);

/// Tune's executor runs the candidate prompt exactly as written: it is exempt from the
/// identity scrub and is delivered byte for byte, while every blind request is scrubbed.
#[tokio::test]
async fn tune_executor_requests_are_exempt_and_byte_exact_while_blind_requests_are_scrubbed() {
    let prompt = "PROMPT answer clearly as member0 with PERSONA_1 via fixture/member2";
    let task = "provide helpful concise answers for MEMBER3 and fixture/judge";
    let bodies = tune_harness::bodies_for_identity_prompt(prompt, task).await;
    let content = |b: &serde_json::Value| {
        b["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["content"].as_str().unwrap_or_default().to_string())
            .collect::<Vec<_>>()
    };
    let executors: Vec<_> = bodies
        .iter()
        .filter(|b| {
            b.get("response_format").is_none()
                && content(b)
                    .iter()
                    .any(|c| c.contains("PROMPT answer clearly"))
        })
        .collect();
    assert!(
        !executors.is_empty(),
        "no executor request carried the candidate prompt"
    );
    for body in &executors {
        assert!(
            content(body).iter().any(|c| c.contains(prompt)),
            "executor request did not carry the prompt byte for byte: {:?}",
            content(body)
        );
        assert!(
            !content(body).iter().any(|c| c.contains("[redacted]")),
            "executor input was scrubbed"
        );
    }
    let mut blind = 0;
    for body in bodies.iter().filter(|b| b.get("response_format").is_some()) {
        let user = content(body).get(1).cloned().unwrap_or_default();
        if !user.contains("Authorized input: ") {
            continue;
        }
        blind += 1;
        for secret in [
            "member0",
            "PERSONA_1",
            "fixture/member2",
            "member2",
            "MEMBER3",
            "fixture/judge",
        ] {
            assert!(
                !word(secret).is_match(&user),
                "a blind request leaked {secret:?}:\n{user}"
            );
        }
    }
    assert!(blind > 10, "only {blind} blind requests were inspected");
}

/// The negotiator's table view carries only the counterpart's public move fields
/// (`message`, `response`, `counterOffer`) AND scrubs the hostile roster, providers such
/// as `x-ai` included, out of them.
#[test]
fn negotiate_table_projection_is_field_restricted_and_redacted() {
    use squad_core::runtime::negotiate::projection;
    const LEAK: &str = "HIDDEN_INTEREST_LEAK";
    let (store, c) = started("negotiate");
    let mut setup = store
        .call(move |db| Ok(squad_store::negotiate::setup(db, c).unwrap()))
        .unwrap();
    let negotiator = setup.participants[0].clone();
    let counterpart = setup.participants[1].clone();
    let table = negotiate::NegotiateTable {
        table: 1,
        negotiator: negotiator.clone(),
        counterpart: counterpart.clone(),
        rounds_used: 1,
        outcome: None,
        turns: vec![
            negotiate::NegotiateTurn {
                table: 1,
                round: 1,
                actor: negotiator.clone(),
                counterpart: false,
                body: json!({"message": HOSTILE, "offer": "ten"}),
            },
            negotiate::NegotiateTurn {
                table: 1,
                round: 1,
                actor: counterpart,
                counterpart: true,
                body: json!({
                    "message": HOSTILE,
                    "response": "counter",
                    "counterOffer": HOSTILE,
                    "violatedInterest": LEAK,
                    "hiddenNote": LEAK,
                }),
            },
        ],
    };
    setup.tables = vec![table.clone()];
    let brief = negotiate::NegotiateBrief {
        strategy_sheet: "s".into(),
        public_scenario: "p".into(),
        counterpart_briefs: vec![],
        assumptions_to_confirm: vec![],
    };
    let data = projection(
        &setup,
        negotiate::NegotiateStatus::Rehearsing,
        1,
        &Target::Member(negotiator),
        &WorkKind::NegotiateMove,
        &Default::default(),
        Some(&brief),
        &Default::default(),
        &[table],
    )
    .unwrap();
    let turns = data["transcript"].as_array().unwrap();
    let public = turns[1]["body"].as_object().unwrap();
    let mut keys: Vec<_> = public.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, ["counterOffer", "message", "response"]);
    let whole = data.to_string();
    assert!(!whole.contains(LEAK) && !whole.contains("violatedInterest"));
    assert_blind("negotiate message", public["message"].as_str().unwrap());
    assert_blind(
        "negotiate counterOffer",
        public["counterOffer"].as_str().unwrap(),
    );
    assert!(public["response"].as_str().unwrap() == "counter");
}
