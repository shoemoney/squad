use squad_protocol::{SquadConfig, config::LoadOptions};
use std::collections::BTreeMap;

#[test]
fn census_seed_strict_syntax_diagnostics_and_precedence() {
    let root = tempfile::tempdir().unwrap();
    let options = LoadOptions {
        env_file: None,
        data_dir: None,
        working_dir: root.path().into(),
        config_dir: None,
        default_data_dir: root.path().join("data"),
    };
    let mut env = BTreeMap::from([("OPENROUTER_API_KEY".into(), "fixture".into())]);
    assert_eq!(
        SquadConfig::load_with_env(&options, &env)
            .unwrap()
            .defaults
            .census_passes,
        2
    );
    std::fs::write(root.path().join(".env"), "SQUAD_CENSUS_PASSES=3\n").unwrap();
    assert_eq!(
        SquadConfig::load_with_env(&options, &env)
            .unwrap()
            .defaults
            .census_passes,
        3
    );
    for value in ["1", "5"] {
        env.insert("SQUAD_CENSUS_PASSES".into(), value.into());
        assert_eq!(
            SquadConfig::load_with_env(&options, &env)
                .unwrap()
                .defaults
                .census_passes,
            value.parse::<u8>().unwrap()
        );
    }
    for value in [
        "",
        "0",
        "6",
        "256",
        "9999999999999999999999999",
        "1.5",
        "2x",
        "-1",
    ] {
        env.insert("SQUAD_CENSUS_PASSES".into(), value.into());
        assert_eq!(
            SquadConfig::load_with_env(&options, &env)
                .unwrap_err()
                .to_string(),
            "SQUAD_CENSUS_PASSES must be an integer from 1 to 5.",
            "{value:?}"
        );
    }
    assert!(!include_str!("../../../.env.example").contains("SQUAD_CENSUS_PASSES"));
}
