mod room_support;

use room_support::{Launcher, accepted, client, config4, message4, open4, store};
use squad_core::RoomHandle;
use squad_protocol::*;

#[tokio::test]
async fn scoped_stop_releases_unrelated_work_without_cancelling_it() {
    let root = tempfile::tempdir().unwrap();
    let launcher = Launcher::default();
    let (room, _provider) = open4(root.path(), &launcher, OpenMode::Interactive).await;

    let plan = pending_tournament(&room).await;
    let contest_id = plan.contest_id;
    accepted(
        room.submit(Request::ContestConfirm {
            request_id: EventId::new(),
            contest_id,
            plan_id: plan.plan_id,
        })
        .await,
    );

    let ordinary_id = EventId::new();
    accepted(
        room.submit(message4("@m0 Preserve this ordinary work", ordinary_id))
            .await,
    );
    let store = store(&launcher);
    let ordinary = || {
        store
            .call(move |db| {
                Ok(db.query_row(
                    "SELECT status, runnable FROM work WHERE trigger_event_id = ?",
                    [ordinary_id.to_string()],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?)),
                )?)
            })
            .unwrap()
    };
    assert_eq!(ordinary(), ("queued".to_owned(), false));

    let unrelated = room
        .submit(Request::ContestStop {
            request_id: EventId::new(),
            contest_id: ContestId::new(),
        })
        .await;
    assert!(matches!(
        unrelated,
        RequestResult::Rejected {
            error: ErrorPayload {
                code: ErrorCode::NOTHING_TO_STOP,
                ..
            },
            ..
        }
    ));
    assert!(matches!(
        room.try_state().await.unwrap().active_contest,
        Some(ContestState::Tournament(state)) if state.contest_id == contest_id
    ));
    assert_eq!(ordinary(), ("queued".to_owned(), false));

    accepted(
        room.submit(Request::ContestStop {
            request_id: EventId::new(),
            contest_id,
        })
        .await,
    );
    assert_eq!(ordinary(), ("queued".to_owned(), true));
    assert!(room.try_state().await.unwrap().active_contest.is_none());
    let counts = store
        .call(move |db| {
            Ok(db.query_row(
                "SELECT COUNT(*), COUNT(CASE WHEN status = 'cancelled' THEN 1 END)
                 FROM work WHERE contest_id = ?",
                [contest_id.to_string()],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
            )?)
        })
        .unwrap();
    assert_eq!(counts, (4, 4));
    drop(store);
    room.shutdown().await.unwrap();
}

#[tokio::test]
async fn pending_tournament_confirmation_obeys_lower_cap_after_reopen() {
    let root = tempfile::tempdir().unwrap();
    let launcher = Launcher::default();
    let (room, provider) = open4(root.path(), &launcher, OpenMode::Interactive).await;
    let plan = pending_tournament(&room).await;
    room.shutdown().await.unwrap();

    let mut config = config4(root.path());
    config.budget.daily_max_usd = 0.;
    let room = RoomHandle::open_with_launcher(
        config,
        "main",
        OpenMode::Interactive,
        client(&provider),
        &launcher,
    )
    .await
    .unwrap();
    let result = room
        .submit(Request::ContestConfirm {
            request_id: EventId::new(),
            contest_id: plan.contest_id,
            plan_id: plan.plan_id,
        })
        .await;
    assert!(matches!(
        result,
        RequestResult::Rejected {
            error: ErrorPayload {
                code: ErrorCode::DAILY_MAX_REACHED,
                ..
            },
            ..
        }
    ));
    let state = room.try_state().await.unwrap();
    assert!(state.active_contest.is_none());
    assert_eq!(state.daily_spend.reserved_usd, 0.);
    room.shutdown().await.unwrap();
}

async fn pending_tournament(room: &RoomHandle) -> ContestPreflight {
    let start = room
        .submit(message4("@all $tournament Propose a plan", EventId::new()))
        .await;
    match start {
        RequestResult::PendingConfirmation {
            preflight: PendingPreflight::Contest(plan),
            ..
        } => plan,
        _ => panic!("expected a tournament preflight, got {start:?}"),
    }
}

#[tokio::test]
async fn over_cap_tournament_is_refused_without_creating_a_contest() {
    let root = tempfile::tempdir().unwrap();
    let launcher = Launcher::default();
    let (room, provider) = open4(root.path(), &launcher, OpenMode::Interactive).await;
    room.shutdown().await.unwrap();

    let mut config = config4(root.path());
    config.budget.daily_max_usd = 0.05;
    let room = RoomHandle::open_with_launcher(
        config,
        "main",
        OpenMode::Interactive,
        client(&provider),
        &launcher,
    )
    .await
    .unwrap();
    let result = room
        .submit(message4("@all $tournament Propose a plan", EventId::new()))
        .await;
    let RequestResult::Rejected { error, .. } = result else {
        panic!("expected a cap rejection, got {result:?}");
    };
    assert_eq!(error.code, ErrorCode::DAILY_MAX_REACHED);
    let text = error.text.as_str();
    assert!(
        text.starts_with(
            "Daily max of $0.05 reached. Raise SQUAD_DAILY_MAX_USD or wait until tomorrow. (Spent $"
        ) && text.contains(" today; this request needs up to $")
            && text.ends_with(" reserved.)"),
        "{text}"
    );
    let state = room.try_state().await.unwrap();
    assert!(state.active_contest.is_none());
    let store = store(&launcher);
    let contests: i64 = store
        .call(|db| Ok(db.query_row("SELECT COUNT(*) FROM contests", [], |r| r.get(0))?))
        .unwrap();
    assert_eq!(contests, 0, "no FAILED contest row for a refused request");
    room.shutdown().await.unwrap();
}
