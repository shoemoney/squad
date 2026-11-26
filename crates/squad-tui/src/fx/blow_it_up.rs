//! Severity accents are static, including with motion disabled; no model text animates.
use ratatui::style::Color;
use squad_protocol::blow_it_up::Severity;
pub fn enabled() -> bool {
    squad_protocol::blow_it_up::ENABLED
}
pub fn severity_color(severity: Severity) -> Color {
    match severity {
        Severity::Critical => Color::Red,
        Severity::High => Color::LightRed,
        Severity::Medium => Color::Yellow,
        Severity::Low => Color::Cyan,
    }
}
