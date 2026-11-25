//! Accepted verdict presentation. Rubric arithmetic is delegated to protocol.
use super::banner::Canvas;
use crate::fx::Palette;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
};
use squad_protocol::{
    CriterionId, DebateRoundScore, DebateVerdict, MemberNick, Verdict, validation::ValidationError,
};

#[derive(Clone, Debug)]
pub struct VerdictView {
    pub verdict: Verdict,
    pub debaters: [MemberNick; 2],
    pub judge_model: String,
    pub substitution_warning: Option<String>,
    pub round_scores: Vec<DebateRoundScore>,
    pub show_criterion_reasons: bool,
    pub scroll: usize,
    pub winner_art: Option<std::sync::Arc<squad_core::banner::RenderedBanner>>,
}
impl VerdictView {
    /// Input is an accepted protocol value. Revalidate its arithmetic instead of
    /// maintaining a second set of rubric weights or trusting model-made totals.
    pub fn new(verdict: &Verdict, debaters: &[MemberNick; 2]) -> Result<Self, ValidationError> {
        squad_protocol::verdict::validate_committed_verdict(
            verdict,
            debaters,
            &verdict.round_tally,
        )?;
        Ok(Self {
            verdict: verdict.clone(),
            debaters: debaters.clone(),
            judge_model: String::new(),
            substitution_warning: None,
            round_scores: vec![],
            show_criterion_reasons: false,
            scroll: 0,
            winner_art: None,
        })
    }
    pub fn from_debate(
        value: &DebateVerdict,
        debaters: &[MemberNick; 2],
    ) -> Result<Self, ValidationError> {
        let mut view = Self::new(&value.verdict, debaters)?;
        view.judge_model = value.completed.model.to_string();
        Ok(view)
    }
    pub fn prepare_banner(&mut self, width: u16, cache: &mut super::banner::BannerCache) {
        self.winner_art = (width >= 90).then(|| {
            cache.get(
                &self.verdict.verdict.winner.to_string().to_uppercase(),
                squad_core::banner::BannerMode::On,
                width,
            )
        });
    }
    /// Conflict nick must come from core's concrete-model conflict resolution.
    pub fn with_judge(mut self, model: &str, conflict: Option<&MemberNick>) -> Self {
        self.judge_model = model.into();
        self.substitution_warning = conflict
            .map(|nick| format!("Judge model changed to {model} because @{nick} is debating."));
        self
    }
    /// Blind tournaments never substitute their judge.
    pub fn with_tournament_judge(mut self, model: &str) -> Self {
        self.judge_model = model.into();
        self.substitution_warning = None;
        self
    }
}
#[derive(Clone, Debug, Default)]
pub struct VerdictRects {
    pub card: Rect,
    pub rows: Vec<Rect>,
    pub score_cells: Vec<(MemberNick, CriterionId, Rect)>,
    pub totals: Vec<Rect>,
    pub winner: Rect,
    pub respect: Rect,
    pub content_height: usize,
}
pub fn render(view: &VerdictView, area: Rect, buf: &mut Buffer, palette: &Palette) -> VerdictRects {
    let outer = area.intersection(buf.area);
    let inner = Rect::new(
        outer.x + u16::from(outer.width > 0),
        outer.y,
        outer.width.saturating_sub(2),
        outer.height,
    );
    let mut c = Canvas::new(inner, buf, view.scroll);
    c.line("", Style::default());
    let mut out = VerdictRects::default();
    let body = Style::default();
    let accent = Style::default().add_modifier(Modifier::BOLD);
    let winner = &view.verdict.verdict.winner;
    let winner_slot = usize::from(winner == &view.debaters[1]);
    let winner_style = Style::default()
        .fg(palette.member(winner.as_str(), winner_slot))
        .add_modifier(Modifier::BOLD);
    c.line("VERDICT", Style::default().add_modifier(Modifier::DIM));
    let winner_start = c.row;
    if let Some(art) = &view.winner_art {
        c.art(art, winner_style);
    }
    c.line(&format!("Winner: @{winner}"), winner_style);
    out.winner = c.span(winner_start, c.row);
    let totals = view.debaters.each_ref().map(|nick| {
        view.verdict
            .weighted_totals
            .iter()
            .find(|t| &t.nick == nick)
            .map_or(0., |t| t.total)
    });
    let paired = format!(
        "{} {:.2} : {:.2} {}",
        view.debaters[0], totals[0], totals[1], view.debaters[1]
    );
    c.line(
        "Weighted score",
        Style::default().add_modifier(Modifier::DIM),
    );
    let total_row = c.line(&paired, Style::default().add_modifier(Modifier::BOLD));
    let offsets = [
        view.debaters[0].as_str().len() + 1,
        view.debaters[0].as_str().len() + 1 + format!("{:.2}", totals[0]).len() + 3,
    ];
    let segments = [
        (0, offsets[0] + format!("{:.2}", totals[0]).len()),
        (offsets[1], paired.len().saturating_sub(offsets[1])),
    ];
    for (i, (offset, width)) in segments.into_iter().enumerate() {
        c.buf.set_style(
            Rect::new(total_row.x + offset as u16, total_row.y, width as u16, 1)
                .intersection(total_row),
            Style::default()
                .fg(palette.member(view.debaters[i].as_str(), i))
                .add_modifier(Modifier::BOLD),
        );
    }
    for (i, offset) in offsets.into_iter().enumerate() {
        let cell = Rect::new(
            total_row.x + offset as u16,
            total_row.y,
            format!("{:.2}", totals[i]).len() as u16,
            1,
        )
        .intersection(total_row);
        c.buf.set_style(
            cell,
            Style::default()
                .fg(palette.member(view.debaters[i].as_str(), i))
                .add_modifier(Modifier::BOLD),
        );
        out.totals.push(cell);
    }
    c.line(
        &format!(
            "Round tally {} : {} · Judge {}",
            view.verdict
                .round_tally
                .get(&view.debaters[0])
                .copied()
                .unwrap_or(0),
            view.verdict
                .round_tally
                .get(&view.debaters[1])
                .copied()
                .unwrap_or(0),
            view.judge_model
        ),
        Style::default().add_modifier(Modifier::DIM),
    );
    if let Some(warning) = &view.substitution_warning {
        c.line(warning, body);
    }
    if let Some(reason) = &view.verdict.verdict.tally_override_reason {
        c.line(&format!("Tally override: {reason}"), body);
    }
    c.line("Decision", accent);
    c.line(&view.verdict.verdict.reasoning, body);
    let start = c.row;
    c.line("Takeaway · What to steal from the loser", accent);
    for takeaway in &view.verdict.verdict.what_to_steal_from_loser {
        c.line(&format!("• {takeaway}"), body);
    }
    out.respect = c.span(start, c.row);
    if view.show_criterion_reasons {
        let wide = c.area.width >= 72 && view.debaters.iter().all(|n| n.as_str().len() <= 17);
        if wide {
            c.line(
                &format!(
                    "{:<24} {:>18} {:>18}",
                    "Criteria · scale 1–10",
                    view.debaters[0].as_str(),
                    view.debaters[1].as_str()
                ),
                accent,
            );
        }
        for (id, label) in [
            (CriterionId::Fit, "Fit"),
            (CriterionId::Feasibility, "Feasibility"),
            (CriterionId::Originality, "Originality"),
            (CriterionId::SurvivedAttacks, "Survived attacks"),
        ] {
            let scores = view.debaters.each_ref().map(|nick| {
                view.verdict
                    .verdict
                    .scores
                    .iter()
                    .find(|s| &s.nick == nick)
                    .and_then(|s| s.criteria.iter().find(|v| v.criterion_id == id))
            });
            let row_start = c.row;
            if wide {
                // Fixed numeric columns keep identities aligned even with long nicks.
                // Long names use the stacked layout below rather than being truncated.
                if view.debaters.iter().all(|n| n.as_str().len() <= 17) {
                    let rect = c.line(
                        &format!(
                            "{label:<24} {:>18} {:>18}",
                            scores[0].map_or("unavailable".into(), |s| format!("{}/10", s.score)),
                            scores[1].map_or("unavailable".into(), |s| format!("{}/10", s.score))
                        ),
                        body,
                    );
                    for (i, nick) in view.debaters.iter().enumerate() {
                        let cell =
                            Rect::new(c.area.x + 25 + i as u16 * 19, rect.y, 18, rect.height)
                                .intersection(rect);
                        for y in cell.y..cell.bottom() {
                            for x in cell.x..cell.right() {
                                c.buf[(x, y)].set_fg(palette.member(nick.as_str(), i));
                            }
                        }
                        out.score_cells.push((nick.clone(), id.clone(), cell));
                    }
                } else {
                    stacked_scores(&mut c, &mut out, view, &id, label, palette);
                }
            } else {
                stacked_scores(&mut c, &mut out, view, &id, label, palette);
            }
            if view.show_criterion_reasons {
                for (nick, score) in view.debaters.iter().zip(scores) {
                    if let Some(score) = score {
                        c.line(&format!("@{nick}: {}", score.reasoning), body);
                    }
                }
            }
            out.rows.push(c.span(row_start, c.row));
        }
        for round in &view.round_scores {
            if let Some(scores) = &round.scores {
                c.line(&format!("Round {}", round.round), accent);
                for nick in &view.debaters {
                    if let Some(score) = scores.get(nick) {
                        c.line(
                            &format!("@{nick}: {}/10 · {}", score.score, score.reason),
                            body,
                        );
                    }
                }
            } else {
                c.line(&format!("Round {} · unscored", round.round), body);
            }
        }
    }
    c.line(
        if view.show_criterion_reasons {
            "F7 Collapse details · Follow up with @judge"
        } else {
            "F7 Expand scores & reasons · Follow up with @judge"
        },
        body,
    );
    c.line("", body);
    out.content_height = c.row;
    out.card = Rect::new(
        outer.x,
        c.span(0, c.row).y,
        outer.width,
        c.span(0, c.row).height,
    );
    // Draw only visible logical edges; scrolling must not manufacture new borders.
    for logical in 0..c.row {
        let visible = c.span(logical, logical + 1);
        if visible.is_empty() || outer.width < 2 {
            continue;
        }
        let y = visible.y;
        let top = logical == 0;
        let bottom = logical + 1 == c.row;
        c.buf.set_string(
            outer.x,
            y,
            if top {
                "╭"
            } else if bottom {
                "╰"
            } else {
                "│"
            },
            winner_style,
        );
        c.buf.set_string(
            outer.right() - 1,
            y,
            if top {
                "╮"
            } else if bottom {
                "╯"
            } else {
                "│"
            },
            winner_style,
        );
        if top || bottom {
            c.buf.set_stringn(
                inner.x,
                y,
                "─".repeat(inner.width as usize),
                inner.width as usize,
                winner_style,
            );
        }
    }
    out
}
fn stacked_scores(
    c: &mut Canvas<'_>,
    out: &mut VerdictRects,
    view: &VerdictView,
    id: &CriterionId,
    label: &str,
    palette: &Palette,
) {
    c.line(label, Style::default().add_modifier(Modifier::BOLD));
    for (i, nick) in view.debaters.iter().enumerate() {
        let score = view
            .verdict
            .verdict
            .scores
            .iter()
            .find(|s| &s.nick == nick)
            .and_then(|s| s.criteria.iter().find(|s| &s.criterion_id == id));
        let rect = c.line(
            &format!(
                "@{nick}: {}",
                score.map_or("unavailable".into(), |s| format!("{}/10", s.score))
            ),
            Style::default().fg(palette.member(nick.as_str(), i)),
        );
        out.score_cells.push((nick.clone(), id.clone(), rect));
    }
}
