# Foundations, Room API and CLI

### Foundations specification

Squad is one self-contained Rust binary named `squad`. It runs one process with one room, `main`, and one front end per room. The interactive interface is a TUI built with Ratatui 0.30.x and crossterm. Tokio runs the router, agent workers, contest orchestrator, and in-process bus.

Durable state lives in one local SQLite file, `<data_dir>/squad.db`. Use `rusqlite` with the `bundled` feature, WAL mode, and a dedicated writer thread. The router is the only logical writer. The transactional commit API, migrations, recovery, typed readers, and shared daily-spend ledger belong to Protocol, store and bus.

The workspace uses Rust stable, edition 2024:

```text
Cargo.toml
Cargo.lock
rust-toolchain.toml
rustfmt.toml
crates/
  squad-protocol/
    src/
      config.rs
      constants.rs
      envelopes/
      errors.rs
      parser/
  squad-store/
    src/
      lib.rs
      migrations/
  squad-agents/
    src/
      lib.rs
      agents/
      openrouter/
      models/
  squad-core/
    src/
      lib.rs
      room.rs
      state.rs
      bus/
      router/
      orchestrator/
      prompts/
      export/
      banner/
    prompts/
  squad-tui/
    src/
      lib.rs
  squad-cli/
    src/
      main.rs
      args.rs
      config.rs
      send.rs
      export.rs
      models.rs
      doctor.rs
      debug.rs
setup.sh
.env.example
.gitignore
README.md
LICENSE
NOTICE
.github/workflows/ci.yml
```

`crates/squad-protocol` contains validated serde types, protocol version `1`, shared constants, and the command parser. It has no runtime, persistence, or OpenRouter dependency. The other crates consume its contracts.

| Crate | Responsibility |
|---|---|
| `squad-protocol` | Envelopes, `Work`, `ErrorCode`, `Cursor`, `SquadConfig`, constants, parser, and reader/submit trait contracts |
| `squad-store` | SQLite schema, migrations, transactional commits, and typed readers |
| `squad-agents` | Agent executors, OpenRouter client, model registry, catalog fetches, alias probes, and context assembly through reader/submit traits |
| `squad-core` | Bus trait and in-process implementation, router, contest orchestration, prompt rendering, export, banner text and embedded fonts, and `RoomHandle` |
| `squad-tui` | Ratatui application using `RoomHandle` |
| `squad-cli` | The `squad` binary, argument parsing, configuration loading, command output, and lifecycle wiring |

`squad-agents` depends on `squad-protocol` and reader/submit traits only for application access. It never depends on `squad-store` or `rusqlite`. `squad-core` constructs context-bound readers and passes those capabilities to executors. Compile-fail isolation tests live in `squad-agents`.

Use `reqwest` for OpenRouter HTTP calls and SSE streaming, `serde` for JSON, `schemars` and `jsonschema` for verdict validation, `clap` for arguments, `dotenvy` for environment loading, `tracing` for logs, and `thiserror` for errors.

Pin the toolchain in `rust-toolchain.toml`, including `clippy` and `rustfmt`. Commit `Cargo.lock`. CI builds and tests with `--locked`.

Logs go to files under the data directory. They never write to the terminal while the TUI is active. Credentials and authorization headers are always redacted. The CLI installs a file-backed JSON tracing subscriber and propagates request, event, work, attempt, contest, and case identities through the applicable spans.

This section owns configuration, `RoomHandle`, room-state projections, command behavior, and command output. Other contracts retain their single owners:

| Contract | Reference |
|---|---|
| Envelopes, committed events, `Work`, `Cursor`, `ErrorCode`, requests, receipts, preflight payloads including `DebatePreflight`, transactional commits, and recovery | Protocol, store and bus |
| Parser AST, grammar, command validation, help content, and exact user-facing command-validation strings | Command grammar and parser |
| Model registry, model capabilities, ordinary execution, queueing, retries, and context assembly | Agents and OpenRouter |
| Contest states, admission, preflight calculation, round 0, round scoring, verdicts, judge cases, tournament and grill execution, pick flow, and contest recovery | Contests |
| Prompt template IDs, texts, placeholders, validation, and snapshots | Prompt pack |
| Timeline interaction, quote selection, contest presentation, banners, search, and keyboard behavior | TUI |
| Effects design and motion behavior | TUI effects and motion |

Do not define alternate envelope, work, verdict, or contest-state types here.

The Contests section also owns the shared moves and scripts for all seven modes, including census, crosscheck, survey, and probe. `RoomHandle`, state projections, command output, and exports consume those scripts’ shared contracts. Foundations does not define another engine, step state machine, visibility policy, tally formula, or admission calculation.

Building begins after the PRD is final, starting with M0 and M1 in the public repository `github.com/shoemoney/squad`. Most coding units go to the GPT Sol 6.1 worker on its fast tier, dispatched asynchronously with the widest fan-out each milestone allows, with one git worktree per worker and merges by the orchestrator. Overflow goes to the other coding workers. Work receives one review pass per milestone and an owner check-in at each milestone boundary. Repository and crate metadata use Apache License 2.0; every crate sets `license = "Apache-2.0"`. Ship `LICENSE` and `NOTICE`, and credit Apache-2.0-compatible embedded fonts and other vendored assets in `NOTICE`. Repository public (D61); release publication requires the owner's M7 decision.

Build order is M0, M1, M2, M3, then M4 and M5 in parallel. M4 exits after M5. M6 follows, then M7. M5 builds debate, grill, census, crosscheck, tournament, survey, then probe, following D71 and the phase list owned by Contests.

### Canonical values

Configuration exposes only the variables documented below. Timeouts, attempt counts, queue depth, output token ceilings, and message-size limits are compile-time constants in `squad-protocol`.

