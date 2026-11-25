//! Census presentation uses committed report arithmetic without deriving estimates.
use super::tournament::paint::Painter;
use crate::fx::Palette;
use ratatui::{buffer::Buffer, layout::Rect, style::Style};
use squad_protocol::*;
#[derive(Debug, Clone, Default)]
pub struct CensusView {
    pub setup: Option<CensusSetup>,
    pub details_expanded: bool,
    pub hunts: Vec<CensusHunt>,
    pub state: Option<CensusState>,
    pub plans: Vec<CensusPlan>,
    pub reports: Vec<CensusReport>,
}
impl CensusView {
    pub fn text(&self) -> String {
        let mut out = String::from("CENSUS\n");
        if let Some(s) = &self.setup {
            out.push_str(&format!(
                "Members: {} · {} passes · {} normal calls · {} reserved attempts\n",
                s.participants
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", "),
                s.passes,
                s.normal_calls(),
                s.reserved_calls()
            ));
        }
        if let Some(s) = &self.state {
            out.push_str(&format!(
                "Pass {} · plan version {} · {:?}\n",
                s.pass, s.plan_version, s.status
            ));
            if let Some(reason) = &s.terminal_reason {
                out.push_str(&format!("{reason}\n"));
            }
        }
        for plan in &self.plans {
            out.push_str(&format!(
                "Plan {}: {}\nSources: {}\n",
                plan.version,
                plan.text,
                plan.source_event_ids
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        for hunt in &self.hunts {
            out.push_str(&format!(
                "Pass {} · @{} · lens {} · {} findings\n",
                hunt.pass,
                hunt.member,
                hunt.lens,
                hunt.output.gaps.len()
            ));
        }
        for report in &self.reports {
            out.push_str(&squad_core::export::census::report_text_with_details(
                report,
                self.details_expanded,
            ));
        }
        if self
            .state
            .as_ref()
            .is_some_and(|s| s.status == CensusStatus::WaitingForPlan)
        {
            out.push_str("/continue keeps this plan; /continue \"replacement plan\" patches it. /stop releases the contest claim.\n");
        }
        out
    }
}
#[derive(Debug, Clone, Default)]
pub struct CensusRects {
    pub banner: Rect,
    pub estimate_graphic: Rect,
    pub content_height: usize,
}
pub fn render(view: &CensusView, area: Rect, buf: &mut Buffer, palette: &Palette) -> CensusRects {
    let mut p = Painter::new(area, buf, 0);
    let banner = p.text("CENSUS", Style::default().fg(palette.chrome()));
    let estimate_graphic = p.text(
        "Estimate unavailable",
        Style::default().fg(palette.chrome()),
    );
    for line in view.text().lines().skip(1) {
        p.text(line, Style::default());
    }
    CensusRects {
        banner,
        estimate_graphic,
        content_height: usize::from(area.height),
    }
}
