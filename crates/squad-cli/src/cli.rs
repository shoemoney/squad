use std::path::PathBuf;

use clap::{Args, FromArgMatches, Parser, Subcommand};
use squad_protocol::{ContestId, ExportScope};

#[derive(Debug, Parser)]
#[command(
    name = "squad",
    version,
    about = "Open main and run the TUI",
    propagate_version = true
)]
pub struct Cli {
    /// Select the environment file explicitly
    #[arg(long, global = true, value_name = "path")]
    pub env_file: Option<PathBuf>,
    /// Override the effective data directory
    #[arg(long, global = true, value_name = "path")]
    pub data_dir: Option<PathBuf>,
    /// Write request, commit, and mailbox wake-up diagnostics as JSON lines
    #[arg(long, global = true, value_name = "file")]
    pub bus_log: Option<PathBuf>,
    /// Include human-visible message bodies in bus diagnostics; requires --bus-log
    #[arg(long, global = true, requires = "bus_log")]
    pub bus_log_bodies: bool,
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Open the room headless and submit one message through RoomHandle
    Send(Send),
    /// Read SQLite without opening the room runtime
    Export(Export),
    /// Print the resolved roster in roster order
    Models,
    /// Run independent checks and print one labeled result per check
    Doctor {
        /// Additionally print every supported variable, its default, effective nonsecret value, and source
        #[arg(long)]
        env: bool,
    },
}

#[derive(Debug, Args)]
pub struct Send {
    #[arg(value_name = "message")]
    pub message: String,
    /// Resolve committed event sequences to quote references; accept at most 12
    #[arg(long, value_name = "seq", num_args = 1, value_delimiter = ',', value_parser = clap::value_parser!(u64).range(1..))]
    pub quote: Vec<u64>,
    /// Preconfirm the contest plan derived from this submission
    #[arg(long)]
    pub yes: bool,
    /// Write the JSON lines records defined below
    #[arg(long)]
    pub json: bool,
    /// Exit after durable commit, print the accepted cursor, and leave accepted work for the next interactive open
    #[arg(long)]
    pub no_wait: bool,
}

#[derive(Debug)]
pub struct Export {
    pub scope: ExportScope,
    pub id: Option<ContestId>,
    pub output: Option<PathBuf>,
}

impl Export {
    fn scopes() -> impl Iterator<Item = (ExportScope, String)> {
        [
            ExportScope::Room,
            ExportScope::Debate,
            ExportScope::Grill,
            ExportScope::Tournament,
            ExportScope::Census,
            ExportScope::Crosscheck,
            ExportScope::Survey,
            ExportScope::Probe,
        ]
        .into_iter()
        .map(|scope| {
            let name = serde_json::to_value(&scope)
                .expect("scope serializes")
                .as_str()
                .expect("scope is a string")
                .to_owned();
            (scope, name)
        })
    }
}

// Adapt the protocol enum to clap without coupling protocol to a CLI dependency.
impl Args for Export {
    fn augment_args(command: clap::Command) -> clap::Command {
        let mut command = command.subcommand_required(true).arg(
            clap::Arg::new("output")
                .short('o')
                .global(true)
                .value_name("file")
                .help("Write UTF-8 markdown to that path and print the resulting path")
                .value_parser(clap::value_parser!(PathBuf)),
        );
        for (scope, name) in Self::scopes() {
            let mut subcommand = clap::Command::new(name.clone())
                .about(format!("Export {name} (not available yet)"));
            if scope != ExportScope::Room {
                subcommand = subcommand.arg(
                    clap::Arg::new("id")
                        .required(true)
                        .value_name("contest-id")
                        .value_parser(clap::value_parser!(ContestId)),
                );
            }
            command = command.subcommand(subcommand);
        }
        command
    }

    fn augment_args_for_update(command: clap::Command) -> clap::Command {
        Self::augment_args(command)
    }
}

