use squad_core::export::{ExportError, ExportSnapshot, render};
use squad_protocol::*;
use std::collections::BTreeMap;
fn text(s: &str) -> Text {
    Text::new(s).unwrap()
}
fn event_id(n: u32) -> EventId {
    format!("019b0000-0000-7000-8000-{n:012}").parse().unwrap()
}
fn cell(n: u32) -> CellId {
    format!("00000000-0000-4000-8000-{n:012}").parse().unwrap()
}
fn completed(n: u32) -> CompletedGeneration {
    CompletedGeneration {
        generation: GenerationIdentity {
            work_id: event_id(n).to_string().parse().unwrap(),
            message_id: event_id(n + 100).to_string().parse().unwrap(),
            trigger_event_id: event_id(1),
            attempt: 1,
        },
        model: text("fixture/member"),
        finish_reason: FinishReason::Stop,
        usage: None,
    }
}
#[test]
fn committed_probe_export_snapshot_and_scope_matching() {
    let contest = event_id(1).to_string().parse().unwrap();
    let step = event_id(2).to_string().parse().unwrap();
    let nicks: Vec<MemberNick> = ["alpha", "beta", "gamma", "delta"]
        .map(|s| s.parse().unwrap())
        .into();
    let setup = ProbeSetup {
        contest_id: contest,
        brief: ProbeBrief {
            observation: text("Noise only during charging"),
            ruled_out: "Cooling fan".into(),
            inspectable: text("External indicator only"),
        },
        participants: nicks.clone(),
        explainers: nicks[..2].to_vec(),
        checkers: nicks[2..].to_vec(),
        sitting_out: Some("epsilon".parse().unwrap()),
    };
    let explanations = vec![
        ProbeItem {
            id: cell(1),
            data: ProbeExplanation {
                explanation: text("Loose connector"),
                key_assumption: text("Charging vibrates the connector"),
                limitations: vec![text("Not observed")],
            },
        },
        ProbeItem {
            id: cell(2),
            data: ProbeExplanation {
                explanation: text("Regulator vibration"),
                key_assumption: text("Regulator changes indicator state"),
                limitations: vec![],
            },
        },
    ];
    let check = ProbeCheck {
        check: text("Read indicator"),
        procedure: text("Observe the outside indicator without opening the case"),
        measurement: text("Indicator color"),
        outcomes: vec![
            ProbeOutcome {
                id: text("red"),
                description: text("Red indicator"),
            },
            ProbeOutcome {
                id: text("green"),
                description: text("Green indicator"),
            },
        ],
        effort: text("One minute"),
        safety_notes: vec![text("Do not open or modify the case")],
    };
    let matrix: BTreeMap<_, _> = [(cell(1), "red"), (cell(2), "green")]
        .map(|(id, outcome)| {
            (
                id,
                Some(ProbePrediction {
                    predictions: vec![ProbePredictionItem {
                        check_id: cell(3),
                        predictable: true,
                        expected_outcome_ids: vec![text(outcome)],
                        expected_observation: Some(text(outcome)),
                        reason: text("Conditional on the explanation"),
                    }],
                }),
            )
        })
        .into();
    let material = ProbeCaseMaterial {
        brief: setup.brief.clone(),
        explanations: explanations.clone(),
        checks: vec![ProbeItem {
            id: cell(3),
            data: check.clone(),
        }],
        prediction_matrix: matrix.clone(),
        missing_rows: vec![],
        skipped_members: vec![nicks[3].clone()],
    };
    let report = ProbeReport {
        prediction_matrix: matrix,
        decision_tree: ProbeTree {
            root: text("root"),
            nodes: BTreeMap::from([
                (
                    "root".into(),
                    ProbeNode::Check {
                        check_id: cell(3),
                        measurement: check.measurement.clone(),
                        branches: BTreeMap::from([
                            ("red".into(), text("red")),
                            ("green".into(), text("green")),
                        ]),
                        unexpected: text("unexpected"),
                    },
                ),
                (
                    "red".into(),
                    ProbeNode::Leaf {
                        remaining: vec![cell(1)],
                        next_action: text("Inspect the connector within the brief's limits"),
                        reassess: false,
                    },
                ),
                (
                    "green".into(),
                    ProbeNode::Leaf {
                        remaining: vec![cell(2)],
                        next_action: text("Review regulator documentation"),
                        reassess: false,
                    },
                ),
                (
                    "unexpected".into(),
                    ProbeNode::Leaf {
                        remaining: vec![cell(1), cell(2)],
                        next_action: text("Reopen the brief; neither predicted result occurred"),
                        reassess: true,
                    },
                ),
            ]),
        },
        discarded_checks: vec![],
        inseparable_groups: vec![],
        limitations: vec![text("Checks have not been executed")],
    };
    report.validate_against(&material).unwrap();
    let mut payloads = vec![
        (Actor::System, EnvelopePayload::ProbeSetup(setup)),
        (
            Actor::System,
            EnvelopePayload::ProbeState(ProbeState {
                contest_id: contest,
                state: ProbeStatus::Complete,
                state_version: 7,
                step_id: None,
                artifact_event_ids: vec![],
                skipped_work_ids: vec![],
                skipped_members: vec![nicks[3].clone()],
                terminal_reason: None,
            }),
        ),
    ];
    for (i, e) in explanations.iter().enumerate() {
        payloads.push((
            Actor::Member(nicks[i].clone()),
            EnvelopePayload::ProbeExplanation(ProbeOutput {
                contest_id: contest,
                step_id: step,
                completed: completed(i as u32 + 10),
                data: e.data.clone(),
            }),
        ));
    }
    payloads.push((
        Actor::Member(nicks[2].clone()),
        EnvelopePayload::ProbeCheck(ProbeOutput {
            contest_id: contest,
            step_id: step,
            completed: completed(12),
            data: check,
        }),
    ));
    payloads.push((
        Actor::System,
        EnvelopePayload::CaseOpened(CaseOpened {
            case_id: event_id(30).to_string().parse().unwrap(),
            judge_invocation_id: event_id(31),
            work_id: None,
            judge_model: text("fixture/judge"),
            material: CaseMaterial::Probe(material),
            debate_id: None,
            contest_id: Some(contest),
        }),
    ));
    payloads.push((
        Actor::Judge,
        EnvelopePayload::ProbeReport(ProbeOutput {
            contest_id: contest,
            step_id: step,
            completed: completed(13),
            data: report,
        }),
    ));
    let snapshot = ExportSnapshot {
        events: payloads
            .into_iter()
            .enumerate()
            .map(|(i, (sender, content))| CommittedEnvelope {
                base: EnvelopeBase {
                    version: 1,
                    id: event_id(i as u32 + 50),
                    room_id: MainRoom::Main,
                    correlation_id: event_id(1),
                    causation_id: None,
                    sender,
                    recipients: vec![Recipient::Me],
                    created_at: "2026-11-25T00:00:00Z".parse().unwrap(),
                },
                content,
                commit: CommitFields {
                    seq: Cursor::new(i as i64 + 1).unwrap(),
                    accepted_at: "2026-11-25T00:00:00Z".parse().unwrap(),
                    commit_index: 0,
                    commit_size: 1,
                },
            })
            .collect(),
        ..Default::default()
    };
    insta::assert_snapshot!(render(&snapshot, ExportScope::Probe, Some(contest)).unwrap());
    assert_eq!(
        render(&snapshot, ExportScope::Debate, Some(contest)),
        Err(ExportError::ScopeMismatch)
    );
    assert_eq!(
        render(&snapshot, ExportScope::Probe, None),
        Err(ExportError::ContestIdRequired)
    );
}
