use std::{ffi::OsString, fmt::Write};

use squad_protocol::config::{ConfigError, LoadOptions, SquadConfig};

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

// TODO(M3.P3): replace skipped checks with typed readers and integration checks.
pub fn run(
    config: &Result<SquadConfig, ConfigError>,
    options: &LoadOptions,
    env: &impl Fn(&str) -> Option<OsString>,
    terminal: bool,
    size: Option<(u16, u16)>,
    show_env: bool,
) -> Report {
    let mut text = String::new();
    let mut failed = false;
    match config {
        Ok(config) => {
            let source = options
                .selected_env_file()
                .ok()
                .flatten()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "built-in defaults and process environment".into());
            let _ = writeln!(
                text,
                "Configuration: PASS - {source}; process environment overrides file values; parsing and references validated"
            );
            for agent in &config.agents {
                let _ = writeln!(text, "  {}: {}", agent.nick, agent.model);
            }
            let _ = writeln!(
                text,
                "  rounds: {}; grill rounds: {}; daily max USD: {}; judge model: {}",
                config.defaults.rounds,
                config.defaults.grill_rounds,
                config.budget.daily_max_usd,
                config.judge.model
            );
            text.push_str("Credential presence: present\n");
        }
        Err(error) => {
            let _ = writeln!(text, "Configuration: FAIL - {error}");
            failed = true;
            if matches!(error, ConfigError::MissingApiKey) {
                let _ = writeln!(text, "Credential presence: missing - {error}");
            } else {
                text.push_str(
                    "Credential presence: unavailable - configuration could not be loaded\n",
                );
            }
        }
    }
    // A failed configuration still permits an independent filesystem probe of
    // the CLI override or OS default; never reinterpret file settings here.
    let fallback = options
        .data_dir
        .as_ref()
        .map(|path| options.working_dir.join(path))
        .unwrap_or_else(|| options.default_data_dir.clone());
    let data_dir = config
        .as_ref()
        .map(|config| config.data_dir.as_path())
        .unwrap_or(&fallback);
    if config.is_err() {
        text.push_str(
            "Data directory source: CLI override or OS default (configuration unavailable)\n",
        );
    }
    text.push_str(
        "OpenRouter reachability: skipped until M2\nModel resolution: skipped until M2\n",
    );
    let writable = match crate::paths::check_writable(data_dir) {
        Ok(()) => {
            let _ = writeln!(text, "Data directory: writable - {}", data_dir.display());
            true
        }
        Err(_) => {
            let _ = writeln!(
                text,
                "Data directory: FAIL - Data directory is not writable: {}.",
                data_dir.display()
            );
            false
        }
    };
    failed |= !writable;
    let colour = match if config.as_ref().is_ok_and(|config| config.no_color) {
        Colour::None
    } else {
        colour(env)
    } {
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
    if writable && let Ok(config) = config {
        if let Err(error) = crate::logging::init(data_dir, crate::logging::level(config.log_level))
        {
            let _ = writeln!(text, "Logging: FAIL - {error}");
            failed = true;
        } else {
            tracing::info!("Doctor initialized");
        }
    }
    if show_env {
        text.push_str("\nSupported environment variables and defaults:\n");
        text.push_str(include_str!("env-help.md"));
        text.push_str("\nProcess environment overrides file values, including empty values. Load exactly one environment file.\nPersisted ROUNDS and GRILL_ROUNDS override their environment seeds.\nLoaded roster and numeric values are reported above; credential values are never displayed.\n");
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
    fn failure_continues_independent_checks() {
        let temp = tempfile::tempdir().unwrap();
        let blocker = temp.path().join("file");
        std::fs::write(&blocker, "fixture").unwrap();
        let mut options =
            LoadOptions::from_environment(temp.path().to_owned(), &Default::default());
        options.data_dir = Some(blocker);
        let report = run(
            &Err(ConfigError::MissingApiKey),
            &options,
            &|_| None,
            false,
            Some((80, 24)),
            true,
        );
        assert_eq!(report.code, 1);
        assert!(
            report
                .text
                .contains(&ConfigError::MissingApiKey.to_string())
        );
        assert!(report.text.contains("Credential presence: missing"));
        assert!(report.text.contains("Data directory: FAIL"));
        assert!(report.text.contains("Terminal capabilities:"));
        assert!(report.text.contains("SQUAD_REASONING_EFFORT_<NICK>"));
    }
}
