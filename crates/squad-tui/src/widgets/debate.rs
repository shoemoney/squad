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
            } => (content.clone(), format!("{finish_reason:?}")),
            DebateTurnOutcome::Forfeit { reason_code, text } => {
                (text.to_string(), format!("forfeit: {reason_code:?}"))
            }
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
    pub setup: Option<DebateSetup>,
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
            setup: None,
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
                    cache.get(&kind.text(), self.banner_mode, width),
                )
            })
            .collect();
        self.banner_art.push((
            Banner::Verdict,
            width,
            cache.get("VERDICT", self.banner_mode, width),
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
                    self.banner_mode,
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
            "{} Debate {} · {:?}",
            if view.collapsed { "▸" } else { "▾" },
            view.state.debate_id,
            view.state.state
        ),
        accent,
    );
    if view.collapsed {
        out.content_height = c.row;
        return out;
    }
    let start = c.row;
    c.line(&"─".repeat(usize::from(c.area.width)), accent);
    let matchup = format!(
        "@{}   VS   @{}",
        view.state.debaters[0], view.state.debaters[1]
    );
    if matchup.len() <= usize::from(c.area.width) {
        let row = c.line(&matchup, accent);
        let offsets = [0, view.state.debaters[0].as_str().len() + 9];
        for (i, nick) in view.state.debaters.iter().enumerate() {
            for y in row.y..row.bottom() {
                for dx in offsets[i]..offsets[i] + nick.as_str().len() + 1 {
                    if dx < usize::from(row.width) {
                        c.buf[(row.x + dx as u16, y)].set_fg(palette.member(nick.as_str(), i));
                    }
                }
            }
        }
    } else {
        for (i, nick) in view.state.debaters.iter().enumerate() {
            c.line(
                &format!("{}@{}", if i == 0 { "" } else { "VS  " }, nick),
                Style::default()
                    .fg(palette.member(nick.as_str(), i))
                    .add_modifier(Modifier::BOLD),
            );
        }
    }
    c.line(&"─".repeat(usize::from(c.area.width)), accent);
    out.versus_card = c.span(start, c.row);
    if let Some(setup) = &view.setup {
        c.line(&format!("Topic: {}", setup.topic_text), body);
        for solution in &setup.solutions {
            c.line(
                &format!(
                    "@{} solution [{}]: {}",
                    solution.nick, solution.message_id, solution.text
                ),
                body,
            );
        }
        if !setup.instruction.is_empty() {
            c.line(&format!("Instruction: {}", setup.instruction), body);
        }
    }
    c.line(
        &format!(
            "{} rounds · Judge: {}",
            view.state.total_rounds,
            if view.state.judge_requested {
                &view.state.judge_model
            } else {
                "off"
            }
        ),
        body,
    );
    for round in &view.rounds {
        let banner = Banner::debate_round(round.round as usize, view.state.total_rounds as usize);
        let art = view.banner(banner, c.area.width);
        out.banners.push((banner, c.art(&art, accent)));
        if round.round == view.state.total_rounds {
            c.line("Closing statements", accent);
        }
        for (i, nick) in view.state.debaters.iter().enumerate() {
            let style = Style::default().fg(palette.member(nick.as_str(), i));
            if let Some(turn) = round.turns.iter().find(|t| &t.nick == nick) {
                c.line(
                    &format!(
                        "@{nick} · {}{}",
                        turn.status,
                        if turn.attempt > 1 {
                            format!(" · retry (attempt {})", turn.attempt)
                        } else {
                            String::new()
                        }
                    ),
                    style,
                );
                out.turn_bodies.push(c.line(&turn.text, body));
            } else {
                c.line(&format!("@{nick} · waiting"), style);
            }
        }
        match &round.scoring {
            Scoring::NotApplicable => {}
            Scoring::Pending => {
                c.line("Round score: pending (later turns may continue)", body);
            }
            Scoring::Cancelled => {
                c.line("Round score: canceled", body);
            }
            Scoring::Accepted(score) => {
                if let Some(scores) = &score.scores {
                    for (i, nick) in view.state.debaters.iter().enumerate() {
                        if let Some(value) = scores.get(nick) {
                            let rect = c.line(
                                &format!("@{nick}: {}/10", value.score),
                                Style::default().fg(palette.member(nick.as_str(), i)),
                            );
                            out.score_cells.push((round.round, nick.clone(), rect));
                            if round.reasons_expanded {
                                c.line(&value.reason, body);
                            }
                        }
                    }
                    let control = c.line(
                        if round.reasons_expanded {
                            "[Collapse scorecard]"
                        } else {
                            "[Expand scorecard]"
                        },
                        body,
                    );
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
                let tally = view
                    .state
                    .debaters
                    .iter()
                    .map(|n| {
                        format!(
                            "@{n} {}",
                            score
                                .tally
                                .get(n)
                                .map_or("unavailable".into(), u32::to_string)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(" · ");
                c.line(&format!("Authoritative tally: {tally}"), body);
            }
        }
    }
    if let Some(tally) = &view.tally {
        let a = tally.get(&view.state.debaters[0]).copied().unwrap_or(0);
        let b = tally.get(&view.state.debaters[1]).copied().unwrap_or(0);
        let width = usize::from(c.area.width.saturating_sub(2));
        let left = if a == 0 && b == 0 {
            width / 2
        } else {
            (width as u64 * u64::from(a) / (u64::from(a) + u64::from(b))) as usize
        };
        out.tug_bar = c.line(
            &format!("[{}{}]", "━".repeat(left), "─".repeat(width - left)),
            accent,
        );
        for y in out.tug_bar.y..out.tug_bar.bottom() {
            for x in out.tug_bar.x + 1..out.tug_bar.right().saturating_sub(1) {
                let side = usize::from(x - out.tug_bar.x - 1) >= left;
                let index = usize::from(side);
                c.buf[(x, y)].set_fg(palette.member(view.state.debaters[index].as_str(), index));
            }
        }
    }
    if view.verdict_entered {
        let art = view.banner(Banner::Verdict, c.area.width);
        out.banners.push((Banner::Verdict, c.art(&art, accent)));
    }
    if view.state.state == DebateStatus::RoundCommitting
        && view.rounds.iter().any(|r| r.scoring == Scoring::Pending)
    {
        c.line("Outstanding round scoring before final judging", body);
    }
    if let Some(reason) = &view.state.terminal_reason {
        c.line(reason, body);
    }
    for case in &view.state.case_ids {
        c.line(&format!("Case: {case}"), body);
    }
    let start = c.row;
    c.line("[Collapse debate] · /stop · /stop @nick", body);
    if matches!(
        view.state.state,
        DebateStatus::RoundRunning
            | DebateStatus::RoundCommitting
            | DebateStatus::WaitingForDebaters
            | DebateStatus::Judging
    ) {
        c.line(
            "Press Esc again within 2 s to stop the active contest.",
            body,
        );
    }
    if view.can_judge {
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
