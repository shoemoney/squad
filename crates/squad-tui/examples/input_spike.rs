//! Input spike evidence (2026-11-24).
//!
//! Custom editor: the executable assertions below exercise soft wrapping without
//! source mutation, Unicode cluster movement, selection replacement, literal code
//! paste, and resize. tests/input.rs covers rendering and terminal event cases.
//! tui-textarea and ratatui-textarea: NOT EVALUATED. Neither candidate's source
//! or documentation is present in ~/.cargo/registry/src on this worker. No new
//! dependency was added, and Ratatui 0.30 compatibility is not asserted.
//! Decision: proceed with Squad's custom widget, as required by the PRD; the
//! candidate comparison is incomplete until compatible candidate source is available.
use squad_tui::{
    text::wrap,
    widgets::input::{EditorAction as A, InputState},
};
fn main() {
    let mut input = InputState::new();
    input
        .apply(A::Insert("👩‍💻e\u{301}\n\tlet x = 1;\r\n".into()))
        .unwrap();
    let source = input.text().to_owned();
    assert!(wrap(&source, 4).len() > wrap(&source, 40).len());
    input.cursor_in(ratatui::layout::Rect::new(0, 0, 6, 4));
    input.cursor_in(ratatui::layout::Rect::new(0, 0, 42, 8));
    assert_eq!(input.text(), source);
    input.apply(A::Home(false)).unwrap();
    input.apply(A::Left(true)).unwrap();
    assert_eq!(&input.text()[input.selection().unwrap()], "\n");
    input.apply(A::Insert("!".into())).unwrap();
    assert!(input.text().ends_with(";!"));
    let mut unicode = InputState::new();
    unicode.apply(A::Insert("👩‍💻e\u{301}".into())).unwrap();
    unicode.apply(A::Left(true)).unwrap();
    assert_eq!(&unicode.text()[unicode.selection().unwrap()], "e\u{301}");
    unicode.apply(A::Left(true)).unwrap();
    assert_eq!(&unicode.text()[unicode.selection().unwrap()], "👩‍💻e\u{301}");
    println!(
        "custom: wrap, grapheme cursor, selection, code paste, resize passed; candidates: not evaluated"
    );
}
