//! Tune effects obey the shared contest dispatch and reduced-motion controls.
pub fn enabled() -> bool {
    squad_protocol::tune::ENABLED
}
