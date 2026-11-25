mod cli;
mod doctor;
mod logging;
mod paths;

use cli::{Cli, Command};
use squad_protocol::ErrorCode;
use std::{io::IsTerminal, process::ExitCode};

struct CliFailure {
    code: ErrorCode,
    message: String,
}
impl From<String> for CliFailure {
    fn from(message: String) -> Self {
        Self {
            code: ErrorCode::INTERNAL_ERROR,
            message,
        }
    }
}
impl From<&str> for CliFailure {
    fn from(message: &str) -> Self {
        message.to_owned().into()
    }
}
impl CliFailure {
    fn config(message: String) -> Self {
        Self {
            code: ErrorCode::CONFIG_INVALID,
            message,
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::validated();
    let json = matches!(&cli.command, Some(Command::Send(args)) if args.json);
    match run(cli) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            if json {
                let _ = squad_cli::output::Output {
                    json: true,
                    stdout: &mut std::io::stdout(),
                    stderr: &mut std::io::stderr(),
                }
                .error(None, error.code, &error.message, false);
            } else {
                eprintln!("{}", error.message);
            }
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<u8, CliFailure> {
    let cwd =
        std::env::current_dir().map_err(|_| "Cannot resolve working directory.".to_owned())?;
    // One snapshot is passed to the sole configuration loader. Terminal detection
    // below reads only terminal variables, never application configuration.
    let (environment, environment_errors) =
        squad_protocol::config::snapshot_environment(std::env::vars_os());
    let mut options = squad_protocol::config::LoadOptions::from_environment(cwd, &environment);
    options.env_file = cli.env_file;
    options.data_dir = cli.data_dir;
    let mut effective = squad_protocol::config::EffectiveEnvironment::load(&options, &environment);
    effective.errors.extend(environment_errors);
    let config = effective.validate(&options).map_err(|mut errors| {
        // The snapshot blanks invalid UTF-8 values to keep them out of diagnostics.
        // Preserve the specific encoding error without claiming the key is missing.
        if errors.iter().any(|error| {
            matches!(error,
            squad_protocol::config::ConfigError::NonUtf8Environment(name)
                if name == "OPENROUTER_API_KEY")
        }) {
            errors.retain(|error| {
                !matches!(error, squad_protocol::config::ConfigError::MissingApiKey)
            });
        }
        errors
    });
    if let Some(Command::Export(export)) = &cli.command {
        let data_dir = effective
            .data_dir(&options)
            .map_err(|error| error.to_string())?;
        squad_cli::export::run(&data_dir, export, &mut std::io::stdout())?;
        return Ok(0);
    }
    if let Some(Command::Models) = &cli.command {
        if let Err(errors) = &config {
            let errors: Vec<_> = errors
                .iter()
                .filter(|error| {
                    !matches!(error, squad_protocol::config::ConfigError::MissingApiKey)
                })
                .map(ToString::to_string)
                .collect();
            if !errors.is_empty() {
                return Err(CliFailure::config(errors.join("\n")));
            }
        }
        let data_dir = effective
            .data_dir(&options)
            .map_err(|error| error.to_string())?;
        print!(
            "{}",
            squad_cli::models::run(&data_dir, config.as_ref().ok())?
        );
        return Ok(0);
    }
    if let Some(Command::Doctor { env: show_env }) = cli.command {
        let report = doctor::run(
            &config,
            &options,
            &effective,
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
    let config = config.map_err(|errors| {
        CliFailure::config(
            errors
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n"),
        )
    })?;
    paths::create_private_dir(&config.data_dir).map_err(|_| {
        format!(
            "Data directory is not writable: {}.",
            config.data_dir.display()
        )
    })?;
    logging::init(
        &config.data_dir,
        logging::level(config.log_level),
        &config.openrouter_api_key,
    )?;
    tracing::info!("CLI initialized");
    // INTEGRATION STUB: the coordinator supplies RoomHandle and binds RoomApi.
    // Open using OpenPolicy::send(send.no_wait), then call send::run.
    // Interactive open uses OpenPolicy::interactive(); squad_tui::run(room).await
    // is the expected M4 entry point. Always shutdown after it returns.
    // TODO(M4.P1): launch the TUI with OpenMode::Interactive.
    let (operation, milestone) = match cli.command {
        None => ("TUI", "M4"),
        Some(Command::Send(_)) => ("send (RoomApi binding required)", "M3"),
        Some(Command::Export(_)) => ("export", "M3"),
        Some(Command::Models) => ("models", "M3"),
        Some(Command::Doctor { .. }) => unreachable!(),
    };
    Err(CliFailure {
        code: ErrorCode::COMMAND_NOT_IMPLEMENTED,
        message: format!("{operation}: not implemented yet (milestone {milestone})"),
    })
}
