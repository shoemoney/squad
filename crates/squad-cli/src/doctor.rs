use std::{ffi::OsString, fmt::Write, path::Path};

#[derive(Debug, PartialEq)]
pub enum Colour {
    None,
    Ansi16,
    Ansi256,
    TrueColor,
}

pub fn colour(env: &impl Fn(&str) -> Option<OsString>) -> Colour {
    let get = |name| {
        env(name)
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase()
    };
    if env("NO_COLOR").is_some_and(|v| !v.is_empty()) {
        return Colour::None;
    }
    let term = get("TERM");
    if term.is_empty() || term == "dumb" {
        return Colour::None;
    }
    if matches!(get("COLORTERM").as_str(), "truecolor" | "24bit") {
        Colour::TrueColor
    } else if term.contains("256color") {
        Colour::Ansi256
    } else {
        Colour::Ansi16
    }
}

pub struct Report {
    pub text: String,
    pub code: u8,
}

// TODO(M0 merge): use SquadConfig for merged values, validation and provenance.
// TODO(M3.P3): replace skipped checks with typed readers and integration checks.
pub fn run(
    data_dir: Result<&Path, &str>,
    config_error: Option<&str>,
    env: &impl Fn(&str) -> Option<OsString>,
    terminal: bool,
    size: Option<(u16, u16)>,
    show_env: bool,
) -> Report {
    let mut text = String::new();
    let mut failed = false;
    if let Some(error) = config_error {
        let _ = writeln!(text, "Configuration: FAIL - {error}");
        failed = true;
    } else {
        text.push_str(
            "Configuration: skipped until M0 merge (SquadConfig and environment-file loading)\n",
        );
    }
    let present = env("OPENROUTER_API_KEY").is_some_and(|v| !v.is_empty());
    if present {
        text.push_str("Credential presence: present\n");
    } else {
        text.push_str("Credential presence: missing - OPENROUTER_API_KEY is required. Add it to .env or the process environment.\n");
        failed = true;
    }
    text.push_str(
        "OpenRouter reachability: skipped until M2\nModel resolution: skipped until M2\n",
    );
    let writable = match data_dir {
        Ok(path) => match crate::paths::check_writable(path) {
            Ok(()) => {
                let _ = writeln!(text, "Data directory: writable - {}", path.display());
                true
            }
            Err(_) => {
                let _ = writeln!(
                    text,
                    "Data directory: FAIL - Data directory is not writable: {}.",
                    path.display()
                );
                false
            }
        },
        Err(error) => {
            let _ = writeln!(text, "Data directory: FAIL - {error}");
            false
        }
    };
    failed |= !writable;
    let colour = match colour(env) {
        Colour::None => "none",
        Colour::Ansi16 => "16 colours",
        Colour::Ansi256 => "256 colours",
        Colour::TrueColor => "true colour",
    };
    let dimensions = size
        .map(|(w, h)| format!("{w}x{h}"))
        .unwrap_or_else(|| "unavailable".into());
    let suitable = terminal
        && size.is_some_and(|(w, h)| w > 0 && h > 0)
        && env("TERM").is_some_and(|v| !v.is_empty() && v != "dumb");
    let status = if suitable {
        "available"
    } else {
        "unsuitable for TUI (noninteractive commands remain usable)"
    };
    let _ = writeln!(
        text,
        "Terminal capabilities: {status}; colour: {colour}; size: {dimensions}"
    );
    text.push_str("Daily accounting: skipped until M3\nModel identity: skipped until M3\n");
    if writable {
        let level_value = env("SQUAD_LOG_LEVEL");
        let level = level_value
            .as_ref()
            .map(|v| v.to_str().unwrap_or("invalid"));
        let result = crate::logging::level(level)
            .and_then(|level| crate::logging::init(data_dir.unwrap(), level));
        if let Err(error) = result {
            let _ = writeln!(text, "Logging: FAIL - {error}");
            failed = true;
        }
    }
    if show_env {
        text.push_str("\nSupported environment variables and defaults:\n");
        text.push_str(include_str!("env-help.md"));
        text.push_str("\nProcess environment overrides file values, including empty values. Load exactly one environment file.\nPersisted ROUNDS and GRILL_ROUNDS override their environment seeds.\nEffective values and sources: skipped until M0 merge (SquadConfig); credential presence is reported above.\n");
    }
    let _ = writeln!(
        text,
        "Overall: {}",
        if failed {
            "FAIL"
        } else {
            "PASS (deferred checks skipped)"
        }
    );
    Report {
        text,
        code: u8::from(failed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn colour_tiers_and_no_color_precedence() {
        for (term, colorterm, no_color, expected) in [
            ("xterm", "", "", Colour::Ansi16),
            ("xterm-256color", "", "", Colour::Ansi256),
            ("xterm", "truecolor", "", Colour::TrueColor),
            ("xterm", "24bit", "", Colour::TrueColor),
            ("xterm", "truecolor", "1", Colour::None),
            ("dumb", "", "", Colour::None),
        ] {
            let env = |name: &str| {
                Some(OsString::from(match name {
                    "TERM" => term,
                    "COLORTERM" => colorterm,
                    "NO_COLOR" => no_color,
                    _ => "",
                }))
            };
            assert_eq!(colour(&env), expected);
        }
    }
    #[test]
    fn failure_snapshot_and_independent_checks() {
        let report = run(
            Err("injected path failure"),
            None,
            &|_| None,
            false,
            Some((80, 24)),
            false,
        );
        assert_eq!(report.code, 1);
        assert_eq!(
            report.text,
            include_str!("../tests/fixtures/doctor-missing.txt")
        );
    }
    #[test]
    fn presence_never_prints_value() {
        let env =
            |name: &str| (name == "OPENROUTER_API_KEY").then(|| OsString::from("fixture-secret"));
        let report = run(
            Err("injected path failure"),
            Some("injected config failure"),
            &env,
            false,
            None,
            true,
        );
        assert!(report.text.contains("Credential presence: present"));
        assert!(!report.text.contains("fixture-secret"));
        assert!(report.text.contains("Model identity: skipped until M3"));
        assert!(report.text.contains("SQUAD_REASONING_EFFORT_<NICK>"));
    }
}
