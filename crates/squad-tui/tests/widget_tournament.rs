use ratatui::{buffer::Buffer, layout::Rect, style::Color};
use squad_tui::{
    fx::{ColorTier, Palette},
    widgets::tournament::*,
};
fn fixture() -> TournamentView {
    let mut v = TournamentView::new(
        "Design a reliable release process.",
        vec![
            Entrant {
                nick: "grok".into(),
                eliminated: true,
                trophies: 2,
            },
            Entrant {
                nick: "gemini".into(),
                eliminated: false,
                trophies: 4,
            },
        ],
    );
    v.state = "COMPLETE".into();
    v.judge_model = "judge/model".into();
    v.survivors = vec!["gemini".into()];
    v.current_round = 2;
    v.rounds = vec![
        Round {
            number: 1,
            remaining: 3,
            expanded: false,
            ..Default::default()
        },
        Round {
            number: 2,
            remaining: 2,
            expanded: true,
            replay: Some("replacement work accepted".into()),
            entries: vec![
                Entry {
                    nick: "grok".into(),
                    status: "forfeit · retry 3/3".into(),
                    text: "Last accepted answer retained.".into(),
                },
                Entry {
                    nick: "gemini".into(),
                    status: "complete".into(),
                    text: "Stage, verify, then publish.".into(),
                },
            ],
            ranking: vec![
                Ranking {
                    nick: "gemini".into(),
                    score: 91,
                    critique: "Clear rollback and verification.".into(),
                    eliminated: false,
                },
                Ranking {
                    nick: "grok".into(),
                    score: 42,
                    critique: "Missing rollback evidence.".into(),
                    eliminated: true,
                },
            ],
        },
    ];
    v.champion = Some("gemini".into());
    v.placements = vec!["gemini".into(), "grok".into(), "claude".into()];
    v
}
fn screen(v: &TournamentView, w: u16, h: u16) -> (Buffer, TournamentRects) {
    let area = Rect::new(0, 0, w, h);
    let mut b = Buffer::empty(area);
    let r = render(v, area, &mut b, &Palette::new(ColorTier::NoColor));
    (b, r)
}
fn text(b: &Buffer) -> String {
    (0..b.area.height)
        .map(|y| {
            (0..b.area.width)
                .map(|x| b[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
#[test]
fn snapshots() {
    for (w, h) in [(80, 24), (120, 40), (40, 24)] {
        let (b, _) = screen(&fixture(), w, h);
        insta::assert_snapshot!(format!("tournament_{w}x{h}"), text(&b));
        assert!(
            b.content
                .iter()
                .all(|c| c.fg == Color::Reset && c.bg == Color::Reset)
        );
    }
}
#[test]
fn accepted_order_retained_markers_and_visible_anchors() {
    let v = fixture();
    let (b, r) = screen(&v, 120, 40);
    let t = text(&b);
    assert!(t.find("1. @gemini · score 91").unwrap() < t.find("2. @grok · score 42").unwrap());
    assert_eq!(
        r.rankings.iter().map(|r| r.1.as_str()).collect::<Vec<_>>(),
        ["gemini", "grok"]
    );
    assert_eq!(r.eliminations.len(), 1);
    assert!(!r.champion.is_empty());
    assert!(t.contains("trophies 2 · ELIMINATED"));
    let (_, replayed) = screen(&v, 120, 40);
    assert_eq!(r, replayed);
}
#[test]
fn collapse_failure_scroll_and_clipping() {
    let mut v = fixture();
    v.expanded = false;
    let (b, r) = screen(&v, 80, 24);
    assert!(!text(&b).contains("score 91"));
    assert!(r.rankings.is_empty());
    v.expanded = true;
    v.state = "FAILED".into();
    v.reason = Some("Judge replay exhausted".into());
    v.scroll = 10;
    let (b, r) = screen(&v, 80, 24);
    assert!(text(&b).contains("Judge replay exhausted"));
    assert!(r.banner.is_empty());
    let mut b = Buffer::empty(Rect::new(5, 7, 10, 4));
    let _ = render(
        &v,
        Rect::new(0, 0, 80, 24),
        &mut b,
        &Palette::new(ColorTier::TrueColor),
    );
    let _ = render(
        &v,
        Rect::new(0, 0, 0, 0),
        &mut b,
        &Palette::new(ColorTier::NoColor),
    );
}

#[test]
fn state_adapter_and_unicode_are_bounded() {
    use squad_protocol::{ContestId, ContestKind, GenericContestState, Text};
    let mut v = fixture();
    let mut s = GenericContestState {
        contest_id: ContestId::new(),
        kind: ContestKind::Tournament,
        state_version: 1,
        status: Text::new("ANSWERING").unwrap(),
        step_id: None,
        artifact_event_ids: vec![],
        terminal_reason: None,
    };
    assert!(v.apply_state(&s));
    s.kind = ContestKind::Grill;
    assert!(!v.apply_state(&s));
    v.prompt = "界e\u{301} 👩‍💻 \u{1b}[31m".into();
    let (b, _) = screen(&v, 8, 24);
    assert!(!text(&b).contains('\u{1b}'));
    assert!(text(&b).contains("e\u{301}"));
}
