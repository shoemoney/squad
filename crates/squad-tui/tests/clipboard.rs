use squad_tui::clipboard::{CopyError, CopyTarget, request_copy};
#[test]
fn osc52_copies_sanitized_unwrapped_source_and_code_only() {
    let copy = request_copy(
        "hello\x1b[31m!\x1b]52;c;evil\x07",
        CopyTarget::Message,
        true,
    )
    .unwrap();
    assert_eq!(copy.sequence, "\x1b]52;c;aGVsbG8h\x07");
    assert!(copy.feedback.contains("not confirmed"));
    let code = request_copy(
        "prose\n\n```rust\nhello\n```",
        CopyTarget::CodeBlock(0),
        true,
    )
    .unwrap();
    assert_eq!(code.sequence, "\x1b]52;c;aGVsbG8K\x07");
    assert_eq!(
        request_copy("body", CopyTarget::CodeBlock(0), true),
        Err(CopyError::MissingCodeBlock)
    );
    assert_eq!(
        request_copy("body", CopyTarget::Message, false),
        Err(CopyError::Unavailable)
    );
    assert!(CopyError::Unavailable.feedback().contains("/export"));
}

#[tokio::test]
async fn event_loop_delivers_copy_once_and_flushes_without_a_frame() {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    use ratatui::{Terminal, backend::TestBackend};
    use squad_tui::{app::App, event_loop, fx::TerminalCaps, session::FakeRoom};
    use std::{io, sync::Arc};
    use tokio::sync::mpsc;

    #[derive(Default)]
    struct Output {
        bytes: Vec<u8>,
        flushes: usize,
    }
    impl io::Write for Output {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            self.flushes += 1;
            Ok(())
        }
    }
    let mut app = App::new(TerminalCaps::resolve(|_| None, false, (80, 24).into()).unwrap());
    app.pending_copy = Some(request_copy("hello!", CopyTarget::Message, true).unwrap());
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    let (tx, mut rx) = mpsc::channel(2);
    tx.send(Ok(Event::Key(KeyEvent::new(
        KeyCode::Char('c'),
        KeyModifiers::CONTROL,
    ))))
    .await
    .unwrap();
    let mut output = Output::default();
    event_loop::run_with_output(
        &mut app,
        Arc::new(FakeRoom::new()),
        &mut terminal,
        &mut rx,
        &mut output,
    )
    .await
    .unwrap();
    assert_eq!(app.draws, 0, "copy must not depend on a scheduled redraw");
    assert_eq!(output.bytes, b"\x1b]52;c;aGVsbG8h\x07");
    assert_eq!(output.flushes, 1);
    assert!(app.take_copy().is_none());
}
