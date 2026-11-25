use super::{
    lexer::{TokenKind, tokenize},
    parser_tests::{context, event},
    *,
};

#[test]
fn boundaries_and_protected_text() {
    let cases = [
        (
            "@claude email person@example.com",
            "email person@example.com",
            vec!["claude"],
        ),
        (
            "@claude (@grok) [@gemini] {@muse}",
            "() [] {}",
            vec!["claude", "grok", "gemini", "muse"],
        ),
        (
            "@claude literal@unknown foo$OTHER",
            "literal@unknown foo$OTHER",
            vec!["claude"],
        ),
        (
            r"@claude \@grok \$ROUNDS \/help \\ \` \q",
            "@grok $ROUNDS /help \\ ` \\q",
            vec!["claude"],
        ),
        (
            "@claude `@grok $OTHER $debate \\@muse`",
            "`@grok $OTHER $debate \\@muse`",
            vec!["claude"],
        ),
        ("@claude @ $ $20", "@ $ $20", vec!["claude"]),
        (
            "@claude Start\n```rust\n@unknown $OTHER $debate\n--\n```\nEnd",
            "Start\n```rust\n@unknown $OTHER $debate\n--\n```\nEnd",
            vec!["claude"],
        ),
        (
            "@claude\n   ```lang\n@unknown\n \t``` \t\nEnd",
            "```lang\n@unknown\n \t``` \t\nEnd",
            vec!["claude"],
        ),
    ];
    let ctx = context();
    for (raw, expected, nicks) in cases {
        let ParsedInput::Message {
            body, recipients, ..
        } = parse(raw, &ctx).unwrap()
        else {
            panic!("{raw}")
        };
        assert_eq!(body, expected, "{raw}");
        assert_eq!(
            recipients.iter().map(|n| n.as_str()).collect::<Vec<_>>(),
            nicks,
            "{raw}"
        );
    }
    for (raw, expected) in [
        ("@grok `é", "Close the inline code with a backtick."),
        ("@grok `a\nb`", "Close the inline code with a backtick."),
        (
            "@grok\n```rust\n@unknown",
            "Close the code block with three backticks.",
        ),
        (
            "@grok $Rounds",
            "Invalid token: $Rounds. Escape the sigil to use it literally.",
        ),
        (
            "@grok $debate-now",
            "Invalid token: $debate-now. Escape the sigil to use it literally.",
        ),
    ] {
        assert_eq!(parse(raw, &ctx).unwrap_err().to_string(), expected);
    }
}
#[test]
fn contiguous_exclusions_and_assignment_positions() {
    let ctx = context();
    for (raw, body, exclusions) in [
        ("@all -grok -muse Hello", "Hello", 2),
        ("@all\n-grok\t-muse Hello -claude", "Hello -claude", 2),
        ("@all -grok `code` -muse", "`code` -muse", 1),
        ("@all -grok \\@muse -claude", "@muse -claude", 1),
        ("@all -grok $ROUNDS=3 -muse", "-muse", 1),
    ] {
        let tokens = tokenize(raw).unwrap();
        assert_eq!(
            tokens
                .iter()
                .filter(|t| matches!(t.kind, TokenKind::Exclusion(_)))
                .count(),
            exclusions
        );
        assert!(
            matches!(parse(raw,&ctx).unwrap(),ParsedInput::Message{body:b,..} if b==body),
            "{raw}"
        );
    }
    for raw in [
        "$ROUNDS=5 @grok Use $ROUNDS rounds",
        "@grok Use $ROUNDS $ROUNDS=5 rounds",
        "@grok Use $ROUNDS rounds $ROUNDS=5",
    ] {
        assert!(
            matches!(parse(raw,&ctx).unwrap(),ParsedInput::Message{body,..} if body=="Use 5 rounds")
        );
    }
}
#[test]
fn unicode_spans_and_removal_whitespace() {
    let mut ctx = context();
    let raw = "@grok café $ROUNDS=0";
    let error = parse(raw, &ctx).unwrap_err();
    assert_eq!(&raw[error.span.start..error.span.end], "$ROUNDS=0");
    assert_eq!(error.span.start, 12);
    ctx.metadata.quote = vec![event(999)];
    let raw = "@grok 日本語";
    let error = parse(raw, &ctx).unwrap_err();
    assert_eq!(
        error.span,
        Span {
            start: 0,
            end: raw.len()
        }
    );
    ctx.metadata.quote.clear();
    for (raw, body) in [
        ("@grok A  B\nC", "A  B\nC"),
        ("@grok A \n $ROUNDS=3 \t B", "A B"),
        ("@grok A (@gemini) B", "A () B"),
        (
            "@grok A\n```\n$ROUNDS=0\n```\nB",
            "A\n```\n$ROUNDS=0\n```\nB",
        ),
    ] {
        assert!(
            matches!(parse(raw,&ctx).unwrap(),ParsedInput::Message{body:b,..} if b==body),
            "{raw}"
        );
    }
    let tokens = tokenize("é (@grok) 日本語 $ROUNDS=5").unwrap();
    for t in tokens {
        let raw = "é (@grok) 日本語 $ROUNDS=5";
        assert!(raw.is_char_boundary(t.span.start) && raw.is_char_boundary(t.span.end));
    }
    insta::assert_json_snapshot!("unicode_metadata_error", error);
}
