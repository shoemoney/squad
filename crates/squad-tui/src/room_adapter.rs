//! Live core room integration and terminal application entry point.
use crate::session::{
    DailySpendView, RoomError, RoomFeed, RoomFuture, RoomPort, RoomSnapshot, Session,
};
use squad_core::{room::RoomHandle, state::RoomState};
use squad_protocol::*;
use std::{
    io,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

fn error(e: squad_core::room::RoomError) -> RoomError {
    RoomError {
        code: e.code,
        message: e.message,
    }
}

/// Replay the public history through the same reducer used for live deliveries,
/// then overlay authoritative persisted variables and room projections.
pub fn snapshot(state: &RoomState, events: Vec<CommittedEnvelope>) -> RoomSnapshot {
    let mut session = Session::default();
    let mut cues = crate::fx::FxQueue::new();
    for event in &events {
        session.reduce(
            FeedItem::Committed(event.clone()),
            crate::fx::FeedOrigin::Replay,
            Duration::ZERO,
            &mut cues,
        );
    }
    let mut view = session.snapshot;
    view.context.roster = state.participants.iter().map(|p| p.nick.clone()).collect();
    view.context.vars = [
        (VariableName::Rounds, state.vars.rounds),
        (VariableName::GrillRounds, state.vars.grill_rounds),
        (VariableName::CensusPasses, state.vars.census_passes),
    ]
    .into();
    view.context.active_contest = state.active_contest.clone();
    if let Some(ContestState::Selection(s)) = &state.active_contest {
        view.context.pending_pick =
            (s.state == ContestSelectionStatus::AwaitingPick).then(|| parser::PendingPick {
                contest_id: s.contest_id,
                mode: if s.kind == ContestKind::Grill {
                    parser::QuestionFirstMode::Grill
                } else {
                    parser::QuestionFirstMode::Debate
                },
                eligible: s.answers.iter().map(|a| a.nick.clone()).collect(),
            });
    } else {
        view.context.pending_pick = None;
    }
    for event in &events {
        if let EnvelopePayload::MessageSuperseded(s) = &event.content
            && let Some(record) = view
                .context
                .records
                .iter_mut()
                .find(|r| r.event_id == s.superseded_event_id)
        {
            record.superseded = true;
            record.eligible = false;
        }
    }
    view.participants = state.participants.clone();
    view.models = state.models.clone();
    view.preflights = state.pending_preflights.clone();
    view.scoreboard = state.scoreboard.clone();
    view.trophies = state.trophies.clone();
    view.daily_spend = DailySpendView {
        day: state.daily_spend.day.clone(),
        spent_usd: state.daily_spend.spent_usd,
        reserved_usd: state.daily_spend.reserved_usd,
        limit_usd: state.daily_spend.limit_usd,
        unknown_calls: state.daily_spend.unknown_calls.min(u64::from(u32::MAX)) as u32,
    };
    view.events = events;
    view
}

pub async fn history(
    room: &RoomHandle,
) -> Result<Vec<CommittedEnvelope>, squad_core::room::RoomError> {
    let mut events = Vec::new();
    let mut after = None;
    loop {
        let page = room.events_after(after, 500).await?;
        if page.is_empty() {
            return Ok(events);
        }
        after = page.last().map(|e| e.commit.seq);
        events.extend(page);
    }
}
impl RoomPort for RoomHandle {
    fn submit(&self, request: Request) -> RoomFuture<'_, RequestResult> {
        Box::pin(self.submit(request))
    }
    fn subscribe(&self, after: Option<Cursor>) -> Result<RoomFeed, RoomError> {
        // Register live delivery first, then replay whole durable commit groups.
        // Core's reader performs synchronous store calls behind an async gate.
        // Poll once: an occupied lifecycle gate is reported as NOT_READY rather
        // than blocking a Tokio executor waiting for a competing operation.
        let live = self.subscribe();
        let mut replay = Vec::new();
        if let Some(mut cursor) = after {
            let mut context = std::task::Context::from_waker(std::task::Waker::noop());
            loop {
                let future = self.events_after(Some(cursor), 500);
                let mut future = std::pin::pin!(future);
                let page = match std::future::Future::poll(future.as_mut(), &mut context) {
                    std::task::Poll::Ready(page) => page.map_err(error)?,
                    std::task::Poll::Pending => {
                        return Err(RoomError {
                            code: ErrorCode::NOT_READY,
                            message: "Room lifecycle operation in progress; retry subscription."
                                .into(),
                        });
                    }
                };
                if page.is_empty() {
                    break;
                }
                cursor = page.last().expect("nonempty page").commit.seq;
                replay.extend(page);
            }
        }
        Ok(RoomFeed::new(live, replay))
    }
    fn page_events(
        &self,
        before: Option<Cursor>,
        limit: usize,
    ) -> RoomFuture<'_, Result<Vec<CommittedEnvelope>, RoomError>> {
        Box::pin(async move {
            let mut events = self.page_events(before, limit).await.map_err(error)?;
            events.reverse();
            Ok(events)
        })
    }
    fn state(&self) -> RoomFuture<'_, RoomSnapshot> {
        Box::pin(async move {
            let events = match history(self).await {
                Ok(events) => events,
                Err(_) => return snapshot(&self.state().await, Vec::new()),
            };
            snapshot(&self.state().await, events)
        })
    }
    fn export(
        &self,
        scope: ExportScope,
        id: Option<ContestId>,
    ) -> RoomFuture<'_, Result<String, RoomError>> {
        Box::pin(async move { self.export(scope, id).await.map_err(error) })
    }
    fn shutdown(&self) -> RoomFuture<'_, Result<(), RoomError>> {
        Box::pin(async move { self.shutdown().await.map_err(error) })
    }
}
impl<T: RoomPort + ?Sized> RoomPort for Arc<T> {
    fn submit(&self, request: Request) -> RoomFuture<'_, RequestResult> {
        (**self).submit(request)
    }
    fn subscribe(&self, after: Option<Cursor>) -> Result<RoomFeed, RoomError> {
        (**self).subscribe(after)
    }
    fn page_events(
        &self,
        before: Option<Cursor>,
        limit: usize,
    ) -> RoomFuture<'_, Result<Vec<CommittedEnvelope>, RoomError>> {
        (**self).page_events(before, limit)
    }
    fn state(&self) -> RoomFuture<'_, RoomSnapshot> {
        (**self).state()
    }
    fn export(
        &self,
        scope: ExportScope,
        id: Option<ContestId>,
    ) -> RoomFuture<'_, Result<String, RoomError>> {
        (**self).export(scope, id)
    }
    fn shutdown(&self) -> RoomFuture<'_, Result<(), RoomError>> {
        (**self).shutdown()
    }
}

