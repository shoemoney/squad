//! Stable palette identities and default-foreground prose.
use crate::fx::{ColorTier, Palette};
use ratatui::style::{Modifier, Style};
#[derive(Debug, Clone)]
pub struct Theme {
    pub palette: Palette,
}
impl Theme {
    pub fn new(tier: ColorTier) -> Self {
        Self {
            palette: Palette::new(tier),
        }
    }
    pub fn chrome(&self) -> Style {
        Style::default()
            .fg(self.palette.chrome())
            .add_modifier(Modifier::BOLD)
    }
    pub fn author(&self, nick: &str, slot: usize) -> Style {
        Style::default()
            .fg(self.palette.member(nick, slot))
            .add_modifier(Modifier::BOLD)
    }
    pub fn body(&self) -> Style {
        Style::default()
    }
}
