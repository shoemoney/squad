use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend, layout::Rect};
use squad_tui::{
    input_events::{InputEvents, KeyboardSession},
    text::{boundaries, cursor_position, wrap},
    widgets::input::{EditorAction as A, InputState, InputWidget},
};

fn insert(state: &mut InputState, text: &str) {
    state.apply(A::Insert(text.into())).unwrap();
}

#[test]
fn unicode_layout_maps_original_bytes_and_cells() {
    let source = "e\u{301}界👩‍💻\tα\r\nZ";
    let rows = wrap(source, 20);
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows[0].cells.iter().map(|c| c.width).collect::<Vec<_>>(),
        [1, 2, 2, 3, 1]
    );
    assert_eq!(rows[1].source_start, source.len() - 1);
    for row in &rows {
        for cell in &row.cells {
            assert!(source.get(cell.source.clone()).is_some());
        }
    }
    assert_eq!(rows[0].cells[2].display, "👩‍💻");
    assert_eq!(
        wrap("α·", 20)[0]
            .cells
            .iter()
            .map(|c| c.width)
            .sum::<usize>(),
        2
    );
    assert_eq!(wrap("a\tb", 4)[1].cells[0].display, "b");
}

#[test]
fn cluster_deletion_selection_and_joining_are_atomic() {
    for cluster in ["e\u{301}", "👨‍👩‍👧‍👦", "🇺🇸", "界"] {
        let mut state = InputState::new();
        insert(&mut state, &format!("a{cluster}b"));
        state.apply(A::Left(false)).unwrap();
        state.apply(A::Left(true)).unwrap();
        assert_eq!(&state.text()[state.selection().unwrap()], cluster);
        state.apply(A::Backspace).unwrap();
        assert_eq!(state.text(), "ab");
        state.apply(A::Delete).unwrap();
        assert_eq!(state.text(), "a");
    }
    let mut state = InputState::new();
    insert(&mut state, "a");
    insert(&mut state, "\u{301}");
    state.apply(A::Backspace).unwrap();
    assert_eq!(state.text(), "");
}

#[test]
fn paste_preserves_code_and_never_submits() {
    let mut state = InputState::new();
    let action = InputEvents::default()
        .translate(Event::Paste(
            "fn main() {\r\n\tprintln!(\"x\");  \r\n}\u{1b}[13u".into(),
        ))
        .unwrap();
    assert_eq!(state.apply(action).unwrap(), None);
    assert_eq!(
        state.text(),
        "fn main() {\n\tprintln!(\"x\");  \n}\u{1b}[13u"
    );
    assert_eq!(
        state.apply(A::Submit).unwrap().as_deref(),
        Some(state.text())
    );
}

#[test]
fn byte_limit_rejects_atomically_and_allows_selection_replacement() {
    let mut state = InputState::new();
    insert(&mut state, &"é".repeat(16_384));
    let before = state.text().to_owned();
    let error = state.apply(A::Insert("界".into())).unwrap_err();
    assert_eq!(error.bytes, 32_771);
    assert_eq!(state.text(), before);
    state.apply(A::SelectAll).unwrap();
    insert(&mut state, "new");
    assert_eq!(state.text(), "new");
}

#[test]
fn history_restores_draft_cursor_and_does_not_accept_recalled_edits() {
    let mut state = InputState::new();
    insert(&mut state, "first");
    state.accept_submission();
    insert(&mut state, "second");
    state.accept_submission();
    insert(&mut state, "draft👩‍💻");
    state.apply(A::Left(false)).unwrap();
    let cursor = state.cursor();
    state.apply(A::HistoryPrevious).unwrap();
    assert_eq!(state.text(), "second");
    insert(&mut state, " edit");
    state.apply(A::HistoryPrevious).unwrap();
    assert_eq!(state.text(), "first");
    state.apply(A::HistoryNext).unwrap();
    assert_eq!(state.text(), "second");
    state.apply(A::HistoryNext).unwrap();
    assert_eq!(state.text(), "draft👩‍💻");
    assert_eq!(state.cursor(), cursor);
}

