use ratatui::{buffer::Buffer, layout::Rect, style::Color};
use squad_tui::effects::status::{Level, StatusStyle, TokenRate};
use std::time::Duration;
fn fx(level: Level, no_color: bool) -> StatusStyle {
    StatusStyle {
        level,
        no_color,
        ansi16: false,
        hue: Color::Cyan,
    }
}
#[test]
fn storyboard() {
    let mut frames = Vec::new();
    for level in [Level::Full, Level::Lite, Level::Off] {
        for nc in [false, true] {
            for ms in [0, 125, 250, 500, 517] {
                let f = fx(level, nc);
                let t = Duration::from_millis(ms);
                let mut b = Buffer::empty(Rect::new(0, 0, 40, 4));
                for (row, indicator) in [
                    f.thinking(t),
                    f.streaming(Some(24.0)),
                    f.backoff(t, Duration::from_millis(500)),
                    f.pips(2, 3, t),
                ]
                .into_iter()
                .enumerate()
                {
                    b.set_stringn(0, row as u16, indicator.text, 40, indicator.style);
                }
                frames.push((level, nc, ms, b));
            }
        }
    }
    insta::assert_debug_snapshot!(frames);
}
#[test]
fn real_rate_uses_rolling_half_second() {
    let mut rate = TokenRate::default();
    assert_eq!(rate.per_second(Duration::ZERO), None);
    rate.record(Duration::ZERO, 10);
    rate.record(Duration::from_millis(250), 20);
    assert_eq!(rate.per_second(Duration::from_millis(499)), Some(60.0));
    assert_eq!(rate.per_second(Duration::from_millis(500)), Some(40.0));
    assert_eq!(rate.per_second(Duration::from_millis(750)), Some(0.0));
    rate.record(Duration::from_millis(1), 3);
    assert_eq!(rate.per_second(Duration::from_millis(1)), Some(6.0));
    assert_eq!(rate.per_second(Duration::ZERO), None);
}
#[test]
fn fallbacks_and_idle() {
    let f = fx(Level::Off, false);
    assert_eq!(f.thinking(Duration::ZERO).text, "T");
    assert_eq!(f.streaming(Some(90.0)).text, "S");
    assert!(f.thinking(Duration::ZERO).next_wake.is_none());
    let f = fx(Level::Full, true);
    assert_eq!(f.thinking(Duration::ZERO).text, "...");
    assert_eq!(
        f.backoff(Duration::from_millis(100), Duration::from_secs(3))
            .text,
        "retry 3s"
    );
    assert_eq!(f.pips(0, 8, Duration::ZERO).text, "q8");
    let f = fx(Level::Full, false);
    assert_eq!(f.pips(0, 8, Duration::from_millis(120)).text, "▪▪▪▪▪+3");
    assert!(f.pips(2, 3, Duration::from_millis(120)).next_wake.is_none());
    assert!(
        f.backoff(Duration::from_secs(3), Duration::from_secs(3))
            .next_wake
            .is_none()
    );
    assert_eq!(f.streaming(None).text, "?");
    assert_eq!(f.streaming(Some(f64::NAN)).text, "?");
    let f = StatusStyle { ansi16: true, ..f };
    assert_eq!(f.streaming(Some(30.0)).text, ">>");
}
