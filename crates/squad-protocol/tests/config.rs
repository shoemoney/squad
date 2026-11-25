use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use squad_protocol::config::*;

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    root: PathBuf,
    options: LoadOptions,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "squad-config-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        let options = LoadOptions {
            env_file: None,
            data_dir: None,
            working_dir: root.clone(),
            config_dir: Some(root.join("config")),
            default_data_dir: root.join("data/squad"),
        };
        Self { root, options }
    }
    fn write(&self, relative: &str, contents: &str) -> PathBuf {
        let path = self.root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, contents).unwrap();
        path
    }
    fn load(&self, pairs: &[(&str, &str)]) -> Result<SquadConfig, ConfigError> {
        SquadConfig::load_with_env(&self.options, &environment(pairs))
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).unwrap();
    }
}
fn environment(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    let mut env = BTreeMap::from([(
        "OPENROUTER_API_KEY".to_owned(),
        "fixture-credential".to_owned(),
    )]);
    env.extend(pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())));
    env
}
fn error(pairs: &[(&str, &str)], expected: &str) {
    assert_eq!(
        Fixture::new().load(pairs).unwrap_err().to_string(),
        expected
    );
}

#[test]
fn defaults() {
    let f = Fixture::new();
    let c = f.load(&[]).unwrap();
    assert_eq!(c.room, "main");
    assert_eq!(
        c.agents
            .iter()
            .map(|a| (a.nick.as_str(), a.model.as_str()))
            .collect::<Vec<_>>(),
        vec![
            ("claude", "~anthropic/claude-opus-latest"),
            ("fable", "~anthropic/claude-fable-latest"),
            ("openai", "~openai/gpt-sol-latest"),
            ("gemini", "google/gemini-4-neon"),
            ("grok", "~x-ai/grok-latest"),
            ("muse", "meta/muse-wonder")
        ]
    );
    assert!(
        c.agents
            .iter()
            .all(|a| a.persona.is_empty() && a.reasoning_effort.is_none())
    );
    assert_eq!(c.judge.model, "~anthropic/claude-fable-latest");
    assert_eq!(
        c.judge
            .conflict_fallbacks
            .iter()
            .map(Nick::as_str)
            .collect::<Vec<_>>(),
        ["claude", "openai"]
    );
    assert_eq!(c.judge.reasoning_effort, ReasoningEffort::High);
    assert_eq!(c.defaults.reasoning_effort, ReasoningEffort::Medium);
    assert!(!c.judge.reasoning_effort_explicit && !c.defaults.reasoning_effort_explicit);
    assert_eq!((c.defaults.rounds, c.defaults.grill_rounds), (10, 2));
    assert_eq!(
        (c.budget.daily_max_usd, c.budget.debate_confirm_usd),
        (20.0, 2.0)
    );
    assert_eq!(c.user.name, "the user");
    assert_eq!(c.prompts.directory, None);
    assert_eq!(
        c.prompts.limits,
        PromptLimits {
            room_words: 500,
            turn_words: 400,
            final_turn_words: 500,
            plan_words: 1200
        }
    );
    assert_eq!(c.data_dir, f.root.join("data/squad"));
    assert_eq!(c.log_level, LogLevel::Info);
    assert_eq!(c.banners, BannerMode::On);
    assert_eq!(c.fx, EffectsMode::Full);
    assert!(!c.fx_explicit && !c.fx_debug && !c.no_color);
}

#[test]
fn missing_and_empty_credentials() {
    let f = Fixture::new();
    assert_eq!(
        SquadConfig::load_with_env(&f.options, &BTreeMap::new()).unwrap_err(),
        ConfigError::MissingApiKey
    );
}

