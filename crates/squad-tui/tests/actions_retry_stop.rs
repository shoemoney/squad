//! M4.P3.L3.T1 retry/regenerate and M4.P3.L1.T4 per-target stop.
use squad_protocol::{parser::SubmissionMeta, *};
use squad_tui::{
    actions,
    session::{FakeRoom, RoomPort},
    validation,
};
use std::time::Duration;

fn err(retryable: bool, work: Option<WorkId>, action: Option<WorkId>) -> ErrorPayload {
    ErrorPayload {
        code: ErrorCode::INVALID_COMMAND,
        text: Text::new("boom").unwrap(),
        retryable,
        related_event_id: None,
        work_id: work,
        debate_id: None,
        case_id: None,
        earliest_cursor: None,
        retry_action: action.map(|work_id| RetryAction {
            kind: RetryKind::Retry,
            work_id,
        }),
        contest_id: None,
        provider_status: None,
        provider_message: None,
    }
}

#[test]
fn stop_defaults_to_global_and_targets_one_member() {
    assert!(matches!(
        actions::stop(),
        Request::Stop { target: None, .. }
    ));
    let nick = Nick::new("grok").unwrap();
    let Request::Stop { target, .. } = actions::stop_target(Some(nick.clone())) else {
        panic!()
    };
    assert_eq!(target, Some(nick));
}

#[test]
fn retry_requires_a_retryable_error_and_names_one_work_item() {
    let w = WorkId::new();
    assert!(actions::retry_for(&err(false, Some(w), None)).is_none());
    let Some(Request::Retry { work_id, .. }) = actions::retry_for(&err(true, None, Some(w))) else {
        panic!()
    };
    assert_eq!(work_id, w);
    let other = WorkId::new();
    let Some(Request::Retry { work_id, .. }) = actions::retry_for(&err(true, Some(other), Some(w)))
    else {
        panic!()
    };
    assert_eq!(work_id, w, "retry_action wins over the bare work id");
    assert!(actions::retry_for(&err(true, None, None)).is_none());
}

#[test]
fn regenerate_names_the_message_or_the_latest() {
    assert!(matches!(
        actions::regenerate(None),
        Request::Regen {
            message_id: None,
            ..
        }
    ));
    let id = MessageId::new();
    assert!(matches!(
        actions::regenerate(Some(id)),
        Request::Regen { message_id: Some(m), .. } if m == id
    ));
}

async fn send(room: &FakeRoom, raw: &str) {
    let context = room.state().await.context;
    let meta = SubmissionMeta::default();
    let request = validation::request(raw, &meta, &context, EventId::new()).unwrap();
    assert!(matches!(
        room.submit(request).await,
        RequestResult::Accepted { .. }
    ));
}

fn cancelled(state: &squad_tui::session::RoomSnapshot) -> Vec<String> {
    state
        .events
        .iter()
        .filter_map(|e| match &e.content {
            EnvelopePayload::WorkState(w) if w.status == WorkStatus::Cancelled => {
                Some(w.context_id.0.clone())
            }
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn targeted_stop_cancels_only_that_member() {
    let room = FakeRoom::new();
    let mut feed = room.subscribe(None).unwrap();
    send(&room, "@claude @grok hello").await;
    // Let both replies begin streaming, then fold them into the room's cards.
    let mut session = squad_tui::session::Session::default();
    let mut q = squad_tui::fx::FxQueue::new();
    let deadline = tokio::time::Instant::now() + Duration::from_millis(900);
    while tokio::time::Instant::now() < deadline {
        if let Ok(Ok(d)) = tokio::time::timeout(Duration::from_millis(50), feed.recv()).await {
            session.reduce(
                d.item,
                squad_tui::fx::FeedOrigin::Live,
                Duration::ZERO,
                &mut q,
            );
        }
    }
    assert!(session.transcript.cards.values().any(|c| !c.complete));
    let result = room
        .submit(actions::stop_target(Some(Nick::new("grok").unwrap())))
        .await;
    assert!(matches!(result, RequestResult::Accepted { .. }));
    let state = room.state().await;
    assert_eq!(cancelled(&state), vec!["grok".to_string()]);
}

#[tokio::test]
async fn retry_of_unknown_work_is_rejected_not_silently_accepted() {
    let room = FakeRoom::new();
    assert!(matches!(
        actions::retry_via(&room, WorkId::new()).await,
        RequestResult::Rejected { .. }
    ));
    assert!(matches!(
        actions::regenerate_via(&room, None).await,
        RequestResult::Rejected { .. }
    ));
}
