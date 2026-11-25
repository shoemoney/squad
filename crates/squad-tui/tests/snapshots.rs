use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};
use squad_protocol::*;
use squad_tui::{app::App, fx::TerminalCaps, ui, widgets::input::EditorAction};
use std::time::Duration;

fn app(size: (u16, u16), no_color: bool) -> App {
    let caps = TerminalCaps::resolve(
        |k| match k {
            "SQUAD_FX" => Some("off".into()),
            "NO_COLOR" if no_color => Some("1".into()),
            _ => None,
        },
        false,
        size.into(),
    )
    .unwrap();
    let mut app = App::new(caps);
    app.session.snapshot.context.roster = ["claude", "gemini", "grok", "openai"]
        .map(|n| Nick::new(n).unwrap())
        .to_vec();
    app
}
fn draw(app: &mut App, size: (u16, u16)) -> String {
    let mut terminal = Terminal::new(TestBackend::new(size.0, size.1)).unwrap();
    terminal
        .draw(|f| ui::render(app, f, Duration::ZERO))
        .unwrap();
    let b = terminal.backend().buffer();
    (0..size.1)
        .map(|y| {
            (0..size.0)
                .map(|x| b[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn press(app: &mut App, code: KeyCode) {
    app.handle(
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE)),
        Duration::ZERO,
    );
}

#[test]
fn completion_popup_snapshots_for_members_and_variables_at_both_viewports() {
    for size in [(80, 24), (120, 40)] {
        for (name, typed) in [("members", "@"), ("filtered", "@g"), ("variables", "$")] {
            let mut app = app(size, false);
            app.input.apply(EditorAction::Insert(typed.into())).unwrap();
            press(&mut app, KeyCode::Tab);
            assert!(app.completion_open, "{name} opens at {size:?}");
            let screen = draw(&mut app, size);
            for item in app.completion.items.iter().take(6) {
                assert!(screen.contains(item.as_str()), "{item} visible\n{screen}");
            }
            insta::assert_snapshot!(format!("completion_{name}_{}x{}", size.0, size.1), screen);
        }
    }
}

#[test]
fn completion_selection_moves_accepts_and_closes_without_leaking_into_the_snapshot() {
    let size = (80, 24);
    let mut app = app(size, true);
    app.input.apply(EditorAction::Insert("@".into())).unwrap();
    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Down);
    assert_eq!(app.completion.selected, 1);
    let moved = draw(&mut app, size);
    insta::assert_snapshot!("completion_second_selected_no_color_80x24", moved);
    press(&mut app, KeyCode::Enter);
    assert!(!app.completion_open);
    assert!(app.input.text().starts_with('@'));
    assert!(app.input.text().ends_with(' '));
    assert!(!draw(&mut app, size).contains("@gemini\n@grok"));
}
