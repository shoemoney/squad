//! Environment-only configuration. File values are literal, never shell-expanded.
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};

use crate::limits::*;

#[derive(Clone, Debug, PartialEq)]
pub struct SquadConfig {
    pub room: String,
    pub openrouter_api_key: OpenRouterApiKey,
    pub user: UserConfig,
    pub agents: Vec<AgentConfig>,
    pub judge: JudgeConfig,
    pub defaults: RoomDefaults,
    pub budget: BudgetConfig,
    pub prompts: PromptConfig,
    pub data_dir: PathBuf,
    pub log_level: LogLevel,
    pub no_color: bool,
    pub banners: BannerMode,
    pub fx: EffectsMode,
    pub fx_explicit: bool,
    pub fx_debug: bool,
}

/// Not serializable. Only the provider integration should request the secret.
#[derive(Clone, PartialEq, Eq)]
pub struct OpenRouterApiKey(String);
impl OpenRouterApiKey {
    /// Credential access for the OpenRouter authorization header only.
    pub fn expose_for_openrouter(&self) -> &str {
        &self.0
    }
}
impl fmt::Debug for OpenRouterApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("OpenRouterApiKey([REDACTED])")
    }
}
impl fmt::Display for OpenRouterApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

/// Validated canonical member identifier.
// TODO(M1.P1): share this validator with the protocol identity contracts.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
#[serde(transparent)]
pub struct Nick(String);
impl<'de> serde::Deserialize<'de> for Nick {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = <String as serde::Deserialize>::deserialize(d)?;
        Self::new(&value).map_err(serde::de::Error::custom)
    }
}
impl Nick {
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn new(value: &str) -> Result<Self, ConfigError> {
        nick(value, "SQUAD_MODEL_<NICK>")
    }
}
impl fmt::Display for Nick {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserConfig {
    pub name: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentConfig {
    pub nick: Nick,
    pub model: String,
    pub persona: String,
    pub reasoning_effort: Option<ReasoningEffort>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JudgeConfig {
    pub model: String,
    pub conflict_fallbacks: Vec<Nick>,
    pub reasoning_effort: ReasoningEffort,
    pub reasoning_effort_explicit: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomDefaults {
    pub rounds: u8,
    pub reasoning_effort: ReasoningEffort,
    pub reasoning_effort_explicit: bool,
    pub grill_rounds: u8,
}
#[derive(Clone, Debug, PartialEq)]
pub struct BudgetConfig {
    pub daily_max_usd: f64,
    pub debate_confirm_usd: f64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PromptConfig {
    pub directory: Option<PathBuf>,
    pub limits: PromptLimits,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PromptLimits {
    pub room_words: u32,
    pub turn_words: u32,
    pub final_turn_words: u32,
    pub plan_words: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReasoningEffort {
    Low,
    Medium,
    High,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Off,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BannerMode {
    On,
    Compact,
    Off,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectsMode {
    Full,
    Lite,
    Off,
}

/// Local startup errors retain the exact diagnostic contract without storing values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfigError {
    EnvironmentFileNotFound(PathBuf),
    EnvironmentFileUnreadable(PathBuf),
    EnvironmentFileSyntax {
        path: PathBuf,
        line: usize,
    },
    MissingApiKey,
    Rounds,
    MemberCount,
    InvalidNick {
        variable: String,
    },
    ReservedNick {
        variable: String,
        nick: String,
    },
    DuplicateNick(String),
    EmptyModel(String),
    /// Constructed by the model registry after its catalog check.
    UnknownModel {
        variable: String,
        slug: String,
    },
    UnconfiguredMember {
        variable: String,
        nick: String,
    },
    EmptyUserName,
    DailyMax,
    DebateConfirm,
    WordLimit(String),
    ReasoningEffort(String),
    InvalidFallback(String),
    MissingFallback(String),
    DuplicateFallback(String),
    LogLevel,
    EmptyPath(String),
    /// Constructed by the room open / doctor writability check.
    DataDirectoryNotWritable(PathBuf),
    Banners,
    Effects,
    GrillRounds,
}
impl std::error::Error for ConfigError {}
impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EnvironmentFileNotFound(p) => {
                write!(f, "Environment file not found: {}.", p.display())
            }
            Self::EnvironmentFileUnreadable(p) => {
                write!(f, "Cannot read environment file: {}.", p.display())
            }
            Self::EnvironmentFileSyntax { path, line } => write!(
                f,
                "Invalid environment file: {}, line {line}.",
                path.display()
            ),
            Self::MissingApiKey => f.write_str(
                "OPENROUTER_API_KEY is required. Add it to .env or the process environment.",
            ),
            Self::Rounds => f.write_str("SQUAD_ROUNDS must be an integer from 1 to 20."),
            Self::MemberCount => f.write_str("Configure 1 to 12 members with SQUAD_MODEL_<NICK>."),
            Self::InvalidNick { variable } => write!(
                f,
                "Invalid member nick in {variable}: use ^[a-z][a-z0-9_]{{0,31}}$."
            ),
            Self::ReservedNick { variable, nick } => {
                write!(f, "Reserved member nick in {variable}: {nick}.")
            }
            Self::DuplicateNick(n) => write!(f, "Duplicate member nick: {n}."),
            Self::EmptyModel(v) => write!(f, "{v} must contain an OpenRouter model slug."),
            Self::UnknownModel { variable, slug } => {
                write!(f, "Unknown OpenRouter model in {variable}: {slug}.")
            }
            Self::UnconfiguredMember { variable, nick } => {
                write!(f, "{variable} refers to an unconfigured member: {nick}.")
            }
            Self::EmptyUserName => f.write_str("SQUAD_USER_NAME must be nonempty."),
            Self::DailyMax => f.write_str("SQUAD_DAILY_MAX_USD must be a positive finite number."),
            Self::DebateConfirm => {
                f.write_str("SQUAD_DEBATE_CONFIRM_USD must be a nonnegative finite number.")
            }
            Self::WordLimit(v) => write!(f, "{v} must be a positive integer."),
            Self::ReasoningEffort(v) => write!(f, "{v} must be low, medium, or high."),
            Self::InvalidFallback(n) => {
                write!(f, "Invalid member nick in SQUAD_JUDGE_FALLBACKS: {n}.")
            }
            Self::MissingFallback(n) => write!(
                f,
                "SQUAD_JUDGE_FALLBACKS refers to an unconfigured member: {n}."
            ),
            Self::DuplicateFallback(n) => {
                write!(f, "Duplicate member nick in SQUAD_JUDGE_FALLBACKS: {n}.")
            }
            Self::LogLevel => {
                f.write_str("SQUAD_LOG_LEVEL must be trace, debug, info, warn, error, or off.")
            }
            Self::EmptyPath(v) => write!(f, "{v} must contain a nonempty path."),
            Self::DataDirectoryNotWritable(p) => {
                write!(f, "Data directory is not writable: {}.", p.display())
            }
            Self::Banners => f.write_str("SQUAD_BANNERS must be on, compact, or off."),
            Self::Effects => f.write_str("SQUAD_FX must be full, lite, or off."),
            Self::GrillRounds => f.write_str("SQUAD_GRILL_ROUNDS must be an integer from 1 to 10."),
        }
    }
}

/// Operational paths supplied by the caller. Absolute working/default paths are recommended.
#[derive(Clone, Debug)]
pub struct LoadOptions {
    pub env_file: Option<PathBuf>,
    pub data_dir: Option<PathBuf>,
    pub working_dir: PathBuf,
    /// OS configuration directory, without the squad suffix.
    pub config_dir: Option<PathBuf>,
    /// OS data directory, including the squad suffix.
    pub default_data_dir: PathBuf,
}

impl LoadOptions {
    /// Selected file for diagnostic provenance, using the same precedence as the loader.
    pub fn selected_env_file(&self) -> Result<Option<PathBuf>, ConfigError> {
        select_file(self)
    }

    /// Determine OS paths from the injected environment, never a hidden process read.
    pub fn from_environment(working_dir: PathBuf, env: &BTreeMap<String, String>) -> Self {
        let home = env
            .get("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| working_dir.clone());
        let (config_dir, data_dir) = if cfg!(target_os = "macos") {
            let support = home.join("Library/Application Support");
            (support.clone(), support)
        } else if cfg!(target_os = "windows") {
            let config = env
                .get("APPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join("AppData/Roaming"));
            let data = env
                .get("LOCALAPPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(|| config.clone());
            (config, data)
        } else {
            let xdg = |key: &str, fallback: &str| {
                env.get(key)
                    .map(PathBuf::from)
                    .filter(|p| p.is_absolute())
                    .unwrap_or_else(|| home.join(fallback))
            };
            (
                xdg("XDG_CONFIG_HOME", ".config"),
                xdg("XDG_DATA_HOME", ".local/share"),
            )
        };
        Self {
            env_file: None,
            data_dir: None,
            working_dir,
            config_dir: Some(config_dir),
            default_data_dir: data_dir.join("squad"),
        }
    }
}

const DEFAULT_MODELS: [(&str, &str); 6] = [
    ("CLAUDE", "~anthropic/claude-opus-latest"),
    ("FABLE", "~anthropic/claude-fable-latest"),
    ("OPENAI", "~openai/gpt-sol-latest"),
    ("GEMINI", "google/gemini-4-neon"),
    ("GROK", "~x-ai/grok-latest"),
    ("MUSE", "meta/muse-wonder"),
];

impl SquadConfig {
    /// Production entry point; does not mutate the process environment.
    pub fn load(options: &LoadOptions) -> Result<Self, ConfigError> {
        Self::load_with_env(options, &std::env::vars().collect())
    }

    /// Deterministic loader for callers and tests supplying a complete environment snapshot.
    /// Reads exactly one selected file; never evaluates any value as shell code.
    pub fn load_with_env(
        options: &LoadOptions,
        env: &BTreeMap<String, String>,
    ) -> Result<Self, ConfigError> {
        let selected = select_file(options)?;
        let entries = if let Some(path) = &selected {
            let content = std::fs::read_to_string(path)
                .map_err(|_| ConfigError::EnvironmentFileUnreadable(path.clone()))?;
            parse_literal_env(&content, path)?
        } else {
            Vec::new()
        };
        let base = selected
            .as_deref()
            .and_then(Path::parent)
            .unwrap_or(&options.working_dir);
        let mut values = BTreeMap::new();
        let mut model_variables = Vec::new();
        for (key, value) in entries {
            if key.starts_with("SQUAD_MODEL_") {
                model_variables.push(key.clone());
            }
            values.insert(key, value);
        }
        for (key, value) in env {
            // Only an exact variable-name override replaces a file declaration.
            if key.starts_with("SQUAD_MODEL_") && !values.contains_key(key) {
                model_variables.push(key.clone());
            }
            values.insert(key.clone(), value.clone());
        }
        if model_variables.is_empty() {
            for (suffix, model) in DEFAULT_MODELS {
                let key = format!("SQUAD_MODEL_{suffix}");
                model_variables.push(key.clone());
                values.insert(key, model.to_owned());
            }
        }
        let get = |key: &str, default: &str| {
            values
                .get(key)
                .cloned()
                .unwrap_or_else(|| default.to_owned())
        };
        let api_key = get("OPENROUTER_API_KEY", "");
        if api_key.trim().is_empty() {
            return Err(ConfigError::MissingApiKey);
        }
        if !(MIN_MEMBERS..=MAX_MEMBERS).contains(&model_variables.len()) {
            return Err(ConfigError::MemberCount);
        }
        let mut agents = Vec::new();
        let mut seen = BTreeSet::new();
        for variable in model_variables {
            let n = nick(&variable["SQUAD_MODEL_".len()..].to_lowercase(), &variable)?;
            if !seen.insert(n.clone()) {
                return Err(ConfigError::DuplicateNick(n.0));
            }
            agents.push(AgentConfig {
                nick: n,
                model: model(&get(&variable, ""), &variable)?,
                persona: String::new(),
                reasoning_effort: None,
            });
        }
        for (variable, value) in &values {
            let (suffix, is_effort) = if let Some(s) = variable.strip_prefix("SQUAD_PERSONA_") {
                (s, false)
            } else if let Some(s) = variable.strip_prefix("SQUAD_REASONING_EFFORT_") {
                (s, true)
            } else {
                continue;
            };
            let n = nick(&suffix.to_lowercase(), variable)?;
            let agent = agents.iter_mut().find(|a| a.nick == n).ok_or_else(|| {
                ConfigError::UnconfiguredMember {
                    variable: variable.clone(),
                    nick: n.0,
                }
            })?;
            if is_effort {
                agent.reasoning_effort = Some(effort(value, variable)?);
            } else {
                agent.persona = value.clone();
            }
        }
        let fallback_value = get("SQUAD_JUDGE_FALLBACKS", "claude,openai");
        let explicit_fallbacks = values.contains_key("SQUAD_JUDGE_FALLBACKS");
        let mut fallbacks = Vec::new();
        let mut seen_fallbacks = BTreeSet::new();
        if !fallback_value.is_empty() {
            for value in fallback_value.split(',').map(str::trim) {
                let n = nick(value, "SQUAD_JUDGE_FALLBACKS")
                    .map_err(|_| ConfigError::InvalidFallback(value.to_owned()))?;
                if !seen_fallbacks.insert(n.clone()) {
                    return Err(ConfigError::DuplicateFallback(n.0));
                }
                if agents.iter().any(|a| a.nick == n) {
                    fallbacks.push(n);
                } else if explicit_fallbacks {
                    return Err(ConfigError::MissingFallback(n.0));
                }
            }
        }
        let name = get("SQUAD_USER_NAME", "the user");
        if name.trim().is_empty() {
            return Err(ConfigError::EmptyUserName);
        }
        let rounds = bounded(
            &get("SQUAD_ROUNDS", &DEFAULT_ROUNDS.to_string()),
            MIN_ROUNDS,
            MAX_ROUNDS,
        )
        .ok_or(ConfigError::Rounds)?;
        let grill_rounds = bounded(
            &get("SQUAD_GRILL_ROUNDS", &DEFAULT_GRILL_ROUNDS.to_string()),
            MIN_GRILL_ROUNDS,
            MAX_GRILL_ROUNDS,
        )
        .ok_or(ConfigError::GrillRounds)?;
        let daily_max_usd =
            money(&get("SQUAD_DAILY_MAX_USD", "20"), false).ok_or(ConfigError::DailyMax)?;
        let debate_confirm_usd =
            money(&get("SQUAD_DEBATE_CONFIRM_USD", "2"), true).ok_or(ConfigError::DebateConfirm)?;
        let words = |key: &str, default: u32| {
            get(key, &default.to_string())
                .parse::<u32>()
                .ok()
                .filter(|v| *v > 0)
                .ok_or_else(|| ConfigError::WordLimit(key.to_owned()))
        };
        let path_value = |key: &str| {
            values
                .get(key)
                .map(|value| configured_path(value, key, base))
                .transpose()
        };
        // TODO(M4.P1): Prompt pack validates directory contents, templates and placeholders.
        let prompt_dir = path_value("SQUAD_PROMPTS_DIR")?;
        let configured_data_dir = path_value("SQUAD_DATA_DIR")?;
        let data_dir = if let Some(path) = &options.data_dir {
            configured_path(&path.to_string_lossy(), "--data-dir", &options.working_dir)?
        } else {
            configured_data_dir.unwrap_or_else(|| options.default_data_dir.clone())
        };
        // TODO(M3.P1): Room open creates and checks data-dir writability before locking.
        Ok(Self {
            room: ROOM.to_owned(),
            openrouter_api_key: OpenRouterApiKey(api_key),
            user: UserConfig { name },
            agents,
            judge: JudgeConfig {
                model: model(
                    &get("SQUAD_JUDGE_MODEL", DEFAULT_MODELS[1].1),
                    "SQUAD_JUDGE_MODEL",
                )?,
                conflict_fallbacks: fallbacks,
                reasoning_effort: effort(
                    &get("SQUAD_JUDGE_REASONING_EFFORT", "high"),
                    "SQUAD_JUDGE_REASONING_EFFORT",
                )?,
                reasoning_effort_explicit: values.contains_key("SQUAD_JUDGE_REASONING_EFFORT"),
            },
            defaults: RoomDefaults {
                rounds,
                grill_rounds,
                reasoning_effort: effort(
                    &get("SQUAD_REASONING_EFFORT", "medium"),
                    "SQUAD_REASONING_EFFORT",
                )?,
                reasoning_effort_explicit: values.contains_key("SQUAD_REASONING_EFFORT"),
            },
            budget: BudgetConfig {
                daily_max_usd,
                debate_confirm_usd,
            },
            prompts: PromptConfig {
                directory: prompt_dir,
                limits: PromptLimits {
                    room_words: words("SQUAD_ROOM_WORDS", ROOM_WORDS)?,
                    turn_words: words("SQUAD_TURN_WORDS", TURN_WORDS)?,
                    final_turn_words: words("SQUAD_FINAL_TURN_WORDS", FINAL_TURN_WORDS)?,
                    plan_words: words("SQUAD_PLAN_WORDS", PLAN_WORDS)?,
                },
            },
            data_dir,
            log_level: match get("SQUAD_LOG_LEVEL", "info").as_str() {
                "trace" => LogLevel::Trace,
                "debug" => LogLevel::Debug,
                "info" => LogLevel::Info,
                "warn" => LogLevel::Warn,
                "error" => LogLevel::Error,
                "off" => LogLevel::Off,
                _ => return Err(ConfigError::LogLevel),
            },
            no_color: values.get("NO_COLOR").is_some_and(|v| !v.is_empty()),
            banners: match get("SQUAD_BANNERS", "on").as_str() {
                "on" => BannerMode::On,
                "compact" => BannerMode::Compact,
                "off" => BannerMode::Off,
                _ => return Err(ConfigError::Banners),
            },
            fx: match get("SQUAD_FX", "full").as_str() {
                "full" => EffectsMode::Full,
                "lite" => EffectsMode::Lite,
                "off" => EffectsMode::Off,
                _ => return Err(ConfigError::Effects),
            },
            fx_explicit: values.contains_key("SQUAD_FX"),
            fx_debug: get("SQUAD_FX_DEBUG", "") == "1",
        })
    }
}

fn absolute(path: &Path, base: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_owned()
    } else {
        base.join(path)
    }
}
fn configured_path(value: &str, variable: &str, base: &Path) -> Result<PathBuf, ConfigError> {
    if value.trim().is_empty() {
        Err(ConfigError::EmptyPath(variable.to_owned()))
    } else {
        Ok(absolute(Path::new(value), base))
    }
}
fn select_file(options: &LoadOptions) -> Result<Option<PathBuf>, ConfigError> {
    if let Some(path) = &options.env_file {
        let resolved = absolute(path, &options.working_dir);
        return match std::fs::metadata(&resolved) {
            Ok(_) => Ok(Some(resolved)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Err(ConfigError::EnvironmentFileNotFound(path.clone()))
            }
            Err(_) => Err(ConfigError::EnvironmentFileUnreadable(path.clone())),
        };
    }
    let candidates = std::iter::once(options.working_dir.join(".env"))
        .chain(options.config_dir.as_ref().map(|p| p.join("squad/.env")));
    for path in candidates {
        match std::fs::metadata(&path) {
            Ok(_) => return Ok(Some(path)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(ConfigError::EnvironmentFileUnreadable(path)),
        }
    }
    Ok(None)
}
fn valid_nick(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= NICK_MAX_BYTES
        && value.as_bytes()[0].is_ascii_lowercase()
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}
fn nick(value: &str, variable: &str) -> Result<Nick, ConfigError> {
    if !valid_nick(value) {
        return Err(ConfigError::InvalidNick {
            variable: variable.to_owned(),
        });
    }
    if RESERVED_NICKS.contains(&value) {
        return Err(ConfigError::ReservedNick {
            variable: variable.to_owned(),
            nick: value.to_owned(),
        });
    }
    Ok(Nick(value.to_owned()))
}
fn model(value: &str, variable: &str) -> Result<String, ConfigError> {
    // TODO(M2.P1): Registry validates concrete slugs, probes aliases and checks capabilities.
    if value.trim().is_empty() {
        Err(ConfigError::EmptyModel(variable.to_owned()))
    } else {
        Ok(value.to_owned())
    }
}
fn bounded(value: &str, min: u8, max: u8) -> Option<u8> {
    value.parse::<u8>().ok().filter(|n| (min..=max).contains(n))
}
fn money(value: &str, zero_allowed: bool) -> Option<f64> {
    value
        .parse::<f64>()
        .ok()
        .filter(|n| n.is_finite() && (*n > 0.0 || (zero_allowed && *n == 0.0)))
}
fn effort(value: &str, variable: &str) -> Result<ReasoningEffort, ConfigError> {
    match value {
        "low" => Ok(ReasoningEffort::Low),
        "medium" => Ok(ReasoningEffort::Medium),
        "high" => Ok(ReasoningEffort::High),
        _ => Err(ConfigError::ReasoningEffort(variable.to_owned())),
    }
}

/// Parse assignments without consulting the environment or expanding dollar/tilde syntax.
/// Quotes delimit values, comments follow unquoted whitespace, and export is optional.
fn parse_literal_env(content: &str, path: &Path) -> Result<Vec<(String, String)>, ConfigError> {
    let mut entries = Vec::new();
    let mut lines = content.lines().enumerate();
    while let Some((index, raw)) = lines.next() {
        let invalid = || ConfigError::EnvironmentFileSyntax {
            path: path.to_owned(),
            line: index + 1,
        };
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line
            .strip_prefix("export ")
            .map(str::trim_start)
            .unwrap_or(line);
        let (key, raw_value) = line.split_once('=').ok_or_else(invalid)?;
        let key = key.trim();
        if key.is_empty()
            || !(key.as_bytes()[0].is_ascii_alphabetic() || key.starts_with('_'))
            || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return Err(invalid());
        }
        let raw_value = raw_value.trim_start();
        let value = if raw_value.starts_with(['\"', '\'']) {
            let quote = raw_value.chars().next().unwrap();
            let mut pending = raw_value[1..].to_owned();
            let mut value = String::new();
            loop {
                if let Some(end) = pending.find(quote) {
                    value.push_str(&pending[..end]);
                    let tail = pending[end + 1..].trim_start();
                    if !tail.is_empty() && !tail.starts_with('#') {
                        return Err(invalid());
                    }
                    break;
                }
                value.push_str(&pending);
                value.push('\n');
                pending = lines.next().ok_or_else(invalid)?.1.to_owned();
            }
            value
        } else {
            let end = raw_value
                .char_indices()
                .find(|(i, c)| {
                    *c == '#' && (*i == 0 || raw_value[..*i].ends_with(char::is_whitespace))
                })
                .map(|(i, _)| i)
                .unwrap_or(raw_value.len());
            let value = raw_value[..end].trim_end();
            if value.contains(['\"', '\'']) {
                return Err(invalid());
            }
            value.to_owned()
        };
        entries.push((key.to_owned(), value));
    }
    Ok(entries)
}
