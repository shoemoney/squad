use std::{fs, process::Command};

#[test]
fn census_seed_reaches_cli_with_file_and_process_precedence() {
    let root = tempfile::tempdir().unwrap();
    for (file, process, expected) in [
        (None, None, "SQUAD_CENSUS_PASSES seed: 2"),
        (Some("3"), None, "SQUAD_CENSUS_PASSES seed: 3"),
        (Some("3"), Some("5"), "SQUAD_CENSUS_PASSES seed: 5"),
        (
            Some("3"),
            Some(""),
            "SQUAD_CENSUS_PASSES must be an integer from 1 to 5.",
        ),
    ] {
        let mut text = "OPENROUTER_API_KEY=fixture\n".to_owned();
        if let Some(value) = file {
            text.push_str(&format!("SQUAD_CENSUS_PASSES={value}\n"));
        }
        fs::write(root.path().join(".env"), text).unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_squad"));
        command
            .env_clear()
            .env("HOME", root.path())
            .current_dir(root.path())
            .args(["doctor", "--env"]);
        if let Some(value) = process {
            command.env("SQUAD_CENSUS_PASSES", value);
        }
        let output = command.output().unwrap();
        assert!(
            String::from_utf8_lossy(&output.stdout).contains(expected),
            "{output:?}"
        );
        assert_eq!(output.status.success(), process != Some(""));
    }
    assert!(!include_str!("../../../.env.example").contains("SQUAD_CENSUS_PASSES"));
}
