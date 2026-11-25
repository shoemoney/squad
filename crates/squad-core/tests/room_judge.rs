//! M3.P1.L2.T5: standalone judge questions, verdict follow-ups and explicit debate judging
//! through `RoomHandle` with real SQLite and a mocked provider.
#[path = "support/m6_room.rs"]
mod support;
use squad_protocol::*;
use support::{fake_openrouter::*, *};
use wiremock::{
    Mock, Request as HttpRequest, Respond, ResponseTemplate,
    matchers::{method, path},
};

fn verdict() -> serde_json::Value {
    let scores=["member0","member1"].map(|nick|serde_json::json!({"nick":nick,"criteria":(["fit","feasibility","originality","survivedAttacks"].map(|id|serde_json::json!({"criterionId":id,"score":5,"reasoning":"Supported by the evidence"})))}));
    serde_json::json!({"winner":"member0","scores":scores,"reasoning":"Member0 wins the tie-breaker.","whatToStealFromLoser":[],"tallyOverrideReason":null})
}

#[derive(Clone)]
struct Answers;
impl Respond for Answers {
    fn respond(&self, r: &HttpRequest) -> ResponseTemplate {
        let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
        let messages = body["messages"].to_string();
        if body["model"] == "fixture/judge" {
            if messages.contains("Score round ") {
                tied_round_score("member0", "member1")
            } else if messages.contains("Compare designs") {
                structured_judge(verdict())
            } else {
                free_form_case("The ruling is: the quoted plan is coherent.")
            }
        } else {
            streaming("A supported proposal with a concrete tradeoff.", "stop")
        }
    }
}

async fn fixture() -> Fixture {
    let f = Fixture::new().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .respond_with(Answers)
        .with_priority(10)
        .mount(&f.fake.server)
        .await;
    f
}

fn quoting(f: &Fixture, text: &str, quoted: &CommittedEnvelope) -> Request {
    let (message_id, case_id, content) = match &quoted.content {
        EnvelopePayload::MessageFinal(m) => (
            Some(m.completed.generation.message_id),
            None,
            m.content.clone(),
        ),
        EnvelopePayload::CaseRuling(r) => (None, Some(r.case_id), r.content.to_string()),
        other => panic!("not quotable: {other:?}"),
    };
    let record = parser::RecordSummary {
        event_id: quoted.base.id,
        message_id,
        seq: quoted.commit.seq,
        kind: Text::new(
            serde_json::to_value(&quoted.content).unwrap()["kind"]
                .as_str()
                .unwrap(),
        )
        .unwrap(),
        author: quoted.base.sender.clone(),
        recipients: quoted.base.recipients.clone(),
        content,
        in_reply_to_message_id: None,
        contest_id: None,
        case_id,
        plan_event_id: None,
        grid_event_id: None,
        cell_ids: vec![],
        state_version: None,
        superseded: false,
        eligible: true,
    };
    let context = parser::ParseContext {
        roster: f.config.agents.iter().map(|a| a.nick.clone()).collect(),
        records: vec![record],
        ..Default::default()
    };
    let meta = parser::SubmissionMeta {
        quote: vec![quoted.base.id],
        ..Default::default()
    };
    Request::from_parsed(
        EventId::new(),
        parse(text, &meta, &context).unwrap(),
        &meta,
        false,
    )
    .unwrap()
}

async fn first_event(room: &squad_core::room::RoomHandle, kind: &str) -> CommittedEnvelope {
    room.events_after(None, 500)
        .await
        .unwrap()
        .into_iter()
        .find(|e| serde_json::to_value(&e.content).unwrap()["kind"] == kind)
        .unwrap_or_else(|| panic!("no {kind} event"))
}

#[tokio::test]
async fn standalone_judge_requires_a_quote_and_opens_one_fresh_case_per_question() {
    let f = fixture().await;
    let room = f.open(OpenMode::Interactive).await;
    // Parsing rejects an unquoted judge question before it can reach the room.
    let context = parser::ParseContext {
        roster: f.config.agents.iter().map(|a| a.nick.clone()).collect(),
        ..Default::default()
    };
    let err = parse(
        "@judge is this right?",
        &parser::SubmissionMeta::default(),
        &context,
    )
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::JUDGE_QUOTE_REQUIRED);
    room.submit(f.request("@member0 propose a plan")).await;
    f.finals(1).await;
    let reply = first_event(&room, "message.final").await;
    let result = room
        .submit(quoting(&f, "@judge Is this plan coherent?", &reply))
        .await;
    assert!(
        matches!(result, RequestResult::Accepted { .. }),
        "{result:?}"
    );
    f.wait(|| f.scalar("SELECT COUNT(*) FROM events WHERE kind='case.ruling'") == 1)
        .await;
    assert_eq!(
        f.scalar("SELECT COUNT(*) FROM events WHERE kind='case.opened'"),
        1
    );
    assert_eq!(
        f.scalar("SELECT COUNT(*) FROM events WHERE kind='verdict'"),
        0
    );
    // The judge saw only the question and the quoted material: no personas, no other private context.
    let bodies = f.fake.captured_bodies().await;
    let judge: Vec<_> = bodies
        .iter()
        .filter(|b| b["model"] == "fixture/judge")
        .collect();
    assert_eq!(judge.len(), 1);
    let text = judge[0].to_string();
    assert!(text.contains("Is this plan coherent?"));
    assert!(text.contains("A supported proposal with a concrete tradeoff."));
    assert!(!text.contains("PERSONA_"));
    // Presence returns to idle and the scoreboard is untouched by standalone rulings.
    let state = room.state().await;
    assert!(state.scoreboard.is_empty());
    assert!(state.debates.is_empty());
    room.shutdown().await.unwrap();
}

