//! Deterministic, bounded presentation layered over a complete static TUI.
//!
//! Feed reducers push live [`TimedCue`] values into [`FxManager::queue_mut`]. Before
//! rendering, register current message-body rectangles in [`FxContext`] and call
//! [`FxManager::pre`]. Widgets sample its read-only view (missing entries mean final
//! state). After widgets render, call [`FxManager::post`]. Arm the TUI's single timer
//! from [`FxManager::next_wake`], then report draw+flush cost with `observe_frame`.
//! Catalogue integration: iterate `ctx.cues` after `pre`, read each committed
//! artifact from reduced state, and `attach_pass(cue, Pass::catalogue(...))` with
//! its dedicated graphic rect. Painters receive elapsed time and governor policy;
//! use `caps().level`, `caps().color` and the shared `Palette` for construction.
//! `FxView::progress` is the scalar widget hook; absent keys mean final state.
//! Draw complete static widgets every frame, including partial committed reports.
//! Call `settle_contest` on collapse, accepted/cancelled pick, STOPPED or FAILED;
//! call `settle` before resize, global stop and animation skip. A skipped key
//! continues to the normal input handler. Reopen/replay draws static state only.
//! Pending row highlights use `FxKey::Candidate { contest, member }`; set
//! `ctx.pending_pick` from authoritative state so acceptance/cancellation removes them.
//! Local UI actions use `FxCue::Catalogue`; protocol artifacts use typed cues.
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
pub use cue::{CatalogueEffect, FeedOrigin, FxCue, FxQueue, ReportMode, TimedCue};
pub use governor::{FrameSample, FxPolicy, Governor, Grade};
pub use level::FxLevel;
pub use manager::{Ease, FxContext, FxKey, FxManager, FxView, Tween};
pub use palette::Palette;
pub use pass::{ColorOnly, Pass, PassAccess};

pub mod banner;
pub mod bubble;
pub mod census;
pub mod chrome;
pub mod command;
pub mod crosscheck;
pub mod debate;
pub mod dispatch;
pub mod filter;
pub mod grill;
pub mod meter;
pub mod particles;
pub mod pick;
pub mod probe;
pub mod status;
pub mod survey;
pub mod tournament;
pub mod verdict;
