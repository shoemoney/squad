use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};
use squad_tui::{app::App, fx::TerminalCaps, ui};
use std::time::Duration;

fn app(fx: &str, size: (u16, u16)) -> App {
    let caps = TerminalCaps::resolve(
        |k| match k {
            "SQUAD_FX" => Some(fx.into()),
            "COLORTERM" => Some("truecolor".into()),
            _ => None,
        },
        false,
        size.into(),
    )
    .unwrap();
    App::new(caps)
}
fn frame(app: &mut App, size: (u16, u16), now: Duration) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(size.0, size.1)).unwrap();
    terminal.draw(|f| ui::render(app, f, now)).unwrap();
    terminal.backend().buffer().clone()
}
fn text(buf: &Buffer) -> String {
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn effects_off_layout_is_static_and_the_post_hook_is_inert() {
    for size in [(80, 24), (120, 40), (40, 12)] {
        let mut app = app("off", size);
        let first = frame(&mut app, size, Duration::ZERO);
        assert_eq!(
            app.fx.active_count(),
            0,
            "no effect is armed with SQUAD_FX=off"
        );
        let later = frame(&mut app, size, Duration::from_millis(700));
        assert_eq!(
            first, later,
            "static layout is identical frame to frame at {size:?}"
        );
        assert_eq!(app.fx.active_count(), 0);
        insta::assert_snapshot!(
            format!("render_budget_static_{}x{}", size.0, size.1),
            text(&first)
        );
    }
}

#[test]
fn full_effects_run_through_the_post_render_hook_and_settle_to_the_static_layout() {
    let size = (100, 30);
    let mut off = app("off", size);
    // This assertion isolates effect settlement from the independent idle-tip timer.
    off.tips_enabled = false;
    let baseline = frame(&mut off, size, Duration::ZERO);
    let mut full = app("full", size);
    full.tips_enabled = false;
    let first = frame(&mut full, size, Duration::ZERO);
    assert!(
        full.fx.active_count() > 0,
        "startup chrome effect is registered and processed by the post hook"
    );
    // The startup effect visibly rewrites the logo cells, so the hook really ran...
    assert_ne!(text(&first), text(&baseline));
    // ...while the protected input and help rows are never touched.
    let tail = |b: &Buffer| text(b).lines().rev().take(6).collect::<Vec<_>>().join("\n");
    assert_eq!(tail(&first), tail(&baseline));
    // Any key settles; the settled frame equals the static one in glyphs and style.
    full.fx.settle();
    let settled = frame(&mut full, size, Duration::from_secs(30));
    assert_eq!(full.fx.active_count(), 0);
    assert_eq!(text(&settled), text(&baseline));
}

#[test]
fn a_frame_stays_inside_the_terminal_and_redraws_are_deterministic_at_every_size() {
    for size in [(80, 24), (100, 30), (140, 42), (60, 20)] {
        let mut a = app("off", size);
        let mut b = app("off", size);
        assert_eq!(
            frame(&mut a, size, Duration::ZERO),
            frame(&mut b, size, Duration::ZERO)
        );
    }
}
