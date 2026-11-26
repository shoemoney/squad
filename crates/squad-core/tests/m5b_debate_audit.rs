#[path = "support/m6_room.rs"]
mod support;
use squad_protocol::*;
use support::{fake_openrouter::*, *};
use wiremock::{
    Mock, Request as HttpRequest, Respond, ResponseTemplate,
    matchers::{method, path},
};

#[derive(Clone)]
struct Answers;
impl Respond for Answers {
    fn respond(&self, request: &HttpRequest) -> ResponseTemplate {
        let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
        if body["model"] == "fixture/judge" {
            if body["messages"].to_string().contains("Score round ") {
                tied_round_score("member0", "member1")
            } else {
                let scores = ["member0", "member1"].map(|nick| serde_json::json!({"nick":nick,"criteria":(["fit","feasibility","originality","survivedAttacks"].map(|criterion| serde_json::json!({"criterionId":criterion,"score":5,"reasoning":"Supported"})))}));
                structured_judge(
                    serde_json::json!({"winner":"member0","scores":scores,"reasoning":"Tie breaker","whatToStealFromLoser":[],"tallyOverrideReason":null}),
                )
            }
        } else {
            streaming(
                &format!(
                    "{}_{}",
                    if body["messages"].to_string().contains("SOURCE_CANARY") {
                        "TURN_CANARY"
                    } else {
                        "SOURCE_CANARY"
                    },
                    body["model"].as_str().unwrap().replace('/', "_")
                ),
                "stop",
            )
        }
    }
}
async fn mount(f: &Fixture) {
    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .respond_with(Answers)
        .mount(&f.fake.server)
        .await;
}

