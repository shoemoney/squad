use squad_tui::terminal::{self, TerminalGuard};
use std::{
    io::{self, IsTerminal, Write},
    panic::{AssertUnwindSafe, catch_unwind},
};

#[test]
fn sync_closes_on_success_error_and_panic() {
    let mut bytes = Vec::new();
    terminal::synchronized_output(&mut bytes, true, |w| w.write_all(b"draw")).unwrap();
    assert_eq!(bytes, b"\x1b[?2026hdraw\x1b[?2026l");
    bytes.clear();
    let result: io::Result<()> =
        terminal::synchronized_output(&mut bytes, true, |_| Err(io::Error::other("draw failed")));
    assert!(result.is_err());
    assert!(bytes.ends_with(b"\x1b[?2026l"));
    bytes.clear();
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            let _: io::Result<()> =
                terminal::synchronized_output(&mut bytes, true, |_| panic!("draw panic"));
        }))
        .is_err()
    );
    assert!(bytes.ends_with(b"\x1b[?2026l"));
    bytes.clear();
    terminal::synchronized_output(&mut bytes, false, |w| w.write_all(b"plain")).unwrap();
    assert_eq!(bytes, b"plain");
}

#[test]
fn capability_replies_and_fallbacks() {
    for (reply, expected) in [
        (&b"\x1b[?2026;1$y"[..], true),
        (&b"\x1b[?2026;2$y"[..], true),
        (&b"\x1b[?2026;0$y"[..], false),
        (&b"garbage"[..], false),
    ] {
        let mut bytes = Vec::new();
        let mut chunks = reply.chunks(3);
        assert_eq!(
            terminal::query_synchronized_output(&mut bytes, |timeout| {
                assert!(timeout <= terminal::CAPABILITY_TIMEOUT);
                Ok(chunks.next().map(<[u8]>::to_vec))
            }),
            expected
        );
        assert_eq!(bytes, terminal::SYNC_QUERY);
    }
    assert!(!terminal::query_synchronized_output(
        &mut Vec::new(),
        |_| Ok(None)
    ));
    assert!(!terminal::query_synchronized_output(
        &mut Vec::new(),
        |_| Err(io::Error::other("probe failed"))
    ));
}

#[test]
fn terminal_lifecycle_when_tty_available() {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        eprintln!("skipping real terminal lifecycle: no TTY");
        return;
    }
    let initially_raw = crossterm::terminal::is_raw_mode_enabled().unwrap();
    if initially_raw {
        return;
    }
    let guard = TerminalGuard::enter().unwrap();
    assert!(crossterm::terminal::is_raw_mode_enabled().unwrap());
    assert!(TerminalGuard::enter().is_err());
    guard.restore().unwrap();
    assert!(!crossterm::terminal::is_raw_mode_enabled().unwrap());
    assert!(
        catch_unwind(|| {
            let _guard = TerminalGuard::enter().unwrap();
            panic!("exercise terminal panic cleanup");
        })
        .is_err()
    );
    assert!(!crossterm::terminal::is_raw_mode_enabled().unwrap());
}

#[test]
fn failed_begin_still_attempts_end_and_skips_draw() {
    #[derive(Default)]
    struct FailOnce {
        failed: bool,
        bytes: Vec<u8>,
    }
    impl Write for FailOnce {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if !self.failed {
                self.failed = true;
                return Err(io::Error::other("begin failed"));
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut writer = FailOnce::default();
    let result = terminal::synchronized_output(&mut writer, true, |_| -> io::Result<()> {
        panic!("draw must not run after failed begin")
    });
    assert!(result.is_err());
    assert_eq!(writer.bytes, b"\x1b[?2026l");
}
