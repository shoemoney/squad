mod fx_support;
use fx_support::*;
use proptest::prelude::*;
use ratatui::{buffer::Buffer, layout::Rect, style::Color};
use squad_tui::fx::*;
use squad_tui::fx::{
    cue::CUE_CAPACITY,
    manager::{EFFECT_CAPACITY, MAX_CUE_AGE},
};
use std::time::Duration;
use tachyonfx::fx;

#[test]
fn queue_drops_oldest_and_retains_order() {
    let mut queue = FxQueue::new();
    for n in 0..1000 {
        queue.push(FeedOrigin::Live, cue(n, 0));
    }
    assert_eq!(queue.len(), CUE_CAPACITY);
    assert_eq!(queue.dropped(), 744);
    for n in 744..1000 {
        assert_eq!(queue.pop_front().unwrap().event_id, id(n));
    }
    assert!(queue.is_empty());
}
#[test]
fn thousand_replayed_events_create_zero_effects() {
    let (_, mut manager) = manager(FxLevel::Full);
    for n in 0..1000 {
        manager.queue_mut().push(FeedOrigin::Replay, cue(n, 0));
    }
    manager.pre(&mut FxContext::default());
    assert_eq!(manager.active_count(), 0);
    assert_eq!(manager.next_wake(), None);
}
#[test]
fn stale_boundary_and_future_time_are_guarded() {
    let (clock, mut manager) = manager(FxLevel::Full);
    clock.set(MAX_CUE_AGE);
    manager.queue_mut().push(FeedOrigin::Live, cue(1, 0));
    manager.pre(&mut FxContext::default());
    assert_eq!(manager.active_count(), 1);
    clock.advance(Duration::from_nanos(1));
    manager.queue_mut().push(FeedOrigin::Live, cue(2, 0));
    manager.queue_mut().push(FeedOrigin::Live, cue(3, 3000));
    manager.pre(&mut FxContext::default());
    assert_eq!(manager.stale_dropped(), 2);
}
#[test]
fn duplicate_event_has_no_second_entrance() {
    let (clock, mut manager) = manager(FxLevel::Full);
    manager.queue_mut().push(FeedOrigin::Live, cue(1, 0));
    manager.pre(&mut FxContext::default());
    clock.advance(Duration::from_secs(1));
    manager.pre(&mut FxContext::default());
    manager.queue_mut().push(FeedOrigin::Live, cue(1, 0));
    manager.pre(&mut FxContext::default());
    assert_eq!(manager.active_count(), 0);
}
#[test]
fn keys_replace_without_a_jump_or_stack() {
    let (clock, mut manager) = manager(FxLevel::Full);
    let mut ctx = FxContext::default();
    manager.queue_mut().push(FeedOrigin::Live, cue(1, 0));
    manager.pre(&mut ctx);
    clock.advance(Duration::from_millis(50));
    manager.pre(&mut ctx);
    let before = ctx.view.progress(&FxKey::DailySpend);
    manager.queue_mut().push(FeedOrigin::Live, cue(2, 50));
    manager.pre(&mut ctx);
    assert_eq!(manager.active_count(), 1);
    assert_eq!(ctx.view.progress(&FxKey::DailySpend), before);
    assert!(before > 0. && before < 1.);
}
fn tween() -> Tween {
    Tween {
        start: Duration::ZERO,
        duration: Duration::from_secs(10),
        from: 0.,
        to: 1.,
        ease: Ease::CubicOut,
    }
}
#[test]
fn cap_evicts_oldest_nonhero_and_preserves_heroes() {
    let (_, mut manager) = manager(FxLevel::Full);
    manager.add_tween(FxKey::Custom("hero".into()), tween(), true);
    for n in 0..60 {
        manager.add_tween(FxKey::Custom(format!("ordinary-{n}")), tween(), false);
    }
    assert_eq!(manager.active_count(), EFFECT_CAPACITY);
    assert!(manager.contains(&FxKey::Custom("hero".into())));
    assert!(!manager.contains(&FxKey::Custom("ordinary-0".into())));
    assert!(manager.contains(&FxKey::Custom("ordinary-59".into())));
}
#[test]
fn all_hero_cap_cannot_be_exceeded() {
    let (_, mut manager) = manager(FxLevel::Full);
    for n in 0..60 {
        manager.add_tween(FxKey::Custom(format!("hero-{n}")), tween(), true);
    }
    manager.add_tween(FxKey::Custom("ordinary".into()), tween(), false);
    assert_eq!(manager.active_count(), 48);
    assert!(!manager.contains(&FxKey::Custom("ordinary".into())));
}
#[test]
fn finished_frame_and_following_frame_equal_static_render() {
    let cues = [cue(1, 0)];
    let off = render_at_level(&cues, Duration::ZERO, FxLevel::Off);
    assert_ne!(render_at(&cues, Duration::ZERO), off);
    for ms in [280, 297, 1000] {
        assert_eq!(render_at(&cues, Duration::from_millis(ms)), off);
    }
}
#[test]
fn protected_rect_keeps_glyphs_styles_and_wide_cell_metadata() {
    let cues = [cue(1, 0)];
    let off = render_at_level(&cues, Duration::ZERO, FxLevel::Off);
    for ms in [0, 70, 140, 210, 280, 297] {
        let animated = render_at(&cues, Duration::from_millis(ms));
        for y in BODY.y..BODY.bottom() {
            for x in BODY.x..BODY.right() {
                assert_eq!(animated[(x, y)], off[(x, y)]);
            }
        }
    }
}
#[test]
fn destructive_shader_is_clipped_and_finishes_to_static() {
    let area = Rect::new(3, 2, 12, 6);
    let protected = Rect::new(5, 3, 6, 2);
    let mut buf = Buffer::filled(area, ratatui::buffer::Cell::new("x"));
    let original = buf.clone();
    let key = FxKey::Custom("dissolve".into());
    let mut pass = Pass::new(
        key,
        fx::dissolve(1000),
        Rect::new(4, 2, 8, 5),
        PassAccess::Unprotected,
    );
    pass.process(Duration::from_millis(500), &mut buf, area, &[protected]);
    for pos in protected.positions() {
        assert_eq!(buf[pos], original[pos]);
    }
    for pos in area
        .positions()
        .filter(|pos| !Rect::new(4, 2, 8, 5).contains(*pos))
    {
        assert_eq!(buf[pos], original[pos]);
    }
    assert_ne!(buf, original);
    let mut final_frame = original.clone();
    pass.process(
        Duration::from_millis(1000),
        &mut final_frame,
        area,
        &[protected],
    );
    assert_eq!(final_frame, original);
}
#[test]
fn color_only_allowlist_cannot_change_glyphs_or_modifiers() {
    let mut buf = Buffer::filled(AREA, ratatui::buffer::Cell::new("字"));
    let original = buf.clone();
    let key = FxKey::Custom("untrusted-color-pass".into());
    let shader = fx::effect_fn_buf((), 1000, |_, _, buffer| {
        for cell in &mut buffer.content {
            cell.set_symbol("!");
            cell.set_fg(Color::Red);
            cell.set_style(
                ratatui::style::Style::default().add_modifier(ratatui::style::Modifier::REVERSED),
            );
        }
    });
    let mut pass = Pass::new(key, shader, AREA, PassAccess::ColorOnly(ColorOnly::Dim));
    pass.process(Duration::from_millis(20), &mut buf, AREA, &[AREA]);
    for (before, after) in original.content.iter().zip(&buf.content) {
        assert_eq!(before.symbol(), after.symbol());
        assert_eq!(before.modifier, after.modifier);
        assert_eq!(after.fg, Color::Red);
    }
}
#[test]
fn blank_cell_pass_never_paints_text_or_protected_blanks() {
    let mut buf = Buffer::empty(AREA);
    buf[(2, 2)].set_symbol("x");
    let key = FxKey::Custom("spark".into());
    let shader = fx::effect_fn_buf((), 1000, |_, _, buffer| {
        for cell in &mut buffer.content {
            cell.set_symbol("*");
        }
    });
    let mut pass = Pass::new(key, shader, AREA, PassAccess::BlankCells);
    pass.process(
        Duration::from_millis(20),
        &mut buf,
        AREA,
        &[Rect::new(3, 2, 1, 1)],
    );
    assert_eq!(buf[(2, 2)].symbol(), "x");
    assert_eq!(buf[(3, 2)].symbol(), " ");
    assert_eq!(buf[(4, 2)].symbol(), "*");
}
#[test]
fn pass_allocation_is_clipped_to_viewport_even_for_large_registered_rect() {
    let mut buf = Buffer::empty(Rect::new(10, 20, 4, 4));
    let key = FxKey::Custom("clip".into());
    let mut pass = Pass::new(
        key,
        fx::fade_to_fg(Color::Red, 1000),
        Rect::new(0, 0, u16::MAX, u16::MAX),
        PassAccess::Unprotected,
    );
    pass.process(
        Duration::from_millis(30),
        &mut buf,
        Rect::new(10, 20, 4, 4),
        &[],
    );
}
#[test]
fn idle_halts_streams_and_periodic_have_distinct_cadences() {
    let (clock, mut manager) = manager(FxLevel::Full);
    assert_eq!(manager.next_wake(), None);
    let mut ctx = FxContext::default();
    manager.queue_mut().push(FeedOrigin::Live, cue(1, 0));
    manager.pre(&mut ctx);
    assert_eq!(manager.next_wake(), Some(Duration::from_secs_f64(1. / 30.)));
    clock.advance(Duration::from_secs(1));
    manager.pre(&mut ctx);
    assert_eq!(manager.next_wake(), None);
    ctx.spinners = true;
    manager.pre(&mut ctx);
    assert_eq!(manager.next_wake(), Some(Duration::from_millis(100)));
    ctx.spinners = false;
    ctx.streams = true;
    manager.pre(&mut ctx);
    assert_eq!(manager.next_wake(), Some(Duration::from_secs_f64(1. / 30.)));
    ctx.streams = false;
    ctx.ambience = true;
    manager.pre(&mut ctx);
    assert_eq!(manager.next_wake(), Some(Duration::from_millis(250)));
    clock.set(Duration::from_secs(20));
    manager.pre(&mut ctx);
    assert_eq!(manager.next_wake(), None);
    clock.advance(Duration::from_secs(5));
    assert_eq!(manager.next_wake(), None);
}
#[test]
fn off_and_resize_settle_without_losing_static_content() {
    let (_, mut manager) = manager(FxLevel::Full);
    manager.queue_mut().push(FeedOrigin::Live, cue(1, 0));
    manager.pre(&mut FxContext::default());
    manager.settle();
    assert_eq!(manager.active_count(), 0);
    assert_eq!(manager.next_wake(), None);
    manager.set_level(FxLevel::Off);
    manager.queue_mut().push(FeedOrigin::Live, cue(2, 0));
    manager.pre(&mut FxContext {
        streams: true,
        spinners: true,
        ..Default::default()
    });
    assert_eq!(manager.active_count(), 0);
    assert_eq!(manager.next_wake(), None);
}
#[test]
fn governor_snap_and_unsynchronized_output_disable_passes() {
    let (_, mut manager) = manager(FxLevel::Full);
    manager.add_tween(FxKey::DailySpend, tween(), true);
    manager.observe_frame(FrameSample {
        draw_and_flush: Duration::from_millis(1),
        bytes_per_second: 0,
        input_wait: Duration::from_millis(51),
    });
    assert_eq!(manager.grade(), Grade::G3);
    assert_eq!(manager.active_count(), 0);
    let clock = ManualClock::default();
    let mut capabilities = caps(FxLevel::Full);
    capabilities.synchronized_output = false;
    let mut manager = FxManager::new(clock, capabilities);
    let key = FxKey::DailySpend;
    manager.add_pass(
        key.clone(),
        Pass::new(
            key,
            fx::fade_to_fg(Color::Red, 1000),
            AREA,
            PassAccess::Unprotected,
        )
        .full_buffer(),
        true,
    );
    assert_eq!(manager.active_count(), 0);
}
proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]
    #[test]
    fn arbitrary_cue_sequence_finishes_like_off(events in prop::collection::vec((0u64..150,0u8..16),0..80)) {
        let cues: Vec<_> = events.iter().enumerate().map(|(n,(t,kind))| {
            let contest_id = id(n as u32 % 3);
            let member = squad_protocol::Nick::new("claude").unwrap();
            let message_id = id(n as u32 % 6).to_string().parse().unwrap();
            let event = match kind {
                0 => FxCue::BubbleOpened { id:message_id,model:member },
                1 => FxCue::BubbleCompleted { id:message_id },
                2 => FxCue::BubbleFailed { id:message_id },
                3 => FxCue::RoundStarted { contest_id,n:n as u32,final_round:n % 2 ==0 },
                4 => FxCue::ScorePosted { contest_id,round:n as u32 },
                5 => FxCue::LeadChanged { contest_id,member },
                6 => FxCue::VerdictReady { contest_id },
                7 => FxCue::TournamentRankingReady { contest_id,round:n as u32 },
                8 => FxCue::TournamentEliminated { contest_id,member },
                9 => FxCue::TournamentChampion { contest_id,member },
                10 => FxCue::GrillTurnChanged { contest_id,member },
                11 => FxCue::GrillPlanReady { contest_id,version:n as u32 },
                12 => FxCue::GrillSurvived { contest_id },
                13 => FxCue::PickRequested { contest_id },
                _ => FxCue::DailySpendChanged,
            };
            TimedCue { event_id:id(n as u32),at:Duration::from_millis(*t),cue:event }
        }).collect();
        let full = render_at(&cues,Duration::from_secs(10));
        let off = render_at_level(&cues,Duration::from_secs(10),FxLevel::Off);
        prop_assert_eq!(full,off);
    }
    #[test]
    fn protected_glyphs_survive_any_sample(time in 0u64..1000) {
        let cues = [cue(1,0)];
        let full = render_at(&cues,Duration::from_millis(time));
        let off = render_at_level(&cues,Duration::ZERO,FxLevel::Off);
        for pos in BODY.positions() { prop_assert_eq!(&full[pos],&off[pos]); }
    }
}

