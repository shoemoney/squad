use std::{
    fs,
    path::Path,
    process::{Command, Output, Stdio},
};

const KEY: &str = "sk-test-not-a-real-key";

fn invoke(root: &Path, args: &[&str], environment: &[(&str, &str)]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_squad"))
        .env_clear()
        .env("HOME", root)
        .envs(environment.iter().copied())
        .current_dir(root)
        .stdin(Stdio::null())
        .args(args)
        .output()
        .unwrap()
}

fn stdout(output: &Output) -> &str {
    std::str::from_utf8(&output.stdout).unwrap()
}

fn assert_secret_absent(output: &Output, log: &Path) {
    assert!(!stdout(output).contains(KEY));
    assert!(!std::str::from_utf8(&output.stderr).unwrap().contains(KEY));
    let contents = fs::read_to_string(log).unwrap();
    assert!(!contents.contains(KEY));
}

#[test]
fn doctor_loads_dotenv_defaults_without_exposing_key_and_rejects_bad_roster() {
    let temp = tempfile::tempdir().unwrap();
    let canonical_root = temp.path().canonicalize().unwrap();
    let root = canonical_root.as_path();
    let env_file = root.join(".env");
    fs::write(
        &env_file,
        format!("OPENROUTER_API_KEY={KEY}\nSQUAD_DATA_DIR=data\n"),
    )
    .unwrap();
    let output = invoke(root, &["doctor", "--env"], &[]);
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let text = stdout(&output);
    let roster: Vec<_> = text
        .lines()
        .filter(|line| line.starts_with("  "))
        .take(6)
        .collect();
    assert_eq!(
        roster,
        [
            "  claude: ~anthropic/claude-opus-latest",
            "  fable: ~anthropic/claude-fable-latest",
            "  openai: ~openai/gpt-sol-latest",
            "  gemini: google/gemini-4-neon",
            "  grok: ~x-ai/grok-latest",
            "  muse: meta/muse-wonder",
        ]
    );
    assert!(text.contains(&format!("Configuration: PASS - {}", env_file.display())));
    assert!(text.contains("rounds: 10; grill rounds: 2; daily max USD: 150; judge model: ~anthropic/claude-fable-latest"));
    assert!(text.contains("Credential presence: present"));
    for variable in [
        "SQUAD_PERSONA_<NICK>",
        "SQUAD_REASONING_EFFORT_<NICK>",
        "SQUAD_BANNERS",
        "SQUAD_FX",
        "SQUAD_FX_DEBUG",
        "SQUAD_PLAN_WORDS",
    ] {
        assert!(text.contains(variable));
    }
    let log = root.join("data/logs/squad.jsonl");
    assert!(!fs::read_to_string(&log).unwrap().is_empty());
    assert_secret_absent(&output, &log);

    fs::write(
        &env_file,
        format!("OPENROUTER_API_KEY={KEY}\nSQUAD_MODEL_CLAUDE=\n"),
    )
    .unwrap();
    let output = invoke(root, &["doctor"], &[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).lines().any(|line| line
        == "Configuration: FAIL - SQUAD_MODEL_CLAUDE must contain an OpenRouter model slug."));
    assert!(stdout(&output).contains("Terminal capabilities:"));
    assert_secret_absent(&output, &log);
    let output = invoke(root, &["models"], &[]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        std::str::from_utf8(&output.stderr).unwrap(),
        "SQUAD_MODEL_CLAUDE must contain an OpenRouter model slug.\n"
    );
    assert_secret_absent(&output, &log);

    fs::write(
        &env_file,
        format!("OPENROUTER_API_KEY={KEY}\nSQUAD_MODEL_CLAUDE\n"),
    )
    .unwrap();
    let output = invoke(root, &["doctor"], &[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).lines().any(|line| line
        == format!(
            "Configuration: FAIL - Invalid environment file: {}, line 2.",
            env_file.display()
        )));
    assert_secret_absent(&output, &log);
}

