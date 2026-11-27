//! Negotiate uses the shared contest activity effects, gated by public admission.
pub fn enabled() -> bool {
    squad_protocol::negotiate::ENABLED
}
