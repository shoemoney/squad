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
            terminal_reason: None,
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