#[test]
fn fractional_frame_ticks_do_not_accumulate_shader_time_drift() {
    let key = FxKey::Custom("fractional".into());
    let mut pass = Pass::new(key, fx::dissolve(1000), AREA, PassAccess::Unprotected);
    let base = Buffer::filled(AREA, ratatui::buffer::Cell::new("x"));
    for frame in 0..60 {
        let mut buffer = base.clone();
        pass.process(
            Duration::from_nanos(frame * 16_666_667),
            &mut buffer,
            AREA,
            &[],
        );
    }
    let mut final_frame = base.clone();
    pass.process(Duration::from_secs(1), &mut final_frame, AREA, &[]);
    assert!(!pass.running());
    assert_eq!(final_frame, base);
}
#[test]
fn stochastic_adapter_is_seeded_from_cue_identity() {
    let base = Buffer::filled(AREA, ratatui::buffer::Cell::new("x"));
    let paint = |event| {
        let mut buffer = base.clone();
        let mut pass = Pass::for_cue(
            &cue(event, 0),
            fx::dissolve(1000),
            AREA,
            PassAccess::Unprotected,
        );
        pass.process(Duration::from_millis(500), &mut buffer, AREA, &[]);
        buffer
    };
    assert_eq!(paint(1), paint(1));
    assert_ne!(paint(1), paint(2));
}
#[test]
fn accepted_cues_are_available_for_catalogue_dispatch_once() {
    let (_, mut manager) = manager(FxLevel::Full);
    let mut ctx = FxContext::default();
    manager.queue_mut().push(FeedOrigin::Live, cue(1, 0));
    manager.pre(&mut ctx);
    assert_eq!(ctx.cues, vec![cue(1, 0)]);
    manager.pre(&mut ctx);
    assert!(ctx.cues.is_empty());
}

#[test]
fn third_party_passes_cannot_bypass_color_tiers() {
    for tier in [ColorTier::NoColor, ColorTier::Indexed256, ColorTier::Ansi16] {
        let clock = ManualClock::default();
        let mut caps = caps(FxLevel::Full);
        caps.color = tier;
        let mut manager = FxManager::new(clock.clone(), caps);
        let key = FxKey::DailySpend;
        manager.add_pass(
            key.clone(),
            Pass::new(
                key,
                fx::fade_to_fg(Color::Rgb(255, 0, 0), 1000),
                AREA,
                PassAccess::Unprotected,
            ),
            false,
        );
        manager.pre(&mut FxContext::default());
        let mut cell = ratatui::buffer::Cell::new("x");
        cell.set_fg(Color::Yellow);
        let mut buffer = Buffer::filled(AREA, cell);
        manager.post(&mut buffer, AREA, Duration::from_millis(500));
        for cell in &buffer.content {
            if tier == ColorTier::Indexed256 {
                assert!(matches!(cell.fg, Color::Indexed(_)));
            } else {
                assert_eq!(cell.fg, Color::Yellow);
            }
        }
    }
}
