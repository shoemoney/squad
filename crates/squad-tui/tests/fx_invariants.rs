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

#[path = "../examples/fx_reel/catalogue.rs"]
mod catalogue;

fn catalogue_cue(n: u32, effect: CatalogueEffect) -> TimedCue {
    TimedCue {
        event_id: id(n),
        at: Duration::ZERO,
        cue: FxCue::Catalogue {
            effect,
            target: format!("fixture-{effect:?}"),
            duration: catalogue::duration(effect),
        },
    }
}
fn bind_catalogue(manager: &mut FxManager<ManualClock>, ctx: &FxContext) {
    for cue in ctx.cues.iter().rev().take(1) {
        if let FxCue::Catalogue { effect, .. } = cue.cue {
            let pass = Pass::catalogue(
                cue.cue.key(),
                AREA,
                PassAccess::Unprotected,
                catalogue::duration(effect),
                move |t, b, policy| {
                    catalogue::paint(
                        effect,
                        b,
                        t,
                        if policy.fade_only {
                            FxLevel::Lite
                        } else {
                            FxLevel::Full
                        },
                    )
                },
            );
            assert!(manager.attach_pass(cue, pass));
        }
    }
}
#[test]
fn entire_catalogue_settles_to_off_and_protects_partial_committed_reports() {
    let area = Rect::new(0, 0, 120, 40);
    for effect in catalogue::ALL {
        let (clock, mut manager) = manager(FxLevel::Full);
        // All glyphs, including blank wide-glyph continuations and partial report
        // labels, are protected. Dedicated graphics remain outside this rectangle.
        let protected = Rect::new(4, 5, 60, 3);
        let mut ctx = FxContext {
            protected: vec![protected],
            ..Default::default()
        };
        let cue = catalogue_cue(7, effect);
        manager.queue_mut().push(FeedOrigin::Live, cue.clone());
        manager.pre(&mut ctx);
        let pass = Pass::catalogue(
            cue.cue.key(),
            area,
            PassAccess::Unprotected,
            catalogue::duration(effect),
            move |t, b, _| catalogue::paint(effect, b, t, FxLevel::Full),
        );
        assert!(manager.attach_pass(&cue, pass));
        let mut baseline = Buffer::empty(area);
        catalogue::paint(effect, &mut baseline, Duration::ZERO, FxLevel::Off);
        baseline.set_string(
            4,
            5,
            "Partial committed report 界: evidence stays; pending stays pending",
            ratatui::style::Style::default(),
        );
        for ms in [0, 25, 100, 200, 400, 900, 1800, 3200, 5700, 6500, 6517] {
            clock.set(Duration::from_millis(ms));
            manager.pre(&mut ctx);
            let mut actual = baseline.clone();
            manager.post(&mut actual, area, clock.now());
            for y in protected.y..protected.bottom() {
                for x in protected.x..protected.right() {
                    assert_eq!(
                        actual[(x, y)].symbol(),
                        baseline[(x, y)].symbol(),
                        "{effect:?} at {ms}"
                    );
                }
            }
            if ms >= 6500 {
                assert_eq!(actual, baseline, "{effect:?}");
                assert_eq!(manager.active_count(), 0);
            }
        }
        assert_eq!(manager.next_wake(), None);
        // Collapse, failure, stop and resize share the same settlement boundary.
        manager.settle();
        let mut actual = baseline.clone();
        manager.post(&mut actual, area, clock.now());
        assert_eq!(actual, baseline);
    }
}
#[test]
fn whole_catalogue_replay_duplicates_drops_and_resize() {
    for effect in catalogue::ALL {
        let (clock, mut manager) = manager(FxLevel::Full);
        for n in 0..1000 {
            manager
                .queue_mut()
                .push(FeedOrigin::Replay, catalogue_cue(n, effect));
        }
        let mut ctx = FxContext::default();
        manager.pre(&mut ctx);
        assert!(ctx.cues.is_empty());
        assert_eq!(manager.active_count(), 0);
        for n in 0..1000 {
            manager
                .queue_mut()
                .push(FeedOrigin::Live, catalogue_cue(n, effect));
        }
        assert_eq!(manager.queue_mut().dropped(), 744);
        manager.pre(&mut ctx);
        assert_eq!(manager.active_count(), 1);
        bind_catalogue(&mut manager, &ctx);
        manager
            .queue_mut()
            .push(FeedOrigin::Live, catalogue_cue(999, effect));
        manager.pre(&mut ctx);
        assert!(ctx.cues.is_empty());
        manager.settle();
        assert_eq!(manager.active_count(), 0);
        assert_eq!(manager.next_wake(), None);
        clock.set(Duration::from_secs(3));
        manager
            .queue_mut()
            .push(FeedOrigin::Live, catalogue_cue(1001, effect));
        manager.pre(&mut ctx);
        assert!(ctx.cues.is_empty());
    }
}
#[test]
fn report_cues_replace_and_collapse_by_contest() {
    let (_, mut manager) = manager(FxLevel::Full);
    let contest = id(66);
    let cues = [
        FxCue::ModeBannerReady {
            contest_id: contest,
            mode: ReportMode::Census,
        },
        FxCue::CensusEstimateReady {
            contest_id: contest,
            pass: 1,
        },
        FxCue::CrosscheckCellsReady {
            contest_id: contest,
            artifact_id: id(90),
        },
        FxCue::SurveyTallyReady {
            contest_id: contest,
            artifact_id: id(91),
        },
        FxCue::ProbeTreeReady {
            contest_id: contest,
        },
    ];
    for (n, cue) in cues.iter().enumerate() {
        manager.queue_mut().push(
            FeedOrigin::Live,
            TimedCue {
                event_id: id(n as u32),
                at: Duration::ZERO,
                cue: cue.clone(),
            },
        );
    }
    let mut ctx = FxContext::default();
    manager.pre(&mut ctx);
    assert_eq!(manager.active_count(), 5);
    manager.settle_contest(&contest.to_string());
    assert_eq!(manager.active_count(), 0);
    assert_eq!(manager.next_wake(), None);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]
    #[test]
    fn arbitrary_catalogue_sequence_finishes_like_off(sequence in prop::collection::vec(0usize..18,0..100), sample in 0u64..6500) {
        let (clock,mut manager)=manager(FxLevel::Full);
        for (n,index) in sequence.into_iter().enumerate() {manager.queue_mut().push(FeedOrigin::Live,catalogue_cue(n as u32,catalogue::ALL[index]));}
        let mut ctx=FxContext {protected:vec![BODY],..Default::default()};manager.pre(&mut ctx);
        // Bind only the newest admitted cue for each replacement key.
        let mut bound=std::collections::BTreeSet::new();
        for cue in ctx.cues.iter().rev() {
            if !bound.insert(cue.cue.key()) {continue;}
            if let FxCue::Catalogue {effect,..}=cue.cue {
                prop_assert!(manager.attach_pass(cue,Pass::catalogue(cue.cue.key(),AREA,PassAccess::Unprotected,catalogue::duration(effect),move |t,b,_|catalogue::paint(effect,b,t,FxLevel::Full))));
            }
        }
        let mut baseline=Buffer::empty(AREA);baseline.set_string(2,3,"Partial report 界 evidence remains",ratatui::style::Style::default());
        clock.set(Duration::from_millis(sample));manager.pre(&mut ctx);let mut actual=baseline.clone();manager.post(&mut actual,AREA,clock.now());
        for y in BODY.y..BODY.bottom(){for x in BODY.x..BODY.right(){prop_assert_eq!(actual[(x,y)].symbol(),baseline[(x,y)].symbol());}}
        clock.set(Duration::from_secs(21));manager.pre(&mut ctx);let mut actual=baseline.clone();manager.post(&mut actual,AREA,clock.now());prop_assert_eq!(actual,baseline);prop_assert_eq!(manager.active_count(),0);
    }
}
#[test]
fn same_artifact_under_a_new_event_does_not_restart_and_new_artifact_replaces() {
    let (_, mut manager) = manager(FxLevel::Full);
    let mut ctx = FxContext::default();
    let make = |event, artifact| TimedCue {
        event_id: id(event),
        at: Duration::ZERO,
        cue: FxCue::SurveyTallyReady {
            contest_id: id(100),
            artifact_id: id(artifact),
        },
    };
    manager.queue_mut().push(FeedOrigin::Live, make(1, 10));
    manager.pre(&mut ctx);
    assert_eq!(ctx.cues.len(), 1);
    manager.queue_mut().push(FeedOrigin::Live, make(2, 10));
    manager.pre(&mut ctx);
    assert!(ctx.cues.is_empty());
    manager.queue_mut().push(FeedOrigin::Live, make(3, 11));
    manager.pre(&mut ctx);
    assert_eq!(ctx.cues.len(), 1);
    assert_eq!(manager.active_count(), 1);
}