#[test]
fn file_selection_environment_precedence_and_relative_paths_reach_binary() {
    let temp = tempfile::tempdir().unwrap();
    let canonical_root = temp.path().canonicalize().unwrap();
    let root = canonical_root.as_path();
    // These are the OS paths used by LoadOptions on the supported platforms.
    let config_dir = if cfg!(target_os = "macos") {
        root.join("Library/Application Support/squad")
    } else {
        root.join(".config/squad")
    };
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(
        config_dir.join(".env"),
        format!("OPENROUTER_API_KEY={KEY}\nSQUAD_DATA_DIR=from-os-file\nSQUAD_ROUNDS=3\n"),
    )
    .unwrap();
    let output = invoke(root, &["doctor"], &[]);
    assert_eq!(output.status.code(), Some(0));
    assert!(stdout(&output).contains("rounds: 3;"));
    assert!(config_dir.join("from-os-file/logs/squad.jsonl").exists());

    fs::write(
        root.join(".env"),
        format!("OPENROUTER_API_KEY={KEY}\nSQUAD_DATA_DIR=local\nSQUAD_ROUNDS=4\n"),
    )
    .unwrap();
    let output = invoke(root, &["doctor"], &[]);
    assert_eq!(output.status.code(), Some(0));
    assert!(stdout(&output).contains("rounds: 4;"));
    assert!(root.join("local/logs/squad.jsonl").exists());

    let selected = root.join("selected");
    fs::create_dir(&selected).unwrap();
    fs::write(selected.join("custom.env"), format!("OPENROUTER_API_KEY={KEY}\nSQUAD_DATA_DIR=file-data\nSQUAD_LOG_LEVEL=off\nSQUAD_MODEL_ZETA=~vendor/zeta-latest\nSQUAD_MODEL_ALPHA=vendor/alpha\nSQUAD_ROUNDS=5\nSQUAD_GRILL_ROUNDS=3\nSQUAD_DAILY_MAX_USD=12.5\nSQUAD_JUDGE_MODEL=vendor/judge\n")).unwrap();
    let args = ["doctor", "--env-file", "selected/custom.env"];
    let output = invoke(
        root,
        &args,
        &[
            ("SQUAD_ROUNDS", "7"),
            ("SQUAD_MODEL_ZETA", "~vendor/override-latest"),
            ("SQUAD_MODEL_BETA", "vendor/beta"),
        ],
    );
    assert_eq!(output.status.code(), Some(0));
    let text = stdout(&output);
    assert!(
        text.contains(
            "  zeta: ~vendor/override-latest\n  alpha: vendor/alpha\n  beta: vendor/beta\n"
        )
    );
    assert!(
        text.contains("rounds: 7; grill rounds: 3; daily max USD: 12.5; judge model: vendor/judge")
    );
    let log = selected.join("file-data/logs/squad.jsonl");
    assert_eq!(fs::read_to_string(&log).unwrap(), "");
    assert_secret_absent(&output, &log);

    let output = invoke(
        root,
        &args,
        &[
            ("SQUAD_DATA_DIR", "environment-data"),
            ("SQUAD_LOG_LEVEL", "info"),
        ],
    );
    assert_eq!(output.status.code(), Some(0));
    assert!(
        !fs::read_to_string(selected.join("environment-data/logs/squad.jsonl"))
            .unwrap()
            .is_empty()
    );
    let output = invoke(
        root,
        &[
            "doctor",
            "--env-file",
            "selected/custom.env",
            "--data-dir",
            "flag-data",
        ],
        &[],
    );
    assert_eq!(output.status.code(), Some(0));
    assert!(root.join("flag-data/logs/squad.jsonl").exists());
    assert!(!selected.join("flag-data").exists());

    let output = invoke(root, &args, &[("OPENROUTER_API_KEY", "")]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).contains("Credential presence: missing"));
    let output = invoke(root, &args, &[("SQUAD_LOG_LEVEL", "invalid")]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).contains(
        "Configuration: FAIL - SQUAD_LOG_LEVEL must be trace, debug, info, warn, error, or off."
    ));
}