#[test]
fn exact_validation_diagnostics() {
    error(
        &[("OPENROUTER_API_KEY", "")],
        "OPENROUTER_API_KEY is required. Add it to .env or the process environment.",
    );
    for value in [
        "",
        "0",
        "21",
        "9999999999999999999999999999",
        "1.5",
        "1x",
        "NaN",
        " 2",
    ] {
        error(
            &[("SQUAD_ROUNDS", value)],
            "SQUAD_ROUNDS must be an integer from 1 to 20.",
        );
    }
    for value in ["", "0", "11", "256", "2.5", "2x"] {
        error(
            &[("SQUAD_GRILL_ROUNDS", value)],
            "SQUAD_GRILL_ROUNDS must be an integer from 1 to 10.",
        );
    }
    for suffix in [
        "",
        "1BOB",
        "HAS-DASH",
        "É",
        "ABCDEFGHIJKLMNOPQRSTUVWXYZABCDEFG",
    ] {
        let variable = format!("SQUAD_MODEL_{suffix}");
        error(
            &[(&variable, "provider/version")],
            &format!("Invalid member nick in {variable}: use ^[a-z][a-z0-9_]{{0,31}}$."),
        );
    }
    for suffix in ["ME", "ALL", "JUDGE", "SYSTEM", "SQUAD"] {
        let variable = format!("SQUAD_MODEL_{suffix}");
        error(
            &[(&variable, "p/v")],
            &format!(
                "Reserved member nick in {variable}: {}.",
                suffix.to_lowercase()
            ),
        );
    }
    error(
        &[("SQUAD_MODEL_A", "p/v"), ("SQUAD_MODEL_a", "p/v")],
        "Duplicate member nick: a.",
    );
    error(
        &[("SQUAD_MODEL_A", "")],
        "SQUAD_MODEL_A must contain an OpenRouter model slug.",
    );
    error(
        &[("SQUAD_JUDGE_MODEL", "")],
        "SQUAD_JUDGE_MODEL must contain an OpenRouter model slug.",
    );
    error(
        &[("SQUAD_PERSONA_NOBODY", "persona")],
        "SQUAD_PERSONA_NOBODY refers to an unconfigured member: nobody.",
    );
    error(
        &[("SQUAD_USER_NAME", "")],
        "SQUAD_USER_NAME must be nonempty.",
    );
    for value in ["", "-1", "0", "NaN", "inf", "-inf", "1e999", "20USD", " 2"] {
        error(
            &[("SQUAD_DAILY_MAX_USD", value)],
            "SQUAD_DAILY_MAX_USD must be a positive finite number.",
        );
    }
    for value in ["", "-1", "NaN", "inf", "-inf", "1e999", "2USD"] {
        error(
            &[("SQUAD_DEBATE_CONFIRM_USD", value)],
            "SQUAD_DEBATE_CONFIRM_USD must be a nonnegative finite number.",
        );
    }
    for variable in [
        "SQUAD_ROOM_WORDS",
        "SQUAD_TURN_WORDS",
        "SQUAD_FINAL_TURN_WORDS",
        "SQUAD_PLAN_WORDS",
    ] {
        for value in ["", "0", "-1", "1.5", "4294967296", "400 words"] {
            error(
                &[(variable, value)],
                &format!("{variable} must be a positive integer."),
            );
        }
    }
    for variable in [
        "SQUAD_REASONING_EFFORT",
        "SQUAD_JUDGE_REASONING_EFFORT",
        "SQUAD_REASONING_EFFORT_CLAUDE",
    ] {
        for value in ["", "Medium", "maximum"] {
            error(
                &[(variable, value)],
                &format!("{variable} must be low, medium, or high."),
            );
        }
    }
    error(
        &[("SQUAD_REASONING_EFFORT_NOBODY", "low")],
        "SQUAD_REASONING_EFFORT_NOBODY refers to an unconfigured member: nobody.",
    );
    error(
        &[("SQUAD_REASONING_EFFORT_1BOB", "low")],
        "Invalid member nick in SQUAD_REASONING_EFFORT_1BOB: use ^[a-z][a-z0-9_]{0,31}$.",
    );
    error(
        &[("SQUAD_REASONING_EFFORT_ME", "low")],
        "Reserved member nick in SQUAD_REASONING_EFFORT_ME: me.",
    );
    error(
        &[("SQUAD_JUDGE_FALLBACKS", "bad-nick")],
        "Invalid member nick in SQUAD_JUDGE_FALLBACKS: bad-nick.",
    );
    error(
        &[("SQUAD_JUDGE_FALLBACKS", "me")],
        "Invalid member nick in SQUAD_JUDGE_FALLBACKS: me.",
    );
    error(
        &[("SQUAD_JUDGE_FALLBACKS", "nobody")],
        "SQUAD_JUDGE_FALLBACKS refers to an unconfigured member: nobody.",
    );
    error(
        &[("SQUAD_JUDGE_FALLBACKS", "claude,claude")],
        "Duplicate member nick in SQUAD_JUDGE_FALLBACKS: claude.",
    );
    for value in ["", "INFO", "warning"] {
        error(
            &[("SQUAD_LOG_LEVEL", value)],
            "SQUAD_LOG_LEVEL must be trace, debug, info, warn, error, or off.",
        );
    }
    for variable in ["SQUAD_PROMPTS_DIR", "SQUAD_DATA_DIR"] {
        error(
            &[(variable, "")],
            &format!("{variable} must contain a nonempty path."),
        );
    }
    for value in ["", "ON", "yes"] {
        error(
            &[("SQUAD_BANNERS", value)],
            "SQUAD_BANNERS must be on, compact, or off.",
        );
    }
    for value in ["", "FULL", "yes"] {
        error(
            &[("SQUAD_FX", value)],
            "SQUAD_FX must be full, lite, or off.",
        );
    }
}