impl FromArgMatches for Export {
    fn from_arg_matches(matches: &clap::ArgMatches) -> Result<Self, clap::Error> {
        let (name, args) = matches.subcommand().expect("required export scope");
        let scope = Self::scopes()
            .find(|(_, candidate)| candidate == name)
            .expect("clap validated scope")
            .0;
        let id = if scope == ExportScope::Room {
            None
        } else {
            args.get_one::<ContestId>("id").copied()
        };
        Ok(Self {
            scope,
            id,
            output: args.get_one::<PathBuf>("output").cloned(),
        })
    }

    fn update_from_arg_matches(&mut self, matches: &clap::ArgMatches) -> Result<(), clap::Error> {
        *self = Self::from_arg_matches(matches)?;
        Ok(())
    }
}

impl Cli {
    pub fn validated() -> Self {
        use clap::CommandFactory;
        let cli = Self::parse();
        if let Some(Command::Send(send)) = &cli.command
            && send.quote.len() > 12
        {
            Self::command()
                .error(
                    clap::error::ErrorKind::TooManyValues,
                    "--quote accepts at most 12 sequences",
                )
                .exit();
        }
        cli
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn clap_definition_is_consistent() {
        Cli::command().debug_assert();
    }

    #[test]
    fn parses_every_command() {
        assert!(Cli::try_parse_from(["squad"]).unwrap().command.is_none());
        let cli = Cli::try_parse_from([
            "squad",
            "send",
            "@all hello",
            "--quote",
            "1,2",
            "--quote",
            "3",
            "--yes",
            "--json",
            "--no-wait",
        ])
        .unwrap();
        let Some(Command::Send(send)) = cli.command else {
            panic!("expected send")
        };
        assert_eq!(send.message, "@all hello");
        assert_eq!(send.quote, [1, 2, 3]);
        assert!(send.yes && send.json && send.no_wait);
        for scope in [
            "room",
            "debate",
            "tournament",
            "grill",
            "census",
            "crosscheck",
            "survey",
            "probe",
        ] {
            let mut args = vec!["squad", "export", scope];
            if scope != "room" {
                args.push("01900000-0000-7000-8000-000000000001");
            }
            args.extend(["-o", "out.md"]);
            let cli = Cli::try_parse_from(args).unwrap();
            let Some(Command::Export(export)) = cli.command else {
                panic!("expected export")
            };
            assert_eq!(export.output, Some(PathBuf::from("out.md")));
            assert_eq!(export.id.is_some(), scope != "room");
            assert_eq!(serde_json::to_value(export.scope).unwrap(), scope);
        }
        assert!(matches!(
            Cli::try_parse_from(["squad", "models"]).unwrap().command,
            Some(Command::Models)
        ));
        assert!(matches!(
            Cli::try_parse_from(["squad", "doctor", "--env"])
                .unwrap()
                .command,
            Some(Command::Doctor { env: true })
        ));
    }

    #[test]
    fn globals_work_before_and_after_subcommands() {
        for args in [
            vec![
                "squad",
                "--env-file",
                "custom",
                "--data-dir",
                "data",
                "--bus-log",
                "bus",
                "--bus-log-bodies",
                "doctor",
            ],
            vec![
                "squad",
                "doctor",
                "--env-file",
                "custom",
                "--data-dir",
                "data",
                "--bus-log",
                "bus",
                "--bus-log-bodies",
            ],
        ] {
            let cli = Cli::try_parse_from(args).unwrap();
            assert_eq!(cli.env_file, Some("custom".into()));
            assert_eq!(cli.data_dir, Some("data".into()));
            assert_eq!(cli.bus_log, Some("bus".into()));
            assert!(cli.bus_log_bodies);
        }
    }

    #[test]
    fn rejects_invalid_combinations() {
        for args in [
            vec!["squad", "--bus-log-bodies"],
            vec!["squad", "doctor", "--bus-log-bodies"],
            vec!["squad", "export", "room", "extra-id"],
            vec!["squad", "export", "debate"],
            vec!["squad", "export", "tournament"],
            vec!["squad", "export", "grill"],
            vec!["squad", "send"],
            vec!["squad", "send", "hello", "--quote", "0"],
        ] {
            assert_eq!(Cli::try_parse_from(args).unwrap_err().exit_code(), 2);
        }
    }
}
