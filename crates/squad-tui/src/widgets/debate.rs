//! Protocol-backed inline debates; rendering never schedules work or changes tallies.
use super::banner::{BannerCache, Canvas};
use crate::fx::Palette;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
};
use squad_core::banner::RenderedBanner;
use squad_core::banner::{Banner, BannerMode};
use squad_protocol::{
    CaseId, DebateRoundScore, DebateSetup, DebateState, DebateStatus, DebateTransition, DebateTurn,
    DebateTurnOutcome, MemberNick,
};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq)]
pub enum Scoring {
    NotApplicable,
    Pending,
    Cancelled,
    Accepted(DebateRoundScore),
}
#[derive(Clone, Debug)]
pub struct TurnView {
    pub nick: MemberNick,
    pub text: String,
    pub status: String,
    pub attempt: u32,
    pub is_final: bool,
}
impl From<&DebateTurn> for TurnView {
    fn from(turn: &DebateTurn) -> Self {
        let (text, status) = match &turn.outcome {
            DebateTurnOutcome::Completed {
                content,
                finish_reason,
                ..
            } => (content.clone(), finish_label(finish_reason).into()),
            DebateTurnOutcome::Forfeit { reason_code, text } => (
                text.to_string(),
                match reason_code {
                    squad_protocol::ForfeitReason::Timeout => "forfeit: timed out",
                    squad_protocol::ForfeitReason::Error => "forfeit: error",
                }
                .into(),
            ),
        };
        Self {
            nick: turn.speaker.clone(),
            text,
            status,
            attempt: turn.generation.attempt,
            is_final: true,
        }
    }
}
#[derive(Clone, Debug)]
pub struct RoundView {
    pub round: u32,
    pub turns: Vec<TurnView>,
    pub scoring: Scoring,
    pub reasons_expanded: bool,
}
#[derive(Clone, Debug)]
pub struct DebateView {
    pub state: DebateState,
    pub number: usize,
    pub records: [(u32, u32); 2],
    pub setup: Option<DebateSetup>,
    pub topic: Option<String>,
    pub collapsed: bool,
    pub rounds: Vec<RoundView>,
    /// Latest committed tally, independent of round-number order.
    pub tally: Option<std::collections::BTreeMap<MemberNick, u32>>,
    pub banner_mode: BannerMode,
    /// Width-keyed core output prepared before the draw loop.
    pub banner_art: Vec<(Banner, u16, Arc<RenderedBanner>)>,
    pub verdict_entered: bool,
    prepared_mode: BannerMode,
    pub scroll: usize,
    /// Frozen admission choice. Judge-only requests must not enable round scoring.
    pub round_scoring: bool,
    /// Supplied by the room's eligibility snapshot, never inferred by the widget.
    pub can_judge: bool,
    pub action_pending: bool,
}
impl From<&DebateState> for DebateView {
    fn from(state: &DebateState) -> Self {
        let mut view = Self {
            state: state.clone(),
            number: 1,
            records: [(0, 0); 2],
            setup: None,
            topic: None,
            collapsed: false,
            rounds: vec![],
            tally: None,
            banner_mode: BannerMode::On,
            banner_art: vec![],
            verdict_entered: matches!(state.state, DebateStatus::Judging | DebateStatus::Completed),
            prepared_mode: BannerMode::On,
            scroll: 0,
            round_scoring: state.judge_requested,
            can_judge: false,
            action_pending: false,
        };
        view.ensure_rounds();
        view
    }
}
impl DebateView {
    /// Freeze the original admission flag when restoring a judge-only snapshot.
    pub fn with_round_scoring(mut self, admitted: bool) -> Self {
        self.round_scoring = admitted;
        self.refresh_scoring();
        self
    }
    /// Call after a width or banner-mode change. Rendering remains read-only.
    pub fn prepare_banners(&mut self, width: u16, cache: &mut BannerCache) {
        self.prepared_mode = self.banner_mode;
        self.banner_art = self
            .rounds
            .iter()
            .map(|round| {
                let kind =
                    Banner::debate_round(round.round as usize, self.state.total_rounds as usize);
                (
                    kind,
                    width,
                    cache.get(
                        &kind.text(),
                        if width < 80 {
                            BannerMode::Off
                        } else {
                            self.banner_mode
                        },
                        width,
                    ),
                )
            })
            .collect();
        self.banner_art.push((
            Banner::Verdict,
            width,
            cache.get(
                "VERDICT",
                if width < 80 {
                    BannerMode::Off
                } else {
                    self.banner_mode
                },
                width,
            ),
        ));
    }
    fn banner(&self, kind: Banner, width: u16) -> Arc<RenderedBanner> {
        self.banner_art
            .iter()
            .find(|(k, w, _)| *k == kind && *w == width && self.prepared_mode == self.banner_mode)
            .map(|(_, _, art)| Arc::clone(art))
            .unwrap_or_else(|| {
                Arc::new(squad_core::banner::render(
                    &kind.text(),
                    usize::from(width),
                    if width < 80 {
                        BannerMode::Off
                    } else {
                        self.banner_mode
                    },
                ))
            })
    }
    fn ensure_rounds(&mut self) {
        if self.state.round == 0
            && matches!(
                self.state.state,
                DebateStatus::RoundRunning | DebateStatus::RoundCommitting
            )
            && !self.rounds.iter().any(|r| r.round == 0)
        {
            self.rounds.insert(
                0,
                RoundView {
                    round: 0,
                    turns: vec![],
                    scoring: Scoring::NotApplicable,
                    reasons_expanded: false,
                },
            );
        }
        for round in 1..=self.state.round.min(self.state.total_rounds) {
            if !self.rounds.iter().any(|r| r.round == round) {
                self.rounds.push(RoundView {
                    round,
                    turns: vec![],
                    scoring: Scoring::NotApplicable,
                    reasons_expanded: false,
                });
            }
        }
        self.rounds.sort_by_key(|r| r.round);
        self.refresh_scoring();
    }
    fn refresh_scoring(&mut self) {
        for round in &mut self.rounds {
            if matches!(round.scoring, Scoring::Accepted(_) | Scoring::Cancelled) {
                continue;
            }
            round.scoring = if round.round == 0 || !self.round_scoring {
                Scoring::NotApplicable
            } else if matches!(
                self.state.state,
                DebateStatus::Stopping
                    | DebateStatus::Stopped
                    | DebateStatus::Failed
                    | DebateStatus::Cancelled
                    | DebateStatus::Expired
            ) {
                Scoring::Cancelled
            } else {
                Scoring::Pending
            };
        }
    }
    pub fn with_setup(mut self, setup: &DebateSetup, opening: bool) -> Self {
        if setup.debate_id == self.state.debate_id {
            self.setup = Some(setup.clone());
            if opening && !self.rounds.iter().any(|r| r.round == 0) {
                self.rounds.insert(
                    0,
                    RoundView {
                        round: 0,
                        turns: vec![],
                        scoring: Scoring::NotApplicable,
                        reasons_expanded: false,
                    },
                );
            }
        }
        self
    }
    /// Caller supplies committed transitions for both live reduction and history replay.
    /// Returns a boundary only on entry, so repeated snapshots do not duplicate banners.
    pub fn apply_state(
        &mut self,
        state: &DebateState,
        transition: &DebateTransition,
    ) -> Option<Banner> {
        if state.debate_id != self.state.debate_id
            || state.state_version <= self.state.state_version
        {
            return None;
        }
        let banner = boundary(Some(&self.state), state, transition);
        self.verdict_entered |= banner == Some(Banner::Verdict);
        self.state = state.clone();
        self.ensure_rounds();
        banner
    }
    /// Render a protocol delta using the room reducer's accumulated text. The
    /// reducer owns chunk deduplication; this widget never appends duplicate data.
    pub fn update_stream(
        &mut self,
        nick: &MemberNick,
        delta: &squad_protocol::Delta,
        accumulated: &str,
    ) -> bool {
        let Some(round_number) = delta.round else {
            return false;
        };
        if delta.debate_id != Some(self.state.debate_id)
            || !self.state.debaters.contains(nick)
            || round_number > self.state.total_rounds
        {
            return false;
        }
        let Some(round) = self.rounds.iter_mut().find(|r| r.round == round_number) else {
            return false;
        };
        let turn = TurnView {
            nick: nick.clone(),
            text: accumulated.into(),
            status: "streaming".into(),
            attempt: delta.generation.attempt,
            is_final: false,
        };
        if let Some(previous) = round.turns.iter_mut().find(|t| &t.nick == nick) {
            if previous.attempt > turn.attempt
                || (previous.is_final && previous.attempt == turn.attempt)
            {
                return false;
            }
            *previous = turn;
        } else {
            round.turns.push(turn);
        }
        true
    }
    pub fn apply_turn(&mut self, turn: &DebateTurn) -> bool {
        if turn.debate_id != self.state.debate_id
            || turn.round > self.state.total_rounds
            || !self.state.debaters.contains(&turn.speaker)
        {
            return false;
        }
        if !self.rounds.iter().any(|r| r.round == turn.round) {
            self.rounds.push(RoundView {
                round: turn.round,
                turns: vec![],
                scoring: Scoring::NotApplicable,
                reasons_expanded: false,
            });
            self.rounds.sort_by_key(|r| r.round);
        }
        let round = self
            .rounds
            .iter_mut()
            .find(|r| r.round == turn.round)
            .expect("inserted round");
        if let Some(previous) = round.turns.iter_mut().find(|t| t.nick == turn.speaker) {
            if turn.generation.attempt < previous.attempt {
                return false;
            }
            *previous = turn.into();
        } else {
            round.turns.push(turn.into());
        }
        self.refresh_scoring();
        true
    }
    /// Replace by (debate, case, round), never add a delta to an existing tally.
    pub fn apply_score(&mut self, score: &DebateRoundScore) -> bool {
        let mut scoring_state = self.state.clone();
        scoring_state.case_id = Some(score.case_id);
        if score.debate_id != self.state.debate_id
            || !self.round_scoring
            || !self.state.case_ids.contains(&score.case_id)
            || score.validate_against(&scoring_state).is_err()
        {
            return false;
        }
        let Some(round) = self.rounds.iter_mut().find(|r| r.round == score.round) else {
            return false;
        };
        if round.scoring == Scoring::Accepted(score.clone()) {
            return true;
        }
        self.tally = Some(score.tally.clone());
        round.scoring = Scoring::Accepted(score.clone());
        true
    }
}
/// A boundary is presentation of committed state, never an extra protocol event.
pub fn boundary(
    previous: Option<&DebateState>,
    state: &DebateState,
    _transition: &DebateTransition,
) -> Option<Banner> {
    if previous
        .is_some_and(|p| p.debate_id != state.debate_id || p.state_version >= state.state_version)
    {
        return None;
    }
    if state.state == DebateStatus::Judging
        && previous.is_none_or(|p| p.state != DebateStatus::Judging)
    {
        return Some(Banner::Verdict);
    }
    if state.state == DebateStatus::RoundRunning
        && previous.is_none_or(|p| {
            p.round != state.round
                || matches!(
                    p.state,
                    DebateStatus::Preparing
                        | DebateStatus::AwaitingConfirmation
                        | DebateStatus::WaitingForDebaters
                )
        })
    {
        return Some(Banner::debate_round(
            state.round as usize,
            state.total_rounds as usize,
        ));
    }
    None
}
#[derive(Clone, Debug, Default)]
pub struct DebateRects {
    pub toggle: Rect,
    pub scorecard_controls: Vec<(u32, Rect)>,
    pub versus_card: Rect,
    pub score_cells: Vec<(u32, MemberNick, Rect)>,
    pub tug_bar: Rect,
    pub banners: Vec<(Banner, Rect)>,
    pub turn_bodies: Vec<Rect>,
    pub controls: Rect,
    pub content_height: usize,
}
pub fn render(view: &DebateView, area: Rect, buf: &mut Buffer, palette: &Palette) -> DebateRects {
    let mut out = DebateRects::default();
    let mut c = Canvas::new(area, buf, view.scroll);
    let accent = Style::default()
        .fg(palette.chrome())
        .add_modifier(Modifier::BOLD);
    let body = Style::default();
    out.toggle = c.line(
        &format!(
            "{} Debate {} · {} vs {} · {}",
            if view.collapsed { "▸" } else { "▾" },
            view.number,
            view.state.debaters[0],
            view.state.debaters[1],
            status_label(&view.state)
        ),
        if matches!(
            view.state.state,
            DebateStatus::Completed | DebateStatus::CompleteUnjudged
        ) {
            Style::default().add_modifier(Modifier::DIM)
        } else {
            accent
        },
    );
    if view.collapsed {
        out.content_height = c.row;
        return out;
    }
    let topic = view
        .setup
        .as_ref()
        .map(|s| s.topic_text.as_str())
        .or(view.topic.as_deref())
        .unwrap_or("Topic unavailable");
    c.line(topic, Style::default().add_modifier(Modifier::DIM));
    out.versus_card = c.span(0, c.row);
    if let Some(setup) = &view.setup {
        for solution in &setup.solutions {
            c.line(
                &format!("@{} solution: {}", solution.nick, solution.text),
                body,
            );
        }
        if !setup.instruction.is_empty() {
            c.line(&format!("Instruction: {}", setup.instruction), body);
        }
    }
    if view.state.state == DebateStatus::WaitingForDebaters {
        for nick in &view.state.debaters {
            c.line(&format!("@{nick} · queued"), body);
        }
    }
    for round in &view.rounds {
        let banner = Banner::debate_round(round.round as usize, view.state.total_rounds as usize);
        let label = if round.round == 0 {
            "Opening".into()
        } else {
            format!(
                "Round {} / {}{}",
                round.round,
                view.state.total_rounds,
                if round.round == view.state.total_rounds {
                    " · Closing"
                } else {
                    ""
                }
            )
        };
        let boundary = round.round == 0
            || round.round == view.state.total_rounds
            || (round.round == 1 && !view.rounds.iter().any(|r| r.round == 0));
        let separator = Style::default().fg(palette.member(
            view.state.debaters[(round.round as usize) % 2].as_str(),
            (round.round as usize) % 2,
        ));
        let rect = if boundary && c.area.width >= 90 {
            c.art(
                &view.banner(banner, c.area.width),
                separator.add_modifier(Modifier::BOLD),
            )
        } else {
            c.line(&format!("── {label} ──"), separator)
        };
        out.banners.push((banner, rect));
        for (i, nick) in view.state.debaters.iter().enumerate() {
            let style = Style::default().fg(palette.member(nick.as_str(), i));
            let start = c.row;
            // Markdown owns two cells of inset, matching ordinary message bodies.
            let original = c.area;
            c.area.x += u16::from(c.area.width > 0);
            c.area.width = c.area.width.saturating_sub(1);
            if let Some(turn) = round.turns.iter().find(|t| &t.nick == nick) {
                c.area.x += 2;
                c.area.width = c.area.width.saturating_sub(2);
                let heading = c.line(
                    &format!(
                        "@{nick}{}{}",
                        if c.area.width < 80 && turn.status == "complete" {
                            String::new()
                        } else {
                            format!(" · {}", turn.status)
                        },
                        if turn.attempt > 1 {
                            format!(" · retry {}", turn.attempt)
                        } else {
                            String::new()
                        }
                    ),
                    Style::default().add_modifier(Modifier::DIM),
                );
                let name = Rect::new(heading.x, heading.y, nick.as_str().len() as u16 + 1, 1)
                    .intersection(heading);
                c.buf.set_style(
                    name,
                    style
                        .add_modifier(Modifier::BOLD)
                        .remove_modifier(Modifier::DIM),
                );
                c.area.x -= 2;
                c.area.width += 2;
                out.turn_bodies.push(c.markdown(
                    &turn.text,
                    turn.is_final,
                    palette.tier == crate::fx::ColorTier::NoColor,
                ));
            } else {
                c.area.x += 2;
                c.area.width = c.area.width.saturating_sub(2);
                c.line(&format!("@{nick} · waiting"), style);
            }
            c.area = original;
            let rail = c.span(start, c.row);
            for y in rail.y..rail.bottom() {
                c.buf.set_string(rail.x, y, "▌", style);
            }
            if c.area.width >= 80 {
                c.line("", body);
            }
        }
        match &round.scoring {
            Scoring::NotApplicable => {}
            Scoring::Pending => {
                c.line("Judge scoring…", body);
            }
            Scoring::Cancelled => {
                c.line("Round score: canceled", body);
            }
            Scoring::Accepted(score) => {
                if let Some(scores) = &score.scores {
                    let nicks = &view.state.debaters;
                    let values = nicks
                        .each_ref()
                        .map(|n| scores.get(n).map_or(0, |s| s.score));
                    let a = format!("Round {} · {} ", round.round, nicks[0]);
                    let middle = format!("{} : ", values[0]);
                    let row = c.line(
                        &format!(
                            "{a}{middle}{} {} · tally {} : {}",
                            values[1],
                            nicks[1],
                            score.tally.get(&nicks[0]).copied().unwrap_or(0),
                            score.tally.get(&nicks[1]).copied().unwrap_or(0)
                        ),
                        Style::default().add_modifier(Modifier::DIM),
                    );
                    for (i, offset) in [a.len(), a.len() + middle.len()].into_iter().enumerate() {
                        let cell = Rect::new(
                            row.x + offset as u16,
                            row.y,
                            values[i].to_string().len() as u16,
                            1,
                        )
                        .intersection(row);
                        c.buf.set_style(
                            cell,
                            Style::default()
                                .fg(palette.member(nicks[i].as_str(), i))
                                .add_modifier(Modifier::BOLD),
                        );
                        out.score_cells.push((round.round, nicks[i].clone(), cell));
                        if round.reasons_expanded
                            && let Some(value) = scores.get(&nicks[i])
                        {
                            c.line(&format!("{}: {}", nicks[i], value.reason), body);
                        }
                    }
                    let control = row;
                    out.scorecard_controls.push((round.round, control));
                } else {
                    c.line(
                        &format!(
                            "Round score: {}",
                            score.reason.as_ref().map_or("unscored", |r| r.as_str())
                        ),
                        body,
                    );
                }
            }
        }
    }
    if let Some(tally) = &view.tally {
        let a = tally.get(&view.state.debaters[0]).copied().unwrap_or(0);
        let b = tally.get(&view.state.debaters[1]).copied().unwrap_or(0);
        let names = &view.state.debaters;
        c.line("Round tally", Style::default().add_modifier(Modifier::DIM));
        let prefix = format!("{} {a} [", names[0]);
        let suffix = format!("] {b} {}", names[1]);
        let width = usize::from(c.area.width)
            .saturating_sub(prefix.len() + suffix.len())
            .min(24);
        let left = if a + b == 0 {
            width / 2
        } else {
            (width as u64 * u64::from(a) / (u64::from(a) + u64::from(b))) as usize
        };
        let row = c.line(&format!("{prefix}{}{suffix}", "━".repeat(width)), body);
        out.tug_bar = Rect::new(row.x + prefix.len() as u16, row.y, width as u16, row.height)
            .intersection(row);
        for x in out.tug_bar.x..out.tug_bar.right() {
            let side = usize::from(usize::from(x - out.tug_bar.x) >= left);
            c.buf[(x, row.y)].set_fg(palette.member(names[side].as_str(), side));
        }
    }
    if view.state.state == DebateStatus::RoundCommitting
        && view.rounds.iter().any(|r| r.scoring == Scoring::Pending)
    {
        c.line("Outstanding round scoring before final judging", body);
    }
    if let Some(reason) = &view.state.terminal_reason {
        c.line(reason, body);
    }
    if view.state.state == DebateStatus::Judging {
        c.line("Judging · awaiting verdict", body);
    }
    let start = c.row;
    if view.can_judge
        || matches!(
            view.state.state,
            DebateStatus::Stopped | DebateStatus::CompleteUnjudged | DebateStatus::Failed
        )
    {
        c.line(
            if view.action_pending {
                "Judge request pending acknowledgement"
            } else {
                "[/judge] Final verdict using existing scores; no retroactive round scoring"
            },
            body,
        );
    }
    out.controls = c.span(start, c.row);
    out.content_height = c.row;
    out
}
/// Case identities remain available even after a judge-only plan restores prior state.
pub fn associated_cases(view: &DebateView) -> &[CaseId] {
    &view.state.case_ids
}

