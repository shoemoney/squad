//! Animation lifetime and frame hooks. A missing view entry always means static final state.
use super::{
    caps::TerminalCaps,
    clock::{Clock, SystemClock},
    cue::{CatalogueEffect, FxCue, FxQueue, ReportMode, TimedCue},
    governor::{FrameSample, FxPolicy, Governor, Grade},
    level::FxLevel,
    pass::Pass,
};
use ratatui::{buffer::Buffer, layout::Rect};
use std::{
    collections::{BTreeMap, VecDeque},
    time::Duration,
};

pub const EFFECT_CAPACITY: usize = 48;
pub const MAX_CUE_AGE: Duration = Duration::from_secs(2);
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FxKey {
    Catalogue {
        effect: CatalogueEffect,
        target: String,
    },
    ModeBanner {
        contest: String,
        mode: ReportMode,
    },
    Census(String),
    Crosscheck(String),
    Survey(String),
    Probe(String),
    Bubble(String),
    Round {
        contest: String,
        round: u32,
    },
    Score {
        contest: String,
        round: u32,
    },
    Lead(String),
    Verdict(String),
    DailySpend,
    Ranking {
        contest: String,
        round: u32,
    },
    Eliminated {
        contest: String,
        member: String,
    },
    Champion(String),
    Turn(String),
    Plan(String),
    Survived(String),
    Pick(String),
    Candidate {
        contest: String,
        member: String,
    },
    Custom(String),
}
impl Default for FxKey {
    fn default() -> Self {
        Self::Custom(String::new())
    }
}
impl FxCue {
    pub fn key(&self) -> FxKey {
        match self {
            Self::Catalogue { effect, target, .. } => FxKey::Catalogue {
                effect: *effect,
                target: target.clone(),
            },
            Self::ModeBannerReady { contest_id, mode } => FxKey::ModeBanner {
                contest: contest_id.to_string(),
                mode: *mode,
            },
            Self::CensusEstimateReady { contest_id, .. } => FxKey::Census(contest_id.to_string()),
            Self::CrosscheckCellsReady { contest_id, .. } => {
                FxKey::Crosscheck(contest_id.to_string())
            }
            Self::SurveyTallyReady { contest_id, .. } => FxKey::Survey(contest_id.to_string()),
            Self::ProbeTreeReady { contest_id } => FxKey::Probe(contest_id.to_string()),
            Self::BubbleOpened { id, .. }
            | Self::BubbleCompleted { id }
            | Self::BubbleFailed { id } => FxKey::Bubble(id.to_string()),
            Self::RoundStarted { contest_id, n, .. } => FxKey::Round {
                contest: contest_id.to_string(),
                round: *n,
            },
            Self::ScorePosted { contest_id, round } => FxKey::Score {
                contest: contest_id.to_string(),
                round: *round,
            },
            Self::LeadChanged { contest_id, .. } => FxKey::Lead(contest_id.to_string()),
            Self::VerdictReady { contest_id } => FxKey::Verdict(contest_id.to_string()),
            Self::DailySpendChanged => FxKey::DailySpend,
            Self::TournamentRankingReady { contest_id, round } => FxKey::Ranking {
                contest: contest_id.to_string(),
                round: *round,
            },
            Self::TournamentEliminated { contest_id, member } => FxKey::Eliminated {
                contest: contest_id.to_string(),
                member: member.to_string(),
            },
            Self::TournamentChampion { contest_id, .. } => FxKey::Champion(contest_id.to_string()),
            Self::GrillTurnChanged { contest_id, .. } => FxKey::Turn(contest_id.to_string()),
            Self::GrillPlanReady { contest_id, .. } => FxKey::Plan(contest_id.to_string()),
            Self::GrillSurvived { contest_id } => FxKey::Survived(contest_id.to_string()),
            Self::PickRequested { contest_id } => FxKey::Pick(contest_id.to_string()),
        }
    }
    pub fn duration(&self) -> Duration {
        if let Self::Catalogue { duration, .. } = self {
            return (*duration).min(Duration::from_secs(20));
        }
        Duration::from_millis(match self {
            Self::ModeBannerReady { .. } => 1900,
            Self::CensusEstimateReady { .. } | Self::SurveyTallyReady { .. } => 400,
            Self::CrosscheckCellsReady { .. } | Self::ProbeTreeReady { .. } => 900,
            Self::RoundStarted { .. } => 1900,
            Self::VerdictReady { .. } | Self::TournamentChampion { .. } => 6500,
            Self::ScorePosted { .. } | Self::TournamentRankingReady { .. } => 400,
            Self::DailySpendChanged | Self::GrillPlanReady { .. } => 150,
            _ => 280,
        })
    }
    pub fn hero(&self) -> bool {
        matches!(
            self,
            Self::RoundStarted { .. }
                | Self::ModeBannerReady { .. }
                | Self::VerdictReady { .. }
                | Self::TournamentChampion { .. }
        )
    }
}
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Ease {
    Linear,
    #[default]
    CubicOut,
    QuadIn,
    QuadOut,
    BackOut,
}
impl Ease {
    pub fn sample(self, t: f64) -> f64 {
        let t = t.clamp(0., 1.);
        match self {
            Self::Linear => t,
            Self::CubicOut => 1. - (1. - t).powi(3),
            Self::QuadIn => t * t,
            Self::QuadOut => 1. - (1. - t).powi(2),
            Self::BackOut => {
                let u = t - 1.;
                1. + 2.70158 * u.powi(3) + 1.70158 * u * u
            }
        }
    }
}
#[derive(Debug, Clone, Copy)]
pub struct Tween {
    pub start: Duration,
    pub duration: Duration,
    pub from: f64,
    pub to: f64,
    pub ease: Ease,
}
impl Tween {
    pub fn value(self, now: Duration) -> f64 {
        let t = if self.duration.is_zero() {
            1.
        } else {
            now.saturating_sub(self.start).as_secs_f64() / self.duration.as_secs_f64()
        };
        self.from + (self.to - self.from) * self.ease.sample(t)
    }
    pub fn done(self, now: Duration) -> bool {
        now.saturating_sub(self.start) >= self.duration
    }
}
#[derive(Debug, Default, Clone)]
pub struct FxView {
    values: BTreeMap<FxKey, f64>,
}
impl FxView {
    pub fn value_or(&self, key: &FxKey, final_value: f64) -> f64 {
        self.values.get(key).copied().unwrap_or(final_value)
    }
    pub fn progress(&self, key: &FxKey) -> f64 {
        self.value_or(key, 1.)
    }
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}
/// Fresh layout registration each frame. Never retain rectangles across resize/reflow.
#[derive(Debug, Default)]
pub struct FxContext {
    /// Fresh accepted notices for catalogue dispatch after `pre`; empty on replay/off.
    pub cues: Vec<TimedCue>,
    pub protected: Vec<Rect>,
    pub streams: bool,
    pub spinners: bool,
    pub daily_spend_pulse: bool,
    pub pending_pick: bool,
    pub ambience: bool,
    pub view: FxView,
}
#[derive(Debug)]
struct Active {
    key: FxKey,
    cue_id: Option<squad_protocol::EventId>,
    start: Duration,
    hero: bool,
    tween: Option<Tween>,
    pass: Option<Pass>,
}
#[derive(Debug)]
pub struct FxManager<C: Clock = SystemClock> {
    clock: C,
    caps: TerminalCaps,
    governor: Governor,
    queue: FxQueue,
    active: VecDeque<Active>,
    protected: Vec<Rect>,
    seen: VecDeque<squad_protocol::EventId>,
    artifacts: VecDeque<FxCue>,
    last_cue: Option<Duration>,
    streams: bool,
    periodic: bool,
    ambience: bool,
    stale_dropped: u64,
}
impl<C: Clock> FxManager<C> {
    pub fn new(clock: C, caps: TerminalCaps) -> Self {
        Self {
            clock,
            caps,
            governor: Governor::default(),
            queue: FxQueue::new(),
            active: VecDeque::with_capacity(EFFECT_CAPACITY),
            protected: Vec::new(),
            seen: VecDeque::with_capacity(256),
            artifacts: VecDeque::with_capacity(256),
            last_cue: None,
            streams: false,
            periodic: false,
            ambience: false,
            stale_dropped: 0,
        }
    }
    pub fn queue_mut(&mut self) -> &mut FxQueue {
        &mut self.queue
    }
    pub fn active_count(&self) -> usize {
        self.active.len()
    }
    pub fn contains(&self, key: &FxKey) -> bool {
        self.active.iter().any(|a| &a.key == key)
    }
    pub fn stale_dropped(&self) -> u64 {
        self.stale_dropped
    }
    pub fn policy(&self) -> FxPolicy {
        self.governor
            .policy(self.caps.level, self.caps.synchronized_output)
    }
    pub fn grade(&self) -> Grade {
        self.governor.grade()
    }
    pub fn debug_tag(&self) -> Option<String> {
        (self.caps.debug && self.grade() != Grade::G0).then(|| format!("fx:{:?}", self.grade()))
    }
    pub fn observe_frame(&mut self, sample: FrameSample) {
        self.governor.observe(sample, self.clock.now());
        if self.policy().snap {
            self.active.clear();
        }
        if !self.policy().full_buffer {
            self.active
                .retain(|a| !a.pass.as_ref().is_some_and(|p| p.full_buffer));
        }
    }
    /// Drain cues before widgets draw; widgets read `ctx.view` without mutating animation state.
    pub fn pre(&mut self, ctx: &mut FxContext) {
        let now = self.clock.now();
        self.protected.clone_from(&ctx.protected);
        self.streams = ctx.streams;
        self.periodic =
            ctx.spinners || (!self.policy().snap && (ctx.daily_spend_pulse || ctx.pending_pick));
        self.ambience = ctx.ambience;
        if !ctx.pending_pick {
            self.active
                .retain(|a| !matches!(a.key, FxKey::Candidate { .. }));
        }
        ctx.cues.clear();
        while let Some(cue) = self.queue.pop_front() {
            if self.accept(&cue, now) {
                ctx.cues.push(cue);
            }
        }
        if self.policy().snap {
            self.active.clear();
        }
        self.active.retain(|a| {
            a.tween.map_or_else(
                || a.pass.as_ref().is_some_and(Pass::running),
                |t| !t.done(now),
            )
        });
        ctx.view.values.clear();
        for active in &self.active {
            if let Some(tween) = active.tween {
                ctx.view.values.insert(active.key.clone(), tween.value(now));
            }
        }
    }
    fn accept(&mut self, cue: &TimedCue, now: Duration) -> bool {
        if now.saturating_sub(cue.at) > MAX_CUE_AGE || cue.at > now {
            self.stale_dropped += 1;
            return false;
        }
        if self.seen.contains(&cue.event_id) {
            return false;
        }
        if self.seen.len() == 256 {
            self.seen.pop_front();
        }
        self.seen.push_back(cue.event_id);
        if matches!(
            cue.cue,
            FxCue::CensusEstimateReady { .. }
                | FxCue::CrosscheckCellsReady { .. }
                | FxCue::SurveyTallyReady { .. }
                | FxCue::ProbeTreeReady { .. }
                | FxCue::ModeBannerReady { .. }
                | FxCue::TournamentRankingReady { .. }
                | FxCue::TournamentEliminated { .. }
                | FxCue::TournamentChampion { .. }
                | FxCue::GrillPlanReady { .. }
                | FxCue::GrillSurvived { .. }
        ) {
            if self.artifacts.contains(&cue.cue) {
                return false;
            }
            if self.artifacts.len() == 256 {
                self.artifacts.pop_front();
            }
            self.artifacts.push_back(cue.cue.clone());
        }
        self.last_cue = Some(now);
        if self.policy().snap {
            return false;
        }
        let duration = if self.policy().fade_only {
            cue.cue.duration().min(Duration::from_millis(150))
        } else {
            cue.cue.duration()
        };
        let key = cue.cue.key();
        // Superseding state animates continuously from the current displayed value.
        let from = self
            .active
            .iter()
            .find(|a| a.key == key)
            .and_then(|a| a.tween)
            .map_or(0., |t| t.value(now));
        self.insert(Active {
            key,
            cue_id: Some(cue.event_id),
            start: now,
            hero: cue.cue.hero(),
            tween: Some(Tween {
                start: now,
                duration,
                from,
                to: 1.,
                ease: Ease::CubicOut,
            }),
            pass: None,
        });
        true
    }
    fn insert(&mut self, active: Active) {
        self.active.retain(|a| a.key != active.key);
        if self.active.len() == EFFECT_CAPACITY {
            if let Some(index) = self.active.iter().position(|a| !a.hero) {
                self.active.remove(index);
            } else if active.hero {
                self.active.pop_front();
            } else {
                return;
            }
        }
        self.active.push_back(active);
    }
    pub fn add_tween(&mut self, key: FxKey, tween: Tween, hero: bool) {
        if !self.policy().snap {
            self.insert(Active {
                key,
                cue_id: None,
                start: tween.start,
                hero,
                tween: Some(tween),
                pass: None,
            });
        }
    }
    /// Register a catalogue pass after layout. Replacement is immediate and bounded.
    pub fn add_pass(&mut self, key: FxKey, pass: Pass, hero: bool) {
        if self.policy().snap || (pass.full_buffer && !self.policy().full_buffer) {
            return;
        }
        let now = self.clock.now();
        self.insert(Active {
            key,
            cue_id: None,
            start: now,
            hero,
            tween: None,
            pass: Some(pass),
        });
    }
    /// Bind a painter only to a cue admitted by `pre`. Keeps the cue's start and
    /// tween so replacement/expiry/collapse cannot leave an orphan renderer.
    /// Build from `ctx.cues` after static layout; stale/replay/duplicate cues are absent.
    pub fn attach_pass(&mut self, cue: &TimedCue, pass: Pass) -> bool {
        if self.policy().snap || (pass.full_buffer && !self.policy().full_buffer) {
            return false;
        }
        if let Some(active) = self
            .active
            .iter_mut()
            .find(|a| a.key == cue.cue.key() && a.cue_id == Some(cue.event_id) && a.pass.is_none())
        {
            if let (Some(tween), Some(duration)) = (&mut active.tween, pass.catalogue_duration()) {
                tween.duration = duration.min(Duration::from_secs(20));
            }
            active.pass = Some(pass);
            return true;
        }
        false
    }
    pub fn caps(&self) -> &TerminalCaps {
        &self.caps
    }
    /// Collapsing a report, accepting/cancelling a pick, or failing a contest
    /// removes all effects for that contest while keeping reduced state intact.
    pub fn settle_contest(&mut self, contest: &str) {
        self.active.retain(|a| !a.key.belongs_to(contest));
        self.queue.retain(|cue| !cue.cue.key().belongs_to(contest));
    }
    pub fn post(&mut self, buffer: &mut Buffer, area: Rect, now: Duration) {
        let policy = self.policy();
        if policy.snap {
            self.active.clear();
            return;
        }
        for active in &mut self.active {
            if !active.tween.is_some_and(|t| t.done(now))
                && let Some(pass) = &mut active.pass
                && (!pass.full_buffer || policy.full_buffer)
            {
                pass.process_with_policy(
                    now.saturating_sub(active.start),
                    buffer,
                    area,
                    &self.protected,
                    self.caps.color,
                    policy,
                );
            }
        }
        self.active.retain(|a| {
            a.tween.map_or_else(
                || a.pass.as_ref().is_some_and(Pass::running),
                |t| !t.done(now),
            )
        });
    }
    /// Relative delay for the one TUI event loop. `None` disarms its frame timer.
    pub fn next_wake(&self) -> Option<Duration> {
        let policy = self.policy();
        if !self.queue.is_empty() {
            return Some(Duration::ZERO);
        }
        if !policy.animate {
            return None;
        }
        if !self.active.is_empty() || (self.streams && !policy.snap) {
            return Some(policy.frame_interval(self.active.iter().any(|a| a.hero)));
        }
        if self.periodic {
            return Some(Duration::from_millis(100).max(policy.frame_interval(false)));
        }
        if self.ambience
            && policy.ambience
            && self
                .last_cue
                .is_some_and(|t| self.clock.now().saturating_sub(t) < Duration::from_secs(20))
        {
            return Some(Duration::from_millis(250));
        }
        None
    }
    /// Skip/resize/stop settles to the widgets' authoritative final state; consumes no key.
    pub fn settle(&mut self) {
        self.active.clear();
        self.queue.clear();
        self.ambience = false;
        self.streams = false;
        self.periodic = false;
    }
    pub fn set_level(&mut self, level: FxLevel) {
        self.caps.level = level;
        if level == FxLevel::Off {
            self.settle();
        }
    }
    pub fn cancel(&mut self, key: &FxKey) {
        self.active.retain(|a| &a.key != key);
    }
}

impl FxKey {
    pub fn belongs_to(&self, id: &str) -> bool {
        match self {
            Self::Catalogue { target, .. } => target == id,
            Self::ModeBanner { contest, .. }
            | Self::Round { contest, .. }
            | Self::Score { contest, .. }
            | Self::Ranking { contest, .. }
            | Self::Eliminated { contest, .. }
            | Self::Candidate { contest, .. } => contest == id,
            Self::Lead(v)
            | Self::Verdict(v)
            | Self::Champion(v)
            | Self::Turn(v)
            | Self::Plan(v)
            | Self::Survived(v)
            | Self::Pick(v)
            | Self::Census(v)
            | Self::Crosscheck(v)
            | Self::Survey(v)
            | Self::Probe(v) => v == id,
            _ => false,
        }
    }
}
