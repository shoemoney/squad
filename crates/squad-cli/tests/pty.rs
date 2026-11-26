//! Real-binary tests inside a pseudo-terminal: startup, a streamed round trip,
//! terminal restoration, hangup mid-stream and degraded terminals. Every wait is
//! a deadline on rendered screen content, never a fixed sleep.
#![cfg(unix)]
#![allow(dead_code)]
#[path = "../../squad-agents/tests/support/fake_openrouter.rs"]
mod fake_openrouter;
use fake_openrouter::{FakeOpenRouter, MODEL};
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::{
    io::{Read, Write},
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const WAIT: Duration = Duration::from_secs(30);
const POLL: Duration = Duration::from_millis(15);

struct Pty {
    child: Box<dyn Child + Send + Sync>,
    master: Box<dyn MasterPty + Send>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    screen: Arc<Mutex<vt100::Parser>>,
    raw: Arc<Mutex<Vec<u8>>>,
}

impl Pty {
    fn spawn(root: &Path, provider: Option<&FakeOpenRouter>, extra: &[(&str, &str)]) -> Self {
        Self::spawn_sized(root, provider, extra, 100, 32)
    }

    fn spawn_sized(
        root: &Path,
        provider: Option<&FakeOpenRouter>,
        extra: &[(&str, &str)],
        cols: u16,
        rows: u16,
    ) -> Self {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
        let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_squad"));
        command.env_clear();
        command.env("HOME", root);
        command.env("TERM", "xterm-256color");
        command.env("PATH", "/usr/bin:/bin");
        command.cwd(root);
        command.arg("--data-dir");
        command.arg(root.join("data"));
        if let Some(provider) = provider {
            command.env("OPENROUTER_API_KEY", "fixture-key");
            command.env("OPENROUTER_BASE_URL", provider.base_url());
            command.env("SQUAD_JUDGE_MODEL", MODEL);
            command.env("SQUAD_MODEL_A", MODEL);
            command.env("SQUAD_MODEL_B", MODEL);
        }
        for (key, value) in extra {
            command.env(key, value);
        }
        let child = pair.slave.spawn_command(command).unwrap();
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().unwrap();
        let writer = Arc::new(Mutex::new(pair.master.take_writer().unwrap()));
        let screen = Arc::new(Mutex::new(vt100::Parser::new(
            std::num::NonZeroU16::new(rows).unwrap(),
            std::num::NonZeroU16::new(cols).unwrap(),
            0,
        )));
        let raw = Arc::new(Mutex::new(Vec::new()));
        {
            let (writer, screen, raw) = (writer.clone(), screen.clone(), raw.clone());
            std::thread::spawn(move || {
                let mut buffer = [0u8; 8192];
                // Terminal capability probes get an answer a real emulator would give.
                let mut tail = Vec::new();
                while let Ok(n) = reader.read(&mut buffer) {
                    if n == 0 {
                        break;
                    }
                    raw.lock().unwrap().extend_from_slice(&buffer[..n]);
                    screen.lock().unwrap().process(&buffer[..n]);
                    tail.extend_from_slice(&buffer[..n]);
                    if tail.windows(3).any(|w| w == b"\x1b[c") {
                        let _ = writer.lock().unwrap().write_all(b"\x1b[?62;c");
                        tail.clear();
                    }
                    if tail.len() > 64 {
                        tail.drain(..tail.len() - 8);
                    }
                }
            });
        }
        Self {
            child,
            master: pair.master,
            writer,
            screen,
            raw,
        }
    }

    fn send(&self, bytes: &[u8]) {
        let mut writer = self.writer.lock().unwrap();
        writer.write_all(bytes).unwrap();
        writer.flush().unwrap();
    }

    fn contents(&self) -> String {
        self.screen.lock().unwrap().screen().contents()
    }

    fn raw(&self) -> Vec<u8> {
        self.raw.lock().unwrap().clone()
    }

    /// Poll the rendered screen until `ready` accepts it, else fail with the screen.
    fn wait_for(&self, what: &str, ready: impl Fn(&str) -> bool) {
        let deadline = Instant::now() + WAIT;
        loop {
            let contents = self.contents();
            if ready(&contents) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for {what}; screen was:\n{contents}"
            );
            std::thread::sleep(POLL);
        }
    }

    fn wait_exit(&mut self) -> portable_pty::ExitStatus {
        let deadline = Instant::now() + WAIT;
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                // Let the reader drain what the child wrote before it exited.
                self.drain();
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "process did not exit; screen was:\n{}",
                self.contents()
            );
            std::thread::sleep(POLL);
        }
    }

    /// Wait until the raw byte count stops changing across consecutive polls.
    fn drain(&self) {
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut last = usize::MAX;
        let mut stable = 0;
        while Instant::now() < deadline && stable < 5 {
            let len = self.raw.lock().unwrap().len();
            stable = if len == last { stable + 1 } else { 0 };
            last = len;
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn signal(&self, name: &str) {
        let status = std::process::Command::new("kill")
            .args([format!("-{name}"), self.pid().to_string()])
            .status()
            .unwrap();
        assert!(status.success(), "kill -{name} {}", self.pid());
    }

    fn pid(&self) -> u32 {
        self.child.process_id().unwrap()
    }

    /// Canonical mode and echo are the cooked-terminal flags raw mode clears.
    fn cooked(&self) -> bool {
        let termios = self.master.get_termios().expect("termios");
        let flags = libc::termios::from(termios).c_lflag;
        flags & libc::ICANON != 0 && flags & libc::ECHO != 0
    }
}

/// Last position of `needle`, if any.
fn last(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).rposition(|w| w == needle)
}

