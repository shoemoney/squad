mod cli;
mod doctor;
mod logging;
mod paths;

use cli::{Cli, Command};
use std::{io::IsTerminal, process::ExitCode};

fn main() -> ExitCode {
    match run(Cli::validated()) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<u8, String> {
    // TODO(M0 merge): use SquadConfig and the selected, merged environment.
    // Do not read .env here while configuration loading is owned separately.
    let env = |name: &str| std::env::var_os(name);
    let cwd =
        std::env::current_dir().map_err(|_| "Cannot resolve working directory.".to_owned())?;
    let platform = paths::Platform::current();
    let data_dir = paths::data_dir(
        cli.data_dir.as_deref(),
        cli.env_file.as_deref(),
        &cwd,
        platform,
        &env,
    );
    let config_error = cli
        .env_file
        .as_ref()
        .filter(|path| !path.exists())
        .map(|path| format!("Environment file not found: {}.", path.display()));
    if let Some(Command::Doctor { env: show_env }) = cli.command {
        let report = doctor::run(
            data_dir.as_deref().map_err(String::as_str),
            config_error.as_deref(),
            &env,
            std::io::stdin().is_terminal() && std::io::stdout().is_terminal(),
            crossterm::terminal::size().ok(),
            show_env,
        );
        print!("{}", report.text);
        if cli.bus_log.is_some() {
            return Err("Bus diagnostics: not implemented yet (milestone M3)".into());
        }
        return Ok(report.code);
    }
    if let Some(error) = config_error {
        return Err(error);
    }
    let data_dir = data_dir?;
    paths::create_private_dir(&data_dir)
        .map_err(|_| format!("Data directory is not writable: {}.", data_dir.display()))?;
    // TODO(M0 merge): use this directory for environment-file selection.
    let _config_dir = paths::config_dir(platform, &env);
    let log_level = env("SQUAD_LOG_LEVEL");
    let level = logging::level(log_level.as_ref().map(|v| v.to_str().unwrap_or("invalid")))?;
    logging::init(&data_dir, level)?;
    tracing::info!("CLI initialized");
    // TODO(M3.P3): wire RoomHandle, offline exports, registry and bus diagnostics.
    // TODO(M4.P1): launch the TUI with OpenMode::Interactive.
    let (operation, milestone) = match cli.command {
        None => ("TUI", "M4"),
        Some(Command::Send(_)) => ("send", "M3"),
        Some(Command::Export(_)) => ("export", "M3"),
        Some(Command::Models) => ("models", "M3"),
        Some(Command::Doctor { .. }) => unreachable!(),
    };
    Err(format!(
        "{operation}: not implemented yet (milestone {milestone})"
    ))
}