#[test]
fn reel_covers_every_scene_at_wide_and_narrow_sizes_with_honest_totals() {
    let verdict = catalogue::canned();
    for side in 0..2 {
        assert_eq!(
            verdict.totals[side],
            verdict.criteria.iter().map(|c| c.scores[side]).sum::<u32>()
        );
    }
    for (w, h) in [(80, 24), (140, 40), (200, 60)] {
        let area = Rect::new(0, 0, w, h);
        let inner = Rect::new(2, 2, w - 4, h - 4);
        for ms in [
            0, 700, 1300, 2199, 2250, 2450, 2900, 3200, 5000, 5150, 5400, 5600, 5900, 7000, 8599,
            9000, 12500, 15900, 17000, 18000, 20800, 22000, 24000, 25000, 27000, 27780, 29000,
        ] {
            let mut b = Buffer::empty(area);
            let reveal = squad_tui::fx::verdict::VerdictReveal::new(FxLevel::Full, false, true, 42);
            catalogue::showcase(&mut b, inner, Duration::from_millis(ms), &reveal);
            assert_eq!(b.area, area);
            if ms == 29000 {
                let text: String = b.content.iter().map(|c| c.symbol()).collect();
                assert!(text.contains("AGREE 4"));
                assert!(text.contains("DISAGREE 1"));
            }
        }
    }
}

#[test]
fn catalogue_scratch_is_bounded_by_viewport_and_candidate_keys_follow_pending_state() {
    let mut pass = Pass::catalogue(
        FxKey::DailySpend,
        Rect::new(0, 0, u16::MAX, u16::MAX),
        PassAccess::BlankCells,
        Duration::from_secs(1),
        |_, b, _| assert_eq!(b.area, AREA),
    );
    let mut buffer = Buffer::empty(AREA);
    pass.process(Duration::ZERO, &mut buffer, AREA, &[]);
    let (_, mut manager) = manager(FxLevel::Full);
    let key = FxKey::Candidate {
        contest: "pick".into(),
        member: "claude".into(),
    };
    manager.add_tween(
        key.clone(),
        Tween {
            start: Duration::ZERO,
            duration: Duration::from_secs(2),
            from: 0.,
            to: 1.,
            ease: Ease::Linear,
        },
        false,
    );
    manager.pre(&mut FxContext {
        pending_pick: true,
        ..Default::default()
    });
    assert!(manager.contains(&key));
    manager.pre(&mut FxContext::default());
    assert!(!manager.contains(&key));
    assert_eq!(manager.next_wake(), None);
}