#[test]
fn numeric_boundaries() {
    let f = Fixture::new();
    for (rounds, grill) in [("1", "1"), ("20", "10")] {
        let c = f
            .load(&[
                ("SQUAD_ROUNDS", rounds),
                ("SQUAD_GRILL_ROUNDS", grill),
                ("SQUAD_PLAN_WORDS", "4294967295"),
                ("SQUAD_DEBATE_CONFIRM_USD", "0"),
                ("SQUAD_DAILY_MAX_USD", "0.01"),
            ])
            .unwrap();
        assert_eq!(c.defaults.rounds.to_string(), rounds);
        assert_eq!(c.defaults.grill_rounds.to_string(), grill);
        assert_eq!(c.prompts.limits.plan_words, u32::MAX);
        assert_eq!(c.budget.debate_confirm_usd, 0.0);
    }
}

#[test]
fn roster_sizes_and_duplicate_models() {
    let f = Fixture::new();
    let mut env = environment(&[]);
    for i in 0..12 {
        env.insert(format!("SQUAD_MODEL_MEMBER_{i:02}"), "same/model".into());
    }
    assert_eq!(
        SquadConfig::load_with_env(&f.options, &env)
            .unwrap()
            .agents
            .len(),
        12
    );
    env.insert("SQUAD_MODEL_EXTRA".into(), "same/model".into());
    assert_eq!(
        SquadConfig::load_with_env(&f.options, &env)
            .unwrap_err()
            .to_string(),
        "Configure 1 to 12 members with SQUAD_MODEL_<NICK>."
    );
    let c = f.load(&[("SQUAD_MODEL_CUSTOM", "custom/version")]).unwrap();
    assert_eq!(c.agents.len(), 1);
    assert_eq!(c.agents[0].nick.as_str(), "custom");
    assert!(c.judge.conflict_fallbacks.is_empty());
    assert_eq!(c.judge.model, "~anthropic/claude-fable-latest");
    let longest = "a".repeat(32);
    assert!(Nick::new(&longest).is_ok());
}

