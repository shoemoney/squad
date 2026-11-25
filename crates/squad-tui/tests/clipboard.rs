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