#[tokio::test]
async fn question_first_pick_confirms_frozen_sources_without_calling_provider() {
    let mut f = Fixture::new().await;
    f.config.budget.debate_confirm_usd = 0.;
    mount(&f).await;
    let room = f.open(OpenMode::Interactive).await;
    room.submit(f.request("@all $debate $ROUNDS=1 @judge Compare designs"))
        .await;
    f.wait(|| f.scalar("SELECT COUNT(*) FROM events WHERE kind='contest.pick_requested'") == 1)
        .await;
    let events = room.events_after(None, 500).await.unwrap();
    let pending = events
        .iter()
        .find_map(|e| {
            if let EnvelopePayload::ContestPickRequested(p) = &e.content {
                Some(p.clone())
            } else {
                None
            }
        })
        .unwrap();
    let sources: Vec<_> = events
        .iter()
        .filter_map(|e| {
            if let EnvelopePayload::MessageFinal(m) = &e.content {
                Some(m.completed.generation.message_id)
            } else {
                None
            }
        })
        .collect();
    let before = f.fake.captured_bodies().await;
    assert_eq!(before.len(), 2);
    for body in &before {
        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages[0]["role"], "system");
        assert!(
            messages
                .iter()
                .any(|m| m["role"] == "user" && m["name"] == "me")
        );
        assert!(
            !messages
                .iter()
                .any(|m| m.to_string().contains("SOURCE_CANARY"))
        );
    }
    let result = room
        .submit(Request::ContestPick {
            request_id: EventId::new(),
            contest_id: pending.contest_id,
            nicks: pending.eligible,
        })
        .await;
    let RequestResult::PendingConfirmation {
        preflight: PendingPreflight::Debate(plan),
        ..
    } = result
    else {
        panic!("{result:?}")
    };
    assert_eq!(f.fake.captured_bodies().await.len(), 2);
    let picked = room
        .events_after(None, 500)
        .await
        .unwrap()
        .into_iter()
        .find_map(|e| {
            if let EnvelopePayload::ContestPicked(p) = e.content {
                Some(p)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(picked.source_message_ids.len(), 2);
    assert!(
        sources
            .iter()
            .all(|id| picked.source_message_ids.contains(id))
    );
    room.submit(Request::DebateConfirm {
        request_id: EventId::new(),
        debate_id: plan.debate_id,
        plan_id: plan.plan_id,
    })
    .await;
    f.wait(|| f.scalar("SELECT COUNT(*) FROM events WHERE kind='verdict'") == 1)
        .await;
    let bodies = f.fake.captured_bodies().await;
    assert_eq!(
        bodies.len(),
        6,
        "two collection replies, two turns, one score and verdict"
    );
    for body in &bodies[2..] {
        let text = body["messages"].to_string();
        for nick in ["member0", "member1"] {
            assert_eq!(
                text.matches(&format!("SOURCE_CANARY_fixture_{nick}"))
                    .count(),
                1,
                "{body}"
            );
        }
    }
    assert_eq!(
        f.scalar("SELECT COUNT(*) FROM events WHERE kind='debate.opening'"),
        0
    );
    assert_eq!(
        f.scalar("SELECT COUNT(*) FROM history WHERE content LIKE 'SOURCE_CANARY%'"),
        2
    );
    room.shutdown().await.unwrap();
}

#[tokio::test]
async fn cancelled_question_first_pick_keeps_ordinary_replies_and_histories() {
    let f = Fixture::new().await;
    mount(&f).await;
    let room = f.open(OpenMode::Interactive).await;
    room.submit(f.request("@all $debate Compare designs")).await;
    f.wait(|| f.scalar("SELECT COUNT(*) FROM events WHERE kind='contest.pick_requested'") == 1)
        .await;
    room.submit(Request::Stop {
        request_id: EventId::new(),
        target: None,
    })
    .await;
    assert_eq!(
        f.scalar("SELECT COUNT(*) FROM events WHERE kind='message.final'"),
        2
    );
    assert_eq!(
        f.scalar("SELECT COUNT(*) FROM history WHERE content LIKE 'SOURCE_CANARY%'"),
        2
    );
    assert_eq!(f.scalar("SELECT COUNT(*) FROM contests WHERE active=1"), 0);
    assert_eq!(f.fake.captured_bodies().await.len(), 2);
    room.shutdown().await.unwrap();
}

#[tokio::test]
async fn duplicate_model_members_keep_separate_scores_across_receipt_replay_and_restart() {
    let mut f = Fixture::new().await;
    f.config.agents[1].model = f.config.agents[0].model.clone();
    mount(&f).await;
    let room = f.open(OpenMode::Interactive).await;
    let request = f.request("@member0 @member1 $debate $ROUNDS=1 @judge Compare designs");
    let receipt = room.submit(request.clone()).await;
    f.wait(|| f.scalar("SELECT COUNT(*) FROM events WHERE kind='verdict'") == 1)
        .await;
    assert_eq!(room.submit(request.clone()).await, receipt);
    assert_eq!(f.scalar("SELECT wins FROM scores WHERE nick='member0'"), 1);
    assert_eq!(
        f.scalar("SELECT losses FROM scores WHERE nick='member1'"),
        1
    );
    assert_eq!(
        f.scalar("SELECT losses FROM scores WHERE nick='member0'"),
        0
    );
    assert_eq!(f.scalar("SELECT wins FROM scores WHERE nick='member1'"), 0);
    room.shutdown().await.unwrap();
    let room = f.open(OpenMode::Interactive).await;
    assert_eq!(room.submit(request).await, receipt);
    assert_eq!(f.scalar("SELECT wins FROM scores WHERE nick='member0'"), 1);
    assert_eq!(
        f.scalar("SELECT COUNT(*) FROM events WHERE kind='verdict'"),
        1
    );
    assert_eq!(f.fake.captured_bodies().await.len(), 6);
    room.shutdown().await.unwrap();
}

#[tokio::test]
async fn oversized_debate_is_refused_before_dispatch() {
    assert!(matches!(
        squad_agents::context::judge_messages(
            "Judge only this sealed case".into(),
            "Mandatory selected case evidence".repeat(1000),
            4096,
            1000,
        ),
        Err(squad_agents::context::ContextError::TooLarge { .. })
    ));
    let f = Fixture::new().await;
    let room = f.open(OpenMode::OneShot).await;
    f.db()
        .execute("UPDATE catalog SET context_length=3000", [])
        .unwrap();
    let result = room
        .submit(f.request("@member0 @member1 $debate $ROUNDS=1 @judge Compare designs"))
        .await;
    if let RequestResult::Rejected { error, .. } = result {
        assert_eq!(error.code, ErrorCode::CONTEXT_TOO_LARGE);
    } else {
        let events = room.events_after(None, 500).await.unwrap();
        assert!(events.iter().any(|e| {
            serde_json::to_string(e)
                .unwrap()
                .contains("CONTEXT_TOO_LARGE")
        }));
    }
    assert!(f.fake.captured_bodies().await.is_empty());
    assert_eq!(f.scalar("SELECT COUNT(*) FROM work"), 0);
    room.shutdown().await.unwrap();
}

#[derive(Clone)]
struct DelayedScoring {
    invalid: bool,
}
impl Respond for DelayedScoring {
    fn respond(&self, request: &HttpRequest) -> ResponseTemplate {
        let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
        let text = body["messages"].to_string();
        if body["model"] == "fixture/judge" && text.contains("Score round 2") {
            let response = if self.invalid {
                structured_judge(serde_json::json!({"invalid":true}))
            } else {
                tied_round_score("member0", "member1")
            };
            response.set_delay(std::time::Duration::from_millis(1200))
        } else {
            Answers.respond(request)
        }
    }
}
async fn delayed(f: &Fixture, invalid: bool) {
    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .respond_with(DelayedScoring { invalid })
        .mount(&f.fake.server)
        .await;
}

#[tokio::test]
async fn final_verdict_waits_for_pending_scoring_and_completes_with_an_unscored_round() {
    for invalid in [false, true] {
        let f = Fixture::new().await;
        delayed(&f, invalid).await;
        let room = f.open(OpenMode::Interactive).await;
        room.submit(f.request("@member0 @member1 $debate $ROUNDS=2 @judge Compare designs"))
            .await;
        f.wait(|| {
            f.scalar("SELECT COUNT(*) FROM events WHERE kind='debate.turn'") == 6
                && f.scalar("SELECT COUNT(*) FROM round_scores") == 1
        })
        .await;
        assert_eq!(
            f.scalar("SELECT COUNT(*) FROM events WHERE kind='verdict'"),
            0
        );
        assert_eq!(f.scalar("SELECT COUNT(*) FROM work WHERE kind='judge'"), 0);
        assert_eq!(
            f.scalar("SELECT COALESCE(SUM(wins+losses),0) FROM scores"),
            0
        );
        f.wait(|| f.scalar("SELECT COUNT(*) FROM events WHERE kind='verdict'") == 1)
            .await;
        assert_eq!(f.scalar("SELECT COUNT(*) FROM round_scores"), 2);
        let events = room.events_after(None, 500).await.unwrap();
        let round = events
            .iter()
            .find_map(|e| match &e.content {
                EnvelopePayload::DebateRoundScore(s) if s.round == 2 => Some(s),
                _ => None,
            })
            .unwrap();
        assert_eq!(round.scores.is_none(), invalid);
        let verdict = events
            .iter()
            .find_map(|e| {
                if let EnvelopePayload::Verdict(v) = &e.content {
                    Some(v)
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(
            verdict.verdict.round_tally[&"member0".parse().unwrap()],
            if invalid { 7 } else { 14 }
        );
        assert_eq!(f.scalar("SELECT wins FROM scores WHERE nick='member0'"), 1);
        room.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn stop_all_fences_pending_scores_and_manual_judge_uses_only_committed_scores() {
    let f = Fixture::new().await;
    delayed(&f, false).await;
    let room = f.open(OpenMode::Interactive).await;
    room.submit(f.request("@member0 @member1 $debate $ROUNDS=2 @judge Compare designs"))
        .await;
    f.wait(|| {
        f.scalar("SELECT COUNT(*) FROM events WHERE kind='debate.turn'") == 6
            && f.scalar("SELECT COUNT(*) FROM round_scores") == 1
            && f.scalar("SELECT COUNT(*) FROM work WHERE kind='judge.round' AND round=2 AND status='running'") == 1
    })
    .await;
    let score_work = f.scalar("SELECT COUNT(*) FROM work WHERE kind='judge.round'");
    room.submit(Request::Stop {
        request_id: EventId::new(),
        target: None,
    })
    .await;
    assert_eq!(
        f.scalar(
            "SELECT COUNT(*) FROM work WHERE kind='judge.round' AND status IN ('queued','running')"
        ),
        0
    );
    let result = room
        .submit(Request::Judge {
            request_id: EventId::new(),
            debate_id: None,
        })
        .await;
    assert!(
        matches!(result, RequestResult::Accepted { .. }),
        "{result:?}"
    );
    f.wait(|| f.scalar("SELECT COUNT(*) FROM events WHERE kind='verdict'") == 1)
        .await;
    assert_eq!(
        f.scalar("SELECT COUNT(*) FROM work WHERE kind='judge.round'"),
        score_work
    );
    assert_eq!(f.scalar("SELECT COUNT(*) FROM round_scores"), 1);
    let events = room.events_after(None, 500).await.unwrap();
    let verdict = events
        .iter()
        .find_map(|e| {
            if let EnvelopePayload::Verdict(v) = &e.content {
                Some(v)
            } else {
                None
            }
        })
        .unwrap();
    let prior_score = events
        .iter()
        .find_map(|e| {
            if let EnvelopePayload::DebateRoundScore(score) = &e.content {
                Some(score)
            } else {
                None
            }
        })
        .unwrap();
    assert_ne!(
        verdict.case_id, prior_score.case_id,
        "manual judging retains the original score's case reference"
    );
    assert_eq!(verdict.verdict.round_tally[&"member0".parse().unwrap()], 7);
    room.shutdown().await.unwrap();
}