/// Run a room in the terminal, restoring terminal modes on every return path.
pub async fn run(
    config: SquadConfig,
    room: impl RoomPort + 'static,
) -> Result<(), Box<dyn std::error::Error>> {
    let guard = crate::terminal::TerminalGuard::enter()?;
    let mut caps = crate::fx::TerminalCaps::detect(guard.capabilities().synchronized_output)?;
    if config.fx_explicit {
        caps.level = match config.fx {
            config::EffectsMode::Full => crate::fx::FxLevel::Full,
            config::EffectsMode::Lite => crate::fx::FxLevel::Lite,
            config::EffectsMode::Off => crate::fx::FxLevel::Off,
        };
    }
    caps.debug = config.fx_debug;
    if config.no_color {
        caps.color = crate::fx::ColorTier::NoColor;
    }
    let mut app = crate::app::App::new(caps);
    app.tips_enabled = config.tips;
    app.input_events = crate::input_events::InputEvents {
        kitty: guard.capabilities().keyboard_enhancement,
    };
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::CrosstermBackend::new(io::stdout()))?;
    let (tx, mut input) = tokio::sync::mpsc::channel(128);
    struct InputOwner(Arc<AtomicBool>);
    impl Drop for InputOwner {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Relaxed);
        }
    }
    let stop = Arc::new(AtomicBool::new(false));
    let _input_owner = InputOwner(stop.clone());
    let reader_stop = stop.clone();
    let reader = std::thread::spawn(move || {
        while !reader_stop.load(Ordering::Relaxed) {
            match crossterm::event::poll(Duration::from_millis(30)) {
                Ok(true) => {
                    if tx.blocking_send(crossterm::event::read()).is_err() {
                        break;
                    }
                }
                Ok(false) => (),
                Err(e) => {
                    let _ = tx.blocking_send(Err(e));
                    break;
                }
            }
        }
    });
    let result = crate::event_loop::run(&mut app, Arc::new(room), &mut terminal, &mut input).await;
    stop.store(true, Ordering::Relaxed);
    drop(input);
    let _ = reader.join();
    drop(terminal);
    guard.restore()?;
    result
}
