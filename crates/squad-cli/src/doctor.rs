use std::{ffi::OsString, fmt::Write};

use squad_protocol::config::{ConfigError, EffectiveEnvironment, LoadOptions, SquadConfig};

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

pub fn run(
    config: &Result<SquadConfig, Vec<ConfigError>>,
    options: &LoadOptions,
    effective: &EffectiveEnvironment,
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
            let _ = writeln!(
                text,
                "SQUAD_CENSUS_PASSES seed: {} (default 2, range 1..=5; stored CENSUS_PASSES takes precedence)",
                config.defaults.census_passes
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
        }
        Err(errors) => {
            for error in errors {
                let _ = writeln!(text, "Configuration: FAIL - {error}");
            }
            failed = true;
        }
    }
    let _ = writeln!(
        text,
        "Credential presence: {}",
        if effective.credential_present() {
            "present"
        } else {
            "missing"
        }
    );
    let resolved_data_dir = effective.data_dir(options);
    text.push_str("Online checks not run (use --online).\n");
    let writable = match resolved_data_dir
        .as_ref()
        .map_err(ToString::to_string)
        .and_then(|path| {
            crate::paths::check_writable(path)
                .map_err(|_| format!("Data directory is not writable: {}.", path.display()))
        }) {
        Ok(()) => {
            let path = resolved_data_dir.as_ref().expect("resolved writable path");
            let _ = writeln!(text, "Data directory: writable - {}", path.display());
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(metadata) = std::fs::metadata(path) {
                    let _ = writeln!(
                        text,
                        "Data directory mode: {:04o}",
                        metadata.permissions().mode() & 0o777
                    );
                }
            }
            true
        }
        Err(error) => {
            let _ = writeln!(text, "Data directory: FAIL - {error}");
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
    if let Ok(path) = &resolved_data_dir {
        let limit = config
            .as_ref()
            .map_or(20.0, |config| config.budget.daily_max_usd);
        let day = chrono::Local::now().format("%Y-%m-%d").to_string();
        for check in [
            squad_cli::diagnostics::accounting(path, &day, limit),
            squad_cli::diagnostics::identities(path),
            squad_cli::diagnostics::storage(
                path,
                squad_protocol::constants::STORAGE_CAPACITY_BYTES,
            ),
            squad_cli::diagnostics::census_passes(
                path,
                config.as_ref().ok().map(|c| c.defaults.census_passes),
                match effective.source("SQUAD_CENSUS_PASSES") {
                    squad_protocol::config::VariableSource::Default => "built-in default",
                    squad_protocol::config::VariableSource::File(_) => "environment file",
                    squad_protocol::config::VariableSource::Process => "process environment",
                },
            ),
        ] {
            match check {
                Ok(report) => text.push_str(&report),
                Err(error) => {
                    let _ = writeln!(text, "Diagnostics: FAIL - {error}");
                    failed = true;
                }
            }
        }
        match squad_cli::models::run(path, config.as_ref().ok()) {
            Ok(report) => {
                let _ = writeln!(text, "Cached model resolution:\n{report}");
            }
            Err(error) => {
                let _ = writeln!(text, "Model resolution: FAIL - {error}");
                failed = true;
            }
        }
    }
    if writable && let Ok(config) = config {
        if let Err(error) = crate::logging::init(
            &config.data_dir,
            crate::logging::level(config.log_level),
            &config.openrouter_api_key,
        ) {
            let _ = writeln!(text, "Logging: FAIL - {error}");
            failed = true;
        } else {
            tracing::info!("Doctor initialized");
        }
    }
    if show_env {
        text.push_str("\nSupported environment variables and defaults:\n");
        text.push_str(include_str!("env-help.md"));
        text.push_str("\nProcess environment overrides file values, including empty values. Load exactly one environment file.\nPersisted ROUNDS, GRILL_ROUNDS and CENSUS_PASSES override their environment seeds.\nLoaded roster and numeric values are reported above; credential values are never displayed.\n");
    }
    let _ = writeln!(text, "Overall: {}", if failed { "FAIL" } else { "PASS" });
    Report {
        text,
        code: u8::from(failed),
    }
}

/// Free, read-only checks. Do not initialize the runtime or probe model aliases.
pub async fn online(config: &SquadConfig) -> Report {
    let Ok(client) = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()
    else {
        return Report {
            text: "Online checks: FAIL - HTTP client unavailable\n".into(),
            code: 1,
        };
    };
    let Ok(base) = reqwest::Url::parse(&config.openrouter_base_url) else {
        return Report {
            text: "Online checks: FAIL - invalid provider URL\n".into(),
            code: 1,
        };
    };
    if !matches!(base.scheme(), "http" | "https")
        || !base.username().is_empty()
        || base.password().is_some()
        || base.query().is_some()
        || base.fragment().is_some()
    {
        return Report {
            text: "Online checks: FAIL - invalid provider URL\n".into(),
            code: 1,
        };
    }
    let Ok(authorization) = config.openrouter_api_key.authorization_header() else {
        return Report {
            text: "Online checks: FAIL - invalid credential format\n".into(),
            code: 1,
        };
    };
    let mut text = String::new();
    let mut failed = false;
    for (path, label) in [
        ("models", "OpenRouter reachability"),
        ("key", "OpenRouter key validity"),
    ] {
        let result = client
            .get(base.join(path).expect("endpoint"))
            .header(reqwest::header::AUTHORIZATION, authorization.clone())
            .send()
            .await;
        match result {
            Ok(response) if response.status().is_success() => {
                match response.json::<serde_json::Value>().await {
                    Ok(value) if value.get("data").is_some() => {
                        let _ = writeln!(text, "{label}: PASS");
                    }
                    _ => {
                        let _ = writeln!(text, "{label}: FAIL - invalid endpoint response");
                        failed = true;
                    }
                }
            }
            Ok(response) => {
                let _ = writeln!(text, "{label}: FAIL - HTTP {}", response.status().as_u16());
                failed = true;
            }
            Err(_) => {
                let _ = writeln!(text, "{label}: FAIL - network request failed");
                failed = true;
            }
        }
    }
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
        let effective = EffectiveEnvironment::load(&options, &Default::default());
        let report = run(
            &Err(vec![ConfigError::MissingApiKey]),
            &options,
            &effective,
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