#[test]
fn optional_values_and_explicit_provenance() {
    let f = Fixture::new();
    let c = f
        .load(&[
            ("SQUAD_MODEL_A", "p/v"),
            ("SQUAD_PERSONA_a", "Be precise"),
            ("SQUAD_REASONING_EFFORT_A", "low"),
            ("SQUAD_REASONING_EFFORT", "medium"),
            ("SQUAD_JUDGE_REASONING_EFFORT", "high"),
            ("SQUAD_JUDGE_FALLBACKS", "a"),
            ("SQUAD_USER_NAME", "Jane"),
            ("SQUAD_FX", "full"),
            ("SQUAD_FX_DEBUG", "1"),
            ("NO_COLOR", "1"),
            ("SQUAD_ROOM_WORDS", "501"),
            ("SQUAD_TURN_WORDS", "401"),
            ("SQUAD_FINAL_TURN_WORDS", "502"),
            ("SQUAD_PLAN_WORDS", "1201"),
        ])
        .unwrap();
    assert_eq!(c.agents[0].persona, "Be precise");
    assert_eq!(c.agents[0].reasoning_effort, Some(ReasoningEffort::Low));
    assert_eq!(c.judge.conflict_fallbacks[0].as_str(), "a");
    assert!(
        c.defaults.reasoning_effort_explicit
            && c.judge.reasoning_effort_explicit
            && c.fx_explicit
            && c.fx_debug
            && c.no_color
    );
    assert_eq!(c.user.name, "Jane");
    assert_eq!(
        c.prompts.limits,
        PromptLimits {
            room_words: 501,
            turn_words: 401,
            final_turn_words: 502,
            plan_words: 1201
        }
    );
    assert!(
        f.load(&[("SQUAD_JUDGE_FALLBACKS", "")])
            .unwrap()
            .judge
            .conflict_fallbacks
            .is_empty()
    );
    let c = f
        .load(&[("SQUAD_JUDGE_FALLBACKS", "openai,claude")])
        .unwrap();
    assert_eq!(
        c.judge
            .conflict_fallbacks
            .iter()
            .map(Nick::as_str)
            .collect::<Vec<_>>(),
        ["openai", "claude"]
    );
    for (v, expected) in [
        ("low", ReasoningEffort::Low),
        ("medium", ReasoningEffort::Medium),
        ("high", ReasoningEffort::High),
    ] {
        assert_eq!(
            f.load(&[("SQUAD_REASONING_EFFORT", v)])
                .unwrap()
                .defaults
                .reasoning_effort,
            expected
        );
    }
    for (v, expected) in [
        ("trace", LogLevel::Trace),
        ("debug", LogLevel::Debug),
        ("info", LogLevel::Info),
        ("warn", LogLevel::Warn),
        ("error", LogLevel::Error),
        ("off", LogLevel::Off),
    ] {
        assert_eq!(
            f.load(&[("SQUAD_LOG_LEVEL", v)]).unwrap().log_level,
            expected
        );
    }
    for (v, expected) in [
        ("on", BannerMode::On),
        ("compact", BannerMode::Compact),
        ("off", BannerMode::Off),
    ] {
        assert_eq!(f.load(&[("SQUAD_BANNERS", v)]).unwrap().banners, expected);
    }
    for (v, expected) in [
        ("full", EffectsMode::Full),
        ("lite", EffectsMode::Lite),
        ("off", EffectsMode::Off),
    ] {
        assert_eq!(f.load(&[("SQUAD_FX", v)]).unwrap().fx, expected);
    }
    assert!(
        !f.load(&[("NO_COLOR", ""), ("SQUAD_FX_DEBUG", "yes")])
            .unwrap()
            .no_color
    );
    assert!(!f.load(&[("SQUAD_FX_DEBUG", "yes")]).unwrap().fx_debug);
}

#[test]
fn file_order_and_environment_overrides() {
    let mut f = Fixture::new();
    f.write("settings", "SQUAD_MODEL_Z=old/version\nSQUAD_MODEL_A=a/version\nSQUAD_FX=off\nSQUAD_BANNERS=off\nSQUAD_REASONING_EFFORT=low\nSQUAD_GRILL_ROUNDS=3\nSQUAD_PLAN_WORDS=1400\nSQUAD_DAILY_MAX_USD=10\nSQUAD_REASONING_EFFORT_Z=low\nSQUAD_FX_DEBUG=0\n");
    f.options.env_file = Some("settings".into());
    let c = f
        .load(&[
            ("SQUAD_MODEL_Z", "new/version"),
            ("SQUAD_MODEL_B", "b/version"),
            ("SQUAD_MODEL_C", "c/version"),
            ("SQUAD_FX", "lite"),
            ("SQUAD_BANNERS", "compact"),
            ("SQUAD_REASONING_EFFORT", "high"),
            ("SQUAD_GRILL_ROUNDS", "4"),
            ("SQUAD_PLAN_WORDS", "1500"),
            ("SQUAD_DAILY_MAX_USD", "30"),
            ("SQUAD_REASONING_EFFORT_Z", "medium"),
            ("SQUAD_FX_DEBUG", "1"),
        ])
        .unwrap();
    assert_eq!(
        c.agents.iter().map(|a| a.nick.as_str()).collect::<Vec<_>>(),
        ["z", "a", "b", "c"]
    );
    assert_eq!(c.agents[0].model, "new/version");
    assert_eq!(c.agents[0].reasoning_effort, Some(ReasoningEffort::Medium));
    assert_eq!(c.fx, EffectsMode::Lite);
    assert_eq!(c.banners, BannerMode::Compact);
    assert_eq!(c.defaults.reasoning_effort, ReasoningEffort::High);
    assert_eq!(c.defaults.grill_rounds, 4);
    assert_eq!(c.prompts.limits.plan_words, 1500);
    assert_eq!(c.budget.daily_max_usd, 30.0);
    assert!(c.fx_debug && c.fx_explicit);
    assert_eq!(
        f.load(&[("SQUAD_MODEL_Z", "")]).unwrap_err().to_string(),
        "SQUAD_MODEL_Z must contain an OpenRouter model slug."
    );
    assert_eq!(
        f.load(&[("SQUAD_FX", "")]).unwrap_err(),
        ConfigError::Effects
    );
    assert_eq!(
        f.load(&[("SQUAD_BANNERS", "")]).unwrap_err(),
        ConfigError::Banners
    );
}

