use squad_core::orchestrator::crosscheck::*;
use squad_protocol::*;
fn text(s: impl Into<String>) -> Text {
    Text::new(s).unwrap()
}
fn fixture(
    m: usize,
) -> (
    CrosscheckSetup,
    Vec<(EventId, CrosscheckInventory)>,
    CrosscheckNormalization,
    PrivateRouting,
) {
    let setup = CrosscheckSetup {
        contest_id: ContestId::new(),
        plan_id: PlanId::new(),
        plan_version: 1,
        plan: text("A specific plan"),
        participants: (0..m)
            .map(|i| MemberNick::new(&format!("member{i}")).unwrap())
            .collect(),
        goal: Some(CrosscheckGoal {
            goal: text("goal"),
            constraints: vec![],
            uncertainties: vec![],
        }),
    };
    let routing = PrivateRouting::draw();
    let inventories = setup
        .participants
        .iter()
        .enumerate()
        .map(|(i, n)| {
            (
                EventId::new(),
                CrosscheckInventory {
                    contest_id: setup.contest_id,
                    work_id: WorkId::new(),
                    member: n.clone(),
                    lens: routing.lens(i),
                    response: CrosscheckInventoryResponse {
                        items: (0..3)
                            .map(|j| CrosscheckInventoryItem {
                                local_id: text(format!("i{j}")),
                                item: text(format!("case{i}-{j}")),
                                rationale: text("reason"),
                            })
                            .collect(),
                    },
                },
            )
        })
        .collect::<Vec<_>>();
    let items = inventories
        .iter()
        .enumerate()
        .flat_map(|(i, (event, inv))| {
            inv.response
                .items
                .iter()
                .enumerate()
                .map(move |(j, entry)| CrosscheckNormalizedItem {
                    id: text(format!("n{i}-{j}")),
                    lens: inv.lens,
                    item: entry.item.clone(),
                    source_item_ids: vec![CrosscheckSourceItem {
                        inventory_event_id: *event,
                        local_id: entry.local_id.clone(),
                    }],
                })
        })
        .collect::<Vec<_>>();
    let priority_order = candidates(&items)
        .unwrap()
        .into_iter()
        .map(|(key, _)| key)
        .collect();
    (
        setup,
        inventories,
        CrosscheckNormalization {
            items,
            priority_order,
        },
        routing,
    )
}
#[test]
fn default_grid_is_153_and_excludes_every_originator() {
    let (s, i, n, r) = fixture(6);
    let grid = build_grid(&s, n, &i, &r).unwrap();
    assert_eq!(grid.cells.len(), 153);
    assert!(grid.omitted.is_empty());
    for cell in &grid.cells {
        assert!(!cell.originators.contains(cell.reviewer.as_ref().unwrap()));
    }
    for nick in &s.participants {
        let material = review_material(&s, &grid, nick);
        let CrosscheckCaseMaterial::Review { cells, .. } = material else {
            panic!()
        };
        assert!(cells.iter().all(|c| {
            grid.cells
                .iter()
                .any(|g| g.cell_id == c.cell_id && g.reviewer.as_ref() == Some(nick))
        }));
        let json = serde_json::to_string(&cells).unwrap();
        assert!(!json.contains("originator"));
        assert!(!json.contains("reviewer"));
    }
}
#[test]
fn cap_preserves_all_omissions_and_rejects_missing_priorities_and_sources() {
    let (s, i, mut n, r) = fixture(12);
    let grid = build_grid(&s, n.clone(), &i, &r).unwrap();
    assert_eq!(grid.cells.len(), 200);
    assert_eq!(grid.cells.len() + grid.omitted.len(), 576);
    n.priority_order.pop();
    assert!(build_grid(&s, n, &i, &r).is_err());
    let (s, i, mut n, r) = fixture(6);
    n.items[0].source_item_ids.clear();
    assert!(build_grid(&s, n, &i, &r).is_err());
}
#[test]
fn reviewer_failure_never_closes_cells_and_audit_samples_are_unique_and_short() {
    let (s, i, n, r) = fixture(6);
    let grid = build_grid(&s, n, &i, &r).unwrap();
    let cell = &grid.cells[0];
    let review = CrosscheckReview {
        contest_id: s.contest_id,
        work_id: WorkId::new(),
        reviewer: cell.reviewer.clone().unwrap(),
        response: CrosscheckReviewResponse {
            cells: vec![CrosscheckCellReview {
                cell_id: cell.cell_id,
                status: CrosscheckDisposition::Covered,
                citation: Some(CrosscheckCitation {
                    location: text("plan"),
                    quote: text("specific plan"),
                }),
                reason: text("handles the case"),
                proposed_fix: None,
            }],
        },
    };
    let samples = audit_selections(&s.participants, &grid, &[review], r.tie_break_seed);
    assert_eq!(samples.len(), 2);
    assert_ne!(samples[0].auditor, samples[1].auditor);
    assert!(samples.iter().all(|s| s.sample.len() == 1));
    assert_eq!(
        samples,
        audit_selections(&s.participants, &grid, &[], r.tie_break_seed)
            .into_iter()
            .map(|mut s| {
                s.sample = vec![cell.cell_id];
                s
            })
            .collect::<Vec<_>>()
    );
    let ruling = CrosscheckRuling {
        cells: vec![],
        additions: vec![],
        residual_risks: vec![],
    };
    let report = report(&s, &grid, &[], &[], &ruling, &samples);
    assert!(!report.bounded_stop);
    assert!(!report.audit_clean);
    assert_eq!(report.ledger.len(), 153);
}
#[test]
fn lifecycle_fences_terminal_and_out_of_order_moves() {
    let state = CrosscheckState {
        contest_id: ContestId::new(),
        plan_id: PlanId::new(),
        plan_version: 1,
        grid_version: 0,
        state_version: 1,
        status: CrosscheckStatus::Preflight,
        step_id: None,
        terminal_reason: None,
    };
    assert!(advance(&state, &CrosscheckTransition::GridCommitted, false).is_err());
    let stopped = advance(&state, &CrosscheckTransition::Stop, false).unwrap();
    assert!(advance(&stopped, &CrosscheckTransition::Confirm, false).is_err());
    assert_eq!(
        advance(
            &advance(&state, &CrosscheckTransition::Confirm, true).unwrap(),
            &CrosscheckTransition::ParticipantsReady,
            true
        )
        .unwrap()
        .status,
        CrosscheckStatus::Extracting
    );
}
#[test]
fn blind_redaction_uses_unicode_boundaries_and_longest_phrases() {
    use squad_core::orchestrator::crosscheck_redaction::redact;
    let tokens = vec![
        "Claude Opus".into(),
        "claude".into(),
        "anthropic/claude-opus".into(),
        "anthropic".into(),
        "Thoughtful persona".into(),
        "élise".into(),
    ];
    let original =
        "CLAUDE OPUS; anthropic/claude-opus. thoughtful PERSONA! Élise, préélise claudes.";
    assert_eq!(
        redact(original, &tokens),
        "[redacted]; [redacted]. [redacted]! [redacted], préélise claudes."
    );
    assert!(original.contains("CLAUDE OPUS"));
}
#[test]
fn rulings_retain_every_cell_and_audit_additions_cannot_disappear_at_the_cap() {
    for members in [6, 12] {
        let (s, i, n, r) = fixture(members);
        let grid = build_grid(&s, n, &i, &r).unwrap();
        let audits = (0..2)
            .map(|index| CrosscheckAudit {
                contest_id: s.contest_id,
                work_id: WorkId::new(),
                auditor: s.participants[index].clone(),
                sample: vec![],
                response: CrosscheckAuditResponse {
                    audits: vec![],
                    missed_case: CrosscheckMissedCase {
                        case: text("A missed interaction"),
                        reason: text("A material omission"),
                        proposed_fix: text("Add an explicit control"),
                    },
                },
            })
            .collect::<Vec<_>>();
        let mut ruling = CrosscheckRuling {
            cells: grid
                .cells
                .iter()
                .map(|cell| CrosscheckCellReview {
                    cell_id: cell.cell_id,
                    status: CrosscheckDisposition::Gap,
                    citation: None,
                    reason: text("Missing handling"),
                    proposed_fix: Some(text("Add handling")),
                })
                .collect(),
            additions: audits
                .iter()
                .map(|a| CrosscheckAddition {
                    audit_work_id: a.work_id,
                    decision: CrosscheckAdditionDecision::Added,
                    reason: text("Material missing case"),
                })
                .collect(),
            residual_risks: vec![],
        };
        assert!(ruling.validate(&grid, &audits, s.plan.as_str()).is_ok());
        let selections = audits
            .iter()
            .map(|a| AuditSelection {
                auditor: a.auditor.clone(),
                sample: vec![],
            })
            .collect::<Vec<_>>();
        let report = report(&s, &grid, &[], &audits, &ruling, &selections);
        assert!(!report.bounded_stop);
        if members == 6 {
            assert_eq!(report.ledger.len(), 155);
            assert!(report.residual_risks.is_empty());
        } else {
            assert_eq!(report.ledger.len(), 200);
            assert_eq!(report.residual_risks.len(), 2);
        }
        assert!(
            grid.cells
                .iter()
                .all(|c| report.ledger.iter().any(|l| l.cell.cell_id == c.cell_id))
        );
        ruling.cells.pop();
        assert!(ruling.validate(&grid, &audits, s.plan.as_str()).is_err());
    }
}
#[test]
fn explicit_acceptances_can_resolve_audit_additions_but_never_replace_a_missing_audit() {
    let (s, i, n, r) = fixture(6);
    let grid = build_grid(&s, n, &i, &r).unwrap();
    let audits = (0..2)
        .map(|index| CrosscheckAudit {
            contest_id: s.contest_id,
            work_id: WorkId::new(),
            auditor: s.participants[index].clone(),
            sample: vec![],
            response: CrosscheckAuditResponse {
                audits: vec![],
                missed_case: CrosscheckMissedCase {
                    case: text("A material additional case"),
                    reason: text("Missing handling"),
                    proposed_fix: text("Add a control"),
                },
            },
        })
        .collect::<Vec<_>>();
    let ruling = CrosscheckRuling {
        cells: grid
            .cells
            .iter()
            .map(|cell| CrosscheckCellReview {
                cell_id: cell.cell_id,
                status: CrosscheckDisposition::Gap,
                citation: None,
                reason: text("Open"),
                proposed_fix: Some(text("Add a control")),
            })
            .collect(),
        additions: audits
            .iter()
            .map(|a| CrosscheckAddition {
                audit_work_id: a.work_id,
                decision: CrosscheckAdditionDecision::Added,
                reason: text("Required case"),
            })
            .collect(),
        residual_risks: vec![text("A remaining risk")],
    };
    let samples = audits
        .iter()
        .map(|a| AuditSelection {
            auditor: a.auditor.clone(),
            sample: vec![],
        })
        .collect::<Vec<_>>();
    let mut report = report(&s, &grid, &[], &audits, &ruling, &samples);
    assert!(!report.bounded_stop);
    for cell in &mut report.ledger {
        cell.acceptance_event_id = Some(EventId::new());
    }
    for risk in &mut report.residual_risks {
        risk.acceptance_event_id = Some(EventId::new());
    }
    recompute_stop(&mut report, &[]);
    assert!(report.bounded_stop);
    assert!(report.audit_clean);
    report.audits_complete = false;
    report.failed_auditors.push(s.participants[0].clone());
    recompute_stop(&mut report, &[]);
    assert!(!report.bounded_stop);
    assert!(!report.audit_clean);
}
#[test]
fn redaction_includes_unselected_roster_members_and_configured_judge_alias() {
    use squad_core::orchestrator::crosscheck_redaction::{redact, tokens};
    let dir = tempfile::tempdir().unwrap();
    let options = config::LoadOptions {
        env_file: None,
        data_dir: Some(dir.path().join("data")),
        working_dir: dir.path().into(),
        config_dir: None,
        default_data_dir: dir.path().join("data"),
    };
    let mut config = SquadConfig::load_with_env(
        &options,
        &std::collections::BTreeMap::from([("OPENROUTER_API_KEY".into(), "fixture-key".into())]),
    )
    .unwrap();
    config.judge.model = "~vendor/judge-family-latest".into();
    let selected = vec![config.agents[0].nick.clone()];
    let excluded = config.agents[1].nick.to_string();
    let set = tokens(
        Some(&config),
        &selected,
        std::iter::once("vendor/judge-family-2026".into()),
    );
    assert_eq!(
        redact(&format!("{} ~vendor/judge-family-latest", excluded), &set),
        "[redacted] [redacted]"
    );
}
