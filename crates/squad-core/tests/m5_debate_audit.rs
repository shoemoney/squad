//! M5 admission, held-input isolation and terminal-release regression fixtures.
#[path = "support/m6_room.rs"]
mod support;
use serde_json::{Value, json};
use squad_protocol::*;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use support::{fake_openrouter::*, *};
use wiremock::{
    Mock, Request as HttpRequest, Respond, ResponseTemplate,
    matchers::{method, path},
};

#[derive(Clone, Default)]
struct Answers {
    calls: Arc<Mutex<BTreeMap<String, usize>>>,
    retry_first_turn: bool,
    fail_members: Vec<usize>,
}
impl Respond for Answers {
    fn respond(&self, request: &HttpRequest) -> ResponseTemplate {
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        let messages = body["messages"].to_string();
        let model = body["model"].as_str().unwrap();
        if model == "fixture/judge" {
            if messages.contains("Score round ") {
                return structured_judge(json!({"scores":{
                    "member0":{"score":7,"reason":"PRIVATE_SCORE_REASON"},
                    "member1":{"score":7,"reason":"PRIVATE_SCORE_REASON"}
                }}));
            }
            let scores = ["member0", "member1"].map(|nick| {
                json!({"nick":nick,"criteria":
                (["fit","feasibility","originality","survivedAttacks"].map(|c|
                    json!({"criterionId":c,"score":5,"reasoning":"supported"})))})
            });
            return structured_judge(json!({"winner":"member0","scores":scores,
                "reasoning":"Tie breaker","whatToStealFromLoser":[],"tallyOverrideReason":null}));
        }
        let turn = messages.contains("round metadata only");
        let key = format!("{model}:{turn}");
        let mut calls = self.calls.lock().unwrap();
        let count = calls.entry(key).or_default();
        *count += 1;
        if turn
            && (self
                .fail_members
                .iter()
                .any(|i| model == format!("fixture/member{i}"))
                || (self.retry_first_turn && model == "fixture/member0" && *count == 1))
        {
            return provider_error(503).set_delay(Duration::from_millis(350));
        }
        if turn {
            return streaming(&format!("CURRENT_TURN_{model}"), "length")
                .set_delay(Duration::from_millis(100));
        }
        if messages.contains("EARLIER_ORDINARY") {
            return streaming("EARLIER_ANSWER", "stop").set_delay(Duration::from_secs(1));
        }
        streaming(&format!("SOURCE_{model}"), "stop")
    }
}
async fn mount(f: &Fixture, answers: Answers) {
    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .respond_with(answers)
        .mount(&f.fake.server)
        .await;
}
fn accepted(result: RequestResult) {
    assert!(
        matches!(result, RequestResult::Accepted { .. }),
        "{result:?}"
    );
}
async fn terminal(f: &Fixture) {
    let result = tokio::time::timeout(Duration::from_secs(15), async {
        while f.scalar("SELECT COUNT(*) FROM contests WHERE kind='debate' AND json_extract(state_json,'$.state') IN ('COMPLETED','COMPLETE_UNJUDGED')") != 1 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await;
    if result.is_err() {
        let db = f.db();
        let mut q = db
            .prepare("SELECT kind,status,attempt,error_json FROM work")
            .unwrap();
        let rows = q
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, u32>(2)?,
                    r.get::<_, Option<String>>(3)?,
                ))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let states: String = db
            .query_row("SELECT state_json FROM contests", [], |r| r.get(0))
            .unwrap();
        panic!(
            "terminal timeout: {states}; {rows:?}; requests {:?}",
            f.fake.captured_bodies().await
        );
    }
}

