//! Public preflights derive call counts and attempt reservations from each script.
#[path = "support/m6_room.rs"]
mod support;
use squad_protocol::*;
use support::Fixture;

#[tokio::test]
async fn enabled_mode_preflights_match_the_specification_without_provider_calls() {
    let cases = [
        (
            "forge",
            6,
            "@all $forge Write a compact launch post",
            15,
            31,
        ),
        (
            "forecast",
            6,
            "@all $forecast Will the project ship by 2027-06-30?",
            14,
            30,
        ),
        (
            "decide",
            6,
            "@all $decide Choose storage\n--\nSQLite\nPostgreSQL",
            14,
            30,
        ),
        (
            "decide",
            6,
            "@all $decide Choose a database for a small offline app",
            21,
            45,
        ),
        (
            "negotiate",
            6,
            "@all $negotiate Agree a support contract with a vendor",
            29,
            60,
        ),
        (
            "blow_it_up",
            6,
            "@all $blow_it_up Deploy on Friday without a rollback plan",
            13,
            27,
        ),
        (
            "readback",
            6,
            "@all $readback Keep records for thirty days",
            16,
            36,
        ),
        (
            "debug",
            6,
            "@all $debug Requests stall after deployment\n--\nInspect local logs only",
            29,
            63,
        ),
        (
            "trace",
            6,
            "@all $trace When does the service run?\n--\nThe service runs from Monday through Friday.",
            14,
            30,
        ),
        (
            "rank",
            6,
            "@all $rank Impact\n--\nFix data loss\nImprove search\nRefresh icons",
            7,
            15,
        ),
        (
            "tune",
            6,
            "@all $tune Answer briefly\n--\nReturn one lowercase category",
            60,
            126,
        ),
        (
            "tune",
            3,
            "@all $tune Answer briefly\n--\nReturn one lowercase category",
            57,
            120,
        ),
        (
            "tune",
            6,
            "$TUNE_GENERATIONS=1 @all $tune Answer briefly\n--\nReturn one lowercase category",
            34,
            72,
        ),
        (
            "tune",
            3,
            "$TUNE_GENERATIONS=1 @all $tune Answer briefly\n--\nReturn one lowercase category",
            31,
            66,
        ),
        (
            "bass_ackwards_prd",
            6,
            "@all $bass_ackwards_prd An offline notes app is ready for release",
            28,
            60,
        ),
        (
            "bass_ackwards_prd",
            3,
            "@all $bass_ackwards_prd An offline notes app is ready for release",
            16,
            36,
        ),
        (
            "bass_ackwards_prd",
            6,
            "$PRD_HUMAN_REVIEW=1 @all $bass_ackwards_prd An offline notes app is ready for release",
            29,
            63,
        ),
    ];
    let mut checked = 0;
    for (mode, members, command, calls, attempts) in cases {
        let kind: ContestKind = serde_json::from_value(serde_json::json!(mode)).unwrap();
        if !kind.is_built() {
            continue;
        }
        let mut f = Fixture::with_roster(members).await;
        f.config.budget.daily_max_usd = 1_000.;
        f.config.budget.debate_confirm_usd = 0.;
        let room = f.open(OpenMode::OneShot).await;
        let result = room.submit(f.request(command)).await;
        assert!(
            matches!(result, RequestResult::PendingConfirmation { .. }),
            "{mode}/{members}: {result:?}"
        );
        let preflight = room
            .events_after(None, 500)
            .await
            .unwrap()
            .into_iter()
            .find_map(|event| {
                let content = serde_json::to_value(event.content).unwrap();
                (content["kind"] == "contest.preflight").then(|| content["payload"].clone())
            })
            .expect("public preflight event");
        assert_eq!(
            preflight["estimate"]["modelCalls"], calls,
            "{mode}/{members} calls"
        );
        assert_eq!(
            preflight["settings"]["reservedAttempts"], attempts,
            "{mode}/{members} attempts"
        );
        assert!(
            f.fake.captured_bodies().await.is_empty(),
            "{mode} dispatched before confirmation"
        );
        room.shutdown().await.unwrap();
        checked += 1;
    }
    assert!(checked > 0);
}
