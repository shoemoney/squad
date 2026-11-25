//! Survey presentation uses committed core classifications and counts only.
use crate::fx::survey::{Level, Motion, SurveyEffect, Votes};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Color,
    widgets::{Paragraph, Widget, Wrap},
};
use squad_protocol::*;
use std::time::Duration;
#[derive(Debug, Clone, Default)]
pub struct SurveyView {
    pub artifacts: Vec<EnvelopePayload>,
    pub collapsed: bool,
    pub work: std::collections::BTreeMap<WorkId, WorkStateEvent>,
}
impl SurveyView {
    pub fn apply(&mut self, payload: &EnvelopePayload) {
        // Replay deduplication belongs to the reducer's event identity gate.
        if let EnvelopePayload::WorkState(w) = payload {
            self.work.insert(w.work_id, w.clone());
        } else {
            self.artifacts.push(payload.clone());
        }
    }
    pub fn report(&self) -> Option<&SurveyReport> {
        self.artifacts.iter().rev().find_map(|p| {
            if let EnvelopePayload::SurveyReport(r) = p {
                Some(r)
            } else {
                None
            }
        })
    }
    pub fn tallies(&self) -> &[SurveyClaimMap] {
        if let Some(r) = self.report() {
            return &r.map;
        }
        self.artifacts
            .iter()
            .rev()
            .find_map(|p| {
                if let EnvelopePayload::SurveySelection(s) = p {
                    Some(s.tallies.as_slice())
                } else {
                    None
                }
            })
            .unwrap_or(&[])
    }
    pub fn text(&self) -> String {
        let mut text = self
            .work
            .values()
            .map(|w| format!("{} · {:?} · attempt {}", w.target, w.status, w.attempt))
            .collect::<Vec<_>>()
            .join("\n");
        if !text.is_empty() {
            text.push_str("\n\n");
        }
        if let Some(summary) = self.report().and_then(SurveyReport::participation_summary) {
            text.insert_str(0, &format!("{summary}\n\n"));
        }
        text.push_str(
            &self
                .artifacts
                .iter()
                .filter(|p| {
                    !self.collapsed
                        || matches!(
                            p,
                            EnvelopePayload::SurveyState(_) | EnvelopePayload::SurveyReport(_)
                        )
                })
                .filter_map(squad_core::export::survey::render)
                .collect::<Vec<_>>()
                .join("\n"),
        );
        text
    }
}
pub fn render(view: &SurveyView, area: Rect, buffer: &mut Buffer) -> usize {
    render_geometry(view, area, buffer).0
}
pub fn render_geometry(
    view: &SurveyView,
    area: Rect,
    buffer: &mut Buffer,
) -> (usize, Rect, Vec<(String, Rect)>) {
    let mode = match std::env::var("SQUAD_BANNERS").as_deref() {
        Ok("off") => squad_core::banner::BannerMode::Off,
        Ok("compact") => squad_core::banner::BannerMode::Compact,
        _ => squad_core::banner::BannerMode::On,
    };
    let banner = squad_core::banner::render("SURVEY", area.width as usize, mode);
    let bh = banner.height().min(area.height as usize) as u16;
    for (i, line) in banner.lines.iter().take(bh as usize).enumerate() {
        buffer.set_stringn(
            area.x,
            area.y + i as u16,
            line,
            area.width as usize,
            ratatui::style::Style::default(),
        );
    }
    let text = view.text();
    let width = area.width.max(1) as usize;
    let lines = text
        .lines()
        .map(|s| {
            unicode_width::UnicodeWidthStr::width(s)
                .max(1)
                .div_ceil(width)
        })
        .sum::<usize>();
    Paragraph::new(text).wrap(Wrap { trim: false }).render(
        Rect::new(area.x, area.y + bh, area.width, area.height - bh),
        buffer,
    );
    let mut y = area
        .y
        .saturating_add(bh)
        .saturating_add(lines.min(u16::MAX as usize) as u16);
    let mut graphics = vec![];
    for (i, m) in view.tallies().iter().enumerate() {
        if y >= area.bottom() {
            break;
        }
        buffer.set_stringn(
            area.x,
            y,
            &m.claim_id,
            area.width as usize,
            ratatui::style::Style::default(),
        );
        y += 1;
        let effect = SurveyEffect::new(
            Motion::new(Level::Off, false, true),
            i as u64,
            Some(votes(&m.first)),
            votes(&m.final_tally),
        );
        let narrow = area.width < 48;
        let height = if narrow { 4 } else { 2 };
        let rect = Rect::new(area.x, y, area.width, height.min(area.bottom() - y));
        effect.render_graphic(
            buffer,
            rect,
            Duration::ZERO,
            narrow,
            [Color::Green, Color::Red, Color::Yellow],
        );
        graphics.push((m.claim_id.clone(), rect));
        y = y.saturating_add(height);
    }
    (
        bh as usize + lines + view.tallies().len() * if area.width < 48 { 5 } else { 3 },
        Rect::new(area.x, area.y, area.width, bh),
        graphics,
    )
}
pub fn votes(t: &SurveyTally) -> Votes {
    Votes {
        agree: t.agree,
        disagree: t.disagree,
        unsure: t.unsure,
        missing: t.missing,
    }
}
