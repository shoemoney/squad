use ratatui::{buffer::Buffer, layout::Rect};
use squad_protocol::MessageId;
use squad_tui::{
    fx::{ColorTier, Palette},
    widgets::transcript::{TranscriptMessage, TranscriptView, render},
};
use std::time::{Duration, Instant};
fn id(n: usize) -> MessageId {
    format!("01900000-0000-7000-8000-{n:012x}").parse().unwrap()
}
fn fixture(width: u16) -> TranscriptView {
    let mut view = TranscriptView::new(width);
    let mut message = TranscriptMessage::new(
        id(1),
        "claude",
        "A **clear answer** with 中文 and emoji 👩‍💻.\n\n```rust\nlet answer = 42;\n```\n\nA streaming tail",
    );
    message.version = "4.7".into();
    message.quotes = vec!["me: How does this work?".into()];
    message.status = "streaming".into();
    message.complete = false;
    view.upsert(message);
    view.upsert(TranscriptMessage::new(
        id(2),
        "gemini",
        "The second reply stays in participant order.",
    ));
    view
}
fn screen(width: u16, height: u16) -> String {
    let view = fixture(width);
    let area = Rect::new(0, 0, width, height);
    let mut buf = Buffer::empty(area);
    render(&view, 0, area, &mut buf, &Palette::new(ColorTier::NoColor));
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
#[test]
fn transcript_snapshots() {
    insta::assert_snapshot!("transcript_80x24", screen(80, 24));
    insta::assert_snapshot!("transcript_120x40", screen(120, 40));
    insta::assert_snapshot!("transcript_narrow", screen(28, 16));
}
#[test]
fn ten_thousand_messages_draw_only_visible_rows_and_keep_anchor() {
    let mut view = TranscriptView::new(80);
    for n in 1..=10_000 {
        view.upsert(TranscriptMessage::new(
            id(n),
            "claude",
            format!("Message {n}"),
        ));
    }
    assert_eq!(view.len(), 10_000);
    assert_eq!(view.total_rows(), 30_000);
    let top = 15_001;
    let anchor = view.anchor_at(top).unwrap();
    view.upsert(TranscriptMessage::new(
        id(10_000),
        "gemini",
        "stream grows\n\nbelow viewport\n\nmore text",
    ));
    assert_eq!(view.row_for_anchor(anchor), Some(top));
    let area = Rect::new(0, 0, 80, 24);
    let mut buf = Buffer::empty(area);
    let palette = Palette::new(ColorTier::NoColor);
    let start = Instant::now();
    for _ in 0..100 {
        let geometry = render(&view, top, area, &mut buf, &palette);
        assert_eq!(geometry.rows_drawn, 24);
        assert!(geometry.messages_visited <= 9);
        assert!(geometry.protected.iter().all(|body| {
            geometry
                .rails
                .iter()
                .all(|rail| body.intersection(*rail).is_empty())
        }));
    }
    let mean = start.elapsed() / 100;
    eprintln!("10,000-message draw mean: {mean:?}");
    assert!(
        mean < if cfg!(debug_assertions) {
            Duration::from_millis(50)
        } else {
            Duration::from_millis(6)
        }
    );
}
#[test]
fn prepend_reflow_attempt_replacement_and_presentation_heights() {
    let mut view = fixture(40);
    let anchor = view.anchor_at(0).unwrap();
    let old = view.total_rows();
    let mut message = view.message(id(1)).unwrap().clone();
    message.before_body.push("ROUND 1".into());
    message.presentation.banner_mode = 1;
    message.after_body.push("score 7 to 8".into());
    message.presentation.scorecard_expanded = true;
    view.upsert(message.clone());
    assert!(view.total_rows() > old);
    view.prepend([TranscriptMessage::new(id(3), "me", "Older page")]);
    assert_eq!(view.row_for_anchor(anchor), Some(3));
    view.resize(80);
    assert_eq!(view.row_for_anchor(anchor), Some(3));
    message.attempt += 1;
    message.source = "replacement".into();
    view.upsert(message);
    assert_eq!(view.len(), 3);
    assert_eq!(view.message(id(1)).unwrap().source, "replacement");
}

#[test]
fn cache_noop_and_source_anchor_survive_reflow_and_offscreen_preceding_growth() {
    let mut view = TranscriptView::new(40);
    view.upsert(TranscriptMessage::new(id(1), "claude", "Short prefix"));
    view.upsert(TranscriptMessage::new(
        id(2),
        "me",
        "abcdefghij ".repeat(30),
    ));
    let anchor = view.anchor_at(8).unwrap();
    let stats = view.cache_stats(id(2)).unwrap();
    view.upsert(view.message(id(2)).unwrap().clone());
    assert_eq!(view.cache_stats(id(2)).unwrap(), stats);
    view.upsert(TranscriptMessage::new(
        id(1),
        "claude",
        "growing earlier\n\nparagraph\n\nmore",
    ));
    let resolved = view.row_for_anchor(anchor).unwrap();
    assert_eq!(view.anchor_at(resolved).unwrap(), anchor);
    view.resize(80);
    let after = view
        .anchor_at(view.row_for_anchor(anchor).unwrap())
        .unwrap();
    assert_eq!(after.id, anchor.id);
    assert!(after.source_grapheme <= anchor.source_grapheme);
}

#[test]
fn human_display_name_preserves_neutral_colour_and_resolved_quotes() {
    use squad_protocol::{Text, UserMessage, UserMessageInput};
    let user = UserMessage {
        input: UserMessageInput {
            message_id: id(44),
            text: Text::new("question").unwrap(),
            quote: None,
            reply_to: None,
        },
        body: "question".into(),
        resolved_quotes: vec![],
        debate_id: None,
        case_id: None,
        contest_id: None,
    };
    let message = TranscriptMessage::from_user(&user, "Jeremy");
    assert!(message.human);
    assert_eq!(message.author, "Jeremy");
    let mut view = TranscriptView::new(80);
    view.upsert(message);
    let mut buf = Buffer::empty(Rect::new(0, 0, 80, 4));
    render(
        &view,
        0,
        buf.area,
        &mut buf,
        &Palette::new(ColorTier::TrueColor),
    );
    assert_eq!(buf[(1, 0)].fg, ratatui::style::Color::Reset);
}
