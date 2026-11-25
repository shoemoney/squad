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
    let cwd =
        std::env::current_dir().map_err(|_| "Cannot resolve working directory.".to_owned())?;
    // One snapshot is passed to the sole configuration loader. Terminal detection
    // below reads only terminal variables, never application configuration.
    let environment = std::env::vars().collect();
    let mut options = squad_protocol::config::LoadOptions::from_environment(cwd, &environment);
    options.env_file = cli.env_file;
    options.data_dir = cli.data_dir;
    let config = squad_protocol::config::SquadConfig::load_with_env(&options, &environment);
    if let Some(Command::Doctor { env: show_env }) = cli.command {
        let report = doctor::run(
            &config,
            &options,
            &|name| std::env::var_os(name),
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
    let config = config.map_err(|error| error.to_string())?;
    paths::create_private_dir(&config.data_dir).map_err(|_| {
        format!(
            "Data directory is not writable: {}.",
            config.data_dir.display()
        )
    })?;
    logging::init(&config.data_dir, logging::level(config.log_level))?;
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
