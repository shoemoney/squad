//! Presentation notices only: committed state and persistent markers stay in the reducer.
use squad_protocol::{EventId, MessageId, Nick};
use std::{collections::VecDeque, time::Duration};

pub const CUE_CAPACITY: usize = 256;
/// Internal delivery metadata; deliberately not part of Protocol's FeedItem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedOrigin {
    Live,
    Replay,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FxCue {
    BubbleOpened {
        id: MessageId,
        model: Nick,
    },
    BubbleCompleted {
        id: MessageId,
    },
    BubbleFailed {
        id: MessageId,
    },
    RoundStarted {
        contest_id: EventId,
        n: u32,
        final_round: bool,
    },
    ScorePosted {
        contest_id: EventId,
        round: u32,
    },
    LeadChanged {
        contest_id: EventId,
        member: Nick,
    },
    VerdictReady {
        contest_id: EventId,
    },
    DailySpendChanged,
    TournamentRankingReady {
        contest_id: EventId,
        round: u32,
    },
    TournamentEliminated {
        contest_id: EventId,
        member: Nick,
    },
    TournamentChampion {
        contest_id: EventId,
        member: Nick,
    },
    GrillTurnChanged {
        contest_id: EventId,
        member: Nick,
    },
    GrillPlanReady {
        contest_id: EventId,
        version: u32,
    },
    GrillSurvived {
        contest_id: EventId,
    },
    PickRequested {
        contest_id: EventId,
    },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimedCue {
    /// Original event identity, suitable for deterministic RNG seeding and deduplication.
    pub event_id: EventId,
    /// Original event time translated to the manager clock's epoch, NOT replay receipt time.
    pub at: Duration,
    pub cue: FxCue,
}
impl TimedCue {
    /// Stable across machines and Rust releases; never use a randomized HashMap hasher.
    pub fn rng_seed(&self) -> u32 {
        seed_bytes(self.event_id.to_string().as_bytes())
    }
}
pub(crate) fn seed_bytes(bytes: &[u8]) -> u32 {
    bytes.iter().fold(2_166_136_261u32, |hash, byte| {
        (hash ^ u32::from(*byte)).wrapping_mul(16_777_619)
    })
}
#[derive(Debug)]
pub struct FxQueue {
    cues: VecDeque<TimedCue>,
    dropped: u64,
}
impl Default for FxQueue {
    fn default() -> Self {
        Self::new()
    }
}
impl FxQueue {
    pub fn new() -> Self {
        Self {
            cues: VecDeque::with_capacity(CUE_CAPACITY),
            dropped: 0,
        }
    }
    /// Replay suppression is a second boundary guard; reducers should not emit replay cues.
    pub fn push(&mut self, origin: FeedOrigin, cue: TimedCue) {
        if origin == FeedOrigin::Replay {
            return;
        }
        if self.cues.len() == CUE_CAPACITY {
            self.cues.pop_front();
            self.dropped += 1;
        }
        self.cues.push_back(cue);
    }
    pub fn pop_front(&mut self) -> Option<TimedCue> {
        self.cues.pop_front()
    }
    pub fn len(&self) -> usize {
        self.cues.len()
    }
    pub fn is_empty(&self) -> bool {
        self.cues.is_empty()
    }
    pub fn dropped(&self) -> u64 {
        self.dropped
    }
    pub fn clear(&mut self) {
        self.cues.clear();
    }
}
