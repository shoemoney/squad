//! Decide uses the shared banner effects; arithmetic is never animated or changed.
pub const DIVIDERS: [&str; 4] = ["OPTIONS", "RUBRIC", "SCORES", "WHAT FLIPS IT"];
pub fn enabled() -> bool {
    squad_protocol::decide::ENABLED
}
