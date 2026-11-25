//! Keyboard reachability of retry, regenerate and per-target stop.
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use squad_protocol::*;
use squad_tui::{app::App, fx::TerminalCaps};
use std::time::Duration;

fn app() -> App {
    let caps = TerminalCaps::resolve(
        |k| (k == "SQUAD_FX").then(|| "off".into()),
        true,
        (100, 30).into(),
    )
    .unwrap();
    App::new(caps)
}
fn press(app: &mut App, code: KeyCode) {
    press_mod(app, code, KeyModifiers::NONE);
}
fn press_mod(app: &mut App, code: KeyCode, m: KeyModifiers) {
    app.handle(Event::Key(KeyEvent::new(code, m)), Duration::ZERO);
}
fn record(seq: i64, author: Actor, message_id: MessageId) -> parser::RecordSummary {
    parser::RecordSummary {
        event_id: EventId::new(),
        message_id: Some(message_id),
        seq: Cursor::new(seq).unwrap(),
        kind: Text::new("message.final").unwrap(),
        author,
        recipients: vec![],
        content: "hello".into(),
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
/// A room-focused app whose single focused message is a member reply.
fn focused(status: Option<WorkStatus>) -> (App, MessageId, WorkId, MemberNick) {
    let mut app = app();
    let nick = MemberNick::new("grok").unwrap();
    let (mid, work) = (MessageId::new(), WorkId::new());
    let rec = record(1, Actor::Member(nick.clone()), mid);
    app.msg_focus = Some(rec.event_id);
    app.session.snapshot.context.records.push(rec);
    app.session
        .transcript
        .complete(mid, Actor::Member(nick.clone()), 0, "reply".into());
    let card = app.session.transcript.cards.get_mut(&mid).unwrap();
    card.work_id = Some(work);
    card.terminal = status;
    app.focus_input = false;
    (app, mid, work, nick)
}

#[test]
fn r_regenerates_the_focused_reply() {
    let (mut app, mid, _, _) = focused(Some(WorkStatus::Completed));
    press(&mut app, KeyCode::Char('r'));
    assert!(matches!(
        app.pending,
        Some(Request::Regen { message_id: Some(m), .. }) if m == mid
    ));
}

#[test]
fn r_without_focus_regenerates_the_latest_reply() {
    let mut app = app();
    app.focus_input = false;
    press(&mut app, KeyCode::Char('r'));
    assert!(matches!(
        app.pending,
        Some(Request::Regen {
            message_id: None,
            ..
        })
    ));
}

#[test]
fn t_retries_only_the_focused_failed_work_item() {
    let (mut app, _, work, _) = focused(Some(WorkStatus::Failed));
    press(&mut app, KeyCode::Char('t'));
    assert!(matches!(
        app.pending,
        Some(Request::Retry { work_id, .. }) if work_id == work
    ));
}

#[test]
fn t_refuses_a_reply_that_did_not_fail() {
    let (mut app, ..) = focused(Some(WorkStatus::Completed));
    press(&mut app, KeyCode::Char('t'));
    assert!(app.pending.is_none());
    assert!(
        app.notice
            .as_deref()
            .unwrap()
            .contains("failed or cancelled")
    );
}

#[test]
fn x_stops_only_the_focused_replys_member() {
    let (mut app, _, _, nick) = focused(Some(WorkStatus::Running));
    press(&mut app, KeyCode::Char('x'));
    assert!(matches!(
        app.pending,
        Some(Request::Stop { target: Some(ref t), .. }) if *t == nick
    ));
}

#[test]
fn keys_are_inert_while_typing_and_ctrl_r_keeps_its_meaning() {
    let (mut app, ..) = focused(Some(WorkStatus::Failed));
    app.focus_input = true;
    for c in ['r', 't', 'x'] {
        press(&mut app, KeyCode::Char(c));
    }
    assert!(app.pending.is_none());
    assert_eq!(app.input.text(), "rtx");
    app.focus_input = false;
    press_mod(&mut app, KeyCode::Char('r'), KeyModifiers::CONTROL);
    assert!(app.pending.is_none(), "Ctrl+R is not regenerate");
}
