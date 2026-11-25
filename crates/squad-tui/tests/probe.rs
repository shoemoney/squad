use ratatui::{buffer::Buffer, layout::Rect};
use serde_json::json;
use squad_protocol::*;
use squad_tui::{
    fx::{ColorTier, FeedOrigin, FxCue, FxQueue, Palette},
    session::Session,
    widgets::probe::*,
};
use std::time::Duration;
fn t(s: &str) -> Text {
    Text::new(s).unwrap()
}
fn cell(n: u8) -> CellId {
    format!("00000000-0000-4000-8000-{n:012}").parse().unwrap()
}
fn fixture() -> ProbeView {
    let nicks: Vec<MemberNick> = [
        "alpha", "beta", "gamma", "delta", "checker1", "checker2", "checker3", "checker4",
    ]
    .map(|s| s.parse().unwrap())
    .into();
    let setup = ProbeSetup {
        contest_id: "019b0000-0000-7000-8000-000000000001".parse().unwrap(),
        brief: ProbeBrief {
            observation: t("Intermittent noise; fan ruled out; inspect exterior indicator only"),
            ruled_out: "Fan".into(),
            inspectable: t("Exterior indicator"),
        },
        participants: nicks.clone(),
        explainers: nicks[..4].to_vec(),
        checkers: nicks[4..].to_vec(),
        sitting_out: Some("epsilon".parse().unwrap()),
    };
    setup.validate().unwrap();
    let explanations = [
        "Loose connector",
        "Regulator vibration",
        "Resonant cover",
        "Unstable sensor",
    ]
    .iter()
    .enumerate()
    .map(|(i, s)| ProbeItem {
        id: cell(i as u8 + 3),
        data: ProbeExplanation {
            explanation: t(s),
            key_assumption: t("Depends on charging state"),
            limitations: vec![t("Not observed")],
        },
    })
    .collect::<Vec<_>>();
    let check = ProbeCheck {
        check: t("Read indicator"),
        procedure: t("Observe without opening or touching the case"),
        measurement: t("Indicator color"),
        outcomes: vec![
            ProbeOutcome {
                id: t("red"),
                description: t("Red"),
            },
            ProbeOutcome {
                id: t("green"),
                description: t("Green"),
            },
        ],
        effort: t("One minute"),
        safety_notes: vec![t("Do not open the case")],
    };
    let checks = vec![
        ProbeItem {
            id: cell(1),
            data: check.clone(),
        },
        ProbeItem {
            id: cell(2),
            data: check.clone(),
        },
    ];
    let unknown = |id| ProbePredictionItem {
        check_id: cell(id),
        predictable: false,
        expected_outcome_ids: vec![],
        expected_observation: None,
        reason: t("Cannot infer this indicator from the explanation"),
    };
    let known = |outcome: &str| ProbePrediction {
        predictions: vec![
            ProbePredictionItem {
                check_id: cell(1),
                predictable: true,
                expected_outcome_ids: vec![t(outcome)],
                expected_observation: Some(t(outcome)),
                reason: t("Conditional on this explanation"),
            },
            unknown(2),
        ],
    };
    let row = ProbePrediction {
        predictions: vec![unknown(1), unknown(2)],
    };
    let matrix = std::collections::BTreeMap::from([
        (cell(3), Some(row.clone())),
        (cell(4), None),
        (cell(5), Some(known("red"))),
        (cell(6), Some(known("green"))),
    ]);
    let report:ProbeReport=serde_json::from_value(json!({"decisionTree":{"root":"root","nodes":{"root":{"kind":"check","checkId":cell(1),"measurement":"Indicator color","branches":{"red":"red","green":"green"},"unexpected":"unexpected"},"red":{"kind":"leaf","remaining":[cell(3),cell(4),cell(5)],"nextAction":"Review remaining possibilities","reassess":false},"green":{"kind":"leaf","remaining":[cell(3),cell(4),cell(6)],"nextAction":"Review remaining possibilities","reassess":false},"unexpected":{"kind":"leaf","remaining":[cell(3),cell(4),cell(5),cell(6)],"nextAction":"Reopen the diagnostic brief and reassess the assumptions","reassess":true}}},"predictionMatrix":matrix,"discardedChecks":[{"checkId":cell(2),"reason":"Does not distinguish the available explanations"}],"inseparableGroups":[{"explanations":[cell(3),cell(4),cell(5)],"reason":"Unknown and missing predictions cannot eliminate explanations"},{"explanations":[cell(3),cell(4),cell(6)],"reason":"Unknown and missing predictions cannot eliminate explanations"}],"limitations":["No checks performed"]})).unwrap();
    let material = ProbeCaseMaterial {
        brief: setup.brief.clone(),
        explanations: explanations.clone(),
        checks,
        prediction_matrix: matrix,
        missing_rows: vec![cell(4)],
        skipped_members: vec![nicks[1].clone(), nicks[6].clone(), nicks[7].clone()],
    };
    report.validate_against(&material).unwrap();
    ProbeView {
        matrix_collapsed: false,
        banner_mode: None,
        banner_art: None,
        state: Some(ProbeState {
            contest_id: setup.contest_id,
            state: ProbeStatus::Complete,
            state_version: 7,
            step_id: None,
            artifact_event_ids: vec![],
            skipped_work_ids: vec![],
            skipped_members: material.skipped_members.clone(),
            terminal_reason: None,
        }),
        setup: Some(setup),
        explanations: explanations
            .iter()
            .enumerate()
            .map(|(i, e)| (nicks[i].to_string(), e.data.clone()))
            .collect(),
        checks: vec![(nicks[4].to_string(), check)],
        predictions: vec![
            (nicks[0].to_string(), row),
            (nicks[2].to_string(), known("red")),
            (nicks[3].to_string(), known("green")),
        ],
        material: Some(material),
        report: Some(report),
    }
}
fn glyphs(buf: &Buffer) -> String {
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim_end()
        .to_owned()
}
#[test]
fn committed_probe_states_at_both_viewports() {
    let view = fixture();
    for (width, height) in [(80, 24), (120, 40)] {
        let mut buf = Buffer::empty(Rect::new(0, 0, width, 2048));
        let geometry = render(&view, buf.area, &mut buf, &Palette::new(ColorTier::NoColor));
        let rows = glyphs(&buf);
        assert!(geometry.content_height > 0);
        assert!(!geometry.connectors.is_empty());
        assert!(rows.contains("Sitting out: @epsilon"));
        assert!(rows.contains("cannot predict"));
        assert!(rows.contains("Unexpected result"));
        assert!(rows.contains("Inseparable"));
        insta::assert_snapshot!(format!("probe_{width}x{height}"), rows);
    }
}
#[test]
fn only_committed_live_reports_emit_tree_cues_and_replay_is_quiet() {
    let view = fixture();
    let contest = view.setup.as_ref().unwrap().contest_id;
    let payload = EnvelopePayload::ProbeReport(ProbeOutput {
        contest_id: contest,
        step_id: StepId::new(),
        completed: CompletedGeneration {
            generation: GenerationIdentity {
                message_id: MessageId::new(),
                work_id: WorkId::new(),
                trigger_event_id: EventId::new(),
                attempt: 1,
            },
            model: t("fixture/judge"),
            finish_reason: FinishReason::Stop,
            usage: None,
        },
        data: view.report.unwrap(),
    });
    let base = EnvelopeBase {
        version: 1,
        id: EventId::new(),
        room_id: MainRoom::Main,
        correlation_id: EventId::new(),
        causation_id: None,
        sender: Actor::Judge,
        recipients: vec![Recipient::Me],
        created_at: std::time::SystemTime::now().into(),
    };
    let committed = CommittedEnvelope {
        base: base.clone(),
        content: payload.clone(),
        commit: CommitFields {
            seq: Cursor::new(1).unwrap(),
            accepted_at: "2026-11-25T00:00:00Z".parse().unwrap(),
            commit_index: 0,
            commit_size: 1,
        },
    };
    let mut session = Session::default();
    let mut queue = FxQueue::new();
    session.reduce(
        FeedItem::Committed(committed.clone()),
        FeedOrigin::Live,
        Duration::ZERO,
        &mut queue,
    );
    assert!(matches!(
        queue.pop_front().unwrap().cue,
        FxCue::ProbeTreeReady { .. }
    ));
    let mut replay = Session::default();
    let mut queue = FxQueue::new();
    replay.reduce(
        FeedItem::Committed(committed),
        FeedOrigin::Replay,
        Duration::ZERO,
        &mut queue,
    );
    assert!(queue.pop_front().is_none());
    assert_eq!(replay.probes.len(), 1);
    let mut provisional = Session::default();
    provisional.reduce(
        FeedItem::Delta(ProducerEnvelope {
            base,
            content: payload,
        }),
        FeedOrigin::Live,
        Duration::ZERO,
        &mut queue,
    );
    assert!(provisional.probes.is_empty());
    assert!(queue.pop_front().is_none());
}

