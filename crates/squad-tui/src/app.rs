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
    /// Transcript message focus, selection and the labeled action menu.
    pub msg_focus: Option<EventId>,
    pub msg_selected: Vec<EventId>,
    pub action_menu: bool,
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
    /// Ctrl+F literal search over the loaded durable transcript.
    pub search: crate::search::SearchState,
    pub search_open: bool,
    /// False when the terminal cannot take OSC 52; copy then says to use /export.
    pub clipboard_available: bool,
    /// Latest copy request; the terminal owner writes `sequence` then clears it.
    pub pending_copy: Option<crate::clipboard::CopyRequest>,
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
            msg_focus: None,
            msg_selected: Vec::new(),
            action_menu: false,
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
            search: Default::default(),
            search_open: false,
            clipboard_available: true,
            pending_copy: None,
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
    /// Durable, eligible, non-superseded records in transcript order.
    fn selectable_records(&self) -> Vec<EventId> {
        let mut records: Vec<_> = self
            .session
            .snapshot
            .context
            .records
            .iter()
            .filter(|r| r.eligible && !r.superseded && r.message_id.is_some())
            .collect();
        records.sort_by_key(|r| r.seq);
        records.into_iter().map(|r| r.event_id).collect()
    }
    fn message_notice(&mut self) {
        let context = &self.session.snapshot.context;
        let focus = self.msg_focus.and_then(|id| context.record(id));
        self.notice = Some(match (self.action_menu, focus) {
            (true, _) => format!(
                "Actions · Enter Quote ({} selected) · Esc Close",
                self.msg_selected
                    .len()
                    .max(usize::from(self.msg_focus.is_some()))
            ),
            (false, Some(r)) => format!(
                "{} {} · {} · ↑/↓ Move · Space Select · Enter Actions",
                if self.msg_selected.contains(&r.event_id) {
                    "[x]"
                } else {
                    "[ ]"
                },
                r.author,
                crate::markdown::sanitize(&r.content)
                    .chars()
                    .take(60)
                    .collect::<String>()
            ),
            (false, None) => "No quotable message".into(),
        });
        self.dirty = true;
    }
    fn navigate_messages(&mut self, delta: isize) {
        self.action_menu = false;
        let ids = self.selectable_records();
        let current = self
            .msg_focus
            .and_then(|id| ids.iter().position(|i| *i == id));
        self.msg_focus = if ids.is_empty() {
            None
        } else {
            let next = current.map_or(if delta < 0 { ids.len() - 1 } else { 0 }, |i| {
                i.saturating_add_signed(delta).min(ids.len() - 1)
            });
            Some(ids[next])
        };
        self.message_notice();
    }
    fn toggle_message_selection(&mut self) {
        if let Some(id) = self.msg_focus {
            if let Some(i) = self.msg_selected.iter().position(|s| *s == id) {
                self.msg_selected.remove(i);
            } else {
                self.msg_selected.push(id);
            }
        }
        self.message_notice();
    }
    /// Enter opens the labeled menu; Enter again activates its Quote action.
    fn activate_message_action(&mut self) {
        if !self.action_menu {
            if self.msg_focus.is_some() || !self.msg_selected.is_empty() {
                self.action_menu = true;
            }
            self.message_notice();
            return;
        }
        let mut ids = if self.msg_selected.is_empty() {
            self.msg_focus.into_iter().collect()
        } else {
            std::mem::take(&mut self.msg_selected)
        };
        let order = self.selectable_records();
        ids.sort_by_key(|id| order.iter().position(|o| o == id));
        let before = self.quotes.ids.len();
        for id in ids {
            if !self.quotes.ids.contains(&id) {
                self.quotes.toggle(id);
            }
        }
        let added = self.quotes.ids.len() - before;
        self.action_menu = false;
        self.notice = Some(format!(
            "Quoted {added} message(s) · {} attached · Esc removes",
            self.quotes.ids.len()
        ));
        self.dirty = true;
    }
    pub fn open_search(&mut self) {
        self.search_open = true;
        self.refresh_search();
    }
    pub fn close_search(&mut self) {
        self.search_open = false;
        self.search = Default::default();
        self.notice = None;
        self.dirty = true;
    }
    /// Rebuilds matches from the loaded durable cards; in-flight text is not searchable.
    pub fn refresh_search(&mut self) {
        let documents: Vec<_> = self
            .session
            .transcript
            .cards
            .values()
            .map(|card| crate::search::SearchDocument {
                id: card.id,
                source: card.text.clone(),
                durable: card.complete,
                collapsed: false,
            })
            .collect();
        let before = self.session.snapshot.cursor;
        self.search.refresh(&documents, false, before);
        self.reveal_current_match();
        self.dirty = true;
    }
    pub fn search_next(&mut self, backwards: bool) {
        if self.search.navigate(backwards).is_some() {
            self.reveal_current_match();
        }
        self.dirty = true;
    }
    fn reveal_current_match(&mut self) {
        let Some(found) = self.search.current.and_then(|i| self.search.matches.get(i)) else {
            return;
        };
        self.composed.anchor = Some(crate::widgets::transcript::TranscriptAnchor {
            id: found.id,
            source_grapheme: found.graphemes.start,
            row_in_header: None,
        });
        self.session.transcript.sync_activity();
        self.session.transcript.follow = false;
    }
    fn search_key(&mut self, key: &crossterm::event::KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc => self.close_search(),
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => self.search_next(true),
            KeyCode::Enter | KeyCode::Down => self.search_next(false),
            KeyCode::Up => self.search_next(true),
            KeyCode::Backspace => {
                self.search.query.pop();
                self.refresh_search();
            }
            KeyCode::Char(c)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.search.query.push(c);
                self.refresh_search();
            }
            _ => return false,
        }
        true
    }
    /// Copies the selected messages (or the focused one) as plain, sanitized text.
    pub fn copy_selected(&mut self) {
        let context = &self.session.snapshot.context;
        let ids: Vec<_> = if self.msg_selected.is_empty() {
            self.msg_focus.into_iter().collect()
        } else {
            let order = self.selectable_records();
            let mut ids = self.msg_selected.clone();
            ids.sort_by_key(|id| order.iter().position(|o| o == id));
            ids
        };
        let text = ids
            .iter()
            .filter_map(|id| context.record(*id))
            .map(|r| r.content.to_string())
            .collect::<Vec<_>>()
            .join("\n\n");
        if text.is_empty() {
            self.notice = Some("Nothing selected to copy. Focus a message first (F6, ↑/↓).".into());
        } else {
            match crate::clipboard::request_copy(
                &text,
                crate::clipboard::CopyTarget::Message,
                self.clipboard_available,
            ) {
                Ok(request) => {
                    self.notice = Some(request.feedback.into());
                    self.pending_copy = Some(request);
                }
                Err(e) => self.notice = Some(e.feedback().into()),
            }
        }
        self.dirty = true;
    }
    pub fn take_copy(&mut self) -> Option<crate::clipboard::CopyRequest> {
        self.pending_copy.take()
    }
    pub fn escape(&mut self, now: Duration) {
        if self.export_path.take().is_some() {
            self.export_scope = None;
        } else if self.action_menu {
            self.action_menu = false;
            self.notice = None;
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
        self.session.transcript.sync_activity();
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
                    self.scroll_transcript(delta);
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
            if key.kind == KeyEventKind::Press && self.export_path.is_none() {
                if key.code == KeyCode::Char('f') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    if self.search_open {
                        self.close_search();
                    } else {
                        self.open_search();
                    }
                    return;
                }
                if key.code == KeyCode::Char('y') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    self.copy_selected();
                    return;
                }
                if self.search_open && self.search_key(key) {
                    return;
                }
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
                    KeyCode::Up | KeyCode::Down
                        if !self.focus_input && !self.participant_details =>
                    {
                        self.navigate_messages(if key.code == KeyCode::Up { -1 } else { 1 });
                        return;
                    }
                    KeyCode::Char(' ') if !self.focus_input && !self.participant_details => {
                        self.toggle_message_selection();
                        return;
                    }
                    KeyCode::Enter if !self.focus_input && !self.participant_details => {
                        self.activate_message_action();
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
                        self.page(key.code == KeyCode::PageUp);
                        return;
                    }
                    KeyCode::End => {
                        self.resume_live();
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
    /// Scrolls by rows, anchored to source; reaching the bottom resumes live following.
    pub fn scroll_transcript(&mut self, delta: isize) {
        if self.session.transcript.follow {
            self.session.transcript.sync_activity();
            // Leave the live edge from where the bottom currently is.
            self.composed.top = self
                .composed
                .transcript
                .total_rows()
                .saturating_sub(usize::from(self.composed.viewport.height));
        }
        if self.composed.scroll(delta) && delta > 0 {
            self.resume_live();
            return;
        }
        self.session.transcript.follow = false;
        self.dirty = true;
    }
    /// One viewport page up or down (one row of overlap kept for orientation).
    pub fn page(&mut self, up: bool) {
        let rows =
            isize::try_from(self.composed.viewport.height.saturating_sub(1).max(1)).unwrap_or(1);
        self.scroll_transcript(if up { -rows } else { rows });
    }
    /// End key or paging back to the bottom: follow again and clear the new-activity count.
    pub fn resume_live(&mut self) {
        self.session.transcript.resume_follow();
        self.composed.anchor = None;
        self.dirty = true;
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
