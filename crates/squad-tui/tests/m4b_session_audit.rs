use squad_protocol::*;
use squad_tui::{
    fx::{FeedOrigin, FxQueue},
    session::{FakeRoom, RoomPort, Session},
};
use std::time::Duration;

#[tokio::test]
async fn typed_role_audit_and_acceptance_records_survive_restore_and_duplicate_replay() {
    let room = FakeRoom::new();
    let probe = ContestId::new();
    let crosscheck = ContestId::new();
    let plan = PlanId::new();
    let cell: CellId = "00000000-0000-4000-8000-000000000001".parse().unwrap();
    let auditor: MemberNick = "beta".parse().unwrap();
    let setup = ProbeSetup {
        contest_id: probe,
        brief: ProbeBrief {
            observation: Text::new("Noise during charging").unwrap(),
            ruled_out: "Fan".into(),
            inspectable: Text::new("Indicator").unwrap(),
        },
        participants: ["alpha", "beta", "gamma", "delta"]
            .map(|n| n.parse().unwrap())
            .to_vec(),
        explainers: ["alpha", "beta"].map(|n| n.parse().unwrap()).to_vec(),
        checkers: ["gamma", "delta"].map(|n| n.parse().unwrap()).to_vec(),
        sitting_out: Some("epsilon".parse().unwrap()),
    };
    setup.validate().unwrap();
    room.commit(Actor::System, EnvelopePayload::ProbeSetup(setup.clone()));
    let audit = CrosscheckAudit {
        contest_id: crosscheck,
        work_id: WorkId::new(),
        auditor: auditor.clone(),
        sample: vec![cell],
        response: CrosscheckAuditResponse {
            audits: vec![CrosscheckAuditFinding {
                cell_id: cell,
                verdict: CrosscheckAuditVerdict::Challenged,
                reason: Text::new("Citation misses a joint failure").unwrap(),
                citation: None,
            }],
            missed_case: CrosscheckMissedCase {
                case: Text::new("Two regions fail").unwrap(),
                reason: Text::new("Shared dependency").unwrap(),
                proposed_fix: Text::new("Test joint rollback").unwrap(),
            },
        },
    };
    room.commit(
        Actor::System,
        EnvelopePayload::CrosscheckAudit(audit.clone()),
    );
    let acceptance = CrosscheckAcceptance {
        contest_id: crosscheck,
        plan_id: plan,
        plan_version: 1,
        grid_version: 1,
        expected_state_version: 3,
        cell_ids: vec![cell],
        residual_risk_ids: vec![],
        reason: Text::new("Accept this specific residual risk").unwrap(),
    };
    room.commit(
        Actor::Me,
        EnvelopePayload::CrosscheckAcceptance(acceptance.clone()),
    );
    let snapshot = room.state().await;
    let mut session = Session::default();
    session.restore(snapshot.clone());
    let mut queue = FxQueue::new();
    for event in &snapshot.events {
        assert!(!session.reduce(
            FeedItem::Committed(event.clone()),
            FeedOrigin::Live,
            Duration::ZERO,
            &mut queue
        ));
    }
    assert!(queue.is_empty());
    assert_eq!(session.probes[&probe].setup, Some(setup));
    assert_eq!(
        session.contests[&crosscheck].crosscheck.audits[&auditor],
        audit
    );
    assert_eq!(
        session.contests[&crosscheck].crosscheck.acceptances,
        vec![acceptance]
    );
    session.restore(snapshot);
    assert_eq!(
        session.contests[&crosscheck].crosscheck.acceptances.len(),
        1
    );
    assert_eq!(session.contests[&crosscheck].crosscheck.audits.len(), 1);
    assert_eq!(
        session.probes[&probe].setup.as_ref().unwrap().explainers[0].as_str(),
        "alpha"
    );
}
