//! Authoritative estimates; the UI does not calculate a competing plan.
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    widgets::{Block, Borders, Paragraph, Widget, Wrap},
};
use squad_protocol::*;
#[derive(Debug, Clone)]
pub struct PreflightView {
    pub plan: PendingPreflight,
    pub pending: bool,
}
impl PreflightView {
    pub fn text(&self) -> String {
        let (e, people, extra, warning) = match &self.plan {
            PendingPreflight::Debate(p) => (
                &p.estimate,
                p.debaters
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
                format!(
                    "{} rounds · judge {} · opening {}",
                    p.rounds,
                    p.judge_model.as_deref().unwrap_or("none"),
                    p.opening_round_required
                ),
                p.warning.as_ref(),
            ),
            PendingPreflight::Contest(p) => (
                &p.estimate,
                p.participants.iter().map(ToString::to_string).collect(),
                if p.kind == ContestKind::Census {
                    format!(
                        "census · {} passes · {} reserved attempts",
                        p.settings["passes"], p.settings["reservedCalls"]
                    )
                } else {
                    p.kind.as_str().to_owned()
                },
                p.warning.as_ref(),
            ),
        };
        let confirmation_required = match &self.plan {
            PendingPreflight::Debate(p) => p.confirmation_required,
            PendingPreflight::Contest(p) => p.confirmation_required,
        };
        format!(
            "{}\n{}\n{} calls · estimate ${:.4} · reserve ${:.4} · remaining ${:.4}\n{}\n{}",
            people.join(" vs "),
            extra,
            e.model_calls,
            e.estimated_usd,
            e.reserved_usd,
            e.remaining_usd,
            warning.map(Text::as_str).unwrap_or(""),
            if self.pending {
                "Waiting for acknowledgement…"
            } else if !confirmation_required {
                "Core auto-confirmed"
            } else {
                "Y Confirm · N Cancel"
            }
        )
    }
}
pub fn render(view: &PreflightView, area: Rect, buffer: &mut Buffer) {
    Paragraph::new(view.text())
        .wrap(Wrap { trim: false })
        .block(Block::default().borders(Borders::ALL).title("Preflight"))
        .render(area, buffer);
}
