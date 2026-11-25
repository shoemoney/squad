use ratatui::{buffer::Buffer, layout::Rect};
use squad_protocol::*;
use squad_tui::{
    fx::Palette,
    widgets::census::{CensusView, render},
};
fn text(value: &str) -> Text {
    Text::new(value).unwrap()
}
#[test]
fn committed_states_and_unavailable_range_snapshot() {
    let id: ContestId = "01900000-0000-7000-8000-000000000001".parse().unwrap();
    let mut result = String::new();
    for status in [
        CensusStatus::Hunting,
        CensusStatus::Rating,
        CensusStatus::WaitingForPlan,
        CensusStatus::Stopped,
        CensusStatus::Failed,
        CensusStatus::Complete,
    ] {
        let state = CensusState {
            contest_id: id,
            state_version: 1,
            status,
            pass: 1,
            plan_version: 1,
            step_id: None,
            artifact_event_ids: vec![],
            unfinished_work_ids: vec![],
            terminal_reason: (status == CensusStatus::Complete).then(|| text("Pass limit reached")),
        };
        let report = CensusReport {
            contest_id: id,
            pass: 1,
            plan_version: 1,
            gaps: vec![],
            additions: vec![],
            missing: vec![CensusMissing {
                member: "member0".parse().unwrap(),
                step: text("census.hunt"),
            }],
            input_gap_ids: vec![],
            f1: 0,
            f2: 0,
            formula_branch: text("zero_doubletons"),
            unseen: 0.0,
            estimate_low: None,
            estimate_high: None,
            discovery: vec![CensusDiscovery {
                pass: 1,
                plan_version: 1,
                new_confirmed: 0,
                cumulative_confirmed: 0,
                singletons: 0,
            }],
        };
        let view = CensusView {
            setup: None,
            details_expanded: true,
            hunts: vec![],
            state: Some(state),
            plans: vec![CensusPlan {
                contest_id: id,
                version: 1,
                text: text("Build a database."),
                source_event_ids: vec![],
            }],
            reports: vec![report],
        };
        let mut buffer = Buffer::empty(Rect::new(0, 0, 90, 30));
        render(
            &view,
            buffer.area,
            &mut buffer,
            &Palette::new(squad_tui::fx::ColorTier::NoColor),
        );
        result.push_str(&format!("{status:?}\n"));
        for y in 0..30 {
            let row = (0..90).map(|x| buffer[(x, y)].symbol()).collect::<String>();
            if !row.trim_end().is_empty() {
                result.push_str(row.trim_end());
                result.push('\n');
            }
        }
        assert!(!view.text().contains("Census is quiet."));
    }
    insta::assert_snapshot!(result);
}
#[test]
fn finder_drilldown_sources_and_lenses_at_both_widths() {
    let id: ContestId = "01900000-0000-7000-8000-000000000001".parse().unwrap();
    let source: EventId = "01900000-0000-7000-8000-000000000002".parse().unwrap();
    let plan = CensusPlan {
        contest_id: id,
        version: 1,
        text: text("Build a database."),
        source_event_ids: vec![source],
    };
    let gap = CensusGapOutcome {
        gap: CensusGap {
            id: text("g1"),
            omission: text("Missing rollback"),
            consequence: text("A failed release may cause an outage"),
            lenses: vec![text("failure modes")],
            source_find_ids: vec![text("find1")],
        },
        disposition: CensusDisposition::Confirmed,
        votes: 3,
        covered: 1,
        trivial: 0,
        finders: vec!["member0".parse().unwrap()],
        finder_count: 1,
        priority: Some(text("Prevent a prolonged outage")),
        citations: vec![CensusCitation {
            location: text("Recovery section"),
            quote: text("Restore from backup"),
        }],
    };
    let report = CensusReport {
        contest_id: id,
        pass: 1,
        plan_version: 1,
        gaps: vec![gap],
        additions: vec![],
        missing: vec![CensusMissing {
            member: "member2".parse().unwrap(),
            step: text("census.hunt"),
        }],
        input_gap_ids: vec![text("g1")],
        f1: 1,
        f2: 0,
        formula_branch: text("zero_doubletons"),
        unseen: 0.0,
        estimate_low: None,
        estimate_high: None,
        discovery: vec![CensusDiscovery {
            pass: 1,
            plan_version: 1,
            new_confirmed: 1,
            cumulative_confirmed: 1,
            singletons: 1,
        }],
    };
    report.validate().unwrap();
    let setup = CensusSetup {
        contest_id: id,
        plan: plan.clone(),
        participants: (0..3)
            .map(|i| format!("member{i}").parse().unwrap())
            .collect(),
        passes: 2,
        room_words: 250,
        plan_words: 500,
        models: Default::default(),
        prompt_refs: Default::default(),
        redaction_tokens: vec![],
    };
    let mut result = String::new();
    for width in [45, 90] {
        for expanded in [false, true] {
            let view = CensusView {
                setup: Some(setup.clone()),
                details_expanded: expanded,
                state: None,
                plans: vec![
                    plan.clone(),
                    CensusPlan {
                        contest_id: id,
                        version: 2,
                        text: text("Revised database plan."),
                        source_event_ids: vec![source],
                    },
                ],
                hunts: vec![CensusHunt {
                    contest_id: id,
                    pass: 1,
                    plan_version: 1,
                    work_id: "01900000-0000-7000-8000-000000000003".parse().unwrap(),
                    member: "member0".parse().unwrap(),
                    lens: text("failure modes"),
                    output: CensusHuntOutput { gaps: vec![] },
                }],
                reports: vec![report.clone()],
            };
            let mut buffer = Buffer::empty(Rect::new(0, 0, width, 60));
            render(
                &view,
                buffer.area,
                &mut buffer,
                &Palette::new(squad_tui::fx::ColorTier::NoColor),
            );
            result.push_str(&format!("width={width}, details={expanded}\n"));
            for y in 0..60 {
                let row = (0..width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>();
                if !row.trim_end().is_empty() {
                    result.push_str(row.trim_end());
                    result.push('\n');
                }
            }
            if !expanded {
                assert!(view.text().contains("F7: member-by-gap details"));
            }
            assert!(view.text().contains("Singleton:"));
            assert!(view.text().contains("Restore from backup"));
        }
    }
    insta::assert_snapshot!(result);
}
