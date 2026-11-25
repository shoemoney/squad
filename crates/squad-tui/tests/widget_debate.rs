use ratatui::{buffer::Buffer, layout::Rect};
use squad_core::banner::{Banner, BannerMode};
use squad_protocol::*;
use squad_tui::{
    fx::{ColorTier, Palette},
    widgets::debate::*,
};
use std::collections::BTreeMap;
fn state() -> DebateState {
    let case = "01900000-0000-7000-8000-000000000002".parse().unwrap();
    DebateState {
        debate_id: "01900000-0000-7000-8000-000000000001".parse().unwrap(),
        state: DebateStatus::RoundRunning,
        debaters: [
            MemberNick::new("claude").unwrap(),
            MemberNick::new("gemini").unwrap(),
        ],
        round: 1,
        total_rounds: 2,
        judge_model: "judge/test".into(),
        judge_requested: true,
        state_version: 1,
        terminal_reason: None,
        case_id: Some(case),
        case_ids: vec![case],
    }
}
fn score(state: &DebateState, round: u32) -> DebateRoundScore {
    DebateRoundScore {
        debate_id: state.debate_id,
        case_id: state.case_id.unwrap(),
        round,
        scores: Some(BTreeMap::from([
            (
                state.debaters[0].clone(),
                RoundScore {
                    score: 7,
                    reason: "Good fit; retain the concrete migration plan.".into(),
                },
            ),
            (
                state.debaters[1].clone(),
                RoundScore {
                    score: 8,
                    reason: "A simpler deployment with fewer moving parts.".into(),
                },
            ),
        ])),
        tally: BTreeMap::from([
            (state.debaters[0].clone(), 17),
            (state.debaters[1].clone(), 19),
        ]),
        reason: None,
    }
}
fn screen(buf: &Buffer) -> String {
    (buf.area.y..buf.area.bottom())
        .map(|y| {
            (buf.area.x..buf.area.right())
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
#[test]
fn inline_scorecard_viewports() {
    for (width, height) in [(80, 24), (120, 40), (32, 24)] {
        let state = state();
        let mut view = DebateView::from(&state);
        view.banner_mode = BannerMode::Off;
        assert!(view.apply_score(&score(&state, 1)));
        view.rounds[0].reasons_expanded = true;
        let area = Rect::new(0, 0, width, height);
        let mut buf = Buffer::empty(area);
        let rects = render(&view, area, &mut buf, &Palette::new(ColorTier::TrueColor));
        assert_eq!(rects.score_cells.len(), 2);
        assert!(
            rects
                .score_cells
                .iter()
                .all(|(_, _, r)| r.intersection(area) == *r)
        );
        assert!(!rects.versus_card.is_empty());
        insta::assert_snapshot!(format!("debate_{width}x{height}"), screen(&buf));
    }
}
#[test]
fn late_duplicate_unscored_and_foreign_scores() {
    let mut state = state();
    state.round = 2;
    let mut view = DebateView::from(&state);
    let accepted = score(&state, 1);
    assert!(view.apply_score(&accepted));
    assert!(view.apply_score(&accepted));
    assert_eq!(view.rounds[0].scoring, Scoring::Accepted(accepted));
    assert_eq!(view.rounds[1].scoring, Scoring::Pending);
    let mut unscored = score(&state, 2);
    unscored.scores = None;
    unscored.reason = Some(Text::new("unscored").unwrap());
    assert!(view.apply_score(&unscored));
    let mut foreign = unscored.clone();
    foreign.case_id = CaseId::new();
    assert!(!view.apply_score(&foreign));
    foreign = unscored;
    foreign.round = 0;
    assert!(!view.apply_score(&foreign));
}
#[test]
fn stopped_judging_does_not_restart_canceled_scoring() {
    let mut state = state();
    let mut view = DebateView::from(&state);
    state.state = DebateStatus::Stopped;
    state.state_version += 1;
    view.apply_state(&state, &DebateTransition::StopCommitted);
    assert_eq!(view.rounds[0].scoring, Scoring::Cancelled);
    state.state = DebateStatus::Preparing;
    state.state_version += 1;
    view.apply_state(&state, &DebateTransition::JudgeOnlyRequested);
    state.state = DebateStatus::Judging;
    state.state_version += 1;
    assert_eq!(
        view.apply_state(&state, &DebateTransition::JudgeOnlyConfirmed),
        Some(Banner::Verdict)
    );
    assert_eq!(view.rounds[0].scoring, Scoring::Cancelled);
    assert_eq!(
        view.apply_state(&state, &DebateTransition::JudgeOnlyConfirmed),
        None
    );
    let mut unjudged = state.clone();
    unjudged.judge_requested = false;
    unjudged.state = DebateStatus::CompleteUnjudged;
    let view = DebateView::from(&unjudged);
    assert_eq!(view.rounds[0].scoring, Scoring::NotApplicable);
}
#[test]
fn boundaries_replay_and_one_round_final() {
    let mut state = state();
    state.round = 0;
    assert_eq!(
        boundary(None, &state, &DebateTransition::RoundEntered),
        Some(Banner::Opening)
    );
    let mut view = DebateView::from(&state);
    state.round = 1;
    state.state_version += 1;
    assert_eq!(
        view.apply_state(&state, &DebateTransition::RoundEntered),
        Some(Banner::Round(1))
    );
    state.state = DebateStatus::RoundCommitting;
    state.state_version += 1;
    assert_eq!(
        view.apply_state(&state, &DebateTransition::RoundResolved),
        None
    );
    state.round = 2;
    state.state = DebateStatus::RoundRunning;
    state.state_version += 1;
    assert_eq!(
        view.apply_state(&state, &DebateTransition::RoundEntered),
        Some(Banner::FinalRound)
    );
    state.total_rounds = 1;
    state.round = 1;
    assert_eq!(
        boundary(None, &state, &DebateTransition::RoundEntered),
        Some(Banner::FinalRound)
    );
}
#[test]
fn turns_are_ordered_and_clipped_and_retry_is_explicit() {
    let state = state();
    let mut view = DebateView::from(&state);
    view.banner_mode = BannerMode::Off;
    for nick in state.debaters.iter().rev() {
        let turn = DebateTurn {
            generation: GenerationIdentity {
                message_id: MessageId::new(),
                work_id: WorkId::new(),
                trigger_event_id: EventId::new(),
                attempt: 2,
            },
            debate_id: state.debate_id,
            round: 1,
            speaker: nick.clone(),
            outcome: DebateTurnOutcome::Forfeit {
                reason_code: ForfeitReason::Timeout,
                text: Text::new("Timed out").unwrap(),
            },
        };
        assert!(view.apply_turn(&turn));
        assert!(view.apply_turn(&turn));
    }
    let area = Rect::new(3, 2, 32, 24);
    let mut buf = Buffer::empty(Rect::new(0, 0, 40, 30));
    render(&view, area, &mut buf, &Palette::new(ColorTier::NoColor));
    let text = screen(&buf);
    assert!(text.contains("retry"));
    assert!(text.find("@claude · forfeit").unwrap() < text.find("@gemini · forfeit").unwrap());
    assert_eq!(view.rounds[0].turns.len(), 2);
    for (x, y) in [(0, 0), (39, 29), (2, 2)] {
        assert_eq!(buf[(x, y)].symbol(), " ");
    }
    view.collapsed = true;
    let compact = render(&view, area, &mut buf, &Palette::new(ColorTier::NoColor));
    assert!(compact.score_cells.is_empty());
    for width in 0..4 {
        render(
            &view,
            Rect::new(0, 0, width, 1),
            &mut buf,
            &Palette::new(ColorTier::NoColor),
        );
    }
}

#[test]
fn parallel_streams_and_permanent_verdict_banner() {
    let mut state = state();
    let mut view = DebateView::from(&state);
    view.banner_mode = BannerMode::Off;
    let delta = Delta {
        generation: GenerationIdentity {
            message_id: MessageId::new(),
            work_id: WorkId::new(),
            trigger_event_id: EventId::new(),
            attempt: 1,
        },
        chunk_index: 0,
        text: "First".into(),
        debate_id: Some(state.debate_id),
        round: Some(1),
        case_id: None,
        contest_id: None,
        step_id: None,
    };
    assert!(view.update_stream(&state.debaters[0], &delta, "First stream"));
    assert!(view.update_stream(&state.debaters[1], &delta, "Second stream"));
    assert_eq!(view.rounds[0].turns.len(), 2);
    assert!(view.update_stream(&state.debaters[0], &delta, "First stream"));
    assert_eq!(view.rounds[0].turns[0].text, "First stream");
    let turn = DebateTurn {
        generation: delta.generation.clone(),
        debate_id: state.debate_id,
        round: 1,
        speaker: state.debaters[0].clone(),
        outcome: DebateTurnOutcome::Completed {
            content: "Completed".into(),
            model: Text::new("test/model").unwrap(),
            finish_reason: FinishReason::Length,
            usage: None,
        },
    };
    assert!(view.apply_turn(&turn));
    assert!(!view.update_stream(&state.debaters[0], &delta, "Stale"));
    state.state = DebateStatus::Judging;
    state.state_version += 1;
    view.apply_state(&state, &DebateTransition::RoundScoringFinished);
    state.state = DebateStatus::Completed;
    state.state_version += 1;
    view.apply_state(
        &state,
        &DebateTransition::VerdictAccepted {
            verdict_event_id: EventId::new(),
        },
    );
    let mut cache = squad_tui::widgets::banner::BannerCache::default();
    view.prepare_banners(80, &mut cache);
    let area = Rect::new(0, 0, 80, 40);
    let mut buf = Buffer::empty(area);
    let geometry = render(&view, area, &mut buf, &Palette::new(ColorTier::NoColor));
    assert!(
        geometry
            .banners
            .iter()
            .any(|(banner, _)| *banner == Banner::Verdict)
    );
    assert!(screen(&buf).contains("Length"));
    assert!(screen(&buf).contains("Second stream"));
}

#[test]
fn opening_has_no_score_and_historical_cases_survive() {
    let mut state = state();
    state.round = 0;
    let opening = DebateView::from(&state);
    assert_eq!(opening.rounds.len(), 1);
    assert_eq!(opening.rounds[0].scoring, Scoring::NotApplicable);
    state.round = 2;
    let original_case = state.case_id.unwrap();
    let mut view = DebateView::from(&state);
    let old_score = score(&state, 1);
    state.case_id = Some(CaseId::new());
    state.case_ids.push(state.case_id.unwrap());
    state.state_version += 1;
    view.apply_state(&state, &DebateTransition::JudgeOnlyRequested);
    assert!(view.apply_score(&old_score));
    assert!(associated_cases(&view).contains(&original_case));
    let mut canceled = state.clone();
    canceled.state = DebateStatus::Stopped;
    canceled.state_version += 1;
    view.apply_state(&canceled, &DebateTransition::StopCommitted);
    let mut unscored = score(&state, 2);
    unscored.scores = None;
    unscored.reason = Some(Text::new("unscored").unwrap());
    assert!(view.apply_score(&unscored));
    view.banner_mode = BannerMode::Off;
    let area = Rect::new(0, 0, 80, 40);
    let mut buf = Buffer::empty(area);
    render(&view, area, &mut buf, &Palette::new(ColorTier::NoColor));
    assert!(screen(&buf).contains("unscored"));
    assert!(!screen(&buf).contains("pending"));
}

#[test]
fn tug_tally_follows_commit_order_instead_of_round_number() {
    let mut state = state();
    state.round = 2;
    let mut view = DebateView::from(&state);
    let round_two = score(&state, 2);
    assert!(view.apply_score(&round_two));
    let mut late_round_one = score(&state, 1);
    late_round_one.tally.insert(state.debaters[0].clone(), 24);
    late_round_one.tally.insert(state.debaters[1].clone(), 27);
    assert!(view.apply_score(&late_round_one));
    assert_eq!(view.tally.as_ref(), Some(&late_round_one.tally));
    assert!(view.apply_score(&round_two));
    assert_eq!(view.tally.as_ref(), Some(&late_round_one.tally));
}
