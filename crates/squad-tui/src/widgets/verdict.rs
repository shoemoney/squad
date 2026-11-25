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
    let mut c = Canvas::new(area, buf, view.scroll);
    let mut out = VerdictRects::default();
    let body = Style::default();
    let accent = Style::default()
        .fg(palette.chrome())
        .add_modifier(Modifier::BOLD);
    c.line(&format!("VERDICT · Judge: {}", view.judge_model), accent);
    if let Some(warning) = &view.substitution_warning {
        c.line(warning, body);
    }
    let wide = c.area.width >= 72 && view.debaters.iter().all(|n| n.as_str().len() <= 17);
    let start = c.row;
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
                    let cell = Rect::new(c.area.x + 25 + i as u16 * 19, rect.y, 18, rect.height)
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
    for (i, nick) in view.debaters.iter().enumerate() {
        let total = view
            .verdict
            .weighted_totals
            .iter()
            .find(|t| &t.nick == nick);
        out.totals.push(
            c.line(
                &format!(
                    "@{nick} weighted total: {} / 10",
                    total.map_or("unavailable".into(), |t| format!("{:.2}", t.total))
                ),
                Style::default()
                    .fg(palette.member(nick.as_str(), i))
                    .add_modifier(Modifier::BOLD),
            ),
        );
    }
    out.card = c.span(start, c.row);
    out.winner = c.line(&format!("Winner: @{}", view.verdict.verdict.winner), accent);
    for nick in &view.debaters {
        c.line(
            &format!(
                "Round tally · @{nick}: {}",
                view.verdict
                    .round_tally
                    .get(nick)
                    .map_or("unavailable".into(), u32::to_string)
            ),
            body,
        );
    }
    if let Some(reason) = &view.verdict.verdict.tally_override_reason {
        c.line(&format!("Tally override: {reason}"), body);
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
    c.line(&view.verdict.verdict.reasoning, body);
    let start = c.row;
    c.line("RESPECT · What to steal from the loser", accent);
    for takeaway in &view.verdict.verdict.what_to_steal_from_loser {
        c.line(&format!("• {takeaway}"), body);
    }
    out.respect = c.span(start, c.row);
    c.line("[Follow up with @judge]", body);
    out.content_height = c.row;
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
