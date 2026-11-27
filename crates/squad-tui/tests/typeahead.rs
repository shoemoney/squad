use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, layout::Rect};
use squad_protocol::{
    Nick,
    parser::{ParseContext, SLASH_COMMANDS, SubmissionMeta},
};
use squad_tui::{
    app::App,
    fx::TerminalCaps,
    typeahead, ui,
    widgets::{completion::CompletionState, input::EditorAction},
};
use std::time::Duration;

fn context() -> ParseContext {
    ParseContext {
        roster: ["grok", "deepseek", "deborah"]
            .map(|s| Nick::new(s).unwrap())
            .to_vec(),
        ..Default::default()
    }
}
fn app() -> App {
    let mut app = App::new(TerminalCaps::resolve(|_| None, true, (80, 24).into()).unwrap());
    app.session.snapshot.context = context();
    app.session.snapshot.models = app
        .session
        .snapshot
        .context
        .roster
        .iter()
        .enumerate()
        .map(|(i, nick)| squad_protocol::Model {
            nick: squad_protocol::Actor::Member(nick.clone()),
            requested_slug: squad_protocol::Text::new("vendor/alias").unwrap(),
            resolved_slug: squad_protocol::Text::new(format!("vendor/{nick}-resolved")).unwrap(),
            roster_order: Some(i as u32),
            context_length: 128000,
            pricing: Default::default(),
            resolved_at: "2026-11-26T00:00:00Z".parse().unwrap(),
        })
        .collect();
    app
}
fn key(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.handle(Event::Key(KeyEvent::new(code, modifiers)), Duration::ZERO);
}
fn plain(app: &mut App, code: KeyCode) {
    key(app, code, KeyModifiers::NONE);
}
fn type_text(app: &mut App, text: &str) {
    app.handle(Event::Paste(text.into()), Duration::ZERO);
}
fn screen(app: &mut App, width: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, 24)).unwrap();
    terminal
        .draw(|f| ui::render(app, f, Duration::ZERO))
        .unwrap();
    lines(terminal.backend().buffer())
}
fn lines(buffer: &Buffer) -> String {
    (buffer.area.y..buffer.area.bottom())
        .map(|y| {
            (buffer.area.x..buffer.area.right())
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn ranking_is_prefix_then_substring_then_subsequence_and_stable() {
    assert_eq!(typeahead::rank("@grok", "@gr"), Some(0));
    assert_eq!(typeahead::rank("$debate", "deb"), Some(0));
    assert_eq!(typeahead::rank("$debate", "dbe"), Some(2));
    assert_eq!(typeahead::rank("@deepseek", "@seek"), Some(1));
    assert_eq!(typeahead::rank("@grok", "@gk"), Some(2));
    assert_eq!(typeahead::rank("$debate", "@deb"), None);
    assert_eq!(typeahead::rank("@grok", "zz"), None);
    let c = CompletionState::with_starters("deb", 3, &context(), &Default::default(), true);
    assert_eq!(c.items, ["@deborah", "$debate"]);
}

#[test]
fn automatic_open_and_each_bare_trigger_tab_and_reopen_list_everything() {
    for (trigger, expected) in [
        ("@", vec!["@grok", "@deepseek", "@deborah", "@all"]),
        ("$", typeahead::MODES.iter().map(|(n, _)| *n).collect()),
        ("/", SLASH_COMMANDS.iter().map(|(n, _)| *n).collect()),
    ] {
        let mut app = app();
        type_text(&mut app, trigger);
        assert!(app.completion_open);
        plain(&mut app, KeyCode::Tab);
        assert_eq!(app.input.text(), trigger);
        assert_eq!(app.completion.items, expected);
        plain(&mut app, KeyCode::Esc);
        assert!(!app.completion_open);
        assert!(app.pending.is_none());
        plain(&mut app, KeyCode::Tab);
        assert!(app.completion_open);
        assert_eq!(app.completion.items, expected);
        insta::assert_snapshot!(
            format!(
                "popup_{}",
                match trigger {
                    "@" => "models",
                    "$" => "modes",
                    _ => "commands",
                }
            ),
            screen(&mut app, 80)
        );
    }
}

#[test]
fn empty_partial_and_next_token_tab_menus() {
    for (text, expected) in [
        ("", "@all"),
        ("deb", "$debate"),
        ("gro", "@grok"),
        ("@grok ", "$debate"),
        ("$debate ", "@judge"),
    ] {
        let mut app = app();
        type_text(&mut app, text);
        plain(&mut app, KeyCode::Tab);
        assert!(app.completion_open, "{text}");
        assert!(
            app.completion.items.iter().any(|s| s == expected),
            "{text}: {:?}",
            app.completion.items
        );
        assert!(!app.input.text().contains('\t'));
        if text.is_empty() {
            assert_eq!(
                app.completion.items.len(),
                4 + typeahead::MODES.len() + SLASH_COMMANDS.len()
            );
            insta::assert_snapshot!("starter_menu", screen(&mut app, 80));
        }
    }
}

#[test]
fn acceptance_replaces_utf8_token_preserves_suffix_and_never_sends() {
    let mut app = app();
    type_text(&mut app, "é @gr suffix");
    for _ in 0..7 {
        app.input.apply(EditorAction::Left(false)).unwrap();
    }
    plain(&mut app, KeyCode::Tab);
    plain(&mut app, KeyCode::Enter);
    assert_eq!(app.input.text(), "é @grok  suffix");
    assert!(app.pending.is_none());
    let mut app = self::app();
    type_text(&mut app, "gro");
    plain(&mut app, KeyCode::Tab);
    plain(&mut app, KeyCode::Tab);
    assert_eq!(app.input.text(), "@grok ");
    assert!(!app.completion_open);
}

#[test]
fn popup_keys_preserve_editor_and_global_bindings() {
    let mut app = app();
    type_text(&mut app, "@");
    plain(&mut app, KeyCode::Down);
    assert_eq!(app.completion.selected, 1);
    key(&mut app, KeyCode::BackTab, KeyModifiers::SHIFT);
    assert_eq!(app.completion.selected, 0);
    key(&mut app, KeyCode::Tab, KeyModifiers::SHIFT);
    assert_eq!(app.completion.selected, 3);
    app.handle(
        Event::Key(KeyEvent::new_with_kind(
            KeyCode::Down,
            KeyModifiers::NONE,
            KeyEventKind::Repeat,
        )),
        Duration::ZERO,
    );
    assert_eq!(app.completion.selected, 0);
    plain(&mut app, KeyCode::Up);
    assert_eq!(app.completion.selected, 3);
    key(&mut app, KeyCode::Char('f'), KeyModifiers::CONTROL);
    assert!(app.search_open);
    plain(&mut app, KeyCode::Esc);
    key(&mut app, KeyCode::Char('y'), KeyModifiers::CONTROL);
    assert_eq!(app.input.text(), "@");
    key(&mut app, KeyCode::Enter, KeyModifiers::ALT);
    assert_eq!(app.input.text(), "@\n");
    type_text(&mut app, "@");
    key(&mut app, KeyCode::Char('j'), KeyModifiers::CONTROL);
    assert!(app.input.text().ends_with("@\n"));
    app.handle(
        Event::Key(KeyEvent::new_with_kind(
            KeyCode::Enter,
            KeyModifiers::ALT,
            KeyEventKind::Repeat,
        )),
        Duration::ZERO,
    );
    assert!(app.input.text().ends_with("@\n\n"));
    app.input_events.kitty = true;
    type_text(&mut app, "@");
    key(&mut app, KeyCode::Enter, KeyModifiers::SHIFT);
    assert!(app.input.text().ends_with("@\n"));
    for c in ['r', 't', 'x'] {
        plain(&mut app, KeyCode::Char(c));
    }
    assert!(app.input.text().ends_with("rtx"));
    assert!(app.pending.is_none());
    app.handle(
        Event::Key(KeyEvent::new_with_kind(
            KeyCode::Enter,
            KeyModifiers::NONE,
            KeyEventKind::Repeat,
        )),
        Duration::ZERO,
    );
    assert!(app.pending.is_none());
    plain(&mut app, KeyCode::F(6));
    assert!(!app.focus_input);
}

#[test]
fn history_and_selection_navigation_bypass_popup() {
    let mut app = app();
    app.input.apply(EditorAction::Insert("old".into())).unwrap();
    app.input.accept_submission();
    type_text(&mut app, "@");
    key(&mut app, KeyCode::Up, KeyModifiers::ALT);
    assert_eq!(app.input.text(), "old");
    key(&mut app, KeyCode::Down, KeyModifiers::ALT);
    assert_eq!(app.input.text(), "@");
    key(&mut app, KeyCode::Left, KeyModifiers::SHIFT);
    assert!(app.input.selection().is_some());
}

#[test]
fn tips_parse_rotate_hide_and_respect_narrow_and_no_colour() {
    for ctx in [ParseContext::default(), context()] {
        for text in typeahead::examples(&ctx) {
            squad_protocol::parse(&text, &SubmissionMeta::default(), &ctx).unwrap();
        }
    }
    let examples = typeahead::examples(&context());
    assert_eq!(
        examples.len(),
        6 + usize::from(squad_protocol::forge::ENABLED),
        "every authored example must parse with this roster"
    );
    assert!(
        examples
            .iter()
            .any(|s| s.contains("@grok @deepseek $debate"))
    );
    assert_ne!(typeahead::tip(&context(), 0), typeahead::tip(&context(), 8));
    for width in [80, 50] {
        let mut buf = Buffer::empty(Rect::new(0, 0, width, 1));
        let theme = squad_tui::theme::Theme::new(squad_tui::fx::ColorTier::NoColor);
        typeahead::render_tip(
            &typeahead::tip(&context(), 0),
            buf.area,
            &mut buf,
            theme.quiet_chrome(),
        );
        insta::assert_snapshot!(format!("tip_{width}"), lines(&buf));
    }
    let mut app = app();
    assert!(screen(&mut app, 80).contains("TIP:"));
    app.expire(Duration::from_secs(8));
    assert_eq!(app.ui_deadline(), Some(Duration::from_secs(16)));
    type_text(&mut app, "hello");
    assert!(!screen(&mut app, 80).contains("TIP:"));
    app.input = Default::default();
    plain(&mut app, KeyCode::Tab);
    assert!(!screen(&mut app, 80).contains("TIP:"));
    plain(&mut app, KeyCode::Esc);
    app.tips_enabled = false;
    assert!(!screen(&mut app, 80).contains("TIP:"));
}

#[test]
fn descriptions_use_resolved_models_and_parser_command_hints() {
    let app = app();
    assert_eq!(
        typeahead::describe("@grok", &app.session.snapshot.models),
        "vendor/grok-resolved"
    );
    assert_eq!(typeahead::describe("/stop", &[]), "[@nick]");
    for (name, hint) in SLASH_COMMANDS {
        assert_eq!(typeahead::describe(name, &[]), *hint);
    }
    for (name, _) in typeahead::MODES {
        let raw = match *name {
            "$debate" => "@grok @deepseek $debate topic @judge".to_string(),
            "$crosscheck" => "@all $crosscheck plan\n--\ngoal".to_string(),
            _ => format!("@all {name} topic"),
        };
        let mut ctx = context();
        ctx.roster.push(Nick::new("claude").unwrap());
        squad_protocol::parse(&raw, &SubmissionMeta::default(), &ctx).unwrap();
    }
}

#[test]
fn repeated_tab_never_inserts_a_tab_and_protected_text_stays_literal() {
    let mut app = app();
    app.handle(
        Event::Key(KeyEvent::new_with_kind(
            KeyCode::Tab,
            KeyModifiers::NONE,
            KeyEventKind::Repeat,
        )),
        Duration::ZERO,
    );
    assert_eq!(app.input.text(), "");
    for raw in ["`gro", "```\ngro", "\\@gro"] {
        let c =
            CompletionState::with_starters(raw, raw.len(), &context(), &Default::default(), true);
        assert!(c.items.is_empty(), "{raw}");
    }
}

#[test]
fn starter_menu_filters_while_typing_and_next_tokens_avoid_duplicate_modes() {
    let mut app = app();
    plain(&mut app, KeyCode::Tab);
    type_text(&mut app, "gro");
    assert!(app.completion_open);
    assert_eq!(app.completion.items, ["@grok"]);
    plain(&mut app, KeyCode::Enter);
    assert_eq!(app.input.text(), "@grok ");
    plain(&mut app, KeyCode::Tab);
    assert!(!app.completion.items.contains(&"@grok".into()));
    assert!(!app.completion.items.contains(&"@all".into()));
    let c = CompletionState::with_starters("$survey ", 8, &context(), &Default::default(), true);
    assert!(c.items.iter().all(|s| s.starts_with('@')));
    assert!(!c.items.contains(&"@judge".into()));
}

#[test]
fn slash_registry_entries_are_recognized_by_the_real_parser() {
    for (name, _) in SLASH_COMMANDS {
        let raw = match *name {
            "/set" => "/set ROUNDS 3".to_string(),
            "/export" => "/export room".to_string(),
            "/accept" => {
                "/accept 550e8400-e29b-41d4-a716-446655440000 -- accepted risk".to_string()
            }
            _ => name.to_string(),
        };
        if let Err(error) = squad_protocol::parse(&raw, &SubmissionMeta::default(), &context()) {
            assert!(
                !error.message.starts_with("Unknown slash command:"),
                "{raw}: {error:?}"
            );
            assert!(!error.message.starts_with("Usage:"), "{raw}: {error:?}");
        }
    }
}

#[test]
fn judge_is_offered_for_debate_or_standalone_quoted_requests() {
    let meta = SubmissionMeta {
        quote: vec![squad_protocol::EventId::new()],
        reply_to: vec![],
    };
    for (raw, meta, expected) in [
        ("@", SubmissionMeta::default(), false),
        ("@", meta.clone(), true),
        ("@grok @", meta, false),
        ("@grok @deepseek $debate @", SubmissionMeta::default(), true),
        ("@grok $grill @", SubmissionMeta::default(), false),
        ("@all $survey @", SubmissionMeta::default(), false),
        (
            "@grok @deepseek $debate @judge @",
            SubmissionMeta::default(),
            false,
        ),
    ] {
        let completion = CompletionState::at(raw, raw.len(), &context(), &meta);
        assert_eq!(
            completion.items.contains(&"@judge".into()),
            expected,
            "{raw}"
        );
    }
}

#[test]
fn all_and_follow_on_menus_respect_recipient_and_slash_syntax() {
    for raw in ["@grok @", "/stop @"] {
        let c = CompletionState::at(raw, raw.len(), &context(), &SubmissionMeta::default());
        assert!(!c.items.contains(&"@all".into()));
        assert!(!c.items.contains(&"@judge".into()));
        assert!(c.items.contains(&"@deepseek".into()));
    }
    let raw = "@all ";
    let c = CompletionState::with_starters(raw, raw.len(), &context(), &Default::default(), true);
    assert!(c.items.iter().all(|s| s.starts_with('$')));
}
