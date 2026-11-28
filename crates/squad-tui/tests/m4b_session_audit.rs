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

#[tokio::test]
async fn census_pass_and_plan_references_survive_restore_and_duplicate_replay() {
    let room = FakeRoom::new();
    let contest = ContestId::new();
    let mut sources = vec![];
    for body in ["Deploy the database", "Add a rollback rehearsal"] {
        sources.push(room.commit(
            Actor::Me,
            EnvelopePayload::UserMessage(UserMessage {
                input: UserMessageInput {
                    message_id: MessageId::new(),
                    text: Text::new(format!("@alpha {body}")).unwrap(),
                    quote: None,
                    reply_to: None,
                },
                body: body.into(),
                resolved_quotes: vec![],
                debate_id: None,
                case_id: None,
                contest_id: None,
            }),
        ));
    }
    let initial_plan = CensusPlan {
        contest_id: contest,
        version: 1,
        text: Text::new("Deploy the database").unwrap(),
        source_event_ids: vec![sources[0]],
    };
    let setup = CensusSetup {
        contest_id: contest,
        plan: initial_plan.clone(),
        participants: ["gamma", "alpha", "beta"]
            .map(|nick| nick.parse().unwrap())
            .to_vec(),
        passes: 3,
        room_words: 250,
        plan_words: 500,
        models: ["gamma", "alpha", "beta", "judge"]
            .map(|nick| {
                (
                    nick.into(),
                    ExecutorBinding {
                        model: Text::new("fixture/model").unwrap(),
                        effort: ReasoningEffort::Low,
                    },
                )
            })
            .into(),
        prompt_refs: Default::default(),
        redaction_tokens: vec![],
    };
    setup.validate().unwrap();
    room.commit(Actor::System, EnvelopePayload::CensusSetup(setup.clone()));
    let initial_plan_event = room.commit(
        Actor::System,
        EnvelopePayload::CensusPlan(initial_plan.clone()),
    );
    let first_hunt = CensusHunt {
        contest_id: contest,
        pass: 1,
        plan_version: 1,
        work_id: WorkId::new(),
        member: "gamma".parse().unwrap(),
        lens: Text::new("hidden assumptions").unwrap(),
        output: CensusHuntOutput { gaps: vec![] },
    };
    let first_hunt_event = room.commit(
        Actor::System,
        EnvelopePayload::CensusHunt(first_hunt.clone()),
    );
    let first_state = CensusState {
        contest_id: contest,
        state_version: 6,
        status: CensusStatus::WaitingForPlan,
        pass: 1,
        plan_version: 1,
        step_id: Some(StepId::new()),
        artifact_event_ids: vec![initial_plan_event, first_hunt_event],
        unfinished_work_ids: vec![],
        terminal_reason: None,
    };
    first_state.validate().unwrap();
    room.commit(Actor::System, EnvelopePayload::CensusState(first_state));
    let revised_plan = CensusPlan {
        contest_id: contest,
        version: 2,
        text: Text::new("Deploy the database after a rollback rehearsal").unwrap(),
        // Preserve committed source order, rather than sorting or selecting again.
        source_event_ids: vec![sources[1], sources[0]],
    };
    let revised_plan_event =
        room.commit(Actor::Me, EnvelopePayload::CensusPlan(revised_plan.clone()));
    let second_hunt = CensusHunt {
        pass: 2,
        plan_version: 2,
        work_id: WorkId::new(),
        member: "beta".parse().unwrap(),
        lens: Text::new("dependencies and sequencing").unwrap(),
        ..first_hunt.clone()
    };
    let second_hunt_event = room.commit(
        Actor::System,
        EnvelopePayload::CensusHunt(second_hunt.clone()),
    );
    let current_state = CensusState {
        contest_id: contest,
        state_version: 8,
        status: CensusStatus::Hunting,
        pass: 2,
        plan_version: 2,
        step_id: Some(StepId::new()),
        artifact_event_ids: vec![revised_plan_event, second_hunt_event],
        unfinished_work_ids: vec![WorkId::new()],
        terminal_reason: None,
    };
    current_state.validate().unwrap();
    room.commit(
        Actor::System,
        EnvelopePayload::CensusState(current_state.clone()),
    );

    // Obtain the precondition through the shared room path. Never seed the
    // session's census views or construct a replacement RoomSnapshot here.
    let snapshot = room.state().await;
    assert_eq!(
        snapshot.context.active_contest,
        Some(ContestState::Census(current_state.clone()))
    );
    let assert_restored = |session: &Session| {
        let view = &session.census[&contest];
        assert_eq!(view.setup.as_ref(), Some(&setup));
        assert_eq!(view.state.as_ref().unwrap().pass, 2);
        assert_eq!(view.state.as_ref(), Some(&current_state));
        assert_eq!(view.plans, vec![initial_plan.clone(), revised_plan.clone()]);
        assert_eq!(view.hunts, vec![first_hunt.clone(), second_hunt.clone()]);
        assert_eq!(
            session.contests[&contest].state,
            Some(ContestState::Census(current_state.clone()))
        );
        assert_eq!(
            session.snapshot.context.active_contest,
            snapshot.context.active_contest
        );
        assert_eq!(session.snapshot.context.records, snapshot.context.records);
        assert_eq!(session.snapshot.cursor, snapshot.cursor);
        assert_eq!(session.snapshot.events, snapshot.events);
        let plan_records = session
            .snapshot
            .context
            .records
            .iter()
            .filter(|record| record.kind.as_str() == "census.plan")
            .map(|record| (record.event_id, record.content.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(
            plan_records,
            vec![
                (initial_plan_event, initial_plan.text.as_str()),
                (revised_plan_event, revised_plan.text.as_str()),
            ]
        );
        for source in &view.plans[1].source_event_ids {
            assert!(
                session
                    .snapshot
                    .events
                    .iter()
                    .any(|event| event.base.id == *source)
            );
        }
    };
    let mut session = Session::default();
    for _ in 0..2 {
        session.restore(snapshot.clone());
        assert_restored(&session);
        let mut queue = FxQueue::new();
        for origin in [FeedOrigin::Replay, FeedOrigin::Live] {
            for event in &snapshot.events {
                assert!(!session.reduce(
                    FeedItem::Committed(event.clone()),
                    origin,
                    Duration::ZERO,
                    &mut queue,
                ));
            }
            assert_restored(&session);
            assert!(queue.is_empty());
        }
    }
    assert_eq!(room.state().await.events, snapshot.events);
}
