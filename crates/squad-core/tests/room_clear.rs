//! M3.P2.L2.T4: `/clear` invalidates conversational state but preserves shared daily accounting.
#[path = "support/m6_room.rs"]
mod live;
#[path = "room_support.rs"]
mod support;
use live::fake_openrouter::*;
use squad_protocol::*;
use std::time::Duration;
use support::*;

fn clear() -> Request {
    Request::Clear {
        request_id: EventId::new(),
    }
}

#[tokio::test]
async fn clear_resets_transcript_variables_scores_and_expires_old_cursors() {
    let root = tempfile::tempdir().unwrap();
    let launcher = Launcher::default();
    let (room, _) = open(root.path(), &launcher, OpenMode::OneShot).await;
    accepted(
        room.submit_commit_only(message("@a $ROUNDS=4 before clear", EventId::new()))
            .await,
    );
    store(&launcher)
        .call(|db| {
            db.execute(
                "INSERT INTO scores(nick,wins,losses,trophies) VALUES ('a',2,1,3)",
                [],
            )?;
            Ok(())
        })
        .unwrap();
    let old = room
        .page_events(None, 10)
        .await
        .unwrap()
        .last()
        .unwrap()
        .commit
        .seq;
    let marker = accepted(room.submit(clear()).await);
    let events = room.page_events(None, 100).await.unwrap();
    assert_eq!(events.len(), 1, "only the clear marker remains");
    let EnvelopePayload::RoomCleared(cleared) = &events[0].content else {
        panic!("{:?}", events[0])
    };
    assert_eq!(cleared.previous_through, Some(old));
    assert!(marker > old, "cursors stay monotonic across clear");
    assert_eq!(
        room.page_events(Some(old), 10).await.unwrap_err().code,
        ErrorCode::CURSOR_EXPIRED
    );
    let state = room.state().await;
    assert_eq!(state.vars.rounds, 10, "variables return to their defaults");
    assert!(state.scoreboard.is_empty() && state.trophies.is_empty());
    assert!(state.participants.iter().all(|p| p.queue_depth == 0));
    // Queued work from before the clear never runs; replaying its receipt is a fresh submission.
    let work: i64 = store(&launcher)
        .call(|db| Ok(db.query_row("SELECT COUNT(*) FROM work", [], |r| r.get(0))?))
        .unwrap();
    assert_eq!(work, 0);
    room.shutdown().await.unwrap();
}

#[tokio::test]
async fn clear_discards_preflights_and_rejects_confirmation_of_the_old_plan() {
    let root = tempfile::tempdir().unwrap();
    let launcher = Launcher::default();
    let (room, _) = open4(root.path(), &launcher, OpenMode::OneShot).await;
    let result = room
        .submit(message4("@all $survey Which checks help?", EventId::new()))
        .await;
    let RequestResult::PendingConfirmation {
        preflight: PendingPreflight::Contest(plan),
        ..
    } = result
    else {
        panic!("{result:?}")
    };
    accepted(room.submit(clear()).await);
    let state = room.state().await;
    assert!(state.pending_preflights.is_empty());
    assert!(state.active_contest.is_none());
    let late = room
        .submit_commit_only(Request::ContestConfirm {
            request_id: EventId::new(),
            contest_id: plan.contest_id,
            plan_id: plan.plan_id,
        })
        .await;
    assert!(matches!(late, RequestResult::Rejected { .. }), "{late:?}");
    assert!(launcher.wakes.lock().unwrap().is_empty());
    room.shutdown().await.unwrap();
}

#[tokio::test]
async fn clear_cancels_pending_picks_and_collected_answers() {
    let f = live::Fixture::new().await;
    let room = f.open(OpenMode::OneShot).await;
    room.submit_commit_only(f.request("@all $debate $ROUNDS=2 Compare designs?"))
        .await;
    let Some(ContestState::Selection(pick)) = room.state().await.active_contest else {
        panic!("question-first debate is collecting")
    };
    accepted(room.submit(clear()).await);
    assert!(room.state().await.active_contest.is_none());
    assert_eq!(f.scalar("SELECT COUNT(*) FROM contests"), 0);
    assert_eq!(f.scalar("SELECT COUNT(*) FROM contest_answers"), 0);
    let late = room
        .submit_commit_only(Request::ContestPick {
            request_id: EventId::new(),
            contest_id: pick.contest_id,
            nicks: vec![nick_of(&f, 0), nick_of(&f, 1)],
        })
        .await;
    assert!(matches!(
        late,
        RequestResult::Rejected { error, .. } if error.code == ErrorCode::CONTEST_NOT_FOUND
    ));
    room.shutdown().await.unwrap();
}

fn nick_of(f: &live::Fixture, index: usize) -> MemberNick {
    f.config.agents[index].nick.clone()
}

#[tokio::test]
async fn clear_fences_streaming_text_and_stale_results_but_keeps_daily_accounting() {
    let f = live::Fixture::new().await;
    f.fake
        .completions(vec![
            streaming("paid answer", "stop"),
            delayed(Duration::from_millis(1500)),
        ])
        .await;
    let room = f.open(OpenMode::Interactive).await;
    room.submit(f.request("@member0 first")).await;
    f.finals(1).await;
    let spent = f.scalar("SELECT SUM(spent_micros) FROM spend_daily");
    let before = room.state().await.daily_spend;
    // A second reply is still in flight when the room is cleared.
    let mut feed = room.subscribe();
    room.submit(f.request("@member0 second")).await;
    f.wait(|| f.scalar("SELECT COUNT(*) FROM work WHERE status='running'") == 1)
        .await;
    accepted(room.submit(clear()).await);
    tokio::time::sleep(Duration::from_millis(2200)).await;
    // The stale attempt's late result never lands, and nothing provisional survives.
    let events = room.page_events(None, 100).await.unwrap();
    assert_eq!(events.len(), 1, "{events:?}");
    assert!(matches!(events[0].content, EnvelopePayload::RoomCleared(_)));
    assert_eq!(
        f.scalar("SELECT COUNT(*) FROM events WHERE kind='message.final'"),
        0
    );
    assert_eq!(
        f.scalar("SELECT COUNT(*) FROM work WHERE status IN ('queued','running')"),
        0
    );
    // Daily spend is shared accounting, not conversation: it is untouched by the clear.
    assert!(f.scalar("SELECT SUM(spent_micros) FROM spend_daily") >= spent);
    // The stale attempt's hold is released once its provider call settles.
    f.wait(|| f.scalar("SELECT COALESCE(SUM(reserved_micros),0) FROM spend_daily") == 0)
        .await;
    let after = room.state().await.daily_spend;
    assert!(after.spent_usd >= before.spent_usd);
    assert_eq!(after.reserved_usd, 0.);
    // No committed or delta item for the cleared reply arrives after the marker.
    let mut after_marker = false;
    while let Ok(item) = feed.try_recv() {
        match item {
            FeedItem::Committed(e) if matches!(e.content, EnvelopePayload::RoomCleared(_)) => {
                after_marker = true
            }
            FeedItem::Committed(e) if after_marker => {
                assert!(
                    !matches!(e.content, EnvelopePayload::MessageFinal(_)),
                    "stale final leaked: {e:?}"
                )
            }
            _ => {}
        }
    }
    room.shutdown().await.unwrap();
}
