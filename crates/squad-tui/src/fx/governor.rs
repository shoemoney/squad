//! Frame cost and output pressure, independent of draw scheduling and OS time.
use super::level::FxLevel;
use std::time::Duration;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Grade {
    #[default]
    G0,
    G1,
    G2,
    G3,
}
#[derive(Debug, Clone, Copy)]
pub struct FrameSample {
    pub draw_and_flush: Duration,
    pub bytes_per_second: u64,
    pub input_wait: Duration,
}
#[derive(Debug, Clone)]
pub struct Governor {
    grade: Grade,
    ewma_ms: Option<f64>,
    over: [u16; 3],
    recovery_since: Option<Duration>,
}
impl Default for Governor {
    fn default() -> Self {
        Self {
            grade: Grade::G0,
            ewma_ms: None,
            over: [0; 3],
            recovery_since: None,
        }
    }
}
impl Governor {
    pub fn grade(&self) -> Grade {
        self.grade
    }
    pub fn ewma_ms(&self) -> f64 {
        self.ewma_ms.unwrap_or(0.)
    }
    pub fn observe(&mut self, sample: FrameSample, now: Duration) -> Grade {
        let cost = sample.draw_and_flush.as_secs_f64() * 1000.;
        let ewma = self.ewma_ms.map_or(cost, |old| old + 0.2 * (cost - old));
        self.ewma_ms = Some(ewma);
        for (count, threshold) in self.over.iter_mut().zip([8., 14., 22.]) {
            *count = if ewma > threshold {
                count.saturating_add(1)
            } else {
                0
            };
        }
        let pressure = if sample.input_wait > Duration::from_millis(50) || self.over[2] >= 10 {
            Grade::G3
        } else if self.over[1] >= 15 || sample.bytes_per_second > 250_000 {
            Grade::G2
        } else if self.over[0] >= 15 || sample.bytes_per_second > 120_000 {
            Grade::G1
        } else {
            Grade::G0
        };
        if pressure > self.grade {
            self.grade = pressure;
            self.recovery_since = None;
        } else if self.grade != Grade::G0 {
            // Recovery has to fit the next better grade, not merely dip below the current one.
            let (limit, bytes) = match self.grade {
                Grade::G3 => (22., 250_000),
                Grade::G2 => (14., 120_000),
                _ => (8., 120_000),
            };
            if ewma <= limit
                && sample.bytes_per_second <= bytes
                && sample.input_wait <= Duration::from_millis(50)
                && pressure < self.grade
            {
                let since = *self.recovery_since.get_or_insert(now);
                if now.saturating_sub(since) >= Duration::from_secs(3) {
                    self.grade = match self.grade {
                        Grade::G3 => Grade::G2,
                        Grade::G2 => Grade::G1,
                        _ => Grade::G0,
                    };
                    self.recovery_since = None;
                    self.over = [0; 3];
                }
            } else {
                self.recovery_since = None;
            }
        } else {
            self.recovery_since = None;
        }
        self.grade
    }
    pub fn policy(&self, level: FxLevel, synchronized: bool) -> FxPolicy {
        let animate = level != FxLevel::Off;
        let full = level == FxLevel::Full;
        FxPolicy {
            animate,
            snap: !animate || self.grade == Grade::G3,
            ambience: full && self.grade == Grade::G0,
            comet_tail: full && self.grade == Grade::G0,
            particle_limit: if !full || self.grade >= Grade::G2 {
                0
            } else if self.grade == Grade::G1 {
                128
            } else {
                256
            },
            full_buffer: full && synchronized && self.grade < Grade::G2,
            fade_only: level == FxLevel::Lite || self.grade >= Grade::G2,
            fps: if self.grade == Grade::G3 {
                15
            } else if !full || self.grade >= Grade::G1 {
                30
            } else {
                60
            },
        }
    }
}
#[derive(Debug, Clone, Copy)]
pub struct FxPolicy {
    pub animate: bool,
    pub snap: bool,
    pub ambience: bool,
    pub comet_tail: bool,
    pub particle_limit: usize,
    pub full_buffer: bool,
    pub fade_only: bool,
    pub fps: u32,
}
impl FxPolicy {
    pub fn frame_interval(self, hero: bool) -> Duration {
        Duration::from_secs_f64(1. / f64::from(if hero { self.fps } else { self.fps.min(30) }))
    }
}