#[tokio::test]
async fn earlier_work_drains_held_quotes_stay_out_of_both_turn_attempts_then_release() {
    let f = Fixture::with_roster(3).await;
    mount(
        &f,
        Answers {
            retry_first_turn: true,
            ..Default::default()
        },
    )
    .await;
    let room = f.open(OpenMode::Interactive).await;
    accepted(
        room.submit(f.request("@member2 QUOTE_ONLY_FOR_LATER"))
            .await,
    );
    f.finals(1).await;
    let quote: EventId = f
        .db()
        .query_row(
            "SELECT id FROM events WHERE kind='message.final'",
            [],
            |r| r.get::<_, String>(0),
        )
        .unwrap()
        .parse()
        .unwrap();
    accepted(room.submit(f.request("@member0 EARLIER_ORDINARY")).await);
    f.wait(|| f.scalar("SELECT COUNT(*) FROM attempts WHERE target='member0'") == 1)
        .await;
    accepted(
        room.submit(f.request("@member0 @member1 $debate $ROUNDS=1 Compare designs"))
            .await,
    );
    assert_eq!(
        f.scalar("SELECT COUNT(*) FROM work WHERE kind='debate.turn'"),
        0
    );
    let mut held = f.request("@member0 @member1 HELD_HUMAN_CANARY");
    let Request::UserMessage { input, parsed, .. } = &mut held else {
        panic!("message")
    };
    input.quote = Some(vec![quote]);
    let ParsedInput::Message { input, .. } = parsed else {
        panic!("parsed message")
    };
    input.quote = vec![quote];
    accepted(room.submit(held).await);
    assert_eq!(f.scalar("SELECT COUNT(*) FROM history WHERE context_id IN ('member0','member1') AND (content LIKE '%HELD_HUMAN_CANARY%' OR content LIKE '%SOURCE_fixture/member2%')"), 0);
    terminal(&f).await;
    f.finals(4).await; // outsider quote, earlier reply, and the two released ordinary replies
    let bodies = f.fake.captured_bodies().await;
    let turns: Vec<_> = bodies
        .iter()
        .filter(|b| b["messages"].to_string().contains("round metadata only"))
        .collect();
    assert_eq!(turns.len(), 5); // opening retry plus four accepted turns
    let first: Vec<_> = turns
        .iter()
        .filter(|b| {
            b["model"] == "fixture/member0" && b["messages"].to_string().contains("opening round 0")
        })
        .collect();
    assert_eq!(first.len(), 2);
    assert_eq!(first[0]["messages"], first[1]["messages"]);
    for body in turns {
        let text = body["messages"].to_string();
        assert!(!text.contains("HELD_HUMAN_CANARY") && !text.contains("SOURCE_fixture/member2"));
        if text.contains("opening round 0") {
            assert!(!text.contains("CURRENT_TURN_"));
        }
        if body["model"] == "fixture/member0" {
            assert!(text.contains("EARLIER_ANSWER"));
        }
        for message in body["messages"].as_array().unwrap() {
            if message["name"] == "squad" {
                assert_eq!(message["role"], "user");
            }
        }
    }
    assert!(!bodies.iter().any(|b| b["model"] == "fixture/judge"));
    assert_eq!(f.scalar("SELECT COUNT(*) FROM deferred_history"), 0);
    assert_eq!(f.scalar("SELECT COUNT(*) FROM history WHERE context_id IN ('member0','member1') AND content LIKE '%HELD_HUMAN_CANARY%'"), 2);
    room.shutdown().await.unwrap();
}

