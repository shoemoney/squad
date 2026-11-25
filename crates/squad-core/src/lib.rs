//! Squad core: bus, router, contest engine, prompts, export and the room handle.
pub mod banner;
pub mod bus;
pub mod export;
pub mod orchestrator;
pub mod prompts;
pub mod room;
pub mod router;
pub mod runtime;
pub mod state;

pub use room::{RoomError, RoomHandle, export_offline};
pub use state::{DailySpend, RoomState, RoomVars};

mod resolution;
