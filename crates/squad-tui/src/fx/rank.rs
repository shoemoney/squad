//! Rank opts into the shared contest-boundary effect only after enablement.
pub fn enabled() -> bool {
    squad_protocol::rank::ENABLED
}
