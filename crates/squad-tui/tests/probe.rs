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
    let nicks: Vec<MemberNick> = ["alpha", "beta", "gamma", "delta"]
        .map(|n| n.parse().unwrap())
        .into();
    let setup = ProbeSetup {
        contest_id: "019b0000-0000-7000-8000-000000000001".parse().unwrap(),
        brief: ProbeBrief {
            observation: t("Intermittent noise; fan ruled out; inspect exterior indicator only"),
            ruled_out: "Fan".into(),
            inspectable: t("Exterior indicator"),
        },
        participants: nicks.clone(),
        explainers: nicks[..2].to_vec(),
        checkers: nicks[2..].to_vec(),
        sitting_out: Some("epsilon".parse().unwrap()),
    };
    let explanation = ProbeExplanation {
        explanation: t("Loose connector"),
        key_assumption: t("Connector moves with vibration"),
        limitations: vec![t("Not verified")],
    };
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
    let prediction = ProbePrediction {
        predictions: vec![ProbePredictionItem {
            check_id: cell(1),
            predictable: false,
            expected_outcome_ids: vec![],
            expected_observation: None,
            reason: t("Indicator does not constrain the connector"),
        }],
    };
    let matrix =
        std::collections::BTreeMap::from([(cell(3), Some(prediction.clone())), (cell(4), None)]);
    let report=serde_json::from_value(json!({"decisionTree":{"root":"root","nodes":{"root":{"kind":"check","checkId":cell(1),"measurement":"Indicator color","branches":{"red":"red","green":"green"},"unexpected":"unexpected"},"red":{"kind":"leaf","remaining":[cell(3),cell(4)],"nextAction":"Inspect brief again","reassess":false},"green":{"kind":"leaf","remaining":[cell(3),cell(4)],"nextAction":"Inspect brief again","reassess":false},"unexpected":{"kind":"leaf","remaining":[cell(3),cell(4)],"nextAction":"Reopen the diagnostic brief and reassess the assumptions","reassess":true}}},"predictionMatrix":matrix,"discardedChecks":[{"checkId":cell(2),"reason":"Does not distinguish the available explanations"}],"inseparableGroups":[{"explanations":[cell(3),cell(4)],"reason":"Unknown and missing predictions cannot eliminate explanations"}],"limitations":["No checks performed"]})).unwrap();
    ProbeView {
        state: Some(ProbeState {
            contest_id: setup.contest_id,
            state: ProbeStatus::Complete,
            state_version: 7,
            step_id: None,
            artifact_event_ids: vec![],
            skipped_work_ids: vec![],
            skipped_members: vec!["beta".parse().unwrap()],
            terminal_reason: None,
        }),
        setup: Some(setup),
        explanations: vec![("alpha".into(), explanation)],
        checks: vec![("gamma".into(), check)],
        predictions: vec![("alpha".into(), prediction)],
        material: None,
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