| Item | Value |
|---|---|
| Room | `main` |
| Members | 1 through 12 |
| MemberNick pattern | `^[a-z][a-z0-9_]{0,31}$` |
| Reserved nicks | `me`, `all`, `judge`, `system`, `squad` |
| `ROUNDS` | Default 10, minimum 1, maximum 20 |
| Prompt word limits | Room 500, debate turn 400, final debate turn 500, judge 1,000 |
| Visible output token ceilings | 2,048 for ordinary replies and debate turns; 4,096 for final judge calls; 512 for `judge.round` |
| Default reasoning effort | Medium for members; low for `judge.round`; high for final judge calls |
| Ordinary reply timeouts | Connect 15,000 ms; first token 90,000 ms; idle 30,000 ms; total 300,000 ms |
| Ordinary reply attempts | 2, including one automatic retry |
| Debate turn deadline | 180,000 ms |
| Debate turn attempts | 2; the orchestrator issues the second |
| Judge attempts | 3 for final judge calls, including schema-repair attempts |
| Round judge attempts | 2 for `judge.round`, including schema-repair attempts |
| Per-agent queue | 20 items; overflow uses `AGENT_QUEUE_FULL` |
| Human message size | 1 through 32,768 UTF-8 bytes |
| Quotes per message | Maximum 12 |
| Debate solution selections | Maximum 2, valid only for `$debate` |
| Daily spending max | Default USD 20 |
| Debate confirmation threshold | Default USD 2 |
| Storage capacity | 1,073,741,824 bytes |
| Reasoning token budgets | Low 1,024; medium 4,096; high 8,192 |
| Final judge call deadline | 300,000 ms |
| `judge.round` and `judge.tournament` deadline | 120,000 ms |
| Preflight plan expiry | 600,000 ms |
| Work lease | 60,000 ms, renewed every 20,000 ms |
| Ingress channel capacity | 256 |
| Mailbox capacity | 1; synchronous `try_send`, with Full meaning a wake-up is already pending |
| Feed broadcast capacity | 4,096; lag recovers from the events table after the last seen cursor |
| `GRILL_ROUNDS` | Default 2, minimum 1, maximum 10 |
| Grill question word limit | 200 |
| Grill answer word limit | `SQUAD_TURN_WORDS`, default 400 |
| Rebuilt plan word limit | `SQUAD_PLAN_WORDS`; default owned by [Prompt pack](09-prompt-pack.md#word-limit-defaults) |
| Rebuilt plan visible output ceiling | 4,096 tokens |
| Tournament entrants | Minimum 3 |
| Tournament judge attempts | 3, including schema repair; one round replay after exhausted judging, then failure |
| `CENSUS_PASSES` | Default 2, minimum 1, maximum 5 |
| Census, crosscheck, survey, and probe judge calls | Fixed medium reasoning effort and 4,096 visible output tokens, including report calls; use the judge system prompt, a mode-specific task prompt, strict JSON validation, and schema repair |

Every output ceiling limits visible output. For a model whose catalog entry lists reasoning support, send `max_tokens = visible ceiling + reasoning budget` and an explicit `reasoning.max_tokens` equal to that budget. Reasoning settings are omitted for unsupported models unless explicitly configured, in which case capability validation uses `MODEL_PARAMETER_UNSUPPORTED`. Request construction and capability validation follow Agents and OpenRouter.

Attempts, retry ownership, model capability checks, and daily-spend enforcement follow their owning contracts. Reasoning text is never forwarded or shown. The router never awaits a channel owned by an agent.

All durable room transcript data remains until `/clear`. There is no automatic history trimming in v1. `/clear` deletes the room’s rows in one transaction and runs `VACUUM`, following Protocol, store and bus. Shared daily accounting is not erased by clearing room history.

### Configuration and environment

`crates/squad-protocol/src/config.rs` defines the only `SquadConfig`. It is built from environment variables. Operational command flags select the environment file, data directory, and diagnostic output; they do not introduce another configuration format.

Select the first applicable environment file:

1. The explicit `--env-file <path>`.
2. `./.env`, if present.
3. `<OS config dir>/squad/.env`, if present.

An explicitly selected missing file is an error. Otherwise, absence of a file permits process-environment values and built-in defaults. Load exactly one file. Process-environment values override file values, including when an overriding value is empty.

Resolve relative configuration paths against the selected environment file’s directory. Without a selected file, resolve them against the working directory. Resolve a relative `--data-dir` argument against the working directory.

The complete normalized configuration is:

```rust
use std::path::PathBuf;

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

pub struct OpenRouterApiKey(String);

pub struct UserConfig {
    pub name: String,
}

pub struct AgentConfig {
    pub nick: MemberNick,
    pub model: String,
    pub persona: String,
    pub reasoning_effort: Option<ReasoningEffort>,
}

pub struct JudgeConfig {
    pub model: String,
    pub conflict_fallbacks: Vec<MemberNick>,
    pub reasoning_effort: ReasoningEffort,
    pub reasoning_effort_explicit: bool,
}

pub struct RoomDefaults {
    pub rounds: u8,
    pub reasoning_effort: ReasoningEffort,
    pub reasoning_effort_explicit: bool,
    pub grill_rounds: u8,
    pub census_passes: u8,
}

pub struct BudgetConfig {
    pub daily_max_usd: f64,
    pub debate_confirm_usd: f64,
}

pub struct PromptConfig {
    pub directory: Option<PathBuf>,
    pub limits: PromptLimits,
}

pub struct PromptLimits {
    pub room_words: u32,
    pub turn_words: u32,
    pub final_turn_words: u32,
    pub plan_words: u32,
}

pub enum ReasoningEffort {
    Low,
    Medium,
    High,
}

pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Off,
}

pub enum BannerMode {
    On,
    Compact,
    Off,
}

pub enum EffectsMode {
    Full,
    Lite,
    Off,
}
```

`MemberNick` and model validators come from their owning contracts. `room` is always `main`. The judge prompt’s 1,000-word instruction belongs to Prompt pack; there is no `judge_words` configuration field. Temperature is omitted from provider requests.

`AgentConfig.reasoning_effort` is `None` when no per-member override is set. The effective effort is the member override, otherwise the room default. Preserve whether global member and judge effort values were explicitly configured so unsupported defaults do not fail capability validation. `fx_explicit` records whether `SQUAD_FX` was set; automatic lite selection applies only when it was unset.

`OpenRouterApiKey` exposes its value only to the OpenRouter integration. It is not serialized, persisted, exported, or included in ordinary debug formatting.

| Environment variable | Default | Meaning and validation |
|---|---|---|
| `OPENROUTER_API_KEY` | None | Required nonempty credential for generation and online model checks |
| `SQUAD_ROUNDS` | `10` | Integer from 1 through 20; seeds `ROUNDS` only when no room value is stored |
| `SQUAD_MODEL_<NICK>` | Six defaults below when none are set | One member per variable; valid unique nick and nonempty OpenRouter slug |
| `SQUAD_MODEL_CLAUDE` | `~anthropic/claude-opus-latest` | Default `claude` selection |
| `SQUAD_MODEL_FABLE` | `~anthropic/claude-fable-latest` | Default `fable` selection |
| `SQUAD_MODEL_OPENAI` | `~openai/gpt-sol-latest` | Default `openai` selection |
| `SQUAD_MODEL_GEMINI` | `google/gemini-4-neon` | Default `gemini` selection |
| `SQUAD_MODEL_GROK` | `~x-ai/grok-latest` | Default `grok` selection |
| `SQUAD_MODEL_MUSE` | `meta/muse-wonder` | Default `muse` selection |
| `SQUAD_JUDGE_MODEL` | `~anthropic/claude-fable-latest` | Separate summoned judge model; never creates a member; default selection is independent of custom member selections |
| `SQUAD_USER_NAME` | `the user` | Nonempty text supplied to prompts; the human remains `me` in the interface |
| `SQUAD_PERSONA_<NICK>` | Empty string | Persona for a configured member; appended after the shared room system prompt |
| `SQUAD_DAILY_MAX_USD` | `150` | Positive finite number; the single daily spending max |
| `SQUAD_DEBATE_CONFIRM_USD` | `2` | Nonnegative finite number |
| `SQUAD_ROOM_WORDS` | `500` | Positive integer prompt limit |
| `SQUAD_TURN_WORDS` | `400` | Positive integer prompt limit |
| `SQUAD_FINAL_TURN_WORDS` | `500` | Positive integer prompt limit |
| `SQUAD_REASONING_EFFORT` | `medium` | `low`, `medium`, or `high` for members |
| `SQUAD_JUDGE_REASONING_EFFORT` | `high` | `low`, `medium`, or `high` for final judge calls; `judge.round` uses fixed low effort; census, crosscheck, survey, and probe judge calls use fixed medium effort |
| `SQUAD_JUDGE_FALLBACKS` | `claude,openai` | Ordered fallback member nicks for debate and standalone judge model conflicts; validate member references only when explicitly set |
| `SQUAD_PROMPTS_DIR` | Unset | Use embedded prompts when unset; otherwise validate the selected directory through Prompt pack |
| `SQUAD_DATA_DIR` | OS data directory | Linux: `~/.local/share/squad`; macOS: `~/Library/Application Support/squad` |
| `SQUAD_LOG_LEVEL` | `info` | `trace`, `debug`, `info`, `warn`, `error`, or `off` |
| `NO_COLOR` | Unset | A nonempty value disables color |
| `SQUAD_BANNERS` | `on` | `on`, `compact`, or `off`; TUI banner rendering follows TUI |
| `SQUAD_FX` | `full` | `full`, `lite`, or `off`; effects behavior follows TUI effects and motion; automatic lite selection applies only when unset |
| `SQUAD_GRILL_ROUNDS` | `2` | Integer from 1 through 10; seeds `GRILL_ROUNDS` only when no room value is stored |
| `SQUAD_PLAN_WORDS` | `1200` | Positive integer prompt limit for rebuilt grill plans |
| `SQUAD_REASONING_EFFORT_<NICK>` | Unset | Optional `low`, `medium`, or `high` override for a configured member |
| `SQUAD_FX_DEBUG` | Unset | `1` shows the effects-grade tag |
| `SQUAD_CENSUS_PASSES` | `2` | Optional integer from 1 through 5; seeds `CENSUS_PASSES` only when no room value is stored; omitted from `.env.example` |

The six default members are used only when no `SQUAD_MODEL_*` variable exists. A partial custom roster does not inherit omitted members. An empty model variable is an error rather than a removal instruction.

Roster order follows the selected file. An environment override replaces the corresponding value without changing its position. Members supplied only through the process environment follow file members in lexicographic variable-name order.

Lowercase each model-variable suffix before validating the nick. Reject duplicate declarations that normalize to the same nick. The process override of a file variable is not a duplicate declaration. Repeated nicks in an explicitly set `SQUAD_JUDGE_FALLBACKS` are invalid. An empty fallback list is permitted; selection still must satisfy Contests. Unconfigured default fallback nicks are skipped rather than making a custom roster fail startup.

A persona suffix must identify a configured member. Multiple members may use the same model with different personas. Roster changes, concrete model selections, personas, and user naming take effect on restart. Personas never enter judge context.

A per-member reasoning-effort suffix must identify a configured member. Normalize and validate it using the same nick rules as member declarations. An explicitly set unsupported reasoning value fails through the Agents and OpenRouter capability contract.

Concrete slugs select that version and must exist in the model catalog. A `~...-latest` alias is resolved by a startup probe and need not itself appear in the catalog. At startup, the model registry fetches `GET /api/v1/models` once to validate concrete selections and read context length and pricing. Unknown selections fail startup and name their environment variable.

For aliases, send a startup probe with `max_tokens: 16` and the lowest reasoning setting the model accepts. On a 400 response, retry once with `max_tokens: 2048`. Take the returned `model` field as the concrete version. Cache the result in the `models` table and keep the displayed version fixed until restart. Every completion uses that concrete slug. Record the response `model` for each attempt; a mismatch logs a warning and is reported by `squad doctor`. Gemini Neon and Muse Wonder use concrete default slugs that the user updates manually. Resolution, probe failures, catalog handling, and capability validation otherwise follow Agents and OpenRouter.

Judge conflicts compare resolved concrete models under Contests. For debates and standalone judging, skip fallback members that are involved in the material being judged, share a conflicting concrete model, or are unconfigured. Tournaments are judged blind and never swap the judge. Preserve the owning warning string:

```text
Judge model changed to {judge_model_slug} because @{nick} is debating.
```

Persisted `ROUNDS` overrides `SQUAD_ROUNDS` on restart. `$ROUNDS=n` and `/set ROUNDS n` continue to change it through the shared parser.

Persisted `GRILL_ROUNDS` overrides `SQUAD_GRILL_ROUNDS` on restart. `$GRILL_ROUNDS=n` and `/set GRILL_ROUNDS n` change it through the shared parser independently of `ROUNDS`.

Persisted `CENSUS_PASSES` overrides `SQUAD_CENSUS_PASSES` on restart. `$CENSUS_PASSES=n` and `/set CENSUS_PASSES n` change it through the shared parser independently of `ROUNDS` and `GRILL_ROUNDS`. Bare `$CENSUS_PASSES` reads the current room value. The other three new modes introduce no room variables or environment settings.

Prompt directory validation, template selection, placeholders, and snapshots follow Prompt pack. Numeric parsing rejects overflow, fractional integer values, nonfinite numbers, and trailing text. Reasoning, log-level, banner, and effects values use the lowercase spellings listed above.

Configuration failures occur before generation starts. Diagnostics identify variables or paths without including credential values. These local configuration diagnostics are exact; placeholders are replaced with the indicated name or path:

| Condition | Exact diagnostic |
|---|---|
| Explicit environment file missing | `Environment file not found: {path}.` |
| Environment file unreadable | `Cannot read environment file: {path}.` |
| Invalid environment-file syntax | `Invalid environment file: {path}, line {line}.` |
| Missing or empty credential | `OPENROUTER_API_KEY is required. Add it to .env or the process environment.` |
| Invalid rounds | `SQUAD_ROUNDS must be an integer from 1 to 20.` |
| Invalid member count | `Configure 1 to 12 members with SQUAD_MODEL_<NICK>.` |
| Invalid nick suffix | `Invalid member nick in {variable}: use ^[a-z][a-z0-9_]{0,31}$.` |
| Reserved nick | `Reserved member nick in {variable}: {nick}.` |
| Duplicate normalized nick | `Duplicate member nick: {nick}.` |
| Empty model selection | `{variable} must contain an OpenRouter model slug.` |
| Unknown model selection | `Unknown OpenRouter model in {variable}: {slug}.` |
| Persona for missing member | `{variable} refers to an unconfigured member: {nick}.` |
| Empty user name | `SQUAD_USER_NAME must be nonempty.` |
| Invalid daily max | `SQUAD_DAILY_MAX_USD must be a positive finite number.` |
| Invalid confirmation threshold | `SQUAD_DEBATE_CONFIRM_USD must be a nonnegative finite number.` |
| Invalid configurable word limit | `{variable} must be a positive integer.` |
| Invalid reasoning effort | `{variable} must be low, medium, or high.` |
| Invalid fallback nick | `Invalid member nick in SQUAD_JUDGE_FALLBACKS: {nick}.` |
| Missing explicitly configured fallback member | `SQUAD_JUDGE_FALLBACKS refers to an unconfigured member: {nick}.` |
| Duplicate fallback member | `Duplicate member nick in SQUAD_JUDGE_FALLBACKS: {nick}.` |
| Invalid log level | `SQUAD_LOG_LEVEL must be trace, debug, info, warn, error, or off.` |
| Empty configured path | `{variable} must contain a nonempty path.` |
| Unwritable data directory | `Data directory is not writable: {path}.` |
| Room lock held | `Room main is already open in another squad process (pid N).` |
| Invalid banner mode | `SQUAD_BANNERS must be on, compact, or off.` |
| Invalid effects mode | `SQUAD_FX must be full, lite, or off.` |
| Invalid grill rounds | `SQUAD_GRILL_ROUNDS must be an integer from 1 to 10.` |
| Reasoning override for missing member | `{variable} refers to an unconfigured member: {nick}.` |
| Invalid census passes | `SQUAD_CENSUS_PASSES must be an integer from 1 to 5.` |

Model capability, prompt-validation, parser, store, and contest failures retain the exact errors of their owning contracts. Do not replace them with configuration-specific alternatives.

Offline markdown export loads only the settings needed to locate and read the database. It does not require a credential or online model resolution. `squad doctor` reports missing or invalid settings and continues independent checks.

The shipped `.env.example` is exactly:

```dotenv
# Squad configuration. Copy to .env and add your key.

# Required. Get one at https://openrouter.ai/keys
OPENROUTER_API_KEY=

# Debate length: rounds per $debate (1 to 20). Change per room at runtime with $ROUNDS=5 or /set ROUNDS 5.
SQUAD_ROUNDS=10

# Daily spending cap in US dollars. Squad stops sending new requests when today's spend reaches it.
SQUAD_DAILY_MAX_USD=150

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
```

Optional variables remain out of this example. Document them in the README and `squad doctor --env`.

`.env` is ignored by version control. `setup.sh` creates it with mode `0600`. The key is never logged, printed by diagnostics, placed in the database, or included in exports. The repository and setup script have no key-vault integration; the owner’s vault use is local setup only.

### Setup script

The repository ships one idempotent Bash script, `./setup.sh`, for macOS arm64 and Linux x64 in Debian/Ubuntu and Fedora families. Each step checks existing state before installing and prints what it did.

| Step | Required behavior |
|---|---|
| Platform detection | Detect OS, architecture, and `brew`, `apt-get`, or `dnf`; exit with a clear message for unsupported combinations |
| Build prerequisites | Ensure a C toolchain, `pkg-config`, `git`, and `curl`; use Xcode Command Line Tools on macOS, `build-essential` on Debian/Ubuntu, or equivalent Fedora packages |
| Rust | Install `rustup` if missing, then install the toolchain from `rust-toolchain.toml` with `clippy` and `rustfmt` |
| Development tools | Install `cargo-nextest` and `cargo-insta` unless `--runtime-only` is set |
| Configuration | Copy `.env.example` to `.env` with mode `0600` only when `.env` is absent |
| Credential guidance | If the effective key is missing, say that `OPENROUTER_API_KEY` must be added to `.env` or the process environment; never echo it |
| Build and diagnostics | Run `cargo build --release`, run the built binary’s `doctor` command, print its result, and show `./target/release/squad` as the next command |

Flags are:

| Flag | Behavior |
|---|---|
| `--yes` | Noninteractive operation; permits unattended privilege escalation after printing its exact command |
| `--runtime-only` | Skip `cargo-nextest` and `cargo-insta` |
| `--no-build` | Skip the release build and built-binary diagnostics; report those skipped steps |
| `--help` | Print supported platforms, steps, and flags; make no changes |

Never overwrite an existing `.env`. Never run `sudo` before printing the exact command. Unattended `sudo` requires `--yes`. The script installs dependencies only and registers no services. It installs neither `toilet` nor `figlet`; banners are rendered in-process using embedded fonts in `squad-core`, following the TUI presentation contract.

A missing key does not prevent compilation. Diagnostics must report the missing credential and its nonzero result clearly. Re-running the script preserves user configuration and installed compatible dependencies.

### Tooling, CI, and distribution

| Command | Purpose |
|---|---|
| `cargo run -p squad-cli --` | Open Squad during development |
| `cargo build --workspace --locked` | Build the workspace |
| `cargo check --workspace --all-targets --locked` | Check all targets |
| `cargo fmt --all -- --check` | Verify formatting |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Reject lint warnings |
| `cargo nextest run --workspace --locked` | Run unit and integration tests |
| `cargo test --workspace --doc --locked` | Run documentation tests |
| `cargo insta review` | Review snapshot changes |
| `shellcheck setup.sh` | Check the setup script |

Use `insta` snapshots, Ratatui `TestBackend`, `wiremock` for fake OpenRouter responses, temporary-directory SQLite databases, `proptest` for the parser, and PTY-driven end-to-end tests.

Tests use isolated temporary data directories. They never read a developer’s credential or modify an existing room. The fake OpenRouter implementation remains owned by M2.

CI runs on macOS arm64 and Ubuntu x64. It installs the pinned toolchain and required development tools, checks formatting and lint, builds with the committed lockfile, and runs unit, integration, documentation, snapshot, and PTY checks. Run `shellcheck` and exercise `./setup.sh --yes` on both platforms. Use controlled test environment values and the fake OpenRouter integration for diagnostic checks.

Release preparation uses `cargo-dist` for macOS arm64 and Linux x64. Distribution includes prebuilt binaries, a Homebrew formula, `cargo install`, and `./setup.sh` for source builds. Publication requires the M7 owner decision.

### RoomHandle and process lifecycle

`RoomHandle`, defined in `crates/squad-core/src/room.rs`, is the single application API used by the TUI and room-opening commands:

```rust
impl RoomHandle {
    pub async fn open(
        config: SquadConfig,
        room: &str,
        mode: OpenMode,
    ) -> Result<RoomHandle, RoomError>;

    pub async fn submit(
        &self,
        request: Request,
    ) -> RequestResult;

    pub fn subscribe(&self) -> FeedReceiver;

    pub async fn page_events(
        &self,
        before: Option<Cursor>,
        limit: usize,
    ) -> Result<Vec<CommittedEnvelope>, RoomError>;

    pub async fn state(&self) -> RoomState;

    pub async fn export(
        &self,
        scope: ExportScope,
        id: Option<ContestId>,
    ) -> Result<String, RoomError>;

    pub async fn shutdown(&self) -> Result<(), RoomError>;
}
```

`Request`, `RequestResult`, `FeedReceiver`, and `CommittedEnvelope` use the shared bus and protocol contracts. `RoomError` preserves the applicable shared code and exact diagnostic. It does not define another `ErrorCode` enum. Use the shared `MemberNick`, `ContestId`, and `EventId` types consistently rather than defining alternate member or contest identifiers here.

`ExportScope` is declared in Protocol’s `squad-protocol` contract; markdown rendering lives in `crates/squad-core/src/export/`. Scopes are room, debate, tournament, grill, census, crosscheck, survey, and probe. `RoomHandle::export` returns markdown text for the TUI and other callers to save. The same module exposes:

```rust
pub fn export_offline(
    data_dir: &std::path::Path,
    room: &str,
    scope: ExportScope,
    id: Option<ContestId>,
) -> Result<String, RoomError>;
```

The offline path takes a consistent read snapshot without opening the room runtime or acquiring its exclusive lock. Neither export path depends on `squad-cli` or `squad-tui`. Banner text rendering and embedded fonts live in `crates/squad-core/src/banner/`.

`open` accepts only `main`. It validates configuration, creates the data directory when needed, acquires the exclusive advisory lock, opens and migrates SQLite, recovers the router, completes startup model resolution, and starts the room tasks. It returns only when the room is ready. Unknown configured slugs fail startup. Other member availability behavior follows Agents and OpenRouter.

`Interactive` resumes eligible unfinished work through the recovery contract. `OneShot` never re-wakes work it did not create. Recovery may reconstruct state and inspect unfinished records without dispatching unrelated work. `squad models` and `squad doctor` read the `models` cache without opening the room or issuing billed alias probes.

Data lives under `--data-dir`, otherwise `SQUAD_DATA_DIR`, otherwise the OS default:

```text
<data_dir>/
  squad.db
  main.lock
  logs/
```

The shared daily ledger’s location and file permissions follow Protocol, store and bus. Room database and auxiliary-file creation at mode `0600` belong to the store open path.

Hold `main.lock` for the room’s lifetime. A second process attempting to open the room exits with:

```text
Room main is already open in another squad process (pid N).
```

Replace `N` with the owning process ID. Read-only export does not acquire this exclusive lock.

`submit` sends the shared request through the router. Requests cover human messages with optional quotes, stop-all, stop-one, ordinary retry, regeneration, contest confirmation, contest cancellation, contest picks, judge actions, variable changes, and clear. Shared parser commands enter the same validated paths. Typed actions cannot bypass permissions, contextual validation, daily-spend limits, queue limits, or idempotency.

Census, crosscheck, survey, and probe use these same methods and shared request contracts. Route plan selection, census plan updates between passes, and crosscheck risk acceptance through the owning parser and contest actions. Preserve explicit human acceptance in the committed coverage artifact; neither a state read nor an export accepts a risk. Per-member payload selection, fresh auditor contexts, and atomic barrier delivery remain inside the shared engine and agent contracts.

Durable acceptance means the router transaction committed. Use `RequestResult::Accepted` directly: `requestId`, `eventIds`, and optional `cursor`. Do not fabricate `messageId` or `commandId` result variants. A preflight, help response, state read, export preparation, or pending-plan cancellation is not durable conversational acceptance.

Retried mutations retain their original request identity and normalized payload. Identical retries return the original receipt. Changed content under the same identity uses the shared idempotency error. Receipts remain valid for the room’s lifetime.

`shutdown` stops room tasks through the owning cancellation and recovery contracts, completes required persistence work, closes the database, releases the lock, and flushes logs. The TUI restores terminal state on normal exit, interruption, and failure.

For `squad send --no-wait`, commit the accepted work without dispatching it, print its accepted cursor, and close the process. Shutdown preserves that work for the next interactive open without consuming an attempt or cancelling it.

### RoomState

Rust fields serialize with camelCase names except for the explicitly named room variables:

```rust
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomState {
    pub room: String,
    pub vars: RoomVars,
    pub participants: Vec<Presence>,
    pub debates: Vec<DebateState>,
    pub pending_preflights: Vec<PendingPreflight>,
    pub models: Vec<Model>,
    pub scoreboard: Vec<Score>,
    pub daily_spend: DailySpend,
    pub trophies: std::collections::BTreeMap<MemberNick, u64>,
    pub active_contest: Option<ContestState>,
}

#[derive(serde::Serialize)]
pub struct RoomVars {
    #[serde(rename = "ROUNDS")]
    pub rounds: u8,
    #[serde(rename = "GRILL_ROUNDS")]
    pub grill_rounds: u8,
    #[serde(rename = "CENSUS_PASSES")]
    pub census_passes: u8,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailySpend {
    pub day: String,
    pub spent_usd: f64,
    pub reserved_usd: f64,
    pub limit_usd: f64,
    pub unknown_calls: u64,
    pub has_unreconciled_charges: bool,
}
```

`Model` uses the registry’s configured slug and startup resolution snapshot. `Presence` supplies shared status, work identity, queue depth, observation time, and optional error code. `Score` is the shared scoreboard type. `DebateState` comes from Contests; `ContestState` comes from Protocol. `PendingPreflight` holds either `DebatePreflight` or the non-debate `ContestPreflight`; all three use Protocol’s contracts.

`debates` preserves debate-state projections. `activeContest` identifies the room’s at most one active debate, tournament, grill, census, crosscheck, survey, or probe, including `COLLECTING_ANSWERS`, `AWAITING_PICK`, and the new modes’ `PREFLIGHT`, `WAITING_FOR_PARTICIPANTS`, and script-step states where active under the owning contract. Determine active status using the owning contest states. Do not introduce another state machine. Preserve the owning round-score and tally projections, including late and unscored rounds, without defining alternate score types.

For each new mode, preserve the owning contest projection’s participants, current step, progress, committed artifact references, skipped members, and terminal outcome. Census preserves the current plan version, pass number and limit, ledger, estimate range, discovery curve, and quiet-stop result. Crosscheck preserves the inventory, grid, closure and audit status, open gaps, and explicit human risk acceptances. Survey preserves the source answer set, claim and voting progress, and five-band report. Probe preserves the human-visible role assignment, any member sitting out, prediction matrix, and decision tree. Use shared types and committed data rather than recomputing tallies or reconstructing reports in `RoomState`. These projections are for the human; agent context readers receive only script-authorized deliveries.

An awaiting-pick projection preserves the contest identity, original question, requested pick count, eligible nicks, and committed answer references through the owning contest type. It survives restart and has no expiry. Other room messages remain usable while the pick is pending.

`scoreboard` is an authoritative projection of `scores`. Verdict notifications, repeated reads, and judge follow-ups do not increment it. Per-round scores and tallies do not change wins or losses.

`trophies` is the authoritative tournament-win count per member. A committed champion updates it once; replayed notifications and repeated state reads do not increment it. Clearing the room clears the room’s debate records and trophies under the store contract.

Census, crosscheck, survey, and probe do not award debate wins, losses, or tournament trophies. Repeated state reads, report delivery, recovery, and export do not mutate their results.

Daily spend values come from authoritative accounting and reservations for the local calendar day. All rooms share the same daily total under Protocol, store and bus. Remaining spend is:

```text
max(0, limitUsd - spentUsd - reservedUsd)
```

`spentUsd` reflects actual per-call cost reported by OpenRouter, including billed failed or cancelled calls. In-flight estimates appear in `reservedUsd` and are replaced by actual accounting when it arrives. Unknown charges remain reserved, increment `unknownCalls`, and set `hasUnreconciledCharges`. Shared `Usage.costUsd` remains nullable; unknown cost is never represented as known zero. State reads do not estimate actual spend from token prices.

The TUI uses this projection for its always-visible daily spend bar, including the pending segment and `~` prefix for unknown cost. At the daily max, reject new generation with `DAILY_MAX_REACHED`; in-flight calls finish. The exact message substitutes the effective max formatted to two decimal places:

```text
Daily max of $150.00 reached. Raise SQUAD_DAILY_MAX_USD or wait until tomorrow.
```

A contest whose preflight estimate exceeds the remaining daily amount is refused before admission with the same code. The next local day uses that day’s ledger; raising the configured max permits admission against the new effective amount. Spend counts Squad calls only.

### History, feed, and streaming

`subscribe` returns the in-process feed of committed events and ephemeral token deltas and presence. Agent tasks never receive this feed.

On opening the TUI, page durable events backward from the newest sequence. Load older pages on scroll. `page_events(None, limit)` selects the newest page; `Some(before)` selects events strictly before that cursor. Returned events are ascending by sequence. Page size defaults to 100 and accepts 1 through 500. An empty page is an empty vector.

`Cursor` follows the integer `events.seq` contract. Each event receives its sequence inside its transaction. UUIDs and timestamps never determine ordering. Invalid or future cursors use `CURSOR_INVALID`. Clear-boundary handling follows Protocol, store and bus.

Subscribe before capturing the initial durable high watermark. Combine the paged history and subsequent feed events without duplicate application. Durable events remain recoverable from SQLite if a receiver falls behind.

`debate.round_score` is human-visible committed content addressed only to `me`. Preserve its owning payload, round identity, tally, and unscored representation through history and feed delivery. A late score belongs to its stated round regardless of arrival order. Neither history paging nor feed delivery grants an agent access to another context; debaters first receive round scores with the verdict announcement under Contests.

Preserve committed tournament rankings, entrant results, champion events, grill questions, answers, passes, plan versions, and contest-pick events using their owning payloads and visibility rules. Human-facing history is not an agent-history reader. Never expose the blind-judge label map through ordinary transcript output or export.

Preserve each new mode’s setup, state, committed-artifact, and report events through the same history and feed paths, using Protocol-owned kinds and payloads. The human can inspect census finder identities and the member-by-gap table even though members receive only the script’s anonymised content. Human-visible census ledgers, crosscheck coverage and audit evidence, survey votes and switches, and probe roles and predictions do not become automatic deliveries to members. Recovery restores committed step progress and artifacts without revealing undelivered blind-step material or repeating a committed barrier.

Banners are derived from committed contest state transitions under TUI. They do not create events or consume cursors. Text rendering and embedded fonts are provided by `squad-core`.

Preserve shared delta identities: `messageId`, `workId`, integer `attempt`, `chunkIndex`, `triggerEventId`, optional contest or debate identity, optional `round`, and text, using the Protocol, store and bus schema. A higher attempt replaces the previous attempt’s provisional text. Batch presentation updates every 50 ms or 8 KiB without losing chunk identity or order. Apply pending deltas before their final event.

Token deltas are never durable. Startup recovery restores finalized events and fresh room state. It does not imply recovery of unfinished token text. A stalled feed consumer cannot block database commits or create unbounded memory growth.

After `/clear`, discard the old transcript, provisional streams, pending preflights, pending picks, active-contest projections, and cached pagination state. Do not resubmit pending mutations from before the clear. Preserve shared daily accounting.

### Submission and action behavior

Human text uses the shared parser and canonical UTF-8 byte limit. It can contain addressed messages, `$debate`, `$tournament`, `$grill`, `$census`, `$crosscheck`, `$survey`, `$probe`, sticky variable assignments, pick messages, or any supported slash command.

Quotes identify committed events. The router resolves their authors and text, labels them, and writes them into each recipient’s private history as part of the human message. Callers cannot supply asserted authors or resolved quote text. Missing or ineligible references fail before acceptance. For a held contest participant, defer the private-history write until the hold is released, following Agents and OpenRouter and Contests. Debate-turn attempts reuse the round’s frozen boundary.

For the four new modes, contest inputs and quotes follow the script’s delivery rules instead of automatic ordinary-message delivery. A crosscheck inventory writer receives only the goal, constraints, and assigned lens; selecting or quoting the plan must not put it into that writer’s context before inventory collection. Survey answer sources, census finder metadata, and probe explanations likewise remain behind their owning visibility boundaries. `RoomHandle` forwards validated references to the router and does not assemble member contexts.

`replyTo` remains valid only for `$debate`, with at most two solution selections. It is separate from ordinary quoting. Selection of a grill plan follows the shared parser and contest contract.

Plan selection for census and crosscheck, reuse of a parallel answer set for survey, and the diagnostic brief for probe follow the shared parser and contest contracts. Preserve the selected committed inputs so later room messages do not silently change the script’s source material. Census plan replacement between passes follows its explicit update path.

`@all -nick [-nick...]`, embedded assignments, and recipient validation use the shared parser. Store the resolved explicit recipient list. Model output is never parsed as routing instructions.

Every successful debate preflight uses the shared parser and canonical UTF-8 byte limit.

Preflight estimates for debates with `@judge` include one `judge.round` call per scored round in addition to the final judge call. Round 0 is not scored. Use the owning cost and reservation rules, fixed low effort, a 1,024-token reasoning budget and 512-token visible ceiling for `judge.round`, and the configured final judge effort. Display the round count and number of model calls with the estimate. Debates without `@judge` include no round-scoring calls.

Preflight creates neither a conversational message nor ordinary reply work. Confirmed admission commits the debate command once. Round 0, `WAITING_FOR_DEBATERS`, and all terminal states remain owned by Contests.

Canceling a pending plan makes no provider calls and creates no conversational message. After admission, stopping uses the ordinary stop path. A debate without `@judge` ends in `COMPLETE_UNJUDGED` and has no round scoring; later `/judge` can judge its transcript.

Stop without a nick stops all generation and queued work, including outstanding `judge.round` work. Stop with a nick targets that member under the owning contest rules. `/judge` on a stopped debate produces the final verdict from the transcript and existing round scores without scoring rounds retroactively. Judge-only re-entry, cancellation, expiry, and restoration of prior terminal status follow Contests.

Round scoring does not block the next debate round. The final judge waits until all round-scoring work has completed or been given up, following Contests. Exhausted round scoring is recorded as unscored and does not by itself fail the debate.

Every successful contest preflight, threshold admission, and confirmation follows the shared daily-spend and validation contracts. Preflight creates neither a conversational message nor ordinary reply work. Every mode’s preflight states the number of model calls and the estimated cost.

| Mode | Preflight call count |
|---|---|
| Debate | Calls required by the owning opening-round, debate-round, round-scoring, and final-judge plan |
| Tournament with `k` entrants | `k + (k-1) + ... + 2` entrant calls, plus `k-1` judge calls |
| Grill | `GRILL_ROUNDS × (grillers × 2 + 1)` calls |
| Census | Computed from the census script and effective `CENSUS_PASSES` |
| Crosscheck | Computed from the crosscheck script, including goal-and-constraints extraction when required |
| Survey | Computed from the survey script, including collection only when source answers must be generated |
| Probe | Computed from the probe script using the participating even member count |

For the four new modes, consume the engine’s script-derived call count and cost estimate rather than maintaining CLI formulas. Show the remaining daily amount, pass count where applicable, reused inputs, and any member sitting out. Apply the new modes’ fixed medium judge effort and visible output ceiling in the owning estimator. A zero-call move does not create a provider reservation.

One contest may be active per room. Preserve the owning rejection string:

```text
A {kind} is already running. Stop it before starting another.
```

Question-first debate and grill requests collect ordinary replies before the pick and later contest preflight. These calls count toward daily spend and are excluded from the later estimate. Preserve the original committed answers as pick candidates even if ordinary follow-ups occur while the pick is pending.

Pick messages become the Protocol-owned `ContestPick { contestId, nicks }` request. The shared parser owns the direct-versus-question-first decision table, mention-only pick recognition, validation strings, and examples. Invalid picks do not end the pending pick; unrelated messages use normal routing. A successful debate pick selects two answers and requests the judge by default. A grill pick selects one leader; every other eligible answering member becomes a griller. Tournaments have no pick step.

`/stop` while a pick is pending stops that contest with reason `pick_cancelled`; collected answers remain ordinary replies in the transcript and private histories. Escape handling follows the TUI’s transient-dismissal and contest-stop contract.

Census, crosscheck, survey, and probe use the shared stop and restart-recovery paths. Preserve committed artifacts and the owning terminal outcome. A member skipped after the script’s retry is named in output and the report; the front end does not restart the step or convert that omission into a successful answer.

Standalone judge questions retain the shared validation error:

```text
Quote at least one message for @judge to rule on.
```

Judge context and case follow-ups follow Contests. Front ends never assemble judge context. Standalone rulings remain free-form text.

### Commands and flags

Global flags apply to every command where relevant:

| Flag | Behavior |
|---|---|
| `--env-file <path>` | Select the environment file explicitly |
| `--data-dir <path>` | Override the effective data directory |
| `--bus-log <file>` | Write request, commit, and mailbox wake-up diagnostics as JSON lines |
| `--bus-log-bodies` | Include human-visible message bodies in bus diagnostics; requires `--bus-log` |
| `-h`, `--help` | Print command usage |
| `-V`, `--version` | Print the Squad version |

Help and version exit without opening a room or contacting OpenRouter.

#### `squad`

```text
squad [global flags]
```

Open `main` with `OpenMode::Interactive` and run the TUI. Acquire the room lock before starting room work. Output uses the terminal interface; logs remain in files. Another process holding the room lock causes the exact lock diagnostic and a nonzero exit.

#### `squad send`

```text
squad send "<message>" [--quote <seq>...] [--yes] [--json] [--no-wait]
```

Open the room headless with `OpenMode::OneShot` and submit one message through `RoomHandle`. Never resume unrelated unfinished work.

| Flag | Behavior |
|---|---|
| `--quote <seq>...` | Resolve committed event sequences to quote references; accept at most 12 |
| `--yes` | Preconfirm the contest plan derived from this submission |
| `--json` | Write the JSON lines records defined below |
| `--no-wait` | Exit after durable commit, print the accepted cursor, and leave accepted work for the next interactive open |

Without `--no-wait`, wait until every addressed model’s work, or the submitted contest and requested judging, reaches its terminal outcome. A member failure does not prevent other addressed members from finishing. Do not wait for unrelated room work.

Text output labels replies with the author nick. Debate output identifies rounds, debaters, round scores and tallies, unscored rounds, verdicts, and terminal failures. Identify late scores by their round. Preserve final verdict fields `roundTally` and `tallyOverrideReason` from the owning contract. Print preflight round count, number of model calls, estimate, remaining daily amount, judge selection, and warnings before admission.

Tournament output identifies entrants, rounds, rankings, scores, critiques, eliminations, final placements, and champion. Grill output identifies the leader, grillers, round and turn, questions, passes, answers, plan changes, plan versions, and completion reason. Pick events identify the pending contest, eligible candidates, and required pick count without fabricating a completed contest.

Census, crosscheck, survey, and probe output identifies the contest, mode, current step, committed artifacts, skipped members, and completion reason. Preserve human-visible source attribution without adding it to anonymous member deliveries. Census output includes pass and plan version, confirmed and covered gaps, finder counts, singletons, citations, estimate range and caveat, discovery curve, quiet-stop result, and the hint to send singletons to `$grill`. Crosscheck output includes coverage status, citations, justified exclusions, open gaps and proposed fixes, audit findings, explicit risk acceptances, and the bounded stop result. Survey output includes all five bands, opposing reasons, switches, and settling questions. Probe output includes the role split, any member sitting out, prediction matrix, decision tree, inseparable explanations, and unexpected-result branch. Report wording and computed values come from committed artifacts.

For an above-threshold plan without `--yes`, an interactive invocation may obtain confirmation and submit the shared confirm or cancel action. A noninteractive invocation fails clearly without admitting or generating the contest.

`--yes` preconfirms only the plan derived from this submission. It cannot override admission validation, the daily max, unavailable pricing, or context-fit checks. It does not choose participants for a pending pick.

For crosscheck, `--yes` does not accept residual risks. Those require the explicit human action defined by the owning parser and contest contracts.

`--no-wait` makes no provider calls for accepted work. Do not dispatch that work and then cancel it during shutdown. For local results without a durable acceptance cursor, print the shared result without inventing a cursor.

#### JSON lines output

`--json` emits one JSON object per line on standard output. Diagnostics go to standard error. Each record has a string `type`.

These are the complete record shapes. Shared payload types are referenced by name and retain their owning schemas:

| `type` | Required fields | Optional fields |
|---|---|---|
| `ack` | `requestId: EventId`, `eventIds: Vec<EventId>` | `cursor: Cursor`, present when supplied by `RequestResult::Accepted` |
| `event` | `cursor: Cursor`, `event: CommittedEnvelope` | None |
| `debate.preflight` | `requestId: EventId`, all fields of `DebatePreflight` at the top level | Only fields optional in `DebatePreflight` |
| `debate.cancelled` | `requestId: EventId`, `debateId: EventId`, `planId: EventId` | None |
| `command.result` | `requestId: EventId`, `result: {"command":"help","text":string} \| {"command":"who","state":RoomState}` | None |
| `export.ready` | `requestId: EventId`, `scope: "room" \| "debate" \| "tournament" \| "grill" \| "census" \| "crosscheck" \| "survey" \| "probe"`, `filename: string`, `path: string` | `id: EventId`, required for non-room scope |
| `error` | `error: {"code": ErrorCode, "message": string, "retryable": boolean}` | `requestId: EventId` |
| `tournament.preflight` | `requestId: EventId`, all fields of the shared tournament preflight payload at the top level | Only fields optional in that payload |
| `grill.preflight` | `requestId: EventId`, all fields of the shared grill preflight payload at the top level | Only fields optional in that payload |
| `contest.cancelled` | `requestId: EventId`, `contestId: EventId`, `planId: EventId` | None |
| `census.preflight` | `requestId: EventId`, all fields of the shared census preflight payload at the top level | Only fields optional in that payload |
| `crosscheck.preflight` | `requestId: EventId`, all fields of the shared crosscheck preflight payload at the top level | Only fields optional in that payload |
| `survey.preflight` | `requestId: EventId`, all fields of the shared survey preflight payload at the top level | Only fields optional in that payload |
| `probe.preflight` | `requestId: EventId`, all fields of the shared probe preflight payload at the top level | Only fields optional in that payload |

Emit committed events relevant to the submitted request in sequence order, including final replies, execution errors, contest events, verdicts, accounting events, and supersession events. Do not rename their kinds or alter their payloads. Round scores use the existing `event` record with kind `debate.round_score`; do not introduce a separate output record type. Preserve final verdict fields `roundTally` and `tallyOverrideReason`.

Tournament, grill, collecting, pick-requested, and picked events also use the existing `event` record with their Protocol-owned kinds and payloads.

Census, crosscheck, survey, and probe setup, state, artifact, and report events use the existing `event` record with their Protocol-owned kinds and payloads. Preserve the distinction between human-visible artifact metadata and anonymous payloads delivered to members.

JSON mode prints completed durable content rather than ephemeral token deltas. It does not duplicate the final answer as a separate text record.

Emit preflight before acceptance when a contest is automatically admitted. A pending preflight is not an `ack`. Local results and pending cancellation do not receive a fabricated durable cursor.

End of output and the process exit status indicate command completion. There is no additional completion record. Failure before request creation may omit `requestId`. Argument-syntax failures use standard error and exit code 2.

#### `squad export`

```text
squad export room|debate|tournament|grill|census|crosscheck|survey|probe [id] [-o file]
```

Read SQLite without opening the room runtime. This command is allowed while the room is open in another process and requires neither an API key nor online model resolution.

`room` exports the room. `debate`, `tournament`, `grill`, `census`, `crosscheck`, `survey`, and `probe` require their contest ID. Reject an ID supplied for room scope.

With `-o`, write UTF-8 markdown to that path and print the resulting path. Without `-o`, write markdown to standard output. Report failures on standard error.

The CLI delegates rendering to `squad-core::export::export_offline`. TUI export uses `RoomHandle::export`; both paths share the same renderer.

Export only committed human-visible content. Preserve attribution, contest structure, verdicts, and supersession labels. Exclude reasoning text, credentials, internal prompts, work records, diagnostic records, and the blind-judge label map. Use a consistent read snapshot.

Include committed round scores, their reasons and tallies, unscored markers, and final verdict fields `roundTally` and `tallyOverrideReason`. Derive banners from committed `debate.state` transitions using the TUI boundary rules: `OPENING`, `ROUND n`, `FINAL ROUND`, and `VERDICT`. Render each banner as a fenced code block using the embedded default FIGlet font in `squad-core`, regardless of `SQUAD_BANNERS`, `SQUAD_FX`, or terminal width. Banners are derived presentation and create no additional events.

Tournament export includes entrant answers, rankings, scores, critiques, eliminations, final placements, and champion. Grill export includes the final plan and a question-and-answer appendix naming each griller, its questions, the leader’s answers, and resulting plan changes. Preserve every committed plan version. Include tournament and grill banners at their owning state boundaries through the same shared banner renderer. Question-first exports preserve the original question, collected answers, pick request, and committed selection.

Census export includes the plan versions and pass boundaries, ranked confirmed-gap ledger with finder counts, lenses and consequences, highlighted singletons, covered items with citations, the human-visible member-by-gap table, and the discovery curve across passes. Preserve the committed estimate range and the caveat that models share blind spots and the true unseen count is at least the estimate. Include the headline `Found {n}; estimated {lo} to {hi} still unfound.`, the committed quiet-stop result, and the hint to send singletons to `$grill`. Export does not calculate a new estimate.

Crosscheck export includes the goal and constraints, normalized inventory, every retained grid cell and its disposition, plan citations, justified exclusions, open gaps with proposed fixes, audit evidence and rulings, and residual risks with their explicit human acceptance status. Preserve the bounded stop statement; export does not close a cell, accept a risk, or turn an unaudited ledger into a completed audit.

Survey export includes the question and source-answer references, atomic claims, votes and confidence, anonymous reasons used for re-voting, switches, and the final five bands: consensus, contested, lone insights, lone dissents, and rejected outliers. Preserve the best reason from each side and the settling question for each contested claim. Human-visible attribution does not change the anonymous content used during the script.

Probe export includes the observation, ruled-out explanations, inspection constraints, role split and any member sitting out, proposed explanations and key assumptions, checks with measurements and outcomes, committed prediction matrix, discarded checks and inseparable explanations, and the runnable decision tree with its unexpected-result branch. It does not declare an explanation true or perform the proposed checks.

Each new mode’s export includes its committed report, skipped-member disclosures, terminal status, and partial artifacts when stopped or failed. Derive `CENSUS`, `CROSSCHECK`, `SURVEY`, and `PROBE` banners and step dividers from the owning committed state boundaries through the shared renderer. Room export includes the same committed human-visible artifacts. Exports remain deterministic read operations and never invoke a model.

#### `squad models`

```text
squad models
```

Read the `models` cache without alias probes or opening the room. If no startup snapshot exists, report that the room must be opened to resolve models. Print the resolved roster in roster order, showing nick, configured slug, concrete resolved version, and any resolution error. Identify the separate judge selection and ordered fallback nicks.

Use the registry and startup-resolution contracts without opening the room or acquiring its exclusive runtime lock. Catalog fetches and alias probes live in `squad-agents`; this command never resumes room work.

#### `squad doctor`

```text
squad doctor [--env]
```

Run independent checks and print one labeled result per check, followed by the overall result. Continue independent checks after a failure.

| Check | Required behavior |
|---|---|
| Configuration | Report selected file or built-in defaults, precedence, parsing, roster, numeric values, and reference validation |
| Credential presence | Report present or missing; never print any part of the key |
| OpenRouter reachability | Check the configured service using the ordinary integration; sanitize failures |
| Model resolution | Validate configured selections against the cached `models` snapshot and catalog; report missing or stale resolution without probing |
| Data directory | Check creation and writability without modifying room rows |
| Terminal capabilities | Check whether the terminal supports the required TUI input and display operations |
| Daily accounting | Report today’s actual spend, reservations, effective daily max, and unknown-cost call count from the shared ledger |
| Model identity | Report recorded response-model mismatches against the concrete slug selected for each attempt |

Diagnostic checks do not open the room, acquire its exclusive runtime lock, resume work, or start ordinary generation. Alias probes run only at room open under synthetic accounting work; diagnostics read the `models` cache and report that a room open is needed when no resolution exists.

`--env` additionally prints every supported variable, its default, effective nonsecret value, and source. Include wildcard member, persona, and per-member reasoning variables, `SQUAD_BANNERS`, `SQUAD_FX`, `SQUAD_FX_DEBUG`, `SQUAD_DAILY_MAX_USD`, `SQUAD_GRILL_ROUNDS`, `SQUAD_PLAN_WORDS`, and `SQUAD_CENSUS_PASSES`. For the key, print only `present` or `missing`. Explain that persisted `ROUNDS`, `GRILL_ROUNDS`, and `CENSUS_PASSES` override their environment seeds.

An unsuitable terminal is reported clearly. It does not invalidate otherwise usable noninteractive commands.

#### Bus diagnostics

```text
squad --bus-log <file> [--bus-log-bodies]
```

Record every request, commit, and mailbox wake-up as JSON lines. Preserve the owning diagnostic identities, destinations, and sequence information. Redact message bodies by default.

`--bus-log-bodies` includes human-visible transcript content only. It never includes credentials, authorization headers, reasoning text, internal prompts, or unrestricted private histories. The TUI debug pane, opened with F12, uses the same diagnostic source.

### Slash commands

These are the complete v1 slash forms accepted through the shared parser. Their arguments, defaults, context checks, and exact validation strings remain owned by Command grammar and parser.

| Command | Result |
|---|---|
| `/set NAME value` | Validate and persist the supported room variable; `ROUNDS` accepts 1 through 20, `GRILL_ROUNDS` accepts 1 through 10, and `CENSUS_PASSES` accepts 1 through 5 |
| `/stop` | Stop all generation and queued work, including outstanding judge work and any pending pick |
| `/stop @nick` | Stop the named member under the shared contest rules |
| `/judge [debateId]` | Judge the eligible debate transcript selected by the parser rules; stopped debates use existing round scores without retroactive scoring |
| `/regen [messageId]` | Regenerate the eligible member reply selected by the parser rules |
| `/export room\|debate\|tournament\|grill\|census\|crosscheck\|survey\|probe [id]` | Export markdown using the shared export behavior |
| `/clear` | Persistently wipe the room through the router |
| `/who` | Return current room state without creating a conversational message |
| `/help` | Return shared parser help without creating a conversational message |

`$ROUNDS=n`, bare `$ROUNDS`, `$GRILL_ROUNDS=n`, bare `$GRILL_ROUNDS`, `$CENSUS_PASSES=n`, bare `$CENSUS_PASSES`, `$debate`, `$tournament`, `$grill`, `$census`, `$crosscheck`, `$survey`, `$probe`, pending-pick messages, addressing, exclusions, and quote eligibility retain their shared grammar. No `/quit` command is introduced.

### Exit codes

| Code | Meaning |
|---|---|
| `0` | Requested operation succeeded; a waiting send completed successfully, or a no-wait send obtained a successful submission result |
| `1` | Configuration, startup, lock, provider, execution, confirmation, export, storage, or required diagnostic failure |
| `2` | Invalid command-line syntax or flag combination |
| `130` | Interrupted with Ctrl+C after cleanup |

A waiting send exits 1 if any addressed execution ends in terminal failure, even when other addressed replies succeed. Report the successful replies before exiting. Shared command errors retain their exact messages. A round recorded as unscored after exhausted `judge.round` attempts is not by itself a terminal execution failure.

Normal TUI exit performs cleanup and returns 0. Required doctor checks that fail produce exit 1. Help and version return 0.


### Automatic PRD export directory

`<data_dir>/exports/` holds automatically saved backward PRDs named
`prd-<slug>-<contestId>.md`. The report commits before the atomic temporary-file
rename, files use mode 0600, and recovery repeats the same destination and bytes.
A disk failure preserves the stored exportable report and records `save_failed`.
One-shot JSON output includes `export.ready` and human output prints the saved
path. Optional PRD human review returns at `AWAITING_HUMAN` before saving.
