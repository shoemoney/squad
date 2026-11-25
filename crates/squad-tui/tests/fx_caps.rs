use ratatui::layout::Size;
use squad_tui::fx::{caps::*, *};
use std::{collections::BTreeMap, io, time::Duration};

#[test]
fn explicit_levels_override_only_automatic_downgrade() {
    for ssh in [false, true] {
        for sync in [false, true] {
            for explicit in [None, Some("full"), Some("lite"), Some("off")] {
                let caps = TerminalCaps::resolve(
                    |key| match key {
                        "SSH_TTY" if ssh => Some("/dev/pts/2".into()),
                        "SQUAD_FX" => explicit.map(str::to_owned),
                        _ => None,
                    },
                    sync,
                    Size::new(120, 40),
                )
                .unwrap();
                let expected = explicit
                    .map(|s| s.parse().unwrap())
                    .unwrap_or(if ssh || !sync {
                        FxLevel::Lite
                    } else {
                        FxLevel::Full
                    });
                assert_eq!(
                    caps.level, expected,
                    "ssh={ssh} sync={sync} explicit={explicit:?}"
                );
                assert_eq!(caps.full_buffer_passes(), expected == FxLevel::Full && sync);
            }
        }
    }
}
#[test]
fn color_environment_precedence_and_tmux() {
    for (vars, expected) in [
        (vec![("COLORTERM", "truecolor")], ColorTier::TrueColor),
        (vec![("COLORTERM", "24bit")], ColorTier::TrueColor),
        (vec![("TERM", "xterm-direct")], ColorTier::TrueColor),
        (vec![("TERM", "xterm-256color")], ColorTier::Indexed256),
        (vec![("TERM", "vt100")], ColorTier::Ansi16),
        (
            vec![("TERM", "dumb"), ("COLORTERM", "truecolor")],
            ColorTier::NoColor,
        ),
        (
            vec![("NO_COLOR", ""), ("COLORTERM", "truecolor")],
            ColorTier::NoColor,
        ),
    ] {
        let vars: BTreeMap<_, _> = vars.into_iter().collect();
        let caps = TerminalCaps::resolve(
            |key| vars.get(key).map(|s| s.to_string()),
            true,
            Size::new(80, 24),
        )
        .unwrap();
        assert_eq!(caps.color, expected);
    }
    let caps = TerminalCaps::resolve(
        |key| (key == "TMUX" || key == "SQUAD_FX_DEBUG").then(|| "1".into()),
        false,
        Size::new(80, 24),
    )
    .unwrap();
    assert!(caps.tmux && caps.debug);
    assert_eq!(caps.level, FxLevel::Lite);
}
#[test]
fn query_accepts_fragmented_decrqm_and_preserves_keys() {
    let clock = ManualClock::default();
    let mut written = Vec::new();
    let mut chunks = [b"q\x1b[?20".to_vec(), b"26;2$yhello".to_vec()].into_iter();
    let result = probe_with_clock(
        &mut written,
        |timeout| {
            assert!(timeout <= PROBE_TIMEOUT);
            clock.advance(Duration::from_millis(10));
            Ok(chunks.next().unwrap_or_default())
        },
        &clock,
    )
    .unwrap();
    assert_eq!(written, SYNC_QUERY);
    assert!(result.supported);
    assert_eq!(result.pending_input, b"qhello");
}
#[test]
fn query_statuses_and_deadline() {
    for status in 0..=4 {
        let clock = ManualClock::default();
        let result = probe_with_clock(
            &mut Vec::new(),
            |_| Ok(format!("\x1b[?2026;{status}$y").into_bytes()),
            &clock,
        )
        .unwrap();
        assert_eq!(result.supported, status == 1 || status == 2);
    }
    let clock = ManualClock::default();
    let mut calls = 0;
    let result = probe_with_clock(
        &mut Vec::new(),
        |timeout| {
            calls += 1;
            clock.advance(timeout);
            Ok(b"typed".to_vec())
        },
        &clock,
    )
    .unwrap();
    assert_eq!(calls, 1);
    assert!(!result.supported);
    assert_eq!(clock.now(), PROBE_TIMEOUT);
    assert_eq!(result.pending_input, b"typed");
}
#[test]
fn timeout_is_unsupported_and_io_errors_propagate() {
    let clock = ManualClock::default();
    let result = probe_with_clock(
        &mut Vec::new(),
        |_| Err(io::ErrorKind::TimedOut.into()),
        &clock,
    )
    .unwrap();
    assert!(!result.supported);
    assert!(
        probe_with_clock(
            &mut Vec::new(),
            |_| Err(io::ErrorKind::BrokenPipe.into()),
            &clock
        )
        .is_err()
    );
}
#[test]
fn manual_clock_clones_share_monotonic_time() {
    let clock = ManualClock::at(Duration::from_micros(1));
    let clone = clock.clone();
    clock.advance(Duration::from_micros(2));
    assert_eq!(clone.now(), Duration::from_micros(3));
    clone.set(Duration::from_secs(1));
    assert_eq!(clock.now(), Duration::from_secs(1));
}
#[test]
#[should_panic(expected = "clock must not run backwards")]
fn manual_clock_rejects_time_travel() {
    ManualClock::at(Duration::from_secs(1)).set(Duration::ZERO);
}
