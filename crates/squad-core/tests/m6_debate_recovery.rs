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
    fn respond(&self, r: &HttpRequest) -> ResponseTemplate {
        let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
        if body["model"] == "fixture/judge" {
            if body["messages"].to_string().contains("Score round ") {
                tied_round_score("member0", "member1")
            } else {
                let scores=["member0","member1"].map(|nick|serde_json::json!({"nick":nick,"criteria":(["fit","feasibility","originality","survivedAttacks"].map(|id|serde_json::json!({"criterionId":id,"score":5,"reasoning":"Supported"})))}));
                structured_judge(
                    serde_json::json!({"winner":"member0","scores":scores,"reasoning":"Tie breaker","whatToStealFromLoser":[],"tallyOverrideReason":null}),
                )
            }
        } else {
            streaming("A concrete proposal", "stop")
        }
    }
}
async fn answers(f: &Fixture) {
    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .respond_with(Answers)
        .mount(&f.fake.server)
        .await;
}

#[tokio::test]
async fn preflight_round_bounds_price_every_score_and_cancel_without_paid_calls() {
    for rounds in [1, 10, 20] {
        let mut f = Fixture::new().await;
        f.config.budget.debate_confirm_usd = 0.;
        f.config.budget.daily_max_usd = 100.;
        let room = f.open(OpenMode::Interactive).await;
        let result = room
            .submit(f.request(&format!(
                "@member0 @member1 $debate $ROUNDS={rounds} @judge Compare designs"
            )))
            .await;
        let RequestResult::PendingConfirmation {
            preflight: PendingPreflight::Debate(plan),
            ..
        } = result
        else {
            panic!("{result:?}")
        };
        assert_eq!(plan.rounds, rounds);
        assert_eq!(plan.estimate.model_calls, 3 * rounds + 3);
        assert!(plan.estimate.reserved_usd >= plan.estimate.estimated_usd);
        assert!(plan.estimate.reserved_usd > 0.);
        let until = (plan.expires_at - chrono::Utc::now()).num_seconds();
        assert!((595..=600).contains(&until));
        assert!(matches!(
            room.submit(Request::DebateCancel {
                request_id: EventId::new(),
                debate_id: plan.debate_id,
                plan_id: plan.plan_id
            })
            .await,
            RequestResult::Accepted { .. }
        ));
        assert_eq!(room.state().await.daily_spend.reserved_usd, 0.);
        assert!(f.fake.captured_bodies().await.is_empty());
        room.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn recovered_admitted_debate_accounts_scores_once_with_low_reasoning_ceiling() {
    let f = Fixture::new().await;
    answers(&f).await;
    let request = f.request("@member0 @member1 $debate $ROUNDS=1 @judge Compare designs");
    let room = f.open(OpenMode::OneShot).await;
    let receipt = room.submit_commit_only(request.clone()).await;
    assert!(matches!(receipt, RequestResult::Accepted { .. }));
    room.shutdown().await.unwrap();
    assert!(f.fake.captured_bodies().await.is_empty());
    let room = f.open(OpenMode::Interactive).await;
    f.wait(|| f.scalar("SELECT COUNT(*) FROM events WHERE kind='verdict'") == 1)
        .await;
    assert_eq!(room.submit(request).await, receipt);
    assert_eq!(
        f.scalar("SELECT COUNT(*) FROM events WHERE kind='debate.round_score'"),
        1
    );
    assert_eq!(f.scalar("SELECT COUNT(*) FROM attempts"), 6);
    assert_eq!(f.scalar("SELECT SUM(spent_micros) FROM spend_daily"), 270);
    assert_eq!(f.scalar("SELECT SUM(reserved_micros) FROM spend_daily"), 0);
    let bodies = f.fake.captured_bodies().await;
    let scores: Vec<_> = bodies
        .iter()
        .filter(|b| b["messages"].to_string().contains("Score round "))
        .collect();
    assert_eq!(scores.len(), 1);
    assert_eq!(scores[0]["reasoning"]["max_tokens"], 1024);
    assert_eq!(scores[0]["reasoning"]["effort"], "low");
    assert_eq!(scores[0]["max_tokens"], 1536);
    room.shutdown().await.unwrap();
    let room = f.open(OpenMode::Interactive).await;
    assert_eq!(room.state().await.daily_spend.spent_usd, 0.000270);
    assert_eq!(f.scalar("SELECT COUNT(*) FROM attempts"), 6);
    room.shutdown().await.unwrap();
}

#[tokio::test]
async fn unjudged_debate_has_no_score_work_reservation_or_retroactive_scoring() {
    let f = Fixture::new().await;
    answers(&f).await;
    let room = f.open(OpenMode::Interactive).await;
    room.submit(f.request("@member0 @member1 $debate $ROUNDS=1 Compare designs"))
        .await;
    f.wait(|| {
        f.scalar("SELECT COUNT(*) FROM events WHERE kind='debate.turn'") == 4
            && f.scalar("SELECT COUNT(*) FROM contests WHERE active=1") == 0
    })
    .await;
    assert_eq!(
        f.scalar("SELECT COUNT(*) FROM work WHERE kind='judge.round'"),
        0
    );
    assert_eq!(f.fake.captured_bodies().await.len(), 4);
    room.submit(Request::Judge {
        request_id: EventId::new(),
        debate_id: None,
    })
    .await;
    f.wait(|| f.scalar("SELECT COUNT(*) FROM events WHERE kind='verdict'") == 1)
        .await;
    assert_eq!(f.fake.captured_bodies().await.len(), 5);
    assert_eq!(
        f.scalar("SELECT COUNT(*) FROM events WHERE kind='debate.round_score'"),
        0
    );
    assert_eq!(f.scalar("SELECT SUM(spent_micros) FROM spend_daily"), 225);
    assert_eq!(f.scalar("SELECT SUM(reserved_micros) FROM spend_daily"), 0);
    room.shutdown().await.unwrap();
}

#[tokio::test]
async fn standalone_case_no_wait_recovers_exactly_once_without_reopening_other_work() {
    let f = Fixture::new().await;
    f.fake
        .completions(vec![
            streaming("Evidence", "stop"),
            free_form_case("A private ruling."),
        ])
        .await;
    let room = f.open(OpenMode::Interactive).await;
    room.submit(f.request("@member0 provide evidence")).await;
    f.finals(1).await;
    let evidence = room
        .events_after(None, 500)
        .await
        .unwrap()
        .into_iter()
        .find(|e| matches!(e.content, EnvelopePayload::MessageFinal(_)))
        .unwrap();
    let EnvelopePayload::MessageFinal(message) = &evidence.content else {
        unreachable!()
    };
    let record = parser::RecordSummary {
        event_id: evidence.base.id,
        message_id: Some(message.completed.generation.message_id),
        seq: evidence.commit.seq,
        kind: Text::new("message.final").unwrap(),
        author: evidence.base.sender.clone(),
        recipients: evidence.base.recipients.clone(),
        content: message.content.clone(),
        in_reply_to_message_id: Some(message.in_reply_to_message_id),
        contest_id: None,
        case_id: None,
        plan_event_id: None,
        grid_event_id: None,
        cell_ids: vec![],
        state_version: None,
        superseded: false,
        eligible: true,
    };
    let quote = evidence.base.id;
    room.shutdown().await.unwrap();
    let room = f.open(OpenMode::OneShot).await;
    let context = parser::ParseContext {
        roster: f.config.agents.iter().map(|a| a.nick.clone()).collect(),
        records: vec![record],
        ..Default::default()
    };
    let meta = parser::SubmissionMeta {
        quote: vec![quote],
        ..Default::default()
    };
    let request = Request::from_parsed(
        EventId::new(),
        parse(
            "@judge Decide whether this proposal is coherent.",
            &meta,
            &context,
        )
        .unwrap(),
        &meta,
        false,
    )
    .unwrap();
    let receipt = room.submit_commit_only(request.clone()).await;
    assert!(
        matches!(receipt, RequestResult::Accepted { .. }),
        "{receipt:?}"
    );
    assert_eq!(f.scalar("SELECT COUNT(*) FROM cases"), 1);
    room.shutdown().await.unwrap();
    assert_eq!(f.fake.captured_bodies().await.len(), 1);
    let room = f.open(OpenMode::Interactive).await;
    f.wait(|| f.scalar("SELECT COUNT(*) FROM work WHERE status='completed'") == 2)
        .await;
    assert_eq!(room.submit(request).await, receipt);
    assert_eq!(f.fake.captured_bodies().await.len(), 2);
    assert_eq!(f.scalar("SELECT COUNT(*) FROM attempts"), 2);
    assert_eq!(f.scalar("SELECT SUM(spent_micros) FROM spend_daily"), 90);
    room.shutdown().await.unwrap();
}