#[test]
fn wrapping_resize_and_visual_navigation_keep_grapheme_boundaries() {
    let mut state = InputState::new();
    insert(&mut state, "界界界界\ne\u{301}👩‍💻tail");
    let source = state.text().to_owned();
    for width in [6, 20, 4, 80] {
        let area = Rect::new(0, 0, width, 4);
        state.cursor_in(area);
        state.apply(A::Up(false)).unwrap();
        assert!(boundaries(state.text()).contains(&state.cursor()));
        state.apply(A::Down(true)).unwrap();
        assert!(boundaries(state.text()).contains(&state.cursor()));
        assert_eq!(state.text(), source);
    }
    assert_eq!(state.desired_height(80), 4);
    assert_eq!(wrap("abcd", 4).len(), 2);
    assert_eq!(cursor_position(&wrap("abcd", 4), 4), (1, 0));
}

#[test]
fn widget_follows_cursor_and_renders_overflow_and_selection() {
    let mut state = InputState::new();
    insert(
        &mut state,
        &(0..16).map(|i| format!("row {i}\n")).collect::<String>(),
    );
    assert_eq!(state.desired_height(20), 8);
    let mut terminal = Terminal::new(TestBackend::new(20, 4)).unwrap();
    terminal
        .draw(|f| f.render_stateful_widget(InputWidget, f.area(), &mut state))
        .unwrap();
    let output: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|c| c.symbol())
        .collect();
    assert!(output.contains("more above"));
    assert!(output.contains("row 15"));
    insta::assert_debug_snapshot!("input_overflow", terminal.backend().buffer());
    state.apply(A::SelectAll).unwrap();
    state.apply(A::Left(false)).unwrap();
    terminal
        .draw(|f| f.render_stateful_widget(InputWidget, f.area(), &mut state))
        .unwrap();
    let output: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|c| c.symbol())
        .collect();
    assert!(output.contains("more below"));
    assert!(output.contains("> row 0"));
    for width in 0..4 {
        assert!(state.cursor_in(Rect::new(0, 0, width, 0)).is_none());
    }
}

#[test]
fn keyboard_fallbacks_and_event_kinds_do_not_duplicate_submissions() {
    let legacy = InputEvents::default();
    let kitty = InputEvents { kitty: true };
    for (code, modifiers) in [
        (KeyCode::Char('j'), KeyModifiers::CONTROL),
        (KeyCode::Enter, KeyModifiers::ALT),
    ] {
        assert_eq!(
            legacy.translate(Event::Key(KeyEvent::new(code, modifiers))),
            Some(A::Insert("\n".into()))
        );
    }
    let shifted = Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT));
    assert_eq!(
        kitty.translate(shifted.clone()),
        Some(A::Insert("\n".into()))
    );
    assert_eq!(legacy.translate(shifted), Some(A::Submit));
    for kind in [
        KeyEventKind::Press,
        KeyEventKind::Repeat,
        KeyEventKind::Release,
    ] {
        let event = Event::Key(KeyEvent::new_with_kind(
            KeyCode::Enter,
            KeyModifiers::NONE,
            kind,
        ));
        assert_eq!(
            kitty.translate(event),
            if kind == KeyEventKind::Press {
                Some(A::Submit)
            } else {
                None
            }
        );
    }
    assert_eq!(
        kitty.translate(Event::Key(KeyEvent::new_with_kind(
            KeyCode::Char('x'),
            KeyModifiers::NONE,
            KeyEventKind::Repeat
        ))),
        Some(A::Insert("x".into()))
    );
}

#[test]
fn keyboard_session_restores_stack_and_paste_once() {
    for kitty in [false, true] {
        let mut bytes = vec![];
        {
            let mut session = KeyboardSession::with_capability(&mut bytes, kitty).unwrap();
            session.restore().unwrap();
            session.restore().unwrap();
        }
        let output = String::from_utf8(bytes).unwrap();
        assert_eq!(output.matches("\u{1b}[?2004h").count(), 1);
        assert_eq!(output.matches("\u{1b}[?2004l").count(), 1);
        assert_eq!(output.contains("\u{1b}[<1u"), kitty);
    }
}

#[test]
fn selected_graphemes_render_with_cell_styles() {
    let mut state = InputState::new();
    insert(&mut state, "e\u{301}界👩‍💻");
    state.apply(A::Left(true)).unwrap();
    let mut terminal = Terminal::new(TestBackend::new(20, 4)).unwrap();
    terminal
        .draw(|f| f.render_stateful_widget(InputWidget, f.area(), &mut state))
        .unwrap();
    let buffer = terminal.backend().buffer();
    assert!(
        buffer[(5, 0)]
            .modifier
            .contains(ratatui::style::Modifier::REVERSED)
    );
    insta::assert_debug_snapshot!("input_selection", buffer);
}
