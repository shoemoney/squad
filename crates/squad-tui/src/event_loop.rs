//! The one scheduler for input, bounded feed work, acknowledgements and frames.
use crate::{
    app::App,
    fx::FrameSample,
    session::{FeedDelivery, RoomError, RoomFeed, RoomPort, RoomSnapshot},
    terminal, ui,
};
use crossterm::event::Event;
use ratatui::{Terminal, backend::Backend};
use squad_protocol::RequestResult;
use std::{future::pending, io, sync::Arc, time::Duration};
use tokio::{
    sync::{broadcast, mpsc},
    time::Instant,
};
pub const FEED_BATCH: usize = 256;
pub const FRAME_PERIOD: Duration = Duration::from_nanos(16_666_667);
/// Earliest dirty/UI/effects deadline, constrained by the previous actual draw.
/// None means no timer at all: an idle room cannot produce idle redraws.
pub fn frame_deadline(
    now: Duration,
    last_draw: Option<Duration>,
    dirty: bool,
    ui: Option<Duration>,
    effects: Option<Duration>,
) -> Option<Duration> {
    let target = [dirty.then_some(now), ui, effects.map(|delay| now + delay)]
        .into_iter()
        .flatten()
        .min()?;
    Some(target.max(last_draw.map_or(Duration::ZERO, |last| last + FRAME_PERIOD)))
}
async fn wait_frame(deadline: Option<Instant>) {
    match deadline {
        Some(at) => tokio::time::sleep_until(at).await,
        None => pending().await,
    }
}

