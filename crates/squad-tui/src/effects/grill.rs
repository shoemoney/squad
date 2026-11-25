//! Grill chrome only. Static labels, PASS, questions and plan bodies belong to widgets.
pub(crate) mod support;
use ratatui::{buffer::Buffer, layout::Rect, style::Color};
use std::time::Duration;
pub use support::{Level, Motion};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompletionReason {
    Survived,
    Normal,
    Stopped,
    LeaderFailed,
}
#[derive(Clone, Copy, Debug)]
pub enum Treatment {
    Turn {
        previous: Rect,
        active: Rect,
        previous_color: Color,
        active_color: Color,
    },
    PlanVersion {
        badge: Rect,
    },
    Complete {
        reason: CompletionReason,
        survived_label: Rect,
    },
}
#[derive(Clone, Debug)]
pub struct GrillEffect {
    pub motion: Motion,
    treatment: Treatment,
    gold: Color,
}
impl GrillEffect {
    pub fn new(motion: Motion, key: u64, treatment: Treatment, gold: Color) -> Self {
        let mut effect = Self {
            motion,
            treatment,
            gold,
        };
        effect.motion.begin(key);
        effect
    }
    pub fn duration(&self) -> Duration {
        self.motion.duration(match self.treatment {
            Treatment::Turn { .. } => 280,
            Treatment::PlanVersion { .. } => 120,
            Treatment::Complete { .. } => 400,
        })
    }
    pub fn is_done(&self, elapsed: Duration) -> bool {
        self.motion
            .is_done(elapsed, self.duration().as_millis() as u64)
            || matches!(
                self.treatment,
                Treatment::Complete {
                    reason: CompletionReason::Normal
                        | CompletionReason::Stopped
                        | CompletionReason::LeaderFailed,
                    ..
                }
            )
    }
    /// Render over the current static frame. No glyph is ever replaced.
    pub fn apply(&self, buf: &mut Buffer, elapsed: Duration) {
        if self.is_done(elapsed) || self.motion.no_color {
            return;
        }
        let progress = elapsed.as_secs_f32() / self.duration().as_secs_f32();
        match self.treatment {
            Treatment::Turn {
                previous,
                active,
                previous_color,
                active_color,
            } => {
                if self.motion.level == Level::Full {
                    support::accent(buf, previous, previous_color, false, 1.0 - progress);
                    let width = (f32::from(active.width) * progress).ceil() as u16;
                    support::accent(
                        buf,
                        Rect { width, ..active },
                        active_color,
                        false,
                        1.0 - progress * 0.5,
                    );
                } else {
                    support::accent(buf, active, active_color, false, 1.0 - progress);
                }
            }
            Treatment::PlanVersion { badge } => {
                support::accent(buf, badge, self.gold, false, 1.0 - progress)
            }
            Treatment::Complete {
                reason: CompletionReason::Survived,
                survived_label,
            } => {
                if self.motion.level == Level::Full {
                    let x = survived_label
                        .x
                        .saturating_add((f32::from(survived_label.width) * progress) as u16);
                    support::accent(
                        buf,
                        Rect::new(x, survived_label.y, 1, survived_label.height)
                            .intersection(survived_label),
                        self.gold,
                        false,
                        1.0,
                    );
                } else {
                    support::accent(buf, survived_label, self.gold, false, 1.0 - progress);
                }
            }
            _ => {}
        }
    }
}
