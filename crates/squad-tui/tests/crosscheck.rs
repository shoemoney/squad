use ratatui::{buffer::Buffer, layout::Rect};
use squad_protocol::*;
use squad_tui::{
    fx::{ColorTier, Palette},
    widgets::crosscheck::*,
};
fn text(s: &str) -> Text {
    Text::new(s).unwrap()
}
#[allow(clippy::field_reassign_with_default)]
fn render(state: CrosscheckStatus) -> String {
    let contest_id = "01900000-0000-7000-8000-000000000001".parse().unwrap();
    let plan_id = "01900000-0000-7000-8000-000000000002".parse().unwrap();
    let cell_id = "550e8400-e29b-41d4-a716-446655440000".parse().unwrap();
    let mut view = CrosscheckView::default();
    view.state = Some(CrosscheckState {
        contest_id,
        plan_id,
        plan_version: 1,
        grid_version: 1,
        state_version: 3,
        status: state,
        step_id: None,
        terminal_reason: None,
    });
    view.setup = Some(CrosscheckSetup {
        contest_id,
        plan_id,
        plan_version: 1,
        plan: text("Deploy one region, then verify."),
        participants: vec![],
        goal: None,
    });
    view.grid = Some(CrosscheckGrid {
        contest_id,
        plan_id,
        plan_version: 1,
        grid_version: 1,
        items: vec![],
        cells: vec![CrosscheckCell {
            cell_id,
            key: "single:a".into(),
            items: vec![text("Rollback during a network partition")],
            originators: vec![],
            reviewer: None,
        }],
        omitted: vec![],
    });
    let mut buffer = Buffer::empty(Rect::new(0, 0, 88, 24));
    squad_tui::widgets::crosscheck::render(
        &view,
        buffer.area,
        &mut buffer,
        &Palette::new(ColorTier::TrueColor),
    );
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
#[test]
fn committed_states() {
    let states = [
        CrosscheckStatus::Preflight,
        CrosscheckStatus::Extracting,
        CrosscheckStatus::Inventorying,
        CrosscheckStatus::BuildingGrid,
        CrosscheckStatus::Reviewing,
        CrosscheckStatus::Auditing,
        CrosscheckStatus::Ruling,
        CrosscheckStatus::Reporting,
        CrosscheckStatus::Complete,
        CrosscheckStatus::Stopped,
        CrosscheckStatus::Failed,
    ];
    insta::assert_snapshot!(
        states
            .into_iter()
            .map(render)
            .collect::<Vec<_>>()
            .join("\n---\n")
    );
}
#[test]
fn evidence_audit_acceptance_and_skipped_review() {
    let contest_id = "01900000-0000-7000-8000-000000000001".parse().unwrap();
    let plan_id = "01900000-0000-7000-8000-000000000002".parse().unwrap();
    let acceptance = "01900000-0000-7000-8000-000000000003".parse().unwrap();
    let ids = (0..4)
        .map(|i| {
            format!("550e8400-e29b-41d4-a716-44665544000{i}")
                .parse::<CellId>()
                .unwrap()
        })
        .collect::<Vec<_>>();
    let cells = ids
        .iter()
        .enumerate()
        .map(|(i, id)| CrosscheckCell {
            cell_id: *id,
            key: format!("single:{i}"),
            items: vec![text("A retained case")],
            originators: vec![],
            reviewer: Some("member0".parse().unwrap()),
        })
        .collect::<Vec<_>>();
    let reviews = ids
        .iter()
        .zip([
            CrosscheckDisposition::Covered,
            CrosscheckDisposition::Inapplicable,
            CrosscheckDisposition::Gap,
        ])
        .map(|(id, status)| CrosscheckCellReview {
            cell_id: *id,
            status,
            citation: (status == CrosscheckDisposition::Covered).then(|| CrosscheckCitation {
                location: text("Section 2"),
                quote: text("roll back immediately"),
            }),
            reason: text("A concrete evidence-based explanation"),
            proposed_fix: (status == CrosscheckDisposition::Gap)
                .then(|| text("Add a rollback control")),
        })
        .collect::<Vec<_>>();
    let auditor: MemberNick = "member1".parse().unwrap();
    let audit = CrosscheckAudit {
        contest_id,
        work_id: "01900000-0000-7000-8000-000000000004".parse().unwrap(),
        auditor: auditor.clone(),
        sample: vec![ids[0]],
        response: CrosscheckAuditResponse {
            audits: vec![CrosscheckAuditFinding {
                cell_id: ids[0],
                verdict: CrosscheckAuditVerdict::Challenged,
                reason: text("Citation does not cover the interaction"),
                citation: None,
            }],
            missed_case: CrosscheckMissedCase {
                case: text("A concurrent outage"),
                reason: text("Two regions may fail together"),
                proposed_fix: text("Add a joint rollback test"),
            },
        },
    };
    let report = CrosscheckReport {
        contest_id,
        plan_id,
        plan_version: 1,
        grid_version: 1,
        ledger: cells
            .iter()
            .enumerate()
            .map(|(i, cell)| CrosscheckLedgerCell {
                cell: cell.clone(),
                review: reviews.get(i).cloned(),
                challenges: vec![],
                ruling: None,
                acceptance_event_id: (i == 2).then_some(acceptance),
            })
            .collect(),
        omitted: vec![],
        residual_risks: vec![],
        failed_inventories: vec![],
        failed_reviewers: vec!["member2".parse().unwrap()],
        failed_auditors: vec![],
        audits_complete: false,
        audit_clean: false,
        bounded_stop: false,
    };
    let mut view = CrosscheckView {
        grid: Some(CrosscheckGrid {
            contest_id,
            plan_id,
            plan_version: 1,
            grid_version: 1,
            items: vec![],
            cells,
            omitted: vec![],
        }),
        reviews: reviews.iter().map(|r| (r.cell_id, r.clone())).collect(),
        audits: std::collections::BTreeMap::from([(auditor, audit)]),
        report: Some(report),
        acceptances: vec![CrosscheckAcceptance {
            contest_id,
            plan_id,
            plan_version: 1,
            grid_version: 1,
            expected_state_version: 3,
            cell_ids: vec![ids[2]],
            residual_risk_ids: vec![],
            reason: text("Explicitly accept this specific risk"),
        }],
        ..Default::default()
    };
    let render = |v: &CrosscheckView| {
        v.lines()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    };
    insta::assert_snapshot!(render(&view));
    view.toggle_cell(ids[0]);
    assert!(!render(&view).contains("Citation Section 2"));
    view.toggle_cell(ids[0]);
    assert!(render(&view).contains("Citation Section 2"));
    use squad_tui::{
        fx::{FeedOrigin, FxQueue},
        session::Session,
    };
    let setup = CrosscheckSetup {
        contest_id,
        plan_id,
        plan_version: 1,
        plan: text("roll back immediately"),
        participants: ["member0", "member1", "member2"]
            .into_iter()
            .map(|n| n.parse().unwrap())
            .collect(),
        goal: Some(CrosscheckGoal {
            goal: text("Keep service available"),
            constraints: vec![],
            uncertainties: vec![],
        }),
    };
    let state = CrosscheckState {
        contest_id,
        plan_id,
        plan_version: 1,
        grid_version: 1,
        state_version: 4,
        status: CrosscheckStatus::Complete,
        step_id: None,
        terminal_reason: None,
    };
    let review = CrosscheckReview {
        contest_id,
        work_id: WorkId::new(),
        reviewer: "member0".parse().unwrap(),
        response: CrosscheckReviewResponse {
            cells: reviews.clone(),
        },
    };
    let payloads = vec![
        EnvelopePayload::CrosscheckSetup(setup),
        EnvelopePayload::CrosscheckGrid(view.grid.clone().unwrap()),
        EnvelopePayload::ContestArtifact(ContestArtifact {kind: None, round: None, actor: None, audience: None, body: None, 
            contest_id,
            step_id: StepId::new(),
            artifact_kind: text("crosscheck.cell"),
            work_id: Some(review.work_id),
            source_refs: vec![],
            data: serde_json::to_value(&review.response).unwrap(),
        }),
        EnvelopePayload::CrosscheckReview(review),
        EnvelopePayload::CrosscheckAcceptance(view.acceptances[0].clone()),
        EnvelopePayload::CrosscheckReport(view.report.clone().unwrap()),
        EnvelopePayload::CrosscheckState(state),
    ];
    let now: DateTime = std::time::SystemTime::now().into();
    let events = payloads
        .into_iter()
        .enumerate()
        .map(|(i, content)| CommittedEnvelope {
            base: EnvelopeBase {
                version: 1,
                id: EventId::new(),
                room_id: MainRoom::Main,
                correlation_id: EventId::new(),
                causation_id: None,
                sender: if matches!(content, EnvelopePayload::CrosscheckAcceptance(_)) {
                    Actor::Me
                } else {
                    Actor::System
                },
                recipients: vec![Recipient::Me],
                created_at: now,
            },
            content,
            commit: CommitFields {
                seq: Cursor::new(i as i64 + 1).unwrap(),
                accepted_at: now,
                commit_index: 0,
                commit_size: 1,
            },
        })
        .collect::<Vec<_>>();
    for origin in [FeedOrigin::Replay, FeedOrigin::Live] {
        let mut session = Session::default();
        let mut cues = FxQueue::new();
        for event in &events {
            session.reduce(
                FeedItem::Committed(event.clone()),
                origin,
                std::time::Duration::ZERO,
                &mut cues,
            );
        }
        assert_eq!(cues.len(), if origin == FeedOrigin::Live { 3 } else { 0 });
        let cue_count = cues.len();
        session.reduce(
            FeedItem::Committed(events[5].clone()),
            origin,
            std::time::Duration::ZERO,
            &mut cues,
        );
        assert_eq!(cues.len(), cue_count);
        assert_eq!(
            session.contests[&contest_id]
                .crosscheck
                .report
                .as_ref()
                .unwrap()
                .ledger
                .len(),
            4
        );
        let raw = format!("/accept {} -- Explicitly accept the open case", ids[3]);
        let meta = parser::SubmissionMeta::default();
        let parsed = parse(&raw, &meta, &session.snapshot.context).unwrap();
        let request = Request::from_parsed_with_context(
            EventId::new(),
            parsed,
            &meta,
            &session.snapshot.context,
            false,
        )
        .unwrap();
        assert!(
            matches!(request,Request::CrosscheckAccept{expected_state_version:4,cell_ids,..} if cell_ids==vec![ids[3]])
        );
    }
}