#[tokio::test]
async fn captured_turns_hide_scores_until_both_debaters_receive_the_verdict() {
    let f = Fixture::new().await;
    mount(&f, Answers::default()).await;
    let room = f.open(OpenMode::Interactive).await;
    accepted(
        room.submit(f.request("@member0 @member1 $debate $ROUNDS=2 @judge Compare designs"))
            .await,
    );
    terminal(&f).await;
    assert_eq!(
        f.scalar("SELECT COUNT(*) FROM events WHERE kind='verdict'"),
        1
    );
    let bodies = f.fake.captured_bodies().await;
    assert_eq!(
        bodies
            .iter()
            .filter(|b| b["model"] != "fixture/judge"
                && b["messages"].to_string().contains("Closing statement"))
            .count(),
        2
    );
    let mut scores = 0;
    for body in &bodies {
        let text = body["messages"].to_string();
        if body["model"] != "fixture/judge" {
            for forbidden in [
                "PRIVATE_SCORE_REASON",
                "roundTally",
                "unscored",
                "roundScores",
            ] {
                assert!(!text.contains(forbidden), "{forbidden}: {text}");
            }
        } else {
            assert_eq!(body["messages"].as_array().unwrap().len(), 2);
            assert_eq!(body["messages"][0]["role"], "system");
            assert_eq!(body["messages"][1]["role"], "user");
            assert!(!text.contains("PERSONA_"));
            if text.contains("Score round ") {
                scores += 1;
                assert_eq!(body["reasoning"]["max_tokens"], 1024);
                assert_eq!(body["max_tokens"], 1536);
                assert!(!text.contains("PRIVATE_SCORE_REASON"));
            } else {
                assert!(text.contains("PRIVATE_SCORE_REASON") && text.contains("roundTally"));
            }
        }
    }
    assert_eq!(scores, 2);
    for nick in ["member0", "member1"] {
        let (role, content): (String, String) = f.db().query_row(
            "SELECT role,content FROM history WHERE context_id=? AND projection_key='verdict.announcement'", [nick],
            |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!(role, "user");
        assert!(content.contains("roundTally"));
        let count: i64 = f.db().query_row("SELECT COUNT(*) FROM history WHERE context_id=? AND projection_key='verdict.round.outcome' AND content LIKE '%PRIVATE_SCORE_REASON%'", [nick], |r| r.get(0)).unwrap();
        assert_eq!(count, 2);
    }
    accepted(room.submit(f.request("@all Explain the verdict")).await);
    f.finals(2).await;
    let after = f.fake.captured_bodies().await;
    assert_eq!(after.len(), bodies.len() + 2);
    for body in &after[bodies.len()..] {
        let messages = body["messages"].as_array().unwrap();
        let records: Vec<_> = messages
            .iter()
            .filter(|m| {
                m["content"]
                    .as_str()
                    .unwrap()
                    .contains("PRIVATE_SCORE_REASON")
            })
            .collect();
        assert_eq!(records.len(), 2);
        for message in records {
            assert_eq!(message["role"], "user");
            assert_eq!(message["name"], "squad");
            assert!(
                message["content"]
                    .as_str()
                    .unwrap()
                    .starts_with("[squad notice]")
            );
        }
        assert_eq!(
            messages
                .iter()
                .filter(|m| m["content"].as_str().unwrap().contains("roundTally"))
                .count(),
            1
        );
    }
    room.shutdown().await.unwrap();
}

#[tokio::test]
async fn pending_pick_survives_restart_without_expiry_or_duplicate_calls_and_cancel_keeps_answers()
{
    let f = Fixture::with_roster(3).await;
    mount(&f, Answers::default()).await;
    let room = f.open(OpenMode::Interactive).await;
    accepted(room.submit(f.request("@all $debate Which design?")).await);
    f.wait(|| f.scalar("SELECT COUNT(*) FROM events WHERE kind='contest.pick_requested'") == 1)
        .await;
    let Some(ContestState::Selection(before)) = room.state().await.active_contest else {
        panic!("selection")
    };
    assert_eq!(before.answers.len(), 3);
    room.shutdown().await.unwrap();
    drop(room);
    let room = f.open(OpenMode::Interactive).await;
    let Some(ContestState::Selection(after)) = room.state().await.active_contest else {
        panic!("selection")
    };
    assert_eq!(before, after);
    assert_eq!(f.fake.captured_bodies().await.len(), 3);
    accepted(
        room.submit(f.request("@member0 Ordinary while picking"))
            .await,
    );
    f.finals(4).await;
    accepted(
        room.submit(Request::Stop {
            request_id: EventId::new(),
            target: None,
        })
        .await,
    );
    assert_eq!(
        f.scalar("SELECT COUNT(*) FROM contest_answers WHERE outcome='answered'"),
        3
    );
    assert_eq!(
        f.scalar("SELECT COUNT(*) FROM events WHERE kind='message.final'"),
        4
    );
    assert!(room.state().await.active_contest.is_none());
    assert_eq!(f.scalar("SELECT COUNT(*) FROM contests WHERE json_extract(pending_request_json,'$.terminalReason')='pick_cancelled'"), 1);
    room.shutdown().await.unwrap();
}

#[tokio::test]
async fn single_and_double_forfeits_finish_and_release_ordinary_queues() {
    for failed in [vec![0], vec![0, 1]] {
        let f = Fixture::new().await;
        mount(
            &f,
            Answers {
                fail_members: failed.clone(),
                ..Default::default()
            },
        )
        .await;
        let room = f.open(OpenMode::Interactive).await;
        accepted(
            room.submit(f.request("@member0 @member1 $debate $ROUNDS=1 Compare designs"))
                .await,
        );
        accepted(
            room.submit(f.request("@member0 @member1 HELD_AFTER_FORFEIT"))
                .await,
        );
        terminal(&f).await;
        f.finals(2).await;
        assert_eq!(f.scalar("SELECT COUNT(*) FROM events WHERE kind='debate.turn' AND json_extract(payload,'$.outcome.status')='forfeit'"), 2 * failed.len() as i64);
        assert_eq!(f.scalar("SELECT COUNT(*) FROM deferred_history"), 0);
        assert_eq!(
            f.scalar("SELECT COUNT(*) FROM work WHERE status IN ('queued','running')"),
            0
        );
        room.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn sqlite_admission_replays_rejects_competing_stale_and_expired_plans_and_freezes_defaults() {
    let mut f = Fixture::new().await;
    f.config.budget.debate_confirm_usd = 0.;
    f.config.budget.daily_max_usd = 100.;
    let ctx = parser::ParseContext {
        roster: f.config.agents.iter().map(|a| a.nick.clone()).collect(),
        ..Default::default()
    };
    let meta = parser::SubmissionMeta::default();
    for rounds in [0, 21] {
        let error = parse(
            &format!("@member0 @member1 $debate $ROUNDS={rounds} Compare designs"),
            &meta,
            &ctx,
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::INVALID_COMMAND);
        assert_eq!(error.to_string(), "ROUNDS must be an integer from 1 to 20.");
    }
    for rounds in [1, 20] {
        let ParsedInput::Debate { rounds: actual, .. } = parse(
            &format!("@member0 @member1 $debate $ROUNDS={rounds} Compare designs"),
            &meta,
            &ctx,
        )
        .unwrap() else {
            panic!("debate")
        };
        assert_eq!(actual, rounds);
    }
    let room = f.open(OpenMode::OneShot).await;
    let request = f.request("@member0 @member1 $debate Compare designs");
    let result = room.submit(request.clone()).await;
    let RequestResult::PendingConfirmation {
        preflight: PendingPreflight::Debate(plan),
        ..
    } = &result
    else {
        panic!("{result:?}")
    };
    assert_eq!(plan.rounds, 10);
    assert_eq!(room.submit(request).await, result);
    assert_eq!(f.scalar("SELECT COUNT(*) FROM contests"), 1);
    assert_eq!(
        f.scalar("SELECT COUNT(*) FROM events WHERE kind='debate.preflight'"),
        1
    );
    assert_eq!(f.scalar("SELECT COUNT(*) FROM work"), 0);
    let RequestResult::Rejected { error, .. } = room
        .submit(Request::DebateConfirm {
            request_id: EventId::new(),
            debate_id: plan.debate_id,
            plan_id: PlanId::new(),
        })
        .await
    else {
        panic!("stale accepted")
    };
    assert_eq!(error.code, ErrorCode::DEBATE_PLAN_STALE);
    let RequestResult::Rejected { error, .. } = room
        .submit(f.request("@member0 @member1 $debate Competing start"))
        .await
    else {
        panic!("competing start accepted")
    };
    assert_eq!(error.code, ErrorCode::DEBATE_BUSY);
    f.db()
        .execute("UPDATE contests SET expires_at='2000-01-01T00:00:00Z',plan_json=json_set(plan_json,'$.expiresAt','2000-01-01T00:00:00Z')", [])
        .unwrap();
    accepted(
        room.submit(Request::DebateConfirm {
            request_id: EventId::new(),
            debate_id: plan.debate_id,
            plan_id: plan.plan_id,
        })
        .await,
    );
    assert_eq!(f.scalar("SELECT COUNT(*) FROM contests WHERE json_extract(state_json,'$.state')='EXPIRED' AND active=0"), 1);
    assert_eq!(f.scalar("SELECT COUNT(*) FROM events WHERE kind='debate.preflight.result' AND json_extract(payload,'$.status')='expired'"), 1);
    assert_eq!(f.fake.captured_bodies().await.len(), 0);
    room.shutdown().await.unwrap();
}

#[tokio::test]
async fn restart_keeps_frozen_word_limits_and_final_composition_provenance() {
    let mut f = Fixture::new().await;
    f.config.budget.debate_confirm_usd = 0.;
    f.config.prompts.limits.turn_words = 123;
    f.config.prompts.limits.final_turn_words = 456;
    mount(&f, Answers::default()).await;
    let room = f.open(OpenMode::OneShot).await;
    let result = room
        .submit(f.request("@member0 @member1 $debate $ROUNDS=1 Compare designs"))
        .await;
    let RequestResult::PendingConfirmation {
        preflight: PendingPreflight::Debate(plan),
        ..
    } = result
    else {
        panic!("{result:?}")
    };
    let raw: String = f
        .db()
        .query_row(
            "SELECT map_json FROM contest_private_maps WHERE scope_key='engine.plan'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let frozen: squad_core::orchestrator::types::DebatePlan = serde_json::from_str(&raw).unwrap();
    let pack = squad_core::prompts::PromptPack::embedded().unwrap();
    for id in [
        "debate.round0",
        "debate.final.round1",
        "debate.final.rebuttal",
        "judge.round",
        "judge.debate",
    ] {
        assert_eq!(
            frozen.prompts.composition_hashes[id],
            pack.composition_hash(id).unwrap()
        );
    }
    room.shutdown().await.unwrap();
    drop(room);
    f.config.prompts.limits.turn_words = 234;
    f.config.prompts.limits.final_turn_words = 567;
    let room = f.open(OpenMode::Interactive).await;
    accepted(
        room.submit(Request::DebateConfirm {
            request_id: EventId::new(),
            debate_id: plan.debate_id,
            plan_id: plan.plan_id,
        })
        .await,
    );
    terminal(&f).await;
    let bodies = f.fake.captured_bodies().await;
    assert_eq!(bodies.len(), 4);
    for body in bodies {
        let text = body["messages"].to_string();
        let opening = text.contains("opening round 0");
        assert!(
            text.contains(if opening { "123 words" } else { "456 words" }),
            "{text}"
        );
        assert_eq!(text.contains("Closing statement"), !opening);
        assert_eq!(
            text.matches("Closing statement").count(),
            usize::from(!opening)
        );
    }
    let mut query = f.db();
    let tx = query.transaction().unwrap();
    let mut q = tx
        .prepare("SELECT prompt_ref,round FROM work WHERE kind='debate.turn'")
        .unwrap();
    for row in q
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, u32>(1)?)))
        .unwrap()
    {
        let (raw, round) = row.unwrap();
        let reference: PromptRef = serde_json::from_str(&raw).unwrap();
        let id = if round == 0 {
            "debate.round0"
        } else {
            "debate.final.round1"
        };
        assert_eq!(reference.template_id.as_str(), id);
        assert_eq!(
            reference.template_hash.as_str(),
            frozen.prompts.composition_hashes[id]
        );
    }
    drop(q);
    drop(tx);
    room.shutdown().await.unwrap();
}
