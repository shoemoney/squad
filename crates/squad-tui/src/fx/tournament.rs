//! Tournament entrances consume committed artifacts in their accepted order.
//! Persistent elimination flags and trophy totals are always supplied by the reducer.
use super::verdict::{Celebration, Contender, Level, Tick, ease, progress, put};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
};
use std::{collections::BTreeSet, time::Duration};

#[derive(Clone, Debug)]
pub struct RankingRow {
    pub nick: String,
    pub score: u32,
    pub hue: Color,
    pub critique: String,
    /// Authoritative state: survives dropped cues, settlement, replay and resize.
    pub eliminated: bool,
}

pub struct RankingReveal {
    level: Level,
    no_color: bool,
    settled: bool,
}
impl RankingReveal {
    pub fn new(level: Level, no_color: bool, live: bool) -> Self {
        Self {
            level,
            no_color,
            settled: !live,
        }
    }
    pub fn duration(&self, rows: usize) -> Duration {
        Duration::from_millis(match self.level {
            Level::Full => rows.saturating_sub(1) as u64 * 250 + 400,
            Level::Lite => 400,
            Level::Off => 0,
        })
    }
    pub fn is_done(&self, t: Duration, rows: usize) -> bool {
        self.settled || t >= self.duration(rows)
    }
    pub fn settle(&mut self) {
        self.settled = true;
    }
    /// Positions never move. The authoritative score and critique remain readable
    /// from frame zero; the extra count-up is a separate decorative graphic.
    pub fn render(
        &self,
        buf: &mut Buffer,
        area: Rect,
        protected: &[Rect],
        t: Duration,
        rows: &[RankingRow],
    ) {
        let done = self.is_done(t, rows.len());
        for (i, row) in rows.iter().enumerate() {
            let y = i.saturating_mul(3) as u16;
            let delay = if self.level == Level::Full {
                (rows.len() - 1 - i) as u64 * 250
            } else {
                0
            };
            let ms = t.as_millis() as u64;
            let p = if done {
                1.0
            } else {
                ease(progress(ms.saturating_sub(delay), 400))
            };
            let style = if self.no_color {
                Style::default()
            } else {
                Style::default().fg(row.hue)
            };
            let style = if !done && ms < delay {
                style.add_modifier(Modifier::DIM)
            } else {
                style.add_modifier(Modifier::BOLD)
            };
            put(
                buf,
                area,
                protected,
                1,
                y,
                &format!(
                    "{}. {} · score {}{}",
                    i + 1,
                    row.nick,
                    row.score,
                    if row.eliminated { " · ELIMINATED" } else { "" }
                ),
                style,
            );
            put(
                buf,
                area,
                protected,
                3,
                y + 1,
                &row.critique,
                Style::default(),
            );
            if !done {
                put(
                    buf,
                    area,
                    protected,
                    3,
                    y + 2,
                    &format!(
                        "{} {:>3}",
                        "━".repeat((p * 16.0).round() as usize),
                        (row.score as f32 * p).round() as u32
                    ),
                    style,
                );
            }
        }
    }
}

/// Deduplicates committed artifact cues for a single tournament. Restore with no
/// cues on replay; render persistent markers from RankingRow regardless of cues.
#[derive(Default)]
pub struct TournamentFx {
    rankings: BTreeSet<String>,
    eliminations: BTreeSet<String>,
    champions: BTreeSet<String>,
}
impl TournamentFx {
    pub fn ranking(
        &mut self,
        id: &str,
        accepted: bool,
        live: bool,
        level: Level,
        no_color: bool,
    ) -> Option<RankingReveal> {
        if !accepted || !live || !self.rankings.insert(id.into()) {
            return None;
        }
        Some(RankingReveal::new(level, no_color, true))
    }
    /// Returns one stamp entrance, never changes the authoritative marker.
    pub fn elimination(
        &mut self,
        id: &str,
        committed: bool,
        live: bool,
        level: Level,
        no_color: bool,
    ) -> Option<EliminatedStamp> {
        if !committed || !live || !self.eliminations.insert(id.into()) {
            return None;
        }
        Some(EliminatedStamp { level, no_color })
    }
    pub fn champion(
        &mut self,
        id: &str,
        live: bool,
        level: Level,
        no_color: bool,
        seed: u64,
    ) -> Option<ChampionReveal> {
        if !live || !self.champions.insert(id.into()) {
            return None;
        }
        Some(ChampionReveal {
            celebration: Celebration::new(level, no_color, true, Tick::Trophy, seed),
        })
    }
}

pub struct EliminatedStamp {
    level: Level,
    no_color: bool,
}
impl EliminatedStamp {
    pub fn duration(&self) -> Duration {
        Duration::from_millis(if self.level == Level::Off || self.no_color {
            0
        } else {
            150
        })
    }
    pub fn is_done(&self, t: Duration) -> bool {
        t >= self.duration()
    }
    /// Accent only: the TUI already rendered the permanent ELIMINATED label.
    pub fn render(&self, buf: &mut Buffer, marker: Rect, t: Duration) {
        if self.is_done(t) {
            return;
        }
        for y in marker.y..marker.bottom() {
            for x in marker.x..marker.right() {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_style(if self.no_color {
                        Style::default().add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                            .fg(super::palette::GOLD)
                            .add_modifier(Modifier::BOLD)
                    });
                }
            }
        }
    }
}

pub struct ChampionReveal {
    pub celebration: Celebration,
}
impl ChampionReveal {
    pub fn new(level: Level, no_color: bool, live: bool, seed: u64) -> Self {
        Self {
            celebration: Celebration::new(level, no_color, live, Tick::Trophy, seed),
        }
    }
    pub fn skip_on_key<K>(&mut self, key: K) -> K {
        self.celebration.skip_on_key(key)
    }
    pub fn settle(&mut self) {
        self.celebration.settle();
    }
    pub fn is_done(&self, t: Duration) -> bool {
        self.celebration.is_done(t)
    }
    pub fn render(
        &self,
        buf: &mut Buffer,
        area: Rect,
        protected: &[Rect],
        t: Duration,
        winner: &Contender,
    ) {
        self.celebration.render_winner(
            buf,
            area,
            protected,
            t,
            winner,
            "CHAMPION",
            1,
            [winner.hue, super::palette::GOLD],
        );
    }
}