pub async fn run<B: Backend>(
    app: &mut App,
    room: Arc<dyn RoomPort>,
    terminal: &mut Terminal<B>,
    input: &mut mpsc::Receiver<io::Result<Event>>,
) -> Result<(), Box<dyn std::error::Error>>
where
    B::Error: std::error::Error + 'static,
{
    // Subscribe first. Events committed during state() queue in the receiver;
    // canonical identity reconciliation removes duplicates after restoration.
    let mut feed = match room.subscribe(None) {
        Ok(feed) => feed,
        Err(error) => {
            let _ = room.shutdown().await;
            return Err(Box::new(error));
        }
    };
    app.session.restore(room.state().await);
    app.refresh_preview();
    let epoch = Instant::now() - app.now();
    let mut last_draw = None;
    let mut feed_open = true;
    let mut ack: Option<tokio::task::JoinHandle<RequestResult>> = None;
    let mut restoration: Option<tokio::task::JoinHandle<RoomSnapshot>> = None;
    let result = async {
        while !app.quit {
            let now = app.now(); app.expire(now);
            if let Some(request) = app.pending.take() {
                let room = room.clone(); ack = Some(tokio::spawn(async move { room.submit(request).await }));
            }
            if let Some(job) = app.pending_export.take() {
                let room = room.clone(); ack = Some(tokio::spawn(async move { crate::actions::export_file(&*room, job).await }));
            }
            let deadline = frame_deadline(now, last_draw, app.dirty, app.ui_deadline(), app.fx.next_wake()).map(|at| epoch + at);
            tokio::select! {
                biased;
                event = input.recv() => match event { Some(Ok(event)) => app.handle(event, app.now()), Some(Err(e)) => return Err(Box::new(e) as Box<dyn std::error::Error>), None => { app.quit = true; } },
                _ = wait_frame(deadline) => {
                    let started = std::time::Instant::now();
                    let now = app.now(); app.expire(now);
                    // Draw must happen ahead of another ready feed batch. A
                    // sustained burst cannot starve a frame once its deadline is due.
                    terminal.draw(|frame| ui::render(app, frame, now))?;
                    app.draws += 1; app.dirty = false; last_draw = Some(app.now());
                    app.fx.observe_frame(FrameSample { draw_and_flush: started.elapsed(), bytes_per_second: 0, input_wait: Duration::ZERO });
                },
                response = async { match ack.as_mut() { Some(task) => task.await, None => pending().await } } => {
                    ack = None; app.acknowledge(response?);
                },
                snapshot = async { match restoration.as_mut() { Some(task) => task.await, None => pending().await } } => {
                    restoration = None;
                    app.session.restore(snapshot?); app.refresh_preview();
                    app.notice = Some("Room feed restored.".into()); app.dirty = true;
                },
                item = feed.recv(), if feed_open && restoration.is_none() => {
                    match item {
                        Ok(item) => {
                            reduce(app, item);
                            let mut processed = 1;
                            for _ in 1..FEED_BATCH {
                                match feed.try_recv() {
                                    Ok(item) => { reduce(app, item); processed += 1; },
                                    Err(broadcast::error::TryRecvError::Empty) => break,
                                    Err(broadcast::error::TryRecvError::Closed) => { feed_open = false; break; }
                                    Err(broadcast::error::TryRecvError::Lagged(_)) => { restoration = Some(recover(app, room.clone(), &mut feed)?); break; }
                                }
                            }
                            app.max_feed_batch = app.max_feed_batch.max(processed);
                            app.refresh_preview();
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => restoration = Some(recover(app, room.clone(), &mut feed)?),
                        Err(broadcast::error::RecvError::Closed) => feed_open = false,
                    }
                }
            }
        }
        Ok(())
    }.await;
    if let Some(task) = ack {
        task.abort();
        let _ = task.await;
    }
    if let Some(task) = restoration {
        task.abort();
        let _ = task.await;
    }
    let shutdown = room.shutdown().await;
    result?;
    shutdown?;
    Ok(())
}
fn reduce(app: &mut App, delivery: FeedDelivery) {
    let now = app.now();
    let old_pick = app.session.pick.as_ref().map(|p| p.request.contest_id);
    let was_active = app.session.active_contest();
    // Successful completion is the result reveal's trigger, not a cancellation.
    let completed = matches!(&delivery.item, squad_protocol::FeedItem::Committed(event)
        if matches!(&event.content, squad_protocol::EnvelopePayload::DebateState(state)
            if state.state == squad_protocol::DebateStatus::Completed));
    app.dirty |= app
        .session
        .reduce(delivery.item, delivery.origin, now, app.fx.queue_mut());
    if let Some(id) = old_pick
        && app
            .session
            .pick
            .as_ref()
            .is_none_or(|p| p.request.contest_id != id)
    {
        app.fx.settle_contest(&id.to_string());
    }
    if was_active && !app.session.active_contest() && !completed {
        app.fx.settle();
    }
    app.feed_items += 1;
}
fn recover(
    app: &mut App,
    room: Arc<dyn RoomPort>,
    feed: &mut RoomFeed,
) -> Result<tokio::task::JoinHandle<RoomSnapshot>, RoomError> {
    *feed = room.subscribe(None)?;
    app.fx.settle();
    app.notice = Some("Restoring room feed…".into());
    app.dirty = true;
    Ok(tokio::spawn(async move { room.state().await }))
}
/// The terminal guard owns cleanup; this wrapper lets callers use the existing
/// synchronized-output contract with a concrete terminal writer.
pub fn synchronized_draw<T>(
    writer: &mut impl io::Write,
    supported: bool,
    draw: impl FnOnce() -> io::Result<T>,
) -> io::Result<T> {
    terminal::synchronized_output(writer, supported, |_| draw())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        fx::{CatalogueEffect, FeedOrigin, FxCue, TerminalCaps},
        session::{FakeRoom, RoomPort},
    };
    use squad_protocol::{DebateStatus, EnvelopePayload, FeedItem};

    #[tokio::test(start_paused = true)]
    async fn completed_debate_preserves_queued_result_reveal() {
        let room = FakeRoom::new();
        room.start_demo().await.unwrap();
        let mut snapshot = room.state().await;
        let start = snapshot
            .events
            .iter()
            .position(|e| matches!(e.content, EnvelopePayload::Verdict(_)))
            .unwrap();
        let events = snapshot.events.split_off(start);
        snapshot.context.active_contest = None;
        snapshot.context.pending_pick = None;
        let mut app = App::new(TerminalCaps::resolve(|_| None, true, (80, 24).into()).unwrap());
        app.session.restore(snapshot);
        for event in events {
            let completed = matches!(&event.content, EnvelopePayload::DebateState(s) if s.state == DebateStatus::Completed);
            reduce(
                &mut app,
                FeedDelivery {
                    item: FeedItem::Committed(event),
                    origin: FeedOrigin::Live,
                },
            );
            if completed {
                break;
            }
        }
        let mut reveal = false;
        while let Some(cue) = app.fx.queue_mut().pop_front() {
            reveal |= matches!(
                cue.cue,
                FxCue::Catalogue {
                    effect: CatalogueEffect::Verdict,
                    ..
                }
            );
        }
        assert!(
            reveal,
            "completion discarded the verdict animation before the next frame"
        );
    }
}
