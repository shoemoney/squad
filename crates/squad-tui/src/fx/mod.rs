//! Deterministic, bounded presentation layered over a complete static TUI.
//!
//! Feed reducers push live [`TimedCue`] values into [`FxManager::queue_mut`]. Before
//! rendering, register current message-body rectangles in [`FxContext`] and call
//! [`FxManager::pre`]. Widgets sample its read-only view (missing entries mean final
//! state). After widgets render, call [`FxManager::post`]. Arm the TUI's single timer
//! from [`FxManager::next_wake`], then report draw+flush cost with `observe_frame`.
//! Persistent state and banner text remain owned by the reducer and squad-core.
pub mod caps;
pub mod clock;
pub mod cue;
pub mod governor;
pub mod level;
pub mod manager;
pub mod palette;
pub mod pass;

pub use caps::{ColorTier, TerminalCaps};
pub use clock::{Clock, ManualClock, SystemClock};
pub use cue::{FeedOrigin, FxCue, FxQueue, TimedCue};
pub use governor::{FrameSample, FxPolicy, Governor, Grade};
pub use level::FxLevel;
pub use manager::{Ease, FxContext, FxKey, FxManager, FxView, Tween};
pub use palette::Palette;
pub use pass::{ColorOnly, Pass, PassAccess};