#[tokio::test]
async fn ruling_followups_reuse_the_sealed_case_and_unknown_cases_are_not_found() {
    let f = fixture().await;
    let room = f.open(OpenMode::Interactive).await;
    room.submit(f.request("@member0 propose a plan")).await;
    f.finals(1).await;
    let reply = first_event(&room, "message.final").await;
    room.submit(quoting(&f, "@judge First question?", &reply))
        .await;
    f.wait(|| f.scalar("SELECT COUNT(*) FROM events WHERE kind='case.ruling'") == 1)
        .await;
    let ruling = first_event(&room, "case.ruling").await;
    let EnvelopePayload::CaseRuling(first) = &ruling.content else {
        panic!()
    };
    let result = room
        .submit(quoting(&f, "@judge And a follow-up?", &ruling))
        .await;
    assert!(
        matches!(result, RequestResult::Accepted { .. }),
        "{result:?}"
    );
    f.wait(|| f.scalar("SELECT COUNT(*) FROM events WHERE kind='case.ruling'") == 2)
        .await;
    // Follow-ups never open another case and stay attached to the original one.
    assert_eq!(
        f.scalar("SELECT COUNT(*) FROM events WHERE kind='case.opened'"),
        1
    );
    assert_eq!(
        f.scalar("SELECT COUNT(*) FROM events WHERE kind='case.followup'"),
        1
    );
    assert_eq!(f.scalar("SELECT COUNT(*) FROM cases"), 1);
    let rulings: Vec<_> = room
        .events_after(None, 500)
        .await
        .unwrap()
        .into_iter()
        .filter_map(|e| match e.content {
            EnvelopePayload::CaseRuling(r) => Some(r.case_id),
            _ => None,
        })
        .collect();
    assert_eq!(rulings, vec![first.case_id, first.case_id]);
    room.shutdown().await.unwrap();
}

#[tokio::test]
async fn explicit_judge_of_an_unjudged_debate_reuses_turns_and_missing_debates_are_rejected() {
    let f = fixture().await;
    let room = f.open(OpenMode::Interactive).await;
    let nothing = room
        .submit(Request::Judge {
            request_id: EventId::new(),
            debate_id: None,
        })
        .await;
    assert!(
        matches!(
            nothing,
            RequestResult::Rejected { ref error, .. } if error.code == ErrorCode::DEBATE_NOT_FOUND
        ),
        "{nothing:?}"
    );
    let unknown = room
        .submit(Request::Judge {
            request_id: EventId::new(),
            debate_id: Some(DebateId::new()),
        })
        .await;
    assert!(
        matches!(unknown, RequestResult::Rejected { .. }),
        "{unknown:?}"
    );
    room.submit(f.request("@member0 @member1 $debate $ROUNDS=1 Compare designs"))
        .await;
    f.wait(|| {
        f.scalar("SELECT COUNT(*) FROM events WHERE kind='debate.turn'") == 4
            && f.scalar("SELECT COUNT(*) FROM contests WHERE active=1") == 0
    })
    .await;
    // No `@judge` was requested, so the debate finished unjudged: no score work or verdict yet.
    assert_eq!(
        f.scalar("SELECT COUNT(*) FROM events WHERE kind='verdict'"),
        0
    );
    assert_eq!(
        f.scalar("SELECT COUNT(*) FROM work WHERE kind LIKE 'judge%'"),
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
    // Judging reuses the committed turns; it never replays them.
    assert_eq!(
        f.scalar("SELECT COUNT(*) FROM events WHERE kind='debate.turn'"),
        4
    );
    assert_eq!(
        f.scalar("SELECT COUNT(*) FROM events WHERE kind='case.opened'"),
        1
    );
    let state = room.state().await;
    assert_eq!(state.debates.len(), 1);
    // A debate that already has its verdict has nothing left to judge.
    let again = room
        .submit(Request::Judge {
            request_id: EventId::new(),
            debate_id: None,
        })
        .await;
    assert!(matches!(again, RequestResult::Rejected { .. }), "{again:?}");
    assert_eq!(
        f.scalar("SELECT COUNT(*) FROM events WHERE kind='verdict'"),
        1
    );
    room.shutdown().await.unwrap();
}