#[test]
fn duplicate_file_declarations_are_not_lost_by_map_merging() {
    let mut f = Fixture::new();
    f.options.env_file = Some("settings".into());
    for content in [
        "SQUAD_MODEL_A=p/a\nSQUAD_MODEL_A=p/b\n",
        "SQUAD_MODEL_A=p/a\nSQUAD_MODEL_a=p/b\n",
    ] {
        f.write("settings", content);
        assert_eq!(
            f.load(&[("SQUAD_MODEL_A", "p/c")]).unwrap_err(),
            ConfigError::DuplicateNick("a".into())
        );
    }
    f.write("settings", "SQUAD_MODEL_A=p/a\n");
    assert_eq!(
        f.load(&[("SQUAD_MODEL_a", "p/b")]).unwrap_err(),
        ConfigError::DuplicateNick("a".into())
    );
}

#[test]
fn literal_values_never_expand() {
    let mut f = Fixture::new();
    f.options.env_file = Some("settings".into());
    f.write("settings", "export SQUAD_MODEL_A=~anthropic/claude-opus-latest\nSQUAD_JUDGE_MODEL=\"~anthropic/claude-fable-latest\"\nSQUAD_PERSONA_A='${HOME} $(echo secret) `uname` ~/'\nSQUAD_USER_NAME=literal#hash # comment\nSQUAD_PROMPTS_DIR=~/prompts\n");
    let c = f.load(&[("HOME", "/not-used")]).unwrap();
    assert_eq!(c.agents[0].model, "~anthropic/claude-opus-latest");
    assert_eq!(c.judge.model, "~anthropic/claude-fable-latest");
    assert_eq!(c.agents[0].persona, "${HOME} $(echo secret) `uname` ~/");
    assert_eq!(c.user.name, "literal#hash");
    assert_eq!(c.prompts.directory, Some(f.root.join("~/prompts")));
}

#[test]
fn file_selection_and_relative_paths() {
    let mut f = Fixture::new();
    f.write(
        "config/squad/.env",
        "SQUAD_USER_NAME=OS\nSQUAD_DATA_DIR=database\nSQUAD_PROMPTS_DIR=prompts\n",
    );
    let c = f.load(&[]).unwrap();
    assert_eq!(c.user.name, "OS");
    assert_eq!(c.data_dir, f.root.join("config/squad/database"));
    assert_eq!(
        c.prompts.directory,
        Some(f.root.join("config/squad/prompts"))
    );
    f.write(".env", "SQUAD_USER_NAME=working\n");
    assert_eq!(f.load(&[]).unwrap().user.name, "working");
    f.write(
        "chosen/settings",
        "SQUAD_USER_NAME=explicit\nSQUAD_DATA_DIR=database\n",
    );
    f.options.env_file = Some("chosen/settings".into());
    let c = f.load(&[]).unwrap();
    assert_eq!(c.user.name, "explicit");
    assert_eq!(c.data_dir, f.root.join("chosen/database"));
    assert_eq!(c.prompts.directory, None); // Exactly one file is loaded.
    assert_eq!(
        f.load(&[("SQUAD_DATA_DIR", "override")]).unwrap().data_dir,
        f.root.join("chosen/override")
    );
    f.options.data_dir = Some("flag".into());
    assert_eq!(f.load(&[]).unwrap().data_dir, f.root.join("flag"));
    f.options.env_file = None;
    std::fs::remove_file(f.root.join(".env")).unwrap();
    std::fs::remove_file(f.root.join("config/squad/.env")).unwrap();
    f.options.data_dir = None;
    assert_eq!(
        f.load(&[("SQUAD_DATA_DIR", "relative")]).unwrap().data_dir,
        f.root.join("relative")
    );
    let absolute = f.root.join("absolute");
    assert_eq!(
        f.load(&[("SQUAD_DATA_DIR", absolute.to_str().unwrap())])
            .unwrap()
            .data_dir,
        absolute
    );
}

