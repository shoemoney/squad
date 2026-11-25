//! Independently scrollable roster. All accounting is supplied as authoritative snapshots.
use crate::{fx::Palette, markdown::sanitize};
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
                format!(
                    "{}{}",
                    sanitize(&self.nick),
                    if self.version.is_empty() {
                        String::new()
                    } else {
                        format!(" · {}", sanitize(&self.version))
                    }
                ),
                Style::default()
                    .fg(palette.member(&self.nick, self.slot))
                    .add_modifier(Modifier::BOLD),
            ),
            Line::raw(format!(
                "{status}{}{}",
                if self.queue_depth > 0 {
                    format!(" · q{}", self.queue_depth)
                } else {
                    String::new()
                },
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
            Line::styled(
                self.tokens_per_second
                    .filter(|n| n.is_finite() && *n >= 0.)
                    .map_or_else(|| "rate n/a".into(), |n| format!("~{n:.1} tok/s")),
                Style::default().add_modifier(if self.presence == PresenceStatus::Streaming {
                    Modifier::empty()
                } else {
                    Modifier::DIM
                }),
            ),
            Line::styled(
                format!(
                    "model {}",
                    if self.model.is_empty() {
                        "n/a"
                    } else {
                        &self.model
                    }
                ),
                Style::default().add_modifier(Modifier::DIM),
            ),
            Line::styled(
                format!(
                    "{} tok · {}",
                    self.tokens.map_or("n/a".into(), |n| n.to_string()),
                    self.cost_usd.map_or("n/a".into(), |n| format!("${n:.3}"))
                ),
                Style::default().add_modifier(Modifier::DIM),
            ),
        ]
    }
    fn compact_rows(&self, width: u16, palette: &Palette) -> Vec<Line<'static>> {
        let status = match self.presence {
            PresenceStatus::Idle => "idle",
            PresenceStatus::Thinking => "think",
            PresenceStatus::Streaming => "live",
            PresenceStatus::Error => "error",
            PresenceStatus::Offline => "off",
        };
        let heading = format!(
            "{}{}",
            sanitize(&self.nick),
            if self.version.is_empty() {
                String::new()
            } else {
                format!(" · {}", self.version)
            }
        );
        let mut rows = vec![Line::styled(
            heading,
            Style::default()
                .fg(palette.member(&self.nick, self.slot))
                .add_modifier(Modifier::BOLD),
        )];
        rows.push(Line::from(vec![
            ratatui::text::Span::styled(
                format!("{status} "),
                Style::default().fg(palette.member(&self.nick, self.slot)),
            ),
            ratatui::text::Span::styled(
                format!(
                    "W{} L{}{}",
                    self.wins,
                    self.losses,
                    if self.eliminated { " out" } else { "" }
                ),
                Style::default().add_modifier(Modifier::DIM),
            ),
        ]));
        let _ = width;
        rows
    }
}
pub struct ParticipantsView<'a> {
    pub members: &'a [ParticipantView],
    pub palette: &'a Palette,
    pub scroll: usize,
}
#[derive(Clone, Debug, Default)]
pub struct ParticipantsRects {
    pub names: Vec<(String, Rect)>,
    pub protected: Vec<Rect>,
    pub animatable: Vec<Rect>,
    pub total_rows: usize,
}
/// Fixed metric rows preserve density. `scroll` is a visual row offset.
pub fn render(view: &ParticipantsView<'_>, area: Rect, buf: &mut Buffer) -> ParticipantsRects {
    let area = area.intersection(buf.area);
    let mut geometry = ParticipantsRects::default();
    if area.is_empty() {
        return geometry;
    }
    let mut offset = 0;
    let mut hidden = 0;
    let total: usize = view
        .members
        .iter()
        .map(|m| m.compact_rows(area.width, view.palette).len())
        .sum();
    for member in view.members {
        let rows = member.compact_rows(area.width, view.palette);
        let overflow = total > usize::from(area.height);
        let available = usize::from(area.height).saturating_sub(usize::from(overflow));
        if offset + rows.len() > view.scroll + available {
            hidden += 1;
            offset += rows.len();
            continue;
        }
        let record_start = rows.len() - 1;
        let record_end = rows.len();
        for (i, row) in rows.iter().enumerate() {
            let absolute = offset + i;
            if absolute >= view.scroll && absolute - view.scroll < usize::from(area.height) {
                let y = area.y + (absolute - view.scroll) as u16;
                let text = row.clone();
                text.render(Rect::new(area.x, y, area.width, 1), buf);
                let rect = Rect::new(area.x, y, area.width, 1);
                if i == 0 {
                    geometry.names.push((member.nick.clone(), rect));
                }
                if (record_start..record_end).contains(&i) {
                    geometry.animatable.push(rect);
                } else {
                    geometry.protected.push(rect);
                }
            }
        }
        offset += rows.len();
    }
    geometry.total_rows = offset;
    if hidden > 0 {
        buf.set_stringn(
            area.x,
            area.bottom() - 1,
            format!("+{hidden} more · scroll"),
            usize::from(area.width),
            Style::default().fg(view.palette.chrome()),
        );
    }
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
        let rows = |member: &ParticipantView| member.compact_rows(width, palette).len();
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

/// Preserve the distinguishing version from the concrete startup identity.
pub fn short_version(slug: &str) -> String {
    let name = slug.rsplit('/').next().unwrap_or(slug);
    let start = name
        .char_indices()
        .find(|(_, c)| c.is_ascii_digit())
        .map(|(i, _)| i);
    start.map_or_else(|| name.to_owned(), |i| name[i..].to_owned())
}
