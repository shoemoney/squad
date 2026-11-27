//! blow_it_up view projection belongs here.
use squad_protocol::*;
pub fn lines(state: &BlowItUpState) -> Vec<String> {
    vec![
        "BLOW IT UP".into(),
        squad_protocol::human::label(&state.state),
    ]
}

#[derive(Debug, Clone, Default)]
pub struct View {
    pub state: Option<BlowItUpState>,
    pub artifacts: std::collections::BTreeMap<EventId, ContestArtifact>,
    pub killed_expanded: bool,
}
impl View {
    pub fn apply(&mut self, event: EventId, payload: &EnvelopePayload) {
        match payload {
            EnvelopePayload::BlowItUpState(s)
            | EnvelopePayload::ContestState(ContestState::BlowItUp(s)) => {
                self.state = Some(s.clone())
            }
            EnvelopePayload::ContestArtifact(a) if a.kind == Some(ContestKind::BlowItUp) => {
                self.artifacts.insert(event, a.clone());
            }
            _ => {}
        }
    }
}
impl View {
    pub fn toggle_killed(&mut self) {
        self.killed_expanded = !self.killed_expanded;
    }
}
pub fn render(view: &View) -> Vec<String> {
    let mut out = view
        .state
        .as_ref()
        .map(lines)
        .unwrap_or_else(|| vec!["BLOW IT UP".into()]);
    out.extend(["ATTACKS".into(), "CHALLENGE".into()]);
    if let Some(report) = view
        .artifacts
        .values()
        .find(|a| a.artifact_kind.as_str() == "blow_it_up.report")
        .and_then(|a| a.body.clone())
        .and_then(|b| {
            squad_protocol::validation::decode::<squad_protocol::blow_it_up::BlowItUpReport>(&b)
                .ok()
        })
    {
        out.extend(
            squad_core::export::blow_it_up::report_text(&report, view.killed_expanded)
                .lines()
                .skip(2)
                .map(str::to_owned),
        );
    } else {
        out.push("WHAT SURVIVED · pending".into());
    }
    out
}
pub fn severity_color(severity: squad_protocol::blow_it_up::Severity) -> ratatui::style::Color {
    crate::fx::blow_it_up::severity_color(severity)
}
pub fn styled_lines(view: &View) -> Vec<ratatui::text::Line<'static>> {
    render(view)
        .into_iter()
        .map(|text| {
            use squad_protocol::blow_it_up::Severity;
            let severity = [
                Severity::Critical,
                Severity::High,
                Severity::Medium,
                Severity::Low,
            ]
            .into_iter()
            .find(|s| text.starts_with(&format!("### {}", squad_protocol::human::label(s))));
            if let Some(s) = severity {
                ratatui::text::Line::styled(
                    text,
                    ratatui::style::Style::default().fg(severity_color(s)),
                )
            } else {
                ratatui::text::Line::raw(text)
            }
        })
        .collect()
}
