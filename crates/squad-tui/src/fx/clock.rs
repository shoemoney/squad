//! The sole wall-clock boundary. All engine timelines use injected durations.
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

pub trait Clock {
    fn now(&self) -> Duration;
}
#[derive(Debug, Clone)]
pub struct SystemClock {
    epoch: Instant,
}
impl Default for SystemClock {
    fn default() -> Self {
        Self {
            epoch: Instant::now(),
        }
    }
}
impl Clock for SystemClock {
    fn now(&self) -> Duration {
        self.epoch.elapsed()
    }
}

/// Clones share one monotonic test timeline, including with a manager that owns a clone.
#[derive(Debug, Default, Clone)]
pub struct ManualClock(Arc<Mutex<Duration>>);
impl ManualClock {
    pub fn at(now: Duration) -> Self {
        Self(Arc::new(Mutex::new(now)))
    }
    pub fn set(&self, now: Duration) {
        let mut time = self.0.lock().expect("manual clock poisoned");
        assert!(now >= *time, "clock must not run backwards");
        *time = now;
    }
    pub fn advance(&self, delta: Duration) {
        let mut time = self.0.lock().expect("manual clock poisoned");
        *time = time.saturating_add(delta);
    }
}
impl Clock for ManualClock {
    fn now(&self) -> Duration {
        *self.0.lock().expect("manual clock poisoned")
    }
}