#[test]
fn matrix_expands_without_hiding_tree_or_skipped_evidence() {
    let mut view = fixture();
    let palette = Palette::new(ColorTier::NoColor);
    let mut before = Buffer::empty(Rect::new(0, 0, 80, 2048));
    let before_geometry = render(&view, before.area, &mut before, &palette);
    view.toggle_matrix();
    let mut after = before.clone();
    let after_geometry = render(&view, after.area, &mut after, &palette);
    assert!(after_geometry.content_height < before_geometry.content_height);
    let text = glyphs(&after);
    assert!(text.contains("F7 to expand"));
    assert!(text.contains("Unexpected result"));
    assert!(text.contains("Skipped member"));
    assert!(text.contains("Inseparable"));
    view.toggle_matrix();
    render(&view, before.area, &mut after, &palette);
    assert_eq!(glyphs(&before), glyphs(&after));
}

#[test]
fn roster_shows_authoritative_role_and_sit_out_until_terminal() {
    let mut view = fixture();
    view.state.as_mut().unwrap().state = ProbeStatus::Proposing;
    let id = view.setup.as_ref().unwrap().contest_id;
    let mut session = Session::default();
    session.probes.insert(id, view);
    assert!(
        session
            .probe_participant_status(&"alpha".parse().unwrap())
            .unwrap()
            .contains("explainer")
    );
    assert!(
        session
            .probe_participant_status(&"epsilon".parse().unwrap())
            .unwrap()
            .contains("sitting out")
    );
    session
        .probes
        .get_mut(&id)
        .unwrap()
        .state
        .as_mut()
        .unwrap()
        .state = ProbeStatus::Stopped;
    assert!(
        session
            .probe_participant_status(&"epsilon".parse().unwrap())
            .is_none()
    );
    assert!(session.snapshot.scoreboard.is_empty());
    assert!(session.snapshot.trophies.is_empty());
}

#[test]
fn banner_off_keeps_the_authoritative_probe_label() {
    let mut view = fixture();
    view.banner_mode = Some(squad_core::banner::BannerMode::Off);
    let mut buf = Buffer::empty(Rect::new(0, 0, 40, 80));
    let geometry = render(&view, buf.area, &mut buf, &Palette::new(ColorTier::NoColor));
    assert_eq!(geometry.banner.height, 1);
    assert!(glyphs(&buf).trim_start().starts_with("PROBE"));
}