fn assert_restored(pty: &Pty) {
    let raw = pty.raw();
    let enter = last(&raw, b"\x1b[?1049h").expect("alternate screen entered");
    let leave = last(&raw, b"\x1b[?1049l").expect("alternate screen left");
    assert!(leave > enter, "alternate screen left after entering");
    let hide = last(&raw, b"\x1b[?25l").expect("cursor hidden");
    let show = last(&raw, b"\x1b[?25h").expect("cursor shown");
    assert!(show > hide, "cursor shown after the last hide");
    if let Some(on) = last(&raw, b"\x1b[?1000h") {
        assert!(
            last(&raw, b"\x1b[?1000l").is_some_and(|off| off > on),
            "mouse capture released"
        );
    }
    if let Some(on) = last(&raw, b"\x1b[?2004h") {
        assert!(
            last(&raw, b"\x1b[?2004l").is_some_and(|off| off > on),
            "bracketed paste released"
        );
    }
    assert!(pty.cooked(), "raw mode is off");
}

async fn provider(responses: Vec<wiremock::ResponseTemplate>) -> FakeOpenRouter {
    let server = FakeOpenRouter::start().await;
    server.catalog(fake_openrouter::catalog()).await;
    server.completions(responses).await;
    server
        .generation(
            fake_openrouter::GENERATION,
            serde_json::json!({"total_cost": 0.000045}),
        )
        .await;
    server
}

