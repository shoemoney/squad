//! M6.P2.L1.T15 (core half): the largest census, probe and survey inputs stay
//! bounded, priced and context-checked at admission, and paged reads stay
//! bounded. The crosscheck 200-cell grid cap is proven in `crosscheck_policy.rs`
//! (`cap_preserves_all_omissions_...`); effects shedding lives in squad-tui.
#[path = "support/m6_room.rs"]
mod support;
use squad_protocol::*;
use support::*;

async fn rich() -> Fixture {
    let mut f = Fixture::with_roster(12).await;
    f.config.budget.daily_max_usd = 1_000.;
    f
}

async fn preflight(room: &squad_core::room::RoomHandle) -> serde_json::Value {
    room.events_after(None, 500)
        .await
        .unwrap()
        .into_iter()
        .find_map(|e| {
            let v = serde_json::to_value(&e.content).unwrap();
            (v["kind"] == "contest.preflight").then(|| v["payload"].clone())
        })
        .expect("a contest.preflight event")
}

async fn error_codes(room: &squad_core::room::RoomHandle) -> Vec<ErrorCode> {
    room.events_after(None, 500)
        .await
        .unwrap()
        .into_iter()
        .filter_map(|e| match e.content {
            EnvelopePayload::ContestPreflightResult(r) => r.error.map(|e| e.code),
            EnvelopePayload::Error(e) => Some(e.code),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn census_at_five_passes_and_twelve_members_is_bounded_and_priced_up_front() {
    let mut f = rich().await;
    f.config.budget.debate_confirm_usd = 0.;
    let room = f.open(OpenMode::OneShot).await;
    let result = room
        .submit(f.request("@all $census A plan to review $CENSUS_PASSES=5"))
        .await;
    // 130 calls need a human confirmation; the plan is staged, nothing dispatches.
    assert!(
        matches!(result, RequestResult::PendingConfirmation { .. }),
        "{result:?}"
    );
    let plan = preflight(&room).await;
    // 5 passes x (12 hunts + merge + 12 ratings + final) = 130 provider calls.
    assert_eq!(plan["estimate"]["modelCalls"], 130);
    assert_eq!(plan["settings"]["passes"], 5);
    assert_eq!(plan["participants"].as_array().unwrap().len(), 12);
    assert!(
        plan["estimate"]["reservedUsd"].as_f64().unwrap()
            > plan["estimate"]["estimatedUsd"].as_f64().unwrap()
    );
    // No provider call happens at admission.
    assert!(f.fake.captured_bodies().await.is_empty());
    room.shutdown().await.unwrap();
}

#[tokio::test]
async fn census_past_five_passes_is_rejected_with_the_exact_message() {
    let f = rich().await;
    let room = f.open(OpenMode::OneShot).await;
    let ctx = parser::ParseContext {
        roster: f.config.agents.iter().map(|a| a.nick.clone()).collect(),
        ..Default::default()
    };
    let error = parse(
        "@all $census Plan $CENSUS_PASSES=6",
        &Default::default(),
        &ctx,
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::INVALID_COMMAND);
    assert_eq!(
        error.message,
        "CENSUS_PASSES must be an integer from 1 to 5."
    );
    assert_eq!(f.scalar("SELECT COUNT(*) FROM contests"), 0);
    room.shutdown().await.unwrap();
}

#[tokio::test]
async fn a_twelve_member_probe_is_admitted_and_a_thirteenth_member_cannot_exist() {
    let f = rich().await;
    let room = f.open(OpenMode::OneShot).await;
    let result = room
        .submit(f.request("@all $probe Requests stall after deploy; inspect logs and metrics."))
        .await;
    assert!(
        matches!(result, RequestResult::Accepted { .. }),
        "{result:?}"
    );
    let plan = preflight(&room).await;
    assert_eq!(plan["participants"].as_array().unwrap().len(), 12);
    assert_eq!(plan["estimate"]["modelCalls"], 19);
    assert!(f.fake.captured_bodies().await.is_empty());
    room.shutdown().await.unwrap();

    let store = squad_store::Store::open(":memory:").unwrap();
    let thirteen: Vec<MemberNick> = (0..13)
        .map(|i| format!("member{i}").parse().unwrap())
        .collect();
    let refused = squad_core::router::Router::new(store, thirteen, Default::default(), 0);
    assert!(matches!(
        refused,
        Err(squad_store::StoreError::Domain(
            ErrorCode::CONFIG_INVALID,
            _
        ))
    ));
}

#[test]
fn survey_extraction_holds_exactly_twenty_five_claims() {
    let answers: Vec<ContestAnswer> = (0..2)
        .map(|i| ContestAnswer {
            nick: format!("member{i}").parse().unwrap(),
            message_id: MessageId::new(),
            text: format!("answer {i}"),
        })
        .collect();
    let claims = |n: usize| SurveyExtraction {
        claims: (0..n)
            .map(|i| SurveyClaim {
                id: format!("c{i}"),
                claim: format!("distinct claim number {i}"),
                source_answer_ids: vec![answers[i % 2].message_id],
            })
            .collect(),
    };
    assert!(claims(25).validate(&answers).is_ok());
    assert!(claims(26).validate(&answers).is_err());
    // Duplicate claim text and unattributed claims never count toward the 25.
    let mut duplicate = claims(2);
    duplicate.claims[1].claim = "  DISTINCT   claim number 0 ".into();
    assert!(duplicate.validate(&answers).is_err());
    let mut orphan = claims(1);
    orphan.claims[0].source_answer_ids = vec![MessageId::new()];
    assert!(orphan.validate(&answers).is_err());
}

#[tokio::test]
async fn context_that_cannot_fit_fails_admission_before_any_provider_call() {
    for (text, message) in [
        (
            "@all $census A plan to review",
            "Census input exceeds model context",
        ),
        (
            "@all $probe Requests stall after deploy; inspect logs and metrics.",
            "Probe evidence does not fit the model context",
        ),
    ] {
        let f = rich().await;
        let room = f.open(OpenMode::OneShot).await;
        f.db()
            .execute("UPDATE catalog SET context_length=3000", [])
            .unwrap();
        let result = room.submit(f.request(text)).await;
        match &result {
            RequestResult::Rejected { error, .. } => {
                assert_eq!(error.code, ErrorCode::CONTEXT_TOO_LARGE, "{text}");
                assert_eq!(error.text.as_str(), message, "{text}");
            }
            // Otherwise the refusal is committed as the contest's preflight result.
            _ => assert!(
                error_codes(&room)
                    .await
                    .contains(&ErrorCode::CONTEXT_TOO_LARGE),
                "{text}: {result:?}"
            ),
        }
        assert!(f.fake.captured_bodies().await.is_empty());
        assert_eq!(f.scalar("SELECT COUNT(*) FROM work"), 0);
        room.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn paged_reads_of_a_large_roster_stay_bounded_and_complete() {
    let mut f = rich().await;
    f.config.budget.debate_confirm_usd = 0.;
    let room = f.open(OpenMode::OneShot).await;
    room.submit(f.request("@all $census A plan to review $CENSUS_PASSES=5"))
        .await;
    room.submit(f.request("$ROUNDS=3")).await;
    let total = f.scalar("SELECT COUNT(*) FROM events");
    let mut seen = 0;
    let mut cursor = None;
    loop {
        let page = room.events_after(cursor, 3).await.unwrap();
        if page.is_empty() {
            break;
        }
        // Pages hold whole commit groups, so a page may round up past the
        // requested size but never splits a group.
        let last = page.last().unwrap();
        assert_eq!(last.commit.commit_index + 1, last.commit.commit_size);
        assert!(
            page.len() < 3 + 12,
            "page far exceeded its bound: {}",
            page.len()
        );
        cursor = Some(page.last().unwrap().commit.seq);
        seen += page.len();
    }
    assert_eq!(seen as i64, total);
    for limit in [0, 501] {
        assert_eq!(
            room.page_events(None, limit).await.unwrap_err().code,
            ErrorCode::INVALID_COMMAND
        );
    }
    room.shutdown().await.unwrap();
}
