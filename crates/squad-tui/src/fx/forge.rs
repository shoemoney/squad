//! Forge uses the shared contest banner and verdict effects at the configured level.
pub fn enabled() -> bool {
    squad_protocol::forge::ENABLED
}
