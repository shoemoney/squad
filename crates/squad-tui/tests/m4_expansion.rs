use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};
use squad_protocol::*;
use squad_tui::{
    app::App,
    fx::TerminalCaps,
    ui,
    widgets::{
        grill::{Exchange, GrillView, PlanVersion, Turn, TurnContent},
        tournament::{Entry, Ranking, Round, TournamentView},
    },
};
use std::time::Duration;

fn app() -> App {
    let caps = TerminalCaps::resolve(
        |k| (k == "SQUAD_FX").then(|| "off".into()),
        true,
        (100, 40).into(),
    )
    .unwrap();
    App::new(caps)
}
fn draw(app: &mut App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(100, 40)).unwrap();
    terminal
        .draw(|f| ui::render(app, f, Duration::ZERO))
        .unwrap();
    let b = terminal.backend().buffer();
    (0..40)
        .map(|y| {
            (0..100)
                .map(|x| b[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn f7(app: &mut App) {
    app.handle(
        Event::Key(KeyEvent::new(KeyCode::F(7), KeyModifiers::NONE)),
        Duration::ZERO,
    );
}
fn identity(id: ContestId) -> MessageId {
    id.to_string().parse().unwrap()
}

#[test]
fn tournament_rankings_expand_through_the_presentation_cache() {
    let mut app = app();
    let id = ContestId::new();
    let mut view = TournamentView::new("Which database?", vec![]);
    view.state = "RUNNING".into();
    view.rounds = vec![Round {
        number: 1,
        remaining: 4,
        expanded: false,
        replay: None,
        entries: vec![Entry {
            nick: "claude".into(),
            text: "Postgres, obviously".into(),
            status: "complete".into(),
        }],
        ranking: vec![Ranking {
            nick: "claude".into(),
            score: 9,
            critique: "Sharp and well argued".into(),
            eliminated: false,
        }],
    }];
    app.session.tournaments.insert(id, view);
    app.session.snapshot.events.clear();
    let collapsed = draw(&mut app);
    assert!(!collapsed.contains("Sharp and well argued"), "{collapsed}");
    let rows = app.composed.transcript.total_rows();
    f7(&mut app);
    let expanded = draw(&mut app);
    assert!(expanded.contains("Sharp and well argued"), "{expanded}");
    assert!(app.composed.transcript.total_rows() > rows);
    let key = &app
        .composed
        .transcript
        .message(identity(id))
        .unwrap()
        .presentation;
    assert!(key.rankings_expanded && key.expanded);
    assert!(app.composed.anchor.is_some(), "toggle keeps an anchor");
    f7(&mut app);
    assert!(!draw(&mut app).contains("Sharp and well argued"));
    assert!(
        !app.composed
            .transcript
            .message(identity(id))
            .unwrap()
            .presentation
            .rankings_expanded
    );
}

#[test]
fn grill_plans_and_appendix_expand_through_the_presentation_cache() {
    let mut app = app();
    let id = ContestId::new();
    let mut view = GrillView::new("claude", vec!["grok".into()]);
    view.state = "RUNNING".into();
    view.turns = vec![Turn {
        round: 1,
        griller: "grok".into(),
        angle: "risk".into(),
        status: "complete".into(),
        content: TurnContent::Questions(vec![Exchange {
            question: "What breaks first?".into(),
            answer: "The cache".into(),
            plan_changes: "add eviction".into(),
        }]),
    }];
    view.plans = vec![
        PlanVersion {
            version: 1,
            text: "OLD PLAN TEXT".into(),
            expanded: None,
        },
        PlanVersion {
            version: 2,
            text: "NEW PLAN TEXT".into(),
            expanded: None,
        },
    ];
    app.session.grills.insert(id, view);
    let collapsed = draw(&mut app);
    assert!(collapsed.contains("NEW PLAN TEXT"), "{collapsed}");
    assert!(!collapsed.contains("OLD PLAN TEXT"));
    f7(&mut app);
    let expanded = draw(&mut app);
    assert!(expanded.contains("OLD PLAN TEXT"), "{expanded}");
    let key = &app
        .composed
        .transcript
        .message(identity(id))
        .unwrap()
        .presentation;
    assert!(key.appendix_expanded && key.rankings_expanded);
}

fn report_text(value: impl AsRef<str>) -> Text {
    Text::new(value.as_ref()).unwrap()
}

// Exercise the actual mode widgets, rather than stand-in Markdown tables.
fn large_report(kind: &str, id: ContestId) -> App {
    use squad_tui::{
        session::ContestView,
        widgets::{
            census::CensusView, crosscheck::CrosscheckView, probe::ProbeView, survey::SurveyView,
        },
    };
    let mut app = app();
    app.composed.verdict_expanded = true;
    let cell = |i: usize| {
        format!("00000000-0000-4000-8000-{i:012}")
            .parse::<CellId>()
            .unwrap()
    };
    let marker = |i: usize| format!("navrow{i:03}");
    match kind {
        "ledger" => {
            app.session.census.insert(
                id,
                CensusView {
                    reports: vec![CensusReport {
                        contest_id: id,
                        pass: 1,
                        plan_version: 1,
                        gaps: (0..48)
                            .map(|i| CensusGapOutcome {
                                gap: CensusGap {
                                    id: report_text(format!("g{i}")),
                                    omission: report_text(marker(i)),
                                    consequence: report_text("Release fails"),
                                    lenses: vec![report_text("operations")],
                                    source_find_ids: vec![],
                                },
                                disposition: CensusDisposition::Confirmed,
                                votes: 2,
                                covered: 0,
                                trivial: 0,
                                finders: vec!["alpha".parse().unwrap(), "beta".parse().unwrap()],
                                finder_count: 2,
                                priority: None,
                                citations: vec![],
                            })
                            .collect(),
                        additions: vec![],
                        missing: vec![],
                        input_gap_ids: vec![],
                        f1: 0,
                        f2: 48,
                        formula_branch: report_text("positive_doubletons"),
                        unseen: 0.,
                        estimate_low: None,
                        estimate_high: None,
                        discovery: vec![],
                    }],
                    ..Default::default()
                },
            );
        }
        "grid" => {
            let plan_id = PlanId::new();
            let cells: Vec<_> = (0..48)
                .map(|i| CrosscheckCell {
                    cell_id: cell(i),
                    key: format!("single:{i}"),
                    items: vec![report_text(marker(i))],
                    originators: vec![],
                    reviewer: None,
                })
                .collect();
            app.session.contests.insert(
                id,
                ContestView {
                    crosscheck: CrosscheckView {
                        state: Some(CrosscheckState {
                            contest_id: id,
                            plan_id,
                            plan_version: 1,
                            grid_version: 1,
                            state_version: 1,
                            status: CrosscheckStatus::Complete,
                            step_id: None,
                            terminal_reason: None,
                        }),
                        grid: Some(CrosscheckGrid {
                            contest_id: id,
                            plan_id,
                            plan_version: 1,
                            grid_version: 1,
                            items: vec![],
                            cells: cells.clone(),
                            omitted: vec![],
                        }),
                        report: Some(CrosscheckReport {
                            contest_id: id,
                            plan_id,
                            plan_version: 1,
                            grid_version: 1,
                            ledger: cells
                                .into_iter()
                                .map(|cell| CrosscheckLedgerCell {
                                    review: Some(CrosscheckCellReview {
                                        cell_id: cell.cell_id,
                                        status: CrosscheckDisposition::Gap,
                                        citation: None,
                                        reason: report_text("Missing failure evidence"),
                                        proposed_fix: Some(report_text("Add a failure test")),
                                    }),
                                    cell,
                                    challenges: vec![],
                                    ruling: None,
                                    acceptance_event_id: None,
                                })
                                .collect(),
                            omitted: vec![],
                            residual_risks: vec![],
                            failed_inventories: vec![],
                            failed_reviewers: vec![],
                            failed_auditors: vec![],
                            audits_complete: true,
                            audit_clean: true,
                            bounded_stop: false,
                        }),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            );
        }
        "map" => {
            let entries: Vec<_> = (0..48)
                .map(|i| SurveyReportEntry {
                    claim_id: format!("c{i}"),
                    explanation: marker(i),
                    settling_question: "Verify independently".into(),
                })
                .collect();
            let report = SurveyReport {
                contest_id: id,
                step_id: StepId::new(),
                completed: CompletedGeneration {
                    generation: GenerationIdentity {
                        message_id: MessageId::new(),
                        work_id: WorkId::new(),
                        trigger_event_id: EventId::new(),
                        attempt: 1,
                    },
                    model: report_text("fixture/judge"),
                    finish_reason: FinishReason::Stop,
                    usage: None,
                },
                map: (0..48)
                    .map(|i| SurveyClaimMap {
                        claim_id: format!("c{i}"),
                        first: SurveyTally {
                            agree: 2,
                            disagree: 0,
                            unsure: 0,
                            missing: 0,
                        },
                        final_tally: SurveyTally {
                            agree: 2,
                            disagree: 0,
                            unsure: 0,
                            missing: 0,
                        },
                        not_revoted: vec![],
                        switches: vec![],
                        bands: vec![SurveyBand::Consensus],
                    })
                    .collect(),
                completeness: None,
                report: SurveyReportText {
                    consensus: entries[..10].to_vec(),
                    contested: entries[10..20].to_vec(),
                    lone_insights: entries[20..30].to_vec(),
                    lone_dissents: entries[30..40].to_vec(),
                    rejected_outliers: entries[40..].to_vec(),
                    limitations: "No independent observations".into(),
                },
            };
            app.session.contests.insert(
                id,
                ContestView {
                    survey: Some(SurveyView {
                        artifacts: vec![EnvelopePayload::SurveyReport(report)],
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            );
        }
        "tree" | "matrix" => {
            let mut nodes = std::collections::BTreeMap::new();
            nodes.insert(
                "root".into(),
                ProbeNode::Check {
                    check_id: cell(100),
                    measurement: report_text("Observe indicator"),
                    branches: (0..48)
                        .map(|i| (format!("outcome{i:03}"), report_text(format!("leaf{i:03}"))))
                        .collect(),
                    unexpected: report_text("unexpected"),
                },
            );
            for i in 0..48 {
                nodes.insert(
                    format!("leaf{i:03}"),
                    ProbeNode::Leaf {
                        remaining: vec![cell(i)],
                        next_action: report_text(if kind == "tree" {
                            marker(i)
                        } else {
                            "Inspect".into()
                        }),
                        reassess: false,
                    },
                );
            }
            nodes.insert(
                "unexpected".into(),
                ProbeNode::Leaf {
                    remaining: vec![],
                    next_action: report_text("Reassess"),
                    reassess: true,
                },
            );
            app.session.probes.insert(
                id,
                ProbeView {
                    banner_mode: Some(squad_core::banner::BannerMode::Off),
                    matrix_collapsed: kind == "tree",
                    report: Some(ProbeReport {
                        decision_tree: ProbeTree {
                            root: report_text("root"),
                            nodes,
                        },
                        prediction_matrix: if kind == "matrix" {
                            (0..48)
                                .map(|i| {
                                    (
                                        cell(i),
                                        Some(ProbePrediction {
                                            predictions: vec![ProbePredictionItem {
                                                check_id: cell(100),
                                                predictable: true,
                                                expected_outcome_ids: vec![report_text(format!(
                                                    "outcome{i:03}"
                                                ))],
                                                expected_observation: Some(report_text(marker(i))),
                                                reason: report_text("Conditional prediction"),
                                            }],
                                        }),
                                    )
                                })
                                .collect()
                        } else {
                            Default::default()
                        },
                        discarded_checks: vec![],
                        inseparable_groups: vec![],
                        limitations: vec![],
                    }),
                    ..Default::default()
                },
            );
        }
        _ => panic!("unknown report kind {kind}"),
    }
    // Let the real reducer/export path build the searchable report record.
    let payload = match kind {
        "ledger" => EnvelopePayload::CensusReport(app.session.census[&id].reports[0].clone()),
        "grid" => EnvelopePayload::CrosscheckReport(
            app.session.contests[&id].crosscheck.report.clone().unwrap(),
        ),
        "map" => app.session.contests[&id].survey.as_ref().unwrap().artifacts[0].clone(),
        _ => EnvelopePayload::ProbeReport(ProbeOutput {
            contest_id: id,
            step_id: StepId::new(),
            completed: CompletedGeneration {
                generation: GenerationIdentity {
                    message_id: MessageId::new(),
                    work_id: WorkId::new(),
                    trigger_event_id: EventId::new(),
                    attempt: 1,
                },
                model: report_text("fixture/judge"),
                finish_reason: FinishReason::Stop,
                usage: None,
            },
            data: app.session.probes[&id].report.clone().unwrap(),
        }),
    };
    let state = match kind {
        "ledger" => EnvelopePayload::CensusState(CensusState {
            contest_id: id,
            state_version: 1,
            status: CensusStatus::Complete,
            pass: 1,
            plan_version: 1,
            step_id: None,
            artifact_event_ids: vec![],
            unfinished_work_ids: vec![],
            terminal_reason: None,
        }),
        "grid" => EnvelopePayload::CrosscheckState(
            app.session.contests[&id].crosscheck.state.clone().unwrap(),
        ),
        "map" => EnvelopePayload::SurveyState(SurveyState {
            contest_id: id,
            state: SurveyStatus::Complete,
            state_version: 1,
            step_id: None,
            artifact_event_ids: vec![],
            terminal_reason: None,
            completeness: None,
        }),
        _ => EnvelopePayload::ProbeState(ProbeState {
            contest_id: id,
            state: ProbeStatus::Complete,
            state_version: 1,
            step_id: None,
            artifact_event_ids: vec![],
            skipped_work_ids: vec![],
            skipped_members: vec![],
            terminal_reason: None,
        }),
    };
    let mut payloads = vec![state];
    if kind == "grid" {
        payloads.push(EnvelopePayload::CrosscheckGrid(
            app.session.contests[&id].crosscheck.grid.clone().unwrap(),
        ));
    }
    payloads.push(payload);
    let now = "2026-11-28T00:00:00Z".parse().unwrap();
    let mut replay = squad_tui::session::Session::default();
    for (index, payload) in payloads.into_iter().enumerate() {
        replay.reduce(
            FeedItem::Committed(CommittedEnvelope {
                base: EnvelopeBase {
                    version: 1,
                    id: EventId::new(),
                    room_id: MainRoom::Main,
                    correlation_id: EventId::new(),
                    causation_id: None,
                    sender: Actor::System,
                    recipients: vec![Recipient::Me],
                    created_at: now,
                },
                content: payload,
                commit: CommitFields {
                    seq: Cursor::new(index as i64 + 1).unwrap(),
                    accepted_at: now,
                    commit_index: 0,
                    commit_size: 1,
                },
            }),
            squad_tui::fx::FeedOrigin::Replay,
            Duration::ZERO,
            &mut squad_tui::fx::FxQueue::new(),
        );
    }
    assert_eq!(
        replay
            .snapshot
            .context
            .records
            .iter()
            .filter(|record| record.kind.as_str().ends_with(".report"))
            .count(),
        1,
        "{kind}: exported report source"
    );
    app.session = replay;
    if let Some(view) = app.session.probes.get_mut(&id) {
        view.banner_mode = Some(squad_core::banner::BannerMode::Off);
        view.matrix_collapsed = kind == "tree";
    }
    app
}

fn report_key(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.handle(Event::Key(KeyEvent::new(code, modifiers)), Duration::ZERO);
}

#[test]
fn large_ledger_grid_map_tree_and_matrix_reports_navigate_and_render_visible_rows() {
    use ratatui::{buffer::Buffer, layout::Rect};
    use squad_tui::widgets::transcript;

    fn rows(buffer: &Buffer, area: Rect) -> Vec<String> {
        (area.y..area.bottom())
            .map(|y| {
                (area.x..area.right())
                    .map(|x| buffer[(x, y)].symbol())
                    .collect()
            })
            .collect()
    }
    fn assert_slice(
        app: &mut App,
        terminal: &mut Terminal<TestBackend>,
        reference: &[String],
        top: usize,
        label: &str,
    ) {
        terminal
            .draw(|f| ui::render(app, f, Duration::ZERO))
            .unwrap();
        let area = app.composed.viewport;
        assert_eq!(app.composed.top, top, "{label}: viewport offset");
        let expected: Vec<_> = (top..top + usize::from(area.height))
            .map(|i| {
                reference
                    .get(i)
                    .cloned()
                    .unwrap_or_else(|| " ".repeat(usize::from(area.width)))
            })
            .collect();
        assert_eq!(
            rows(terminal.backend().buffer(), area),
            expected,
            "{label}: visible rows"
        );
    }

    for kind in ["ledger", "grid", "map", "tree", "matrix"] {
        for (width, height) in [(100, 40), (80, 24)] {
            let label = format!("{kind} {width}x{height}");
            let id = ContestId::new();
            let mut app = large_report(kind, id);
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal
                .draw(|f| ui::render(&mut app, f, Duration::ZERO))
                .unwrap();
            let message = app.composed.transcript.message(identity(id)).unwrap();
            let widget_rows: Vec<_> = message
                .widget_rows
                .iter()
                .map(ToString::to_string)
                .collect();
            for i in 0..48 {
                assert!(
                    widget_rows
                        .iter()
                        .any(|row| row.contains(&format!("navrow{i:03}"))),
                    "{label}: missing entry {i}"
                );
            }
            let last_row = widget_rows
                .iter()
                .rposition(|row| !row.trim().is_empty())
                .unwrap();
            let total = app.composed.transcript.total_rows();
            let viewport_height = usize::from(app.composed.viewport.height);
            assert!(
                total > viewport_height * 3,
                "{label}: fixture must exceed three viewports"
            );
            let mut full = Buffer::empty(Rect::new(
                0,
                0,
                app.composed.viewport.width,
                total.try_into().unwrap(),
            ));
            transcript::render(
                &app.composed.transcript,
                0,
                full.area,
                &mut full,
                &app.theme.palette,
            );
            let reference = rows(&full, full.area);
            let bottom = total - viewport_height;
            assert_slice(&mut app, &mut terminal, &reference, bottom, &label);
            assert!(
                rows(terminal.backend().buffer(), app.composed.viewport)
                    .contains(&reference[last_row]),
                "{label}: End must show last report row"
            );

            let report_event = app
                .session
                .snapshot
                .context
                .records
                .iter()
                .find(|record| record.kind.as_str().ends_with(".report"))
                .unwrap()
                .event_id;
            // Tree search reveals the matrix as well; it is empty in that fixture.
            for needle in ["navrow000", "navrow047"] {
                report_key(&mut app, KeyCode::Char('f'), KeyModifiers::CONTROL);
                for ch in needle.chars() {
                    report_key(&mut app, KeyCode::Char(ch), KeyModifiers::NONE);
                }
                // Crosscheck exports each case in both the grid and final ledger.
                let matches = if kind == "grid" { 2 } else { 1 };
                assert_eq!(app.search.matches.len(), matches, "{label}: search targets");
                assert_eq!(app.msg_focus, Some(report_event), "{label}: report focus");
                let target = reference
                    .iter()
                    .position(|row| row.contains(needle))
                    .unwrap();
                assert_slice(&mut app, &mut terminal, &reference, target, &label);
                // The real next/previous keys reveal the matched report row.
                for (code, current) in [
                    (KeyCode::Down, 1 % matches),
                    (KeyCode::Up, 0),
                    (KeyCode::Enter, 1 % matches),
                ] {
                    report_key(&mut app, code, KeyModifiers::NONE);
                    assert_eq!(
                        app.search.current,
                        Some(current),
                        "{label}: search navigation"
                    );
                    assert_slice(&mut app, &mut terminal, &reference, target, &label);
                }
                report_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
                if needle == "navrow000" {
                    let page = viewport_height - 1;
                    for n in 1..=2 {
                        report_key(&mut app, KeyCode::PageDown, KeyModifiers::NONE);
                        assert_slice(
                            &mut app,
                            &mut terminal,
                            &reference,
                            (target + n * page).min(bottom),
                            &label,
                        );
                    }
                    report_key(&mut app, KeyCode::End, KeyModifiers::NONE);
                    assert_slice(&mut app, &mut terminal, &reference, bottom, &label);
                    assert!(
                        rows(terminal.backend().buffer(), app.composed.viewport)
                            .contains(&reference[last_row]),
                        "{label}: last report row unreachable"
                    );
                }
            }
        }
    }
}
