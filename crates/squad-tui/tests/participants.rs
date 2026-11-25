use ratatui::{buffer::Buffer, layout::Rect};
use squad_protocol::PresenceStatus;
use squad_tui::{
    fx::{ColorTier, Palette},
    widgets::participants::{ParticipantView, ParticipantsView, render},
};
fn roster() -> Vec<ParticipantView> {
    (0..12)
        .map(|i| ParticipantView {
            nick: format!("persona_{i}_with_a_very_long_name"),
            version: "4.7".into(),
            model: "provider/full-concrete-model-version".into(),
            slot: i,
            presence: PresenceStatus::Streaming,
            queue_depth: 2,
            contest: Some("debate round 3".into()),
            tokens: Some(12345),
            cost_usd: None,
            tokens_per_second: Some(24.5),
            wins: 7,
            losses: 2,
            trophies: 1,
            eliminated: i == 2,
        })
        .collect()
}
fn screen(w: u16, h: u16, scroll: usize) -> String {
    let members = roster();
    let palette = Palette::new(ColorTier::NoColor);
    let area = Rect::new(0, 0, w, h);
    let mut buf = Buffer::empty(area);
    let rects = render(
        &ParticipantsView {
            members: &members,
            palette: &palette,
            scroll,
        },
        area,
        &mut buf,
    );
    assert!(rects.total_rows > usize::from(h));
    assert_eq!(members[0].detail(), "provider/full-concrete-model-version");
    (0..h)
        .map(|y| {
            (0..w)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
#[test]
fn participants_snapshots() {
    insta::assert_snapshot!("participants_80x24", screen(80, 24, 0));
    insta::assert_snapshot!("participants_120x40", screen(120, 40, 0));
    insta::assert_snapshot!("participants_narrow", screen(24, 24, 0));
    insta::assert_snapshot!("participants_scrolled", screen(80, 24, 48));
}

#[test]
fn every_persona_can_be_focused_and_record_rects_are_animatable() {
    use squad_tui::widgets::participants::ParticipantsState;
    let members = roster();
    let palette = Palette::new(ColorTier::Ansi16);
    let mut state = ParticipantsState::default();
    for i in 1..12 {
        state.navigate(1, &members, 24, 12, &palette);
        assert_eq!(state.focused, i);
    }
    assert!(state.scroll > 0);
    let mut buf = Buffer::empty(Rect::new(0, 0, 24, 12));
    let geometry = render(
        &ParticipantsView {
            members: &members,
            palette: &palette,
            scroll: state.scroll,
        },
        buf.area,
        &mut buf,
    );
    assert!(!geometry.animatable.is_empty());
    assert!(geometry.animatable.iter().all(|a| {
        geometry
            .protected
            .iter()
            .all(|p| a.intersection(*p).is_empty())
    }));
    assert_ne!(
        palette.member(&members[0].nick, 0),
        palette.member(&members[1].nick, 1)
    );
}

#[test]
fn protocol_models_keep_personas_distinct_exclude_judge_and_presence_is_idempotent() {
    use squad_protocol::{Actor, Model, Presence, Text};
    let model = Model {
        nick: Actor::Judge,
        requested_slug: Text::new("provider/model").unwrap(),
        resolved_slug: Text::new("provider/model-v4.7").unwrap(),
        roster_order: None,
        context_length: 1000,
        pricing: Default::default(),
        resolved_at: "2026-11-24T00:00:00Z".parse().unwrap(),
    };
    assert!(ParticipantView::from_model(&model, "4.7").is_none());
    let mut first = model.clone();
    first.nick = Actor::Member("persona_a".parse().unwrap());
    first.roster_order = Some(0);
    let mut second = first.clone();
    second.nick = Actor::Member("persona_b".parse().unwrap());
    second.roster_order = Some(1);
    let mut a = ParticipantView::from_model(&first, "4.7").unwrap();
    let b = ParticipantView::from_model(&second, "4.7").unwrap();
    assert_eq!(a.model, b.model);
    assert_ne!(a.nick, b.nick);
    let presence = Presence {
        nick: "persona_a".parse().unwrap(),
        status: PresenceStatus::Streaming,
        work_id: None,
        queue_depth: 4,
        error_code: None,
        observed_at: model.resolved_at,
    };
    a.apply_presence(&presence);
    a.apply_presence(&presence);
    assert_eq!(a.queue_depth, 4);
    assert_eq!(a.wins, 0);
    assert_eq!(a.trophies, 0);
}

#[test]
fn compact_metrics_do_not_wrap_and_old_rates_are_dim() {
    use ratatui::style::Modifier;
    let mut members = roster();
    members[0].nick = "claude".into();
    members[0].version.clear();
    members[0].queue_depth = 0;
    members[0].presence = PresenceStatus::Idle;
    let palette = Palette::new(ColorTier::TrueColor);
    let mut buf = Buffer::empty(Rect::new(0, 0, 16, 17));
    render(
        &ParticipantsView {
            members: &members,
            palette: &palette,
            scroll: 0,
        },
        buf.area,
        &mut buf,
    );
    let row = (0..16).map(|x| buf[(x, 1)].symbol()).collect::<String>();
    assert!(row.contains("~24.5 tok/s"));
    assert!(buf[(0, 1)].modifier.contains(Modifier::DIM));
    assert_eq!(buf[(0, 16)].symbol(), "+");
}

#[test]
fn compact_roster_preserves_long_identity_across_heading_rows() {
    let mut members = roster();
    members.truncate(1);
    let palette = Palette::new(ColorTier::NoColor);
    let mut buf = Buffer::empty(Rect::new(0, 0, 16, 24));
    render(
        &ParticipantsView {
            members: &members,
            palette: &palette,
            scroll: 0,
        },
        buf.area,
        &mut buf,
    );
    let text = (0..24)
        .flat_map(|y| (0..16).map(move |x| (x, y)))
        .map(|p| buf[p].symbol())
        .collect::<String>()
        .replace(' ', "");
    assert!(text.contains(&members[0].nick));
    assert!(text.contains("q2"));
}