#[test]
fn file_failures_and_line_numbers() {
    let mut f = Fixture::new();
    f.options.env_file = Some("missing".into());
    assert_eq!(
        f.load(&[]).unwrap_err().to_string(),
        "Environment file not found: missing."
    );
    f.options.env_file = Some(f.root.clone()); // Reading a directory is an error even as root.
    assert_eq!(
        f.load(&[]).unwrap_err().to_string(),
        format!("Cannot read environment file: {}.", f.root.display())
    );
    f.options.env_file = Some("settings".into());
    for invalid in [
        "not an assignment",
        "9BAD=value",
        "=value",
        "KEY='unterminated",
        "KEY=\"value\"junk",
        "KEY=unquoted\"quote",
    ] {
        let path = f.write("settings", &format!("# comment\n\n{invalid}\n"));
        assert_eq!(
            f.load(&[]).unwrap_err().to_string(),
            format!("Invalid environment file: {}, line 3.", path.display())
        );
    }
    std::fs::write(f.root.join("settings"), [0xff]).unwrap();
    assert!(matches!(
        f.load(&[]),
        Err(ConfigError::EnvironmentFileUnreadable(_))
    ));
}

#[test]
fn credential_and_syntax_errors_are_redacted() {
    let mut f = Fixture::new();
    let c = f.load(&[]).unwrap();
    assert_eq!(format!("{}", c.openrouter_api_key), "[REDACTED]");
    assert_eq!(
        format!("{:?}", c.openrouter_api_key),
        "OpenRouterApiKey([REDACTED])"
    );
    assert!(!format!("{c:?}").contains("fixture-credential"));
    assert_eq!(
        c.openrouter_api_key.expose_for_openrouter(),
        "fixture-credential"
    );
    f.options.env_file = Some("settings".into());
    f.write(
        "settings",
        "OPENROUTER_API_KEY='fixture-credential' garbage",
    );
    let err = f.load(&[]).unwrap_err();
    assert!(!format!("{err:?} {err}").contains("fixture-credential"));
}

#[test]
fn deferred_error_strings() {
    assert_eq!(
        ConfigError::UnknownModel {
            variable: "SQUAD_MODEL_A".into(),
            slug: "unknown/slug".into()
        }
        .to_string(),
        "Unknown OpenRouter model in SQUAD_MODEL_A: unknown/slug."
    );
    assert_eq!(
        ConfigError::DataDirectoryNotWritable(Path::new("/unwritable").into()).to_string(),
        "Data directory is not writable: /unwritable."
    );
}

#[test]
fn os_paths_use_only_injected_environment() {
    let env = BTreeMap::from([
        ("HOME".into(), "/home/tester".into()),
        ("XDG_CONFIG_HOME".into(), "/custom/config".into()),
        ("XDG_DATA_HOME".into(), "/custom/data".into()),
    ]);
    let o = LoadOptions::from_environment("/work".into(), &env);
    if cfg!(target_os = "macos") {
        assert_eq!(
            o.config_dir,
            Some("/home/tester/Library/Application Support".into())
        );
        assert_eq!(
            o.default_data_dir,
            PathBuf::from("/home/tester/Library/Application Support/squad")
        );
    } else if cfg!(target_os = "linux") {
        assert_eq!(o.config_dir, Some("/custom/config".into()));
        assert_eq!(o.default_data_dir, PathBuf::from("/custom/data/squad"));
    }
}

