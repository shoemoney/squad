use squad_protocol::*;
use squad_tui::{
    app::{App, SystemCommand},
    fx::TerminalCaps,
    widgets::{input::EditorAction, transcript::TranscriptMessage},
};

fn app() -> App {
    App::new(TerminalCaps::resolve(|_| None, false, (80, 24).into()).unwrap())
}
fn type_and_submit(app: &mut App, text: &str) {
    app.input.apply(EditorAction::Insert(text.into())).unwrap();
    app.submit();
}

#[test]
fn dot_commands_parse_and_slash_or_prose_do_not() {
    assert_eq!(SystemCommand::parse(".quit"), Some(SystemCommand::Quit));
    assert_eq!(SystemCommand::parse("  .exit "), Some(SystemCommand::Quit));
    assert_eq!(SystemCommand::parse(".dump"), Some(SystemCommand::Dump));
    for other in [
        "/quit",
        "/exit",
        "quit",
        ".quit now",
        ".5 is a number",
        ".dumpster",
    ] {
        assert_eq!(SystemCommand::parse(other), None, "{other}");
    }
}

#[test]
fn quit_and_exit_leave_the_room_without_sending_a_request() {
    for text in [".quit", ".exit"] {
        let mut app = app();
        type_and_submit(&mut app, text);
        assert!(app.quit, "{text}");
        assert!(app.pending.is_none(), "{text}");
    }
}

#[test]
fn dump_writes_the_whole_buffer_to_a_timestamped_file_and_never_overwrites() {
    let mut app = app();
    let id = |_| MessageId::from_event(EventId::new());
    app.composed
        .transcript
        .upsert(TranscriptMessage::new(id(0), "me", "first question"));
    app.composed
        .transcript
        .upsert(TranscriptMessage::new(id(1), "claude", "the full answer"));
    let dir = std::env::temp_dir().join(format!("squad-dump-test-{}", EventId::new()));
    std::fs::create_dir_all(&dir).unwrap();

    let first = app.dump_buffer(&dir).unwrap();
    let second = app.dump_buffer(&dir).unwrap();
    assert_ne!(
        first, second,
        "a second dump in the same second must not overwrite"
    );
    let name = first.file_name().unwrap().to_str().unwrap();
    assert!(
        name.starts_with("output-") && name.ends_with(".txt"),
        "{name}"
    );
    let text = std::fs::read_to_string(&first).unwrap();
    assert!(text.contains("== me\nfirst question\n"), "{text}");
    assert!(text.contains("== claude\nthe full answer\n"), "{text}");
    assert!(text.find("first question") < text.find("the full answer"));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn dot_dump_via_the_composer_clears_the_draft_and_reports_the_file() {
    let dir = std::env::temp_dir().join(format!("squad-dump-cwd-{}", EventId::new()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut app = app();
    app.composed.transcript.upsert(TranscriptMessage::new(
        MessageId::from_event(EventId::new()),
        "claude",
        "hello",
    ));
    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(&dir).unwrap();
    type_and_submit(&mut app, ".dump");
    std::env::set_current_dir(original).unwrap();
    assert_eq!(app.input.text(), "");
    assert!(
        app.notice
            .as_deref()
            .unwrap()
            .starts_with("Dumped the buffer to ")
    );
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    std::fs::remove_dir_all(&dir).unwrap();
}
