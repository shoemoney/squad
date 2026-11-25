//! Independently scrollable roster. All accounting is supplied as authoritative snapshots.
use crate::{
    fx::Palette,
    markdown::{sanitize, wrap_lines},
};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::Line,
    widgets::Widget,
};
use squad_protocol::{Actor, Model, Presence, PresenceStatus};

#[derive(Clone, Debug)]
pub struct ParticipantView {
    pub nick: String,
    pub version: String,
    pub model: String,
    pub slot: usize,
    pub presence: PresenceStatus,
    pub queue_depth: u32,
    pub contest: Option<String>,
    pub tokens: Option<u64>,
    pub cost_usd: Option<f64>,
    pub tokens_per_second: Option<f64>,
    pub wins: u32,
    pub losses: u32,
    pub trophies: u32,
    pub eliminated: bool,
}
impl ParticipantView {
    /// Judges and system actors never enter the room roster.
    pub fn from_model(model: &Model, version: impl Into<String>) -> Option<Self> {
        let Actor::Member(nick) = &model.nick else {
            return None;
        };
        Some(Self {
            nick: nick.to_string(),
            version: version.into(),
            model: model.resolved_slug.to_string(),
            slot: model.roster_order.unwrap_or(0) as usize,
            presence: PresenceStatus::Offline,
            queue_depth: 0,
            contest: None,
            tokens: None,
            cost_usd: None,
            tokens_per_second: None,
            wins: 0,
            losses: 0,
            trophies: 0,
            eliminated: false,
        })
    }
    pub fn apply_presence(&mut self, presence: &Presence) {
        if self.nick == presence.nick.as_str() {
            self.presence = presence.status.clone();
            self.queue_depth = presence.queue_depth;
        }
    }
    pub fn detail(&self) -> String {
        sanitize(&self.model)
    }
    pub fn lines(&self, palette: &Palette) -> Vec<Line<'static>> {
        let status = match self.presence {
            PresenceStatus::Idle => "idle",
            PresenceStatus::Thinking => "thinking",
            PresenceStatus::Streaming => "streaming",
            PresenceStatus::Error => "error",
            PresenceStatus::Offline => "offline",
        };
        vec![
            Line::styled(
                format!("{} · {}", sanitize(&self.nick), sanitize(&self.version)),
                Style::default()
                    .fg(palette.member(&self.nick, self.slot))
                    .add_modifier(Modifier::BOLD),
            ),
            Line::raw(format!(
                "{status} · queue {}{}",
                self.queue_depth,
                self.contest
                    .as_ref()
                    .map_or(String::new(), |c| format!(" · {}", sanitize(c)))
            )),
            Line::raw(format!(
                "{}-{} · trophies {}{}",
                self.wins,
                self.losses,
                self.trophies,
                if self.eliminated {
                    " · eliminated"
                } else {
                    ""
                }
            )),
            Line::raw(format!(
                "tokens {} · {}{}",
                self.tokens.map_or("unavailable".into(), |n| n.to_string()),
                self.cost_usd
                    .filter(|c| c.is_finite() && *c >= 0.)
                    .map_or("cost unavailable".into(), |c| format!("${c:.4}")),
                self.tokens_per_second
                    .filter(|n| n.is_finite() && *n >= 0.)
                    .map_or(String::new(), |n| format!(" · {n:.1} tok/s"))
            )),
        ]
    }
}
pub struct ParticipantsView<'a> {
    pub members: &'a [ParticipantView],
    pub palette: &'a Palette,
    pub scroll: usize,
}
#[derive(Clone, Debug, Default)]
pub struct ParticipantsRects {
    pub protected: Vec<Rect>,
    pub animatable: Vec<Rect>,
    pub total_rows: usize,
}
/// Entries wrap without truncating identity. `scroll` is a visual row offset.
pub fn render(view: &ParticipantsView<'_>, area: Rect, buf: &mut Buffer) -> ParticipantsRects {
    let area = area.intersection(buf.area);
    let mut geometry = ParticipantsRects::default();
    if area.is_empty() {
        return geometry;
    }
    let mut offset = 0;
    for member in view.members {
        let rows = wrap_lines(
            &member.lines(view.palette),
            usize::from(area.width).saturating_add(4),
        );
        let record_start = wrap_lines(
            &member.lines(view.palette)[..2],
            usize::from(area.width) + 4,
        )
        .len();
        let record_end = record_start
            + wrap_lines(
                &member.lines(view.palette)[2..3],
                usize::from(area.width) + 4,
            )
            .len();
        for (i, row) in rows.iter().enumerate() {
            let absolute = offset + i;
            if absolute >= view.scroll && absolute - view.scroll < usize::from(area.height) {
                let y = area.y + (absolute - view.scroll) as u16;
                // wrap_lines adds two-cell body padding; the roster uses all available columns.
                let text = Line::from(
                    row.spans
                        .iter()
                        .skip(1)
                        .take(row.spans.len().saturating_sub(2))
                        .cloned()
                        .collect::<Vec<_>>(),
                );
                text.render(Rect::new(area.x, y, area.width, 1), buf);
                let rect = Rect::new(area.x, y, area.width, 1);
                if (record_start..record_end).contains(&i) {
                    geometry.animatable.push(rect);
                } else {
                    geometry.protected.push(rect);
                }
            }
        }
        offset += rows.len() + 1;
    }
    geometry.total_rows = offset.saturating_sub(1);
    geometry
}

/// Focus follows the roster rather than a model slug (personas may share a model).
#[derive(Clone, Debug, Default)]
pub struct ParticipantsState {
    pub focused: usize,
    pub scroll: usize,
}
impl ParticipantsState {
    pub fn navigate(
        &mut self,
        delta: isize,
        members: &[ParticipantView],
        width: u16,
        height: u16,
        palette: &Palette,
    ) {
        if members.is_empty() {
            *self = Self::default();
            return;
        }
        self.focused = self
            .focused
            .saturating_add_signed(delta)
            .min(members.len() - 1);
        let rows = |member: &ParticipantView| {
            wrap_lines(&member.lines(palette), usize::from(width) + 4).len() + 1
        };
        let start: usize = members[..self.focused].iter().map(rows).sum();
        let end = start + rows(&members[self.focused]);
        if start < self.scroll {
            self.scroll = start;
        } else if end > self.scroll + usize::from(height) {
            self.scroll = if end - start > usize::from(height) {
                start
            } else {
                end.saturating_sub(usize::from(height))
            };
        }
    }
}
