//! Human interaction state; durable room facts live in Session.
use crate::{
    actions,
    fx::{Clock, FxContext, FxManager, SystemClock, TerminalCaps},
    input_events::InputEvents,
    quotes::Quotes,
    session::Session,
    validation,
    widgets::{
        completion::CompletionState,
        input::{EditorAction, InputState},
    },
};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use squad_protocol::*;
use std::time::Duration;

pub struct App {
    pub composed: crate::composed::Composed,
    pub session: Session,
    pub input: InputState,
    pub quotes: Quotes,
    pub completion: CompletionState,
    pub completion_open: bool,
    pub focus_input: bool,
    pub participant_details: bool,
    pub quit: bool,
    pub dirty: bool,
    pub pending: Option<Request>,
    pub export_path: Option<InputState>,
    pub export_scope: Option<(ExportScope, Option<ContestId>)>,
    pub pending_export: Option<actions::ExportJob>,
    export_inflight: bool,
    pub awaiting_ack: bool,
    pub pending_updates: Vec<VariableUpdate>,
    pub notice: Option<String>,
    pub stop_confirmation: Option<Duration>,
    pub transient: Option<String>,
    pub debug_open: bool,
    pub last_recipients: Vec<String>,
    pub fx: FxManager,
    pub clock: SystemClock,
    pub input_events: InputEvents,
    pub preview: Result<validation::ValidatedInput, squad_protocol::parser::ParseError>,
    pub theme: crate::theme::Theme,
    pub draws: u64,
    pub feed_items: u64,
    pub max_feed_batch: usize,
    submitted_draft: Option<String>,
    submitted_recipients: Vec<String>,
}
impl App {
    pub fn new(caps: TerminalCaps) -> Self {
        let clock = SystemClock::default();
        let theme = crate::theme::Theme::new(caps.color);
        Self {
            session: Session::default(),
            composed: crate::composed::Composed::default(),
            input: InputState::new(),
            quotes: Quotes::default(),
            completion: CompletionState::default(),
            completion_open: false,
            focus_input: true,
            participant_details: false,
            quit: false,
            dirty: true,
            pending: None,
            export_path: None,
            export_scope: None,
            pending_export: None,
            export_inflight: false,
            awaiting_ack: false,
            pending_updates: vec![],
            notice: None,
            stop_confirmation: None,
            transient: None,
            debug_open: false,
            last_recipients: vec![],
            fx: FxManager::new(clock.clone(), caps),
            clock,
            input_events: InputEvents::default(),
            preview: validation::validate("", &Default::default(), &Default::default()),
            theme,
            draws: 0,
            feed_items: 0,
            max_feed_batch: 0,
            submitted_draft: None,
            submitted_recipients: vec![],
        }
    }
    pub fn now(&self) -> Duration {
        self.clock.now()
    }
    pub fn queue(&mut self, request: Request) {
        if !self.awaiting_ack {
            self.pending = Some(request);
            self.awaiting_ack = true;
            self.dirty = true;
        }
    }
    pub fn submit(&mut self) {
        if self.awaiting_ack {
            return;
        }
        let meta = self.quotes.metadata();
        match validation::validate(self.input.text(), &meta, &self.session.snapshot.context) {
            Err(e) => self.notice = Some(e.message),
            Ok(valid) => {
                let id = EventId::new();
                match Request::from_parsed_with_context(
                    id,
                    valid.parsed,
                    &meta,
                    &self.session.snapshot.context,
                    false,
                ) {
                    Err(e) => self.notice = Some(e.message),
                    Ok(request) => {
                        self.pending_updates = valid.updates;
                        self.submitted_recipients = valid.recipients;
                        self.submitted_draft = Some(self.input.text().into());
                        if let Request::Export { scope, id } = request {
                            let mut path = InputState::new();
                            let _ = path.apply(EditorAction::Insert("squad-export.md".into()));
                            let _ = path.apply(EditorAction::SelectAll);
                            self.export_path = Some(path);
                            self.export_scope = Some((scope, id));
                        } else {
                            self.queue(request);
                        }
                    }
                }
            }
        }
        self.dirty = true;
    }
    pub fn acknowledge(&mut self, result: RequestResult) {
        self.awaiting_ack = false;
        self.pending_updates.clear();
        match result {
            RequestResult::Rejected { error, .. } => {
                self.notice = Some(error.text.to_string());
            }
            RequestResult::Accepted { .. } => {
                // Typing during acknowledgement is preserved. Only the exact
                // accepted draft is moved into history and cleared.
                if self.submitted_draft.as_deref() == Some(self.input.text()) {
                    self.input.accept_submission();
                    self.quotes.clear();
                }
                if !self.submitted_recipients.is_empty() {
                    self.last_recipients = std::mem::take(&mut self.submitted_recipients);
                }
                self.notice = None;
            }
            RequestResult::PendingConfirmation { preflight, .. } => {
                self.session.snapshot.preflights = vec![preflight];
                self.notice = Some("Admission pending · Y Confirm · N Cancel".into());
                // The direct request and its draft remain pending until admission.
                self.dirty = true;
                return;
            }
            RequestResult::Internal(InternalResult::Text { text }) => {
                if self.export_inflight {
                    if self.submitted_draft.as_deref() == Some(self.input.text()) {
                        self.input.accept_submission();
                    }
                    self.notice = Some(text);
                } else {
                    self.transient = Some(text);
                }
            }
            _ => {}
        }
        self.export_inflight = false;
        self.submitted_draft = None;
        self.refresh_preview();
        self.dirty = true;
    }
    pub fn escape(&mut self, now: Duration) {
        if self.export_path.take().is_some() {
            self.export_scope = None;
        } else if self.debug_open {
            self.debug_open = false;
        } else if self.completion_open {
            self.completion_open = false;
        } else if self.transient.take().is_some() {
        } else if let Some(plan) = self.session.snapshot.preflights.first().cloned() {
            if self.awaiting_ack {
                return;
            }
            self.submitted_draft = None;
            self.submitted_recipients.clear();
            self.queue(actions::preflight(&plan, false));
        } else if !self.quotes.ids.is_empty() {
            self.quotes.clear();
        } else if self.session.active_contest() {
            if self.stop_confirmation.is_some_and(|until| now < until) {
                self.stop_confirmation = None;
                self.notice = Some("Stopping…".into());
                self.queue(actions::stop());
            } else {
                self.stop_confirmation = Some(now + Duration::from_secs(2));
                self.notice = Some("Press Esc again within 2 s to stop the active contest.".into());
            }
        } else {
            self.notice = Some("Stopping…".into());
            self.queue(actions::stop());
        }
        self.refresh_preview();
        self.dirty = true;
    }
    pub fn expire(&mut self, now: Duration) {
        if self.stop_confirmation.is_some_and(|until| now >= until) {
            self.stop_confirmation = None;
            self.notice = None;
            self.dirty = true;
        }
        self.dirty |= self.session.transcript.flush(now);
    }
    pub fn ui_deadline(&self) -> Option<Duration> {
        [
            self.stop_confirmation,
            self.session.transcript.batch_deadline,
        ]
        .into_iter()
        .flatten()
        .min()
    }
    pub fn handle(&mut self, event: Event, now: Duration) {
        if let Event::Resize(_, _) = event {
            self.fx.settle();
            self.dirty = true;
            return;
        }
        if let Event::Mouse(mouse) = &event {
            let delta = match mouse.kind {
                crossterm::event::MouseEventKind::ScrollUp => -3,
                crossterm::event::MouseEventKind::ScrollDown => 3,
                _ => 0,
            };
            if delta != 0 {
                self.fx.settle();
                if self
                    .composed
                    .roster_area
                    .contains((mouse.column, mouse.row).into())
                {
                    self.composed.participants.scroll = self
                        .composed
                        .participants
                        .scroll
                        .saturating_add_signed(delta)
                        .min(
                            self.composed
                                .roster_rows
                                .saturating_sub(usize::from(self.composed.roster_area.height)),
                        );
                } else {
                    self.composed.scroll(delta);
                    self.session.transcript.follow = false;
                }
                self.dirty = true;
                return;
            }
        }
        if let Event::Key(key) = &event {
            if key.kind == KeyEventKind::Release {
                return;
            }
            // Settle animation without consuming the key. Input always proceeds.
            self.fx.settle();
            if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                self.quit = true;
                return;
            }
            if self.export_path.is_some()
                && key.code == KeyCode::Esc
                && key.kind == KeyEventKind::Press
            {
                self.escape(now);
                return;
            }
            if self.export_path.is_none() && key.kind == KeyEventKind::Press {
                match key.code {
                    KeyCode::Esc => {
                        self.escape(now);
                        return;
                    }
                    KeyCode::F(8) => {
                        self.participant_details = !self.participant_details;
                        self.dirty = true;
                        return;
                    }
                    KeyCode::Up | KeyCode::Down if self.participant_details => {
                        let max = self.session.snapshot.context.roster.len().saturating_sub(1);
                        self.composed.participants.focused = self
                            .composed
                            .participants
                            .focused
                            .saturating_add_signed(if key.code == KeyCode::Up { -1 } else { 1 })
                            .min(max);
                        self.dirty = true;
                        return;
                    }
                    KeyCode::F(5) => {
                        if let Some((_, contest)) = self
                            .session
                            .contests
                            .iter_mut()
                            .rev()
                            .find(|(_, v)| v.crosscheck.grid.is_some())
                        {
                            let view = &mut contest.crosscheck;
                            if view.collapsed_cells.is_empty() {
                                if let Some(grid) = &view.grid {
                                    view.collapsed_cells
                                        .extend(grid.cells.iter().map(|c| c.cell_id));
                                }
                            } else {
                                view.collapsed_cells.clear();
                            }
                            self.composed.invalidate_artifacts();
                            self.dirty = true;
                        }
                        return;
                    }
                    KeyCode::F(7) => {
                        self.composed.toggle_verdict();
                        self.session.transcript.follow = false;
                        for view in self.session.probes.values_mut() {
                            view.toggle_matrix();
                        }
                        self.dirty = true;
                        return;
                    }
                    KeyCode::F(12) => {
                        self.debug_open = !self.debug_open;
                        self.dirty = true;
                        return;
                    }
                    KeyCode::PageUp | KeyCode::PageDown => {
                        self.composed.scroll(if key.code == KeyCode::PageUp {
                            -(isize::try_from(self.composed.viewport.height).unwrap_or(1))
                        } else {
                            isize::try_from(self.composed.viewport.height).unwrap_or(1)
                        });
                        self.session.transcript.follow = false;
                        self.dirty = true;
                        return;
                    }
                    KeyCode::End => {
                        self.session.transcript.follow = true;
                        self.composed.anchor = None;
                        self.dirty = true;
                        return;
                    }
                    KeyCode::F(6) => {
                        self.focus_input = !self.focus_input;
                        self.dirty = true;
                        return;
                    }
                    KeyCode::Char('q') if !self.focus_input => {
                        self.quit = true;
                        return;
                    }
                    KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        let text = format!("{} ", self.last_recipients.join(" "));
                        self.edit(EditorAction::Insert(text));
                        return;
                    }
                    KeyCode::Tab => {
                        if self.completion_open {
                            self.accept_completion();
                        } else {
                            self.refresh_completion();
                            self.completion_open = !self.completion.items.is_empty();
                        }
                        self.dirty = true;
                        return;
                    }
                    KeyCode::Up | KeyCode::Down if self.completion_open => {
                        self.completion.move_selection(key.code == KeyCode::Down);
                        self.dirty = true;
                        return;
                    }
                    KeyCode::Enter if self.completion_open => {
                        self.accept_completion();
                        return;
                    }
                    KeyCode::Char('y' | 'n')
                        if !self.session.snapshot.preflights.is_empty() && !self.awaiting_ack =>
                    {
                        if key.code == KeyCode::Char('n') {
                            self.submitted_draft = None;
                            self.submitted_recipients.clear();
                        }
                        self.queue(actions::preflight(
                            &self.session.snapshot.preflights[0],
                            key.code == KeyCode::Char('y'),
                        ));
                        return;
                    }
                    _ => {}
                }
            }
        }
        if self.export_path.is_some() {
            if let Some(action) = self.input_events.translate(event) {
                if action == EditorAction::Submit {
                    let path = self
                        .export_path
                        .as_ref()
                        .expect("path editor")
                        .text()
                        .trim()
                        .to_owned();
                    if !path.is_empty()
                        && let Some((scope, id)) = self.export_scope.take()
                    {
                        self.pending_export = Some(actions::ExportJob {
                            request_id: EventId::new(),
                            scope,
                            id,
                            path: path.into(),
                        });
                        self.export_path = None;
                        self.export_inflight = true;
                        self.awaiting_ack = true;
                    }
                } else {
                    let _ = self
                        .export_path
                        .as_mut()
                        .expect("path editor")
                        .apply(action);
                }
                self.dirty = true;
            }
            return;
        }
        if self.focus_input
            && let Some(action) = self.input_events.translate(event)
        {
            if action == EditorAction::Submit {
                self.submit();
            } else {
                self.edit(action);
            }
        }
    }
    pub fn refresh_preview(&mut self) {
        self.preview = validation::validate(
            self.input.text(),
            &self.quotes.metadata(),
            &self.session.snapshot.context,
        );
    }
    fn edit(&mut self, action: EditorAction) {
        if let Err(e) = self.input.apply(action) {
            self.notice = Some(format!(
                "Input is {} bytes; maximum is {}.",
                e.bytes, e.max_bytes
            ));
        }
        self.refresh_preview();
        if self.completion_open {
            self.refresh_completion();
            self.completion_open = !self.completion.items.is_empty();
        }
        self.dirty = true;
    }
    fn refresh_completion(&mut self) {
        self.completion = CompletionState::at(
            self.input.text(),
            self.input.cursor(),
            &self.session.snapshot.context,
            &self.quotes.metadata(),
        );
    }
    fn accept_completion(&mut self) {
        if let Some(value) = self.completion.replacement() {
            let range = self.completion.replace.clone();
            // Existing editor owns every grapheme operation. Select exactly the
            // token span, then replace; byte spans are never terminal columns.
            while self.input.cursor() < range.end {
                let _ = self.input.apply(EditorAction::Right(false));
            }
            while self.input.cursor() > range.start {
                let _ = self.input.apply(EditorAction::Left(true));
            }
            self.edit(EditorAction::Insert(value));
        }
        self.completion_open = false;
        self.dirty = true;
    }
    pub fn fx_context(&self, protected: Vec<ratatui::layout::Rect>) -> FxContext {
        FxContext {
            protected,
            streams: self.session.transcript.streaming(),
            pending_pick: self.session.pick.is_some(),
            ..FxContext::default()
        }
    }
}
