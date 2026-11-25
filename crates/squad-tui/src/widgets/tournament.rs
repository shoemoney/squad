//! Static tournament presentation. M5 adapters supply committed values; no local ranking.
pub(crate) mod paint;
use crate::fx::Palette;
use paint::{Painter, heading};
use ratatui::{buffer::Buffer, layout::Rect, style::Style};
use squad_protocol::{ContestId, ContestKind, GenericContestState};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Entrant {
    pub nick: String,
    /// Persistent, authoritative marker, including eliminations in older rounds.
    pub eliminated: bool,
    pub trophies: u32,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Entry {
    pub nick: String,
    pub text: String,
    /// e.g. streaming, retry 2/3, forfeit. Supplied by the core.
    pub status: String,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ranking {
    pub nick: String,
    pub score: i64,
    pub critique: String,
    pub eliminated: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Round {
    pub number: u32,
    pub remaining: usize,
    pub expanded: bool,
    pub replay: Option<String>,
    /// Stable participant order, independent of completion order.
    pub entries: Vec<Entry>,
    /// Accepted best-first order. Never sorted by the widget.
    pub ranking: Vec<Ranking>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TournamentView {
    pub id: Option<ContestId>,
    pub prompt: String,
    pub state: String,
    pub reason: Option<String>,
    pub judge_model: String,
    pub entrants: Vec<Entrant>,
    pub survivors: Vec<String>,
    pub current_round: u32,
    pub rounds: Vec<Round>,
    /// Committed final order, independent of displayed round scores.
    pub placements: Vec<String>,
    pub champion: Option<String>,
    pub expanded: bool,
    pub scroll: u16,
}
impl TournamentView {
    pub fn new(prompt: impl Into<String>, entrants: Vec<Entrant>) -> Self {
        Self {
            prompt: prompt.into(),
            entrants,
            expanded: true,
            ..Self::default()
        }
    }
    /// Generic M1 carrier adapter; typed artifacts can populate the plain fields in M5.
    pub fn apply_state(&mut self, state: &GenericContestState) -> bool {
        if state.kind != ContestKind::Tournament || self.id.is_some_and(|id| id != state.contest_id)
        {
            return false;
        }
        self.id = Some(state.contest_id);
        self.state = state.status.to_string();
        self.reason = state.terminal_reason.as_ref().map(ToString::to_string);
        true
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TournamentRects {
    pub banner: Rect,
    pub rounds: Vec<(u32, Rect)>,
    pub entries: Vec<(u32, String, Rect)>,
    pub rankings: Vec<(u32, String, Rect)>,
    pub eliminations: Vec<(String, Rect)>,
    pub champion: Rect,
}
/// Renders a complete effects-off view and returns only visible effect anchors.
pub fn render(
    view: &TournamentView,
    area: Rect,
    buf: &mut Buffer,
    palette: &Palette,
) -> TournamentRects {
    let mut p = Painter::new(area, buf, view.scroll);
    let mut rects = TournamentRects {
        banner: p.text(
            &format!(
                "{} TOURNAMENT · {}",
                if view.expanded { "▾" } else { "▸" },
                view.state
            ),
            heading(palette),
        ),
        ..Default::default()
    };
    if !view.expanded {
        return rects;
    }
    if let Some(id) = view.id {
        p.text(&format!("ID {id}"), Style::default());
    }
    p.text(&view.prompt, Style::default());
    p.text(
        &format!(
            "Judge {} · round {} · survivors {}",
            view.judge_model,
            view.current_round,
            view.survivors.join(", ")
        ),
        Style::default(),
    );
    for (i, entrant) in view.entrants.iter().enumerate() {
        let r = p.text(
            &format!(
                "@{} · trophies {}{}",
                entrant.nick,
                entrant.trophies,
                if entrant.eliminated {
                    " · ELIMINATED"
                } else {
                    ""
                }
            ),
            Style::default().fg(palette.member(&entrant.nick, i)),
        );
        if entrant.eliminated && !r.is_empty() {
            rects.eliminations.push((entrant.nick.clone(), r));
        }
    }
    for round in &view.rounds {
        let label = if round.remaining == 2 {
            "FINAL".into()
        } else {
            format!("ROUND {} · {} REMAIN", round.number, round.remaining)
        };
        let r = p.text(
            &format!("{} {label}", if round.expanded { "▾" } else { "▸" }),
            heading(palette),
        );
        if !r.is_empty() {
            rects.rounds.push((round.number, r));
        }
        if !round.expanded {
            continue;
        }
        if let Some(replay) = &round.replay {
            p.text(&format!("ROUND REPLAY · {replay}"), Style::default());
        }
        for (i, entry) in round.entries.iter().enumerate() {
            let r = p.text(
                &format!("@{} · {}", entry.nick, entry.status),
                Style::default().fg(palette.member(&entry.nick, i)),
            );
            if !r.is_empty() {
                rects.entries.push((round.number, entry.nick.clone(), r));
            }
            p.text(&entry.text, Style::default());
        }
        for (i, rank) in round.ranking.iter().enumerate() {
            let r = p.text(
                &format!(
                    "{}. @{} · score {}{}",
                    i + 1,
                    rank.nick,
                    rank.score,
                    if rank.eliminated {
                        " · ELIMINATED"
                    } else {
                        ""
                    }
                ),
                heading(palette),
            );
            if !r.is_empty() {
                rects.rankings.push((round.number, rank.nick.clone(), r));
            }
            p.text(&rank.critique, Style::default());
        }
    }
    if let Some(champion) = &view.champion {
        rects.champion = p.text(&format!("CHAMPION · @{champion}"), heading(palette));
    }
    if !view.placements.is_empty() {
        p.text("FINAL PLACEMENTS", heading(palette));
    }
    for (i, nick) in view.placements.iter().enumerate() {
        p.text(&format!("{}. @{nick}", i + 1), Style::default());
    }
    if let Some(reason) = &view.reason {
        p.text(reason, Style::default());
    }
    rects
}