pub fn status_label(state: &DebateState) -> String {
    match state.state {
        DebateStatus::RoundRunning | DebateStatus::RoundCommitting if state.round == 0 => {
            "Opening".into()
        }
        DebateStatus::RoundRunning | DebateStatus::RoundCommitting => {
            format!("Round {} of {}", state.round, state.total_rounds)
        }
        DebateStatus::Judging => "Judging".into(),
        DebateStatus::CompleteUnjudged => "Complete · unjudged".into(),
        DebateStatus::Rejected => "Rejected".into(),
        DebateStatus::Completed => "Complete".into(),
        DebateStatus::Preparing => "Preparing".into(),
        DebateStatus::AwaitingConfirmation => "Awaiting confirmation".into(),
        DebateStatus::WaitingForDebaters => "Waiting for debaters".into(),
        DebateStatus::Stopping => "Stopping".into(),
        DebateStatus::Stopped => "Stopped".into(),
        DebateStatus::Failed => "Failed".into(),
        DebateStatus::Cancelled => "Cancelled".into(),
        DebateStatus::Expired => "Expired".into(),
    }
}

pub fn finish_label(reason: &squad_protocol::FinishReason) -> &'static str {
    match reason {
        squad_protocol::FinishReason::Stop => "complete",
        squad_protocol::FinishReason::Length => "length limit",
        squad_protocol::FinishReason::ContentFilter => "filtered",
    }
}
