use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use squad_protocol::*;
use squad_tui::{app::App, fx::TerminalCaps, widgets::input::EditorAction};
use std::time::Duration;
fn app() -> App {
    App::new(TerminalCaps::resolve(|_| None, false, (80, 24).into()).unwrap())
}
fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}
fn active(app: &mut App) {
    app.session.snapshot.context.active_contest =
        Some(ContestState::Generic(GenericContestState {
            contest_id: ContestId::new(),
            kind: ContestKind::Grill,
            state_version: 1,
            status: Text::new("RUNNING").unwrap(),
            step_id: None,
            artifact_event_ids: vec![],
            terminal_reason: None,
        }));
}
#[test]
fn transient_first_escape_and_confirmation_expiry() {
    let mut app = app();
    active(&mut app);
    app.transient = Some("Help".into());
    app.escape(Duration::ZERO);
    assert!(app.pending.is_none());
    assert!(app.stop_confirmation.is_none());
    app.escape(Duration::from_secs(1));
    assert!(app.pending.is_none());
    app.escape(Duration::from_secs(4));
    assert!(app.pending.is_none());
    app.escape(Duration::from_secs(5));
    assert!(matches!(app.pending, Some(Request::Stop { .. })));
}
#[test]
fn q_edits_in_input_and_quits_outside_it() {
    let mut app = app();
    app.handle(key(KeyCode::Char('q')), Duration::ZERO);
    assert_eq!(app.input.text(), "q");
    assert!(!app.quit);
    app.handle(key(KeyCode::F(6)), Duration::ZERO);
    app.handle(key(KeyCode::Char('q')), Duration::ZERO);
    assert!(app.quit);
}
#[test]
fn invalid_input_and_rejection_keep_draft_and_quotes() {
    let mut app = app();
    app.input
        .apply(EditorAction::Insert("hello".into()))
        .unwrap();
    app.quotes.toggle(EventId::new());
    app.submit();
    assert!(app.pending.is_none());
    assert_eq!(app.input.text(), "hello");
    assert_eq!(app.quotes.ids.len(), 1);
}
#[test]
fn accepted_ack_cannot_erase_new_typing_and_recall_is_explicit() {
    let mut app = app();
    app.session.snapshot.context.roster = vec![Nick::new("claude").unwrap()];
    app.input
        .apply(EditorAction::Insert("@claude hi".into()))
        .unwrap();
    app.submit();
    assert!(app.awaiting_ack);
    app.input
        .apply(EditorAction::Insert(" more".into()))
        .unwrap();
    app.acknowledge(RequestResult::Accepted {
        request_id: EventId::new(),
        event_ids: vec![],
        cursor: None,
    });
    assert_eq!(app.input.text(), "@claude hi more");
    assert_eq!(app.last_recipients, ["@claude"]);
}
#[test]
fn completion_only_replaces_current_token() {
    let mut app = app();
    app.session.snapshot.context.roster = vec![Nick::new("claude").unwrap()];
    app.input
        .apply(EditorAction::Insert("é @cl".into()))
        .unwrap();
    app.handle(key(KeyCode::Tab), Duration::ZERO);
    app.handle(key(KeyCode::Tab), Duration::ZERO);
    assert_eq!(app.input.text(), "é @claude ");
}

#[test]
fn ctrl_r_reinserts_previous_accepted_recipients_without_submitting() {
    let mut app = app();
    app.last_recipients = vec!["@claude".into(), "@openai".into()];
    app.handle(
        Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Char('r'),
            crossterm::event::KeyModifiers::CONTROL,
        )),
        std::time::Duration::ZERO,
    );
    assert_eq!(app.input.text(), "@claude @openai ");
    assert!(app.pending.is_none());
}

#[test]
fn export_destination_dialog_cancels_without_losing_command() {
    let mut app = app();
    app.input
        .apply(EditorAction::Insert("/export room".into()))
        .unwrap();
    app.submit();
    assert!(app.export_path.is_some());
    assert!(app.pending.is_none());
    app.escape(std::time::Duration::ZERO);
    assert!(app.export_path.is_none());
    assert_eq!(app.input.text(), "/export room");
}

#[test]
fn variable_assignment_remains_pending_until_acknowledged() {
    let mut app = app();
    app.input
        .apply(EditorAction::Insert("$GRILL_ROUNDS=4".into()))
        .unwrap();
    app.submit();
    assert!(app.awaiting_ack);
    assert_eq!(app.pending_updates[0].value, 4);
    assert!(app.session.snapshot.context.vars.is_empty());
    app.acknowledge(RequestResult::Accepted {
        request_id: EventId::new(),
        event_ids: vec![],
        cursor: None,
    });
    assert!(app.pending_updates.is_empty());
    assert!(app.input.text().is_empty());
}

#[test]
fn debug_is_a_transient_and_escape_does_not_stop_contest() {
    let mut app = app();
    active(&mut app);
    app.handle(key(KeyCode::F(12)), Duration::ZERO);
    assert!(app.debug_open);
    app.handle(key(KeyCode::Esc), Duration::ZERO);
    assert!(!app.debug_open);
    assert!(app.pending.is_none());
}
fn record(seq: i64, text: &str) -> parser::RecordSummary {
    parser::RecordSummary {
        event_id: EventId::new(),
        message_id: Some(MessageId::new()),
        seq: Cursor::new(seq).unwrap(),
        kind: Text::new("message.final").unwrap(),
        author: Actor::Me,
        recipients: vec![],
        content: text.into(),
        in_reply_to_message_id: None,
        contest_id: None,
        case_id: None,
        plan_event_id: None,
        grid_event_id: None,
        cell_ids: vec![],
        state_version: None,
        superseded: false,
        eligible: true,
    }
}
#[test]
fn transcript_focus_select_and_action_menu_attach_quotes() {
    let mut app = app();
    let (a, b) = (record(1, "first"), record(2, "second"));
    let ids = (a.event_id, b.event_id);
    app.session.snapshot.context.records = vec![b, a];
    // Input focus never steals message keys.
    app.handle(key(KeyCode::Char(' ')), Duration::ZERO);
    assert!(app.quotes.ids.is_empty());
    app.handle(key(KeyCode::F(6)), Duration::ZERO);
    app.handle(key(KeyCode::Down), Duration::ZERO);
    app.handle(key(KeyCode::Char(' ')), Duration::ZERO);
    app.handle(key(KeyCode::Down), Duration::ZERO);
    app.handle(key(KeyCode::Char(' ')), Duration::ZERO);
    assert!(app.quotes.ids.is_empty(), "selection alone attaches nothing");
    app.handle(key(KeyCode::Enter), Duration::ZERO);
    assert!(app.action_menu);
    assert!(app.quotes.ids.is_empty());
    app.handle(key(KeyCode::Enter), Duration::ZERO);
    assert_eq!(app.quotes.ids, vec![ids.0, ids.1]);
    assert!(!app.action_menu);
    // Submission metadata carries the quotes.
    assert_eq!(app.quotes.metadata().quote, vec![ids.0, ids.1]);
}
