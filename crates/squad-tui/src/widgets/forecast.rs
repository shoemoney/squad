//! Human-visible committed forecasts, with a pooled bar and room-spread band.
use squad_protocol::{forecast::*, *};
pub fn lines(state: &ForecastState) -> Vec<String> {
    vec![
        "FORECAST".into(),
        match state.state {
            ForecastStatus::Framing => "FRAME",
            ForecastStatus::Forecasting => "FIRST FORECASTS",
            ForecastStatus::Revising => "REVISION",
            ForecastStatus::Aggregating
            | ForecastStatus::Explaining
            | ForecastStatus::Reporting
            | ForecastStatus::Complete => "POOLED",
            _ => "FORECAST",
        }
        .into(),
        format!(
            "{}{}",
            squad_protocol::human::label(&state.state),
            state
                .terminal_reason
                .as_ref()
                .map(|r| format!(": {r}"))
                .unwrap_or_default()
        ),
    ]
}
#[derive(Debug, Clone, Default)]
pub struct View {
    pub state: Option<ForecastState>,
    pub artifacts: std::collections::BTreeMap<EventId, ContestArtifact>,
}
impl View {
    pub fn apply(&mut self, event: EventId, payload: &EnvelopePayload) {
        match payload {
            EnvelopePayload::ForecastState(s)
            | EnvelopePayload::ContestState(ContestState::Forecast(s)) => {
                if self.state.as_ref().is_none_or(|prior| {
                    prior.contest_id != s.contest_id || prior.state_version < s.state_version
                }) {
                    self.state = Some(s.clone());
                }
            }
            EnvelopePayload::ContestArtifact(a) if a.kind == Some(ContestKind::Forecast) => {
                self.artifacts.insert(event, a.clone());
            }
            _ => {}
        }
    }
}
pub fn bar(a: &ForecastAggregate) -> String {
    let mut cells = [' '; 51];
    let at = |p: f64| (p * 50.0).round().clamp(0.0, 50.0) as usize;
    for c in &mut cells[at(a.low)..=at(a.high)] {
        *c = '░'
    }
    for p in &a.probabilities_pct {
        cells[at(f64::from(*p) / 100.0)] = '│'
    }
    cells[at(a.p)] = '█';
    format!("0% [{}] 100%", cells.iter().collect::<String>())
}
pub fn render(view: &View) -> Vec<String> {
    let Some(state) = &view.state else {
        return vec![];
    };
    let mut out = lines(state);
    let latest_aggregate = view
        .artifacts
        .iter()
        .rev()
        .find(|(_, a)| {
            a.contest_id == state.contest_id && a.artifact_kind.as_str() == "forecast.aggregate"
        })
        .map(|(id, _)| id);
    for (event, a) in view
        .artifacts
        .iter()
        .filter(|(_, a)| a.contest_id == state.contest_id)
    {
        if a.artifact_kind.as_str() == "forecast.aggregate" && Some(event) != latest_aggregate {
            continue;
        }
        if let Some(body) = &a.body {
            match a.artifact_kind.as_str() {
                "forecast.frame" => {
                    if let Ok(frame) = validation::decode::<ForecastFrame>(body) {
                        out.extend([
                            "FRAME".into(),
                            frame.question.to_string(),
                            format!(
                                "Resolve {}: {}",
                                frame.resolution_date, frame.resolution_criteria
                            ),
                            format!("Source: {}", frame.resolution_source),
                            format!("Void: {}", frame.void_rule),
                        ]);
                    }
                }
                "forecast.estimate" => {
                    if let Ok(e) = validation::decode::<ForecastEstimate>(body) {
                        out.push(format!(
                            "{}: {}%",
                            squad_protocol::human::actor_label(&a.actor),
                            e.probability_pct
                        ));
                    }
                }
                "forecast.revise" => {
                    if let Ok(r) = validation::decode::<ForecastRevision>(body) {
                        out.push(format!(
                            "{}: revised {}%",
                            squad_protocol::human::actor_label(&a.actor),
                            r.probability_pct
                        ));
                    }
                }
                "forecast.aggregate" => {
                    if let Ok(aggregate) = validation::decode::<ForecastAggregate>(body) {
                        if let Some(split) = &aggregate.split {
                            out.push("The room is split.".into());
                            out.push(format!(
                                "Sides: {:.1}% / {:.1}%",
                                100.0 * split.lower,
                                100.0 * split.higher
                            ));
                        }
                        out.push(format!(
                            "Pooled {:.1}% · spread of the room {:.1}%–{:.1}%",
                            100.0 * aggregate.p,
                            100.0 * aggregate.low,
                            100.0 * aggregate.high
                        ));
                        out.push(bar(&aggregate));
                    }
                }
                "forecast.report" => {
                    if let Ok(report) = validation::decode::<ForecastReport>(body) {
                        out.extend(
                            squad_core::export::forecast::report_text(&report)
                                .lines()
                                .map(str::to_owned),
                        );
                    }
                }
                _ => {}
            }
        }
    }
    out
}