#[test]
fn shipped_example_is_verbatim() {
    assert_eq!(
        include_str!("../../../.env.example"),
        r#"# Squad configuration. Copy to .env and add your key.

# Required. Get one at https://openrouter.ai/keys
OPENROUTER_API_KEY=

# Debate length: rounds per $debate (1 to 20). Change per room at runtime with $ROUNDS=5 or /set ROUNDS 5.
SQUAD_ROUNDS=10

# Daily spending cap in US dollars. Squad stops sending new requests when today's spend reaches it.
SQUAD_DAILY_MAX_USD=20

# Grill length: rounds per $grill (1 to 10). Change per room at runtime with $GRILL_ROUNDS=2 or /set GRILL_ROUNDS 2.
SQUAD_GRILL_ROUNDS=2

# The room. One line per member: SQUAD_MODEL_<NICK>=<OpenRouter model slug>.
# The nick is the lowercase of <NICK> and is what you type after @.
# Slugs starting with ~ are OpenRouter "latest" aliases and follow new releases automatically.
# Add a line to add a member (up to 12), delete a line to remove one.
SQUAD_MODEL_CLAUDE=~anthropic/claude-opus-latest
SQUAD_MODEL_FABLE=~anthropic/claude-fable-latest
SQUAD_MODEL_OPENAI=~openai/gpt-sol-latest
SQUAD_MODEL_GEMINI=google/gemini-4-neon
SQUAD_MODEL_GROK=~x-ai/grok-latest
SQUAD_MODEL_MUSE=meta/muse-wonder

# The judge is summoned, never a room member. Uses Fable by default.
SQUAD_JUDGE_MODEL=~anthropic/claude-fable-latest
"#
    );
}

#[test]
fn all_presentation_modes_and_boolean_semantics() {
    let f = Fixture::new();
    for (value, expected) in [
        ("on", BannerMode::On),
        ("compact", BannerMode::Compact),
        ("off", BannerMode::Off),
    ] {
        assert_eq!(
            f.load(&[("SQUAD_BANNERS", value)]).unwrap().banners,
            expected
        );
    }
    for (value, expected) in [
        ("full", EffectsMode::Full),
        ("lite", EffectsMode::Lite),
        ("off", EffectsMode::Off),
    ] {
        let c = f.load(&[("SQUAD_FX", value)]).unwrap();
        assert_eq!(c.fx, expected);
        assert!(c.fx_explicit);
    }
    for value in ["", "0", "true", "2"] {
        assert!(!f.load(&[("SQUAD_FX_DEBUG", value)]).unwrap().fx_debug);
    }
    assert!(!f.load(&[("NO_COLOR", "")]).unwrap().no_color);
    assert!(f.load(&[("NO_COLOR", "0")]).unwrap().no_color);
}

#[test]
fn literal_multiline_crlf_and_empty_overrides() {
    let mut f = Fixture::new();
    f.options.env_file = Some("settings".into());
    f.write("settings", "# comment\r\nSQUAD_PERSONA_CLAUDE=\"first line\r\n${UNCHANGED}\" # comment\r\nNO_COLOR=1\r\nSQUAD_FX_DEBUG=1\r\nSQUAD_REASONING_EFFORT=low\r\nSQUAD_FX=lite\r\n");
    let c = f
        .load(&[("NO_COLOR", ""), ("SQUAD_FX_DEBUG", "0")])
        .unwrap();
    assert_eq!(c.agents[0].persona, "first line\n${UNCHANGED}");
    assert!(!c.no_color && !c.fx_debug);
    assert!(c.defaults.reasoning_effort_explicit && c.fx_explicit);
    assert_eq!(
        f.load(&[("SQUAD_REASONING_EFFORT", "")]).unwrap_err(),
        ConfigError::ReasoningEffort("SQUAD_REASONING_EFFORT".into())
    );
    f.write("settings", "OPENROUTER_API_KEY=file-fixture\n");
    assert_eq!(
        f.load(&[("OPENROUTER_API_KEY", "")]).unwrap_err(),
        ConfigError::MissingApiKey
    );
    let env = BTreeMap::new();
    let c = SquadConfig::load_with_env(&f.options, &env).unwrap();
    assert_eq!(c.openrouter_api_key.expose_for_openrouter(), "file-fixture");
}
