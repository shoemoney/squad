//! Pending-pick decoration; eligible candidates and lifecycle are authoritative inputs.
use super::banner::{ColourTier, Level};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
};
use squad_core::banner::Banner;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Debate,
    Grill,
}
impl Kind {
    pub fn banner(self) -> Banner {
        match self {
            Self::Debate => Banner::ChooseDebate,
            Self::Grill => Banner::ChooseGrill,
        }
    }
    pub fn hint(self) -> &'static str {
        match self {
            Self::Debate => "Type two names, e.g. @grok @gemini · Esc to cancel",
            Self::Grill => "Type one name, e.g. @grok · Esc to cancel",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    AwaitingPick,
    Accepted,
    Cancelled,
    Failed,
}

pub struct Candidate<'a> {
    pub nick: &'a str,
    /// An owned marker cell outside the nick/message body, rendered by the widget.
    pub marker: Rect,
    /// Chrome only: do not include message text in this rectangle.
    pub accent: Rect,
    pub colour: Color,
}

/// Keep one instance per contest identity; reconcile from committed state each draw.
pub struct Pick {
    pub kind: Kind,
    eligible: Vec<String>,
    state: State,
    restored: bool,
}
impl Pick {
    pub fn new(kind: Kind, eligible: impl IntoIterator<Item = String>, restored: bool) -> Self {
        Self {
            kind,
            eligible: eligible.into_iter().collect(),
            state: State::AwaitingPick,
            restored,
        }
    }
    pub fn update(&mut self, state: State) {
        self.state = state;
    }
    /// Validation failure leaves the committed AWAITING_PICK state untouched.
    pub fn rejected(&mut self) {}
    pub fn pending(&self) -> bool {
        self.state == State::AwaitingPick
    }
    pub fn entrance_live(&self) -> bool {
        self.pending() && !self.restored
    }
    pub fn is_done(&self, level: Level, tier: ColourTier) -> bool {
        !self.pending() || self.restored || level != Level::Full || tier == ColourTier::NoColor
    }
    pub fn next_wake(&self, level: Level, tier: ColourTier) -> Option<Duration> {
        (!self.is_done(level, tier)).then_some(Duration::from_millis(100))
    }
    /// Call after static widget rendering each frame. No input is consumed here.
    /// Static widgets own candidate markers; when pending ends this is a no-op,
    /// so the freshly rendered ordinary rows are restored immediately.
    #[allow(clippy::too_many_arguments)] // Explicit post-pass inputs, with no global capabilities or clock.
    pub fn render(
        &self,
        buf: &mut Buffer,
        candidates: &[Candidate<'_>],
        elapsed: Duration,
        level: Level,
        tier: ColourTier,
        protected: &[Rect],
    ) {
        if !self.pending() {
            return;
        }
        for candidate in candidates {
            if !self.eligible.iter().any(|nick| nick == candidate.nick) {
                continue;
            }
            let animated = !self.is_done(level, tier);
            // A smooth two-second pulse, never a luminance flash.
            let strong = !animated || (elapsed.as_secs_f32() * std::f32::consts::PI).cos() >= 0.0;
            let modifier = if strong {
                Modifier::BOLD
            } else {
                Modifier::DIM
            };
            for y in candidate.accent.intersection(buf.area).y
                ..candidate.accent.intersection(buf.area).bottom()
            {
                for x in candidate.accent.intersection(buf.area).x
                    ..candidate.accent.intersection(buf.area).right()
                {
                    if !protected.iter().any(|r| r.contains((x, y).into())) {
                        let cell = &mut buf[(x, y)];
                        cell.set_style(Style::default().add_modifier(modifier).remove_modifier(
                            if strong {
                                Modifier::DIM
                            } else {
                                Modifier::BOLD
                            },
                        ));
                        if tier != ColourTier::NoColor {
                            cell.set_fg(candidate.colour);
                        } else {
                            cell.set_fg(Color::Reset).set_bg(Color::Reset);
                        }
                    }
                }
            }
            let marker = candidate.marker.intersection(buf.area);
            if !marker.is_empty()
                && !protected
                    .iter()
                    .any(|r| r.contains((marker.x, marker.y).into()))
            {
                let cell = &mut buf[(marker.x, marker.y)];
                cell.set_symbol("▸").set_style(
                    Style::default()
                        .add_modifier(modifier)
                        .remove_modifier(if strong {
                            Modifier::DIM
                        } else {
                            Modifier::BOLD
                        }),
                );
                if tier != ColourTier::NoColor {
                    cell.set_fg(candidate.colour);
                } else {
                    cell.set_fg(Color::Reset).set_bg(Color::Reset);
                }
            }
        }
    }
}