fn ready_screen(screen: &str) -> bool {
    screen.contains("six minds, one room")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn at_all_hello_streams_replies_and_ctrl_c_restores_the_terminal() {
    let server = provider(vec![
        fake_openrouter::streaming("Alpha fixture answer", "stop"),
        fake_openrouter::streaming("Beta fixture answer", "stop"),
    ])
    .await;
    let root = tempfile::tempdir().unwrap();
    let mut pty = Pty::spawn(root.path(), Some(&server), &[("SQUAD_LOG_LEVEL", "debug")]);
    pty.wait_for("the room to render", ready_screen);
    pty.send(b"@all hello");
    pty.wait_for("the draft in the input", |s| s.contains("@all hello"));
    pty.send(b"\r");
    pty.wait_for("both streamed replies", |s| {
        s.contains("fixture answer") && s.matches("fixture answer").count() >= 2
    });
    assert!(!pty.contents().contains("PRIVATE REASONING"));
    pty.send(b"\x03");
    let status = pty.wait_exit();
    // A normal TUI quit is exit 0 (PRD 05); 130 is reserved for SIGINT, tested below.
    assert_eq!(status.exit_code(), 0, "Ctrl+C key quits the TUI cleanly");
    assert_restored(&pty);
    let log = std::fs::read_to_string(root.path().join("data/logs/squad.jsonl")).unwrap();
    assert!(log.contains("Router commit accepted"));
    let terminal = String::from_utf8_lossy(&pty.raw()).into_owned();
    assert!(!terminal.contains("Router commit accepted"));
    for line in log.lines() {
        serde_json::from_str::<serde_json::Value>(line).unwrap();
    }
    assert!(!log.contains("fixture-key") && !log.contains("PRIVATE REASONING"));

    assert!(
        server.captured_bodies().await.len() >= 2,
        "both members called the provider"
    );
}

fn work_statuses(root: &Path) -> Vec<(String, String)> {
    let connection = rusqlite::Connection::open_with_flags(
        root.join("data/squad.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    connection.busy_timeout(Duration::from_secs(5)).unwrap();
    let mut query = connection
        .prepare("SELECT target, status FROM work ORDER BY target")
        .unwrap();
    query
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn integrity_ok(root: &Path) -> bool {
    let connection = rusqlite::Connection::open(root.join("data/squad.db")).unwrap();
    connection.busy_timeout(Duration::from_secs(5)).unwrap();
    let check: String = connection
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .unwrap();
    let violations: i64 = connection
        .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| {
            r.get(0)
        })
        .unwrap();
    check == "ok" && violations == 0
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sigint_exits_130_after_restoring_the_terminal() {
    let server = provider(vec![fake_openrouter::streaming("unused", "stop"); 4]).await;
    let root = tempfile::tempdir().unwrap();
    let mut pty = Pty::spawn(root.path(), Some(&server), &[]);
    pty.wait_for("the room to render", ready_screen);
    pty.signal("INT");
    let status = pty.wait_exit();
    assert_eq!(status.exit_code(), 130, "SIGINT exits 130 after cleanup");
    assert_restored(&pty);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn hangup_mid_stream_exits_leaves_the_store_sound_and_the_next_start_recovers() {
    // The reply is held open by the provider, so the work is in flight at hangup.
    let server = provider(vec![
        fake_openrouter::delayed(Duration::from_secs(120)),
        fake_openrouter::delayed(Duration::from_secs(120)),
    ])
    .await;
    let root = tempfile::tempdir().unwrap();
    let mut pty = Pty::spawn(root.path(), Some(&server), &[]);
    pty.wait_for("the room to render", ready_screen);
    pty.send(b"@all hello\r");
    let deadline = Instant::now() + WAIT;
    while server.captured_bodies().await.len() < 2 {
        assert!(
            Instant::now() < deadline,
            "provider never received both requests; screen:\n{}",
            pty.contents()
        );
        std::thread::sleep(POLL);
    }
    pty.signal("HUP");
    let status = pty.wait_exit();
    drop(pty);
    // The process is gone (wait_exit has a deadline), however it handled the hangup.
    eprintln!("hangup exit status: {status:?}");
    assert!(integrity_ok(root.path()), "database is sound after hangup");
    let interrupted = work_statuses(root.path());
    assert_eq!(
        interrupted.len(),
        2,
        "both replies were recorded: {interrupted:?}"
    );
    assert!(
        interrupted
            .iter()
            .all(|(_, status)| status == "running" || status == "queued"),
        "interrupted work stays unfinished in the store: {interrupted:?}"
    );

    // Next start: a fresh provider answers, and the interrupted work completes.
    let fresh = provider(vec![
        fake_openrouter::streaming("Recovered alpha answer", "stop"),
        fake_openrouter::streaming("Recovered beta answer", "stop"),
    ])
    .await;
    let mut pty = Pty::spawn(root.path(), Some(&fresh), &[]);
    pty.wait_for("recovered replies", |s| s.matches("Recovered").count() >= 2);
    pty.send(b"\x03");
    assert_eq!(pty.wait_exit().exit_code(), 0);
    assert_restored(&pty);
    let recovered = work_statuses(root.path());
    assert!(
        recovered.iter().all(|(_, status)| status == "completed"),
        "interrupted work completed after restart: {recovered:?}"
    );
    assert!(integrity_ok(root.path()));
    // A clean shutdown leaves no write-ahead log behind.
    assert!(!root.path().join("data/squad.db-wal").exists());
}

#[test]
fn missing_key_prints_the_message_and_never_enters_the_alternate_screen() {
    let root = tempfile::tempdir().unwrap();
    let mut pty = Pty::spawn(root.path(), None, &[]);
    let status = pty.wait_exit();
    assert_eq!(status.exit_code(), 1);
    let screen = pty.contents();
    assert!(
        screen
            .contains("OPENROUTER_API_KEY is required. Add it to .env or the process environment."),
        "screen was:\n{screen}"
    );
    let raw = pty.raw();
    assert!(
        last(&raw, b"\x1b[?1049h").is_none(),
        "alternate screen never entered"
    );
    assert!(pty.cooked(), "terminal left in cooked mode");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn too_small_terminal_shows_the_resize_message_and_recovers_on_resize() {
    let server = provider(vec![fake_openrouter::streaming("unused", "stop"); 4]).await;
    let root = tempfile::tempdir().unwrap();
    let mut pty = Pty::spawn_sized(root.path(), Some(&server), &[], 60, 15);
    pty.wait_for("the too-small notice", |s| {
        s.contains("Terminal too small: 60×15. Required: 80×24.")
    });
    assert!(pty.contents().contains("Resize to continue"));
    pty.master
        .resize(PtySize {
            rows: 30,
            cols: 100,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    pty.screen.lock().unwrap().screen_mut().set_size(
        std::num::NonZeroU16::new(30).unwrap(),
        std::num::NonZeroU16::new(100).unwrap(),
    );
    pty.wait_for("the room after resize", |s| {
        !s.contains("Terminal too small") && s.contains("Input")
    });
    pty.send(b"\x03");
    assert_eq!(pty.wait_exit().exit_code(), 0);
    assert_restored(&pty);
}

/// True when any cell of the screen carries a foreground or background colour.
fn any_colour(pty: &Pty) -> bool {
    let guard = pty.screen.lock().unwrap();
    let screen = guard.screen();
    let (rows, cols) = screen.size();
    (0..rows.get()).any(|row| {
        (0..cols.get()).any(|col| {
            screen.cell(row, col).is_some_and(|cell| {
                cell.fgcolor() != vt100::Color::Default || cell.bgcolor() != vt100::Color::Default
            })
        })
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_color_renders_without_colour_and_colour_is_the_default() {
    let server = provider(vec![
        fake_openrouter::streaming("Alpha fixture answer", "stop"),
        fake_openrouter::streaming("Beta fixture answer", "stop"),
    ])
    .await;
    let root = tempfile::tempdir().unwrap();
    let mut plain = Pty::spawn(root.path(), Some(&server), &[("NO_COLOR", "1")]);
    plain.wait_for("the room to render", ready_screen);
    plain.send(b"@all hello\r");
    plain.wait_for("both replies", |s| s.matches("fixture answer").count() >= 2);
    assert!(!any_colour(&plain), "NO_COLOR draws no colour");
    plain.send(b"\x03");
    assert_eq!(plain.wait_exit().exit_code(), 0);
    assert_restored(&plain);
    let sgr_colour = |raw: &[u8]| {
        String::from_utf8_lossy(raw)
            .split("\x1b[")
            .skip(1)
            .any(|seq| {
                let params = seq.split('m').next().unwrap_or("");
                seq.contains('m')
                    && params.chars().all(|c| c.is_ascii_digit() || c == ';')
                    && params.split(';').any(|p| matches!(p, "38" | "48"))
            })
    };
    assert!(
        !sgr_colour(&plain.raw()),
        "no extended colour escapes with NO_COLOR"
    );

    // Same room, colour allowed: the control run must actually use colour,
    // otherwise the NO_COLOR assertion above proves nothing.
    let other = tempfile::tempdir().unwrap();
    let coloured = Pty::spawn(other.path(), Some(&server), &[("COLORTERM", "truecolor")]);
    coloured.wait_for("the room to render", ready_screen);
    assert!(
        any_colour(&coloured),
        "colour is used when NO_COLOR is unset"
    );
    coloured.send(b"\x03");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn startup_draws_before_catalog_and_ctrl_c_cancels_and_restores() {
    use wiremock::{
        Mock, ResponseTemplate,
        matchers::{method, path},
    };
    let provider = FakeOpenRouter::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/models"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(fake_openrouter::catalog())
                .set_delay(Duration::from_secs(5)),
        )
        .mount(&provider.server)
        .await;
    let root = tempfile::tempdir().unwrap();
    let started = Instant::now();
    let mut pty = Pty::spawn(root.path(), Some(&provider), &[]);
    pty.wait_for("startup checklist", |s| {
        s.contains("Resolving 3 models") && s.contains("Ctrl+C to quit") && s.contains("judge")
    });
    // Leave scheduling slack in CI; the measured local first frame is reported
    // separately. A five-second catalog must never gate terminal output.
    assert!(started.elapsed() < Duration::from_secs(2));
    let cancel = Instant::now();
    pty.send(b"\x03");
    assert_eq!(pty.wait_exit().exit_code(), 130);
    assert!(cancel.elapsed() < Duration::from_secs(2));
    assert_restored(&pty);
    assert!(provider.captured_bodies().await.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn startup_reports_each_seat_before_the_last_probe_finishes() {
    use wiremock::{
        Mock,
        matchers::{body_partial_json, method, path},
    };
    let provider = FakeOpenRouter::start().await;
    provider.catalog(fake_openrouter::catalog()).await;
    for (alias, delay) in [("~fixture/fast", 20), ("~fixture/slow", 1200)] {
        Mock::given(method("POST"))
            .and(path("/api/v1/chat/completions"))
            .and(body_partial_json(serde_json::json!({"model": alias})))
            .respond_with(
                fake_openrouter::alias_probe(MODEL).set_delay(Duration::from_millis(delay)),
            )
            .mount(&provider.server)
            .await;
    }
    let root = tempfile::tempdir().unwrap();
    let mut pty = Pty::spawn(
        root.path(),
        Some(&provider),
        &[
            ("SQUAD_MODEL_A", "~fixture/fast"),
            ("SQUAD_MODEL_B", "~fixture/slow"),
        ],
    );
    pty.wait_for("partial seat progress", |s| {
        s.contains(&format!("✓ a → {MODEL}")) && s.contains("⏳ b") && s.contains("✓ judge")
    });
    pty.wait_for("ready room", |s| s.contains("six minds, one room"));
    pty.send(b"\x03");
    assert!(pty.wait_exit().success());
    assert_restored(&pty);
}
