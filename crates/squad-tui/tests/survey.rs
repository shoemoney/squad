use ratatui::{buffer::Buffer, layout::Rect};
use squad_protocol::*;
use squad_tui::{
    fx::{FeedOrigin, FxQueue},
    session::Session,
    widgets::survey::{self, SurveyView},
};
use std::time::Duration;
fn event_id() -> EventId {
    "01900000-0000-7000-8000-000000000001".parse().unwrap()
}
fn fixture(collect: bool) -> SurveyView {
    let contest_id = event_id().to_string().parse().unwrap();
    let step_id = event_id().to_string().parse().unwrap();
    let completed = CompletedGeneration {
        generation: GenerationIdentity {
            message_id: event_id().to_string().parse().unwrap(),
            work_id: event_id().to_string().parse().unwrap(),
            trigger_event_id: event_id(),
            attempt: 1,
        },
        model: Text::new("fixture/judge").unwrap(),
        finish_reason: FinishReason::Stop,
        usage: None,
    };
    let s = SurveySetup {
        contest_id,
        question_message_id: event_id().to_string().parse().unwrap(),
        question: Text::new("What would establish the claim?").unwrap(),
        participants: vec![
            "alpha".parse().unwrap(),
            "beta".parse().unwrap(),
            "gamma".parse().unwrap(),
            "delta".parse().unwrap(),
        ],
        answers: vec![],
        collect,
    };
    let item = SurveyBallotItem {
        claim_id: "c1".into(),
        vote: SurveyChoice::Agree,
        confidence: 4,
        reason: "A reproducible observation supports this claim.".into(),
        change_evidence: Some("A contrary measurement".into()),
    };
    let vote = SurveyVote {
        contest_id,
        step_id,
        completed: completed.clone(),
        voter: "alpha".parse().unwrap(),
        ballot: SurveyBallot { votes: vec![item] },
    };
    let map = vec![
        SurveyClaimMap {
            claim_id: "c1".into(),
            first: SurveyTally {
                agree: 2,
                disagree: 1,
                unsure: 1,
                missing: 0,
            },
            final_tally: SurveyTally {
                agree: 3,
                disagree: 1,
                unsure: 0,
                missing: 0,
            },
            not_revoted: vec!["delta".parse().unwrap()],
            switches: vec![SurveySwitch {
                voter: "gamma".parse().unwrap(),
                from: SurveyChoice::Unsure,
                to: SurveyChoice::Agree,
            }],
            bands: vec![
                SurveyBand::Consensus,
                SurveyBand::Contested,
                SurveyBand::LoneDissents,
            ],
        },
        SurveyClaimMap {
            claim_id: "c2".into(),
            first: SurveyTally {
                agree: 0,
                disagree: 0,
                unsure: 3,
                missing: 1,
            },
            final_tally: SurveyTally {
                agree: 0,
                disagree: 0,
                unsure: 3,
                missing: 1,
            },
            not_revoted: vec![],
            switches: vec![],
            bands: vec![],
        },
    ];
    let e = SurveyReportEntry {
        claim_id: "c1".into(),
        explanation: "Reasons on both sides are retained; consensus is not proof.".into(),
        settling_question: "Which independent measurement settles the disagreement?".into(),
    };
    let report = SurveyReport {
        contest_id,
        step_id,
        completed,
        map,
        report: SurveyReportText {
            consensus: vec![e.clone()],
            contested: vec![e.clone()],
            lone_insights: vec![],
            lone_dissents: vec![e],
            rejected_outliers: vec![],
            limitations: "Shared model blind spots; missing ballots remain missing.".into(),
        },
    };
    let mut v = SurveyView::default();
    for p in [
        EnvelopePayload::SurveySetup(s),
        EnvelopePayload::SurveyRevote(vote),
        EnvelopePayload::SurveyReport(report),
    ] {
        v.apply(&p);
    }
    v
}
fn snapshot(width: u16, collect: bool) -> String {
    let v = fixture(collect);
    let mut b = Buffer::empty(Rect::new(0, 0, width, 100));
    let h = survey::render(&v, b.area, &mut b);
    (0..h.min(100))
        .map(|y| {
            (0..width)
                .map(|x| b[(x, y as u16)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
#[test]
fn committed_report_at_both_viewports_and_source_paths() {
    insta::assert_snapshot!("survey_new_80", snapshot(80, true));
    insta::assert_snapshot!("survey_reused_120", snapshot(120, false));
    let text = fixture(false).text();
    for required in [
        "Consensus",
        "Contested",
        "Lone insights",
        "Lone dissents",
        "Rejected outliers",
        "None",
        "c2 · unresolved",
        "UNSURE 3 · MISSING 1",
        "Switch @gamma",
        "Not re-voted",
        "confidence 4/5",
        "Change evidence",
    ] {
        assert!(text.contains(required), "{required}");
    }
    assert!(!text.contains("wins"));
    assert!(!text.contains("winner"));
}
fn committed(payload: EnvelopePayload, n: i64) -> CommittedEnvelope {
    let id = EventId::new();
    CommittedEnvelope {
        base: EnvelopeBase {
            version: 1,
            id,
            room_id: MainRoom::Main,
            correlation_id: id,
            causation_id: None,
            sender: Actor::System,
            recipients: vec![Recipient::Me],
            created_at: std::time::SystemTime::now().into(),
        },
        content: payload,
        commit: CommitFields {
            seq: Cursor::new(n).unwrap(),
            accepted_at: std::time::SystemTime::now().into(),
            commit_index: 0,
            commit_size: 1,
        },
    }
}
#[test]
fn reducer_emits_only_live_committed_cues_and_replay_is_static() {
    let v = fixture(true);
    let mut s = Session::default();
    let mut q = FxQueue::new();
    let setup = committed(v.artifacts[0].clone(), 1);
    s.reduce(
        FeedItem::Committed(setup.clone()),
        FeedOrigin::Live,
        Duration::ZERO,
        &mut q,
    );
    assert_eq!(q.len(), 1);
    s.reduce(
        FeedItem::Committed(setup),
        FeedOrigin::Live,
        Duration::ZERO,
        &mut q,
    );
    assert_eq!(q.len(), 1);
    let report = committed(v.artifacts[2].clone(), 2);
    s.reduce(
        FeedItem::Committed(report.clone()),
        FeedOrigin::Live,
        Duration::ZERO,
        &mut q,
    );
    assert_eq!(q.len(), 2);
    let mut replay = Session::default();
    let mut quiet = FxQueue::new();
    replay.reduce(
        FeedItem::Committed(report),
        FeedOrigin::Replay,
        Duration::ZERO,
        &mut quiet,
    );
    assert!(quiet.is_empty());
    assert_eq!(replay.contests.len(), 1);
}
