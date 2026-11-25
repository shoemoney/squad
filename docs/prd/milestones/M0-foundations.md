# M0 — Foundations

**Goal:** Establish a reproducible Rust workspace, one environment-derived configuration contract, and a working setup script.

**Exit criteria:**

- The six workspace crates build with the pinned toolchain and committed lockfile.
- Formatting, lint, unit, integration, and documentation checks pass.
- Configuration fixtures cover defaults, precedence, custom rosters, personas, user naming, judge references, and exact diagnostics.
- `.env.example` matches the specified text, and generated `.env` files have mode `0600`.
- `setup.sh` passes `shellcheck`, runs on the required CI platforms, and preserves existing configuration on rerun.
- Credentials are absent from logs, diagnostic output, snapshots, and database fixtures.
- The milestone receives one review pass and an owner check-in.
- `SQUAD_BANNERS` and `SQUAD_FX` have validated defaults, environment precedence, exact diagnostics, and optional-variable documentation.
- `squad-agents` has no dependency on `squad-store` or `rusqlite`; executor access is limited to the shared reader/submit traits.
- Apache-2.0 crate metadata, `LICENSE`, `NOTICE`, public repository documentation, and vendored-asset credits agree.
- Daily max, grill rounds, plan words, per-member reasoning overrides, and effects-debug settings have documented defaults and configuration fixtures.
- Visible-output ceilings, reasoning budgets, judge deadlines, plan expiry, work leases, and channel capacities match the canonical values.
- File-backed JSON tracing propagates applicable identities and never writes logs to the active TUI terminal.
- `SQUAD_CENSUS_PASSES` has default 2, range 1 through 5, environment precedence fixtures, and the exact configuration diagnostic; it remains absent from `.env.example`.
- Shared constants and documentation distinguish the four new modes’ fixed medium judge effort and 4,096-token visible ceiling from existing judge settings.
- Build-order documentation follows D71 and leaves the engine and M5 phase definitions with Contests.

## Phase M0.P1 — Workspace and tooling

- **M0.P1.L1 — Establish crate boundaries**
  - [ ] M0.P1.L1.T1 — Declare the six crates, edition 2024, and shared dependencies in root `Cargo.toml` and each crate’s `Cargo.toml`.
  - [ ] M0.P1.L1.T2 — Define the `squad` binary in `crates/squad-cli/Cargo.toml` and its entry point in `crates/squad-cli/src/main.rs`.
  - [ ] M0.P1.L1.T3 — Pin the toolchain and required components in `rust-toolchain.toml` and commit the workspace `Cargo.lock`.
  - [ ] M0.P1.L1.T4 — Add Apache-2.0 metadata for every workspace crate in its `Cargo.toml`; ship `LICENSE` and `NOTICE`, credit compatible vendored assets, and document the public repository and M7 release-publication decision in `README.md`.
  - [ ] M0.P1.L1.T5 — Establish the executor boundary in `crates/squad-agents/Cargo.toml` and `crates/squad-agents/src/lib.rs`, allowing application access through `squad-protocol` reader/submit traits and prohibiting dependencies on `squad-store` and `rusqlite`.

- **M0.P1.L2 — Configure development checks**
  - [ ] M0.P1.L2.T1 — Configure workspace formatting and lint expectations in root `Cargo.toml` and `rustfmt.toml` for all six crates.
  - [ ] M0.P1.L2.T2 — Configure nextest discovery for workspace crates in `.config/nextest.toml`.
  - [ ] M0.P1.L2.T3 — Add temporary-data and snapshot test scaffolding for `squad-protocol`, `squad-store`, and `squad-core` in their `tests/` directories.
  - [ ] M0.P1.L2.T4 — Add compile-fail isolation test scaffolding in `crates/squad-agents/tests/` and dependency-boundary checks for `crates/squad-agents/Cargo.toml`.

## Phase M0.P2 — Configuration and safe startup

- **M0.P2.L1 — Define the configuration contract**
  - [ ] M0.P2.L1.T1 — Implement the complete configuration types, defaults, and variable validation in `crates/squad-protocol/src/config.rs`.
  - [ ] M0.P2.L1.T2 — Define fixed limits, timeouts, and attempt constants in `crates/squad-protocol/src/constants.rs` using the owning contracts.
  - [ ] M0.P2.L1.T3 — Add configuration fixtures for one-member and twelve-member rosters, duplicate model selections, invalid nicks, reserved names, and a thirteenth member in `crates/squad-protocol/tests/config.rs`.
  - [ ] M0.P2.L1.T4 — Test daily spending limits, rounds, word limits, reasoning values, personas, user naming, and judge references in `crates/squad-protocol/tests/config.rs`.
  - [ ] M0.P2.L1.T5 — Assert exact local configuration diagnostics in `crates/squad-protocol/tests/config_errors.rs`.
  - [ ] M0.P2.L1.T6 — Add `BannerMode` and `EffectsMode`, their defaults, and variable validation in `crates/squad-protocol/src/config.rs`; test all accepted values, empty and invalid values, and exact diagnostics in `crates/squad-protocol/tests/config.rs` and `crates/squad-protocol/tests/config_errors.rs`.
  - [ ] M0.P2.L1.T7 — Add the fixed low reasoning effort, 1,024-token reasoning budget, 512-token visible output ceiling, and two-attempt limit for `judge.round` in `crates/squad-protocol/src/constants.rs` using Contests.
  - [ ] M0.P2.L1.T8 — Add `daily_max_usd`, `grill_rounds`, `plan_words`, per-member reasoning overrides, explicit-setting provenance, and `fx_debug` to `crates/squad-protocol/src/config.rs`; test defaults, boundaries, missing-member references, invalid values, and exact diagnostics in `crates/squad-protocol/tests/config.rs` and `crates/squad-protocol/tests/config_errors.rs`.
  - [ ] M0.P2.L1.T9 — Define low, medium, and high reasoning budgets, final and round judge deadlines, preflight expiry, lease duration and renewal, ingress/mailbox/feed capacities, and grill limits in `crates/squad-protocol/src/constants.rs`.
  - [ ] M0.P2.L1.T10 — Test implicit fallback skipping, explicitly configured fallback validation, empty lists, duplicates, and custom rosters without `claude` or `openai` in `crates/squad-protocol/tests/config.rs`.
  - [ ] M0.P2.L1.T11 — Verify that configuration preserves whether reasoning effort and effects mode were explicitly set in `crates/squad-protocol/tests/config.rs`, allowing the owning runtime capability and auto-lite rules to distinguish defaults.
  - [ ] M0.P2.L1.T12 — Add `RoomDefaults.census_passes`, default 2, and `SQUAD_CENSUS_PASSES` validation for integers 1 through 5 in `crates/squad-protocol/src/config.rs`; test boundaries, empty values, overflow, fractions, trailing text, and the exact diagnostic in `crates/squad-protocol/tests/config.rs` and `crates/squad-protocol/tests/config_errors.rs`.
  - [ ] M0.P2.L1.T13 — Define the four new modes’ shared fixed medium judge effort and 4,096-token visible ceiling in `crates/squad-protocol/src/constants.rs` using the Contests and Agents contracts; verify existing judge constants remain unchanged in `crates/squad-protocol/tests/config.rs`.

- **M0.P2.L2 — Load the effective environment**
  - [ ] M0.P2.L2.T1 — Implement environment-file selection, process overrides, roster order, and relative-path handling in `crates/squad-cli/src/config.rs`.
  - [ ] M0.P2.L2.T2 — Test explicit missing files, malformed files, duplicate nicks, empty overrides, and environment-only members in `crates/squad-cli/tests/config.rs`.
  - [ ] M0.P2.L2.T3 — Add the verbatim example to `.env.example` and ignore `.env` and local runtime data in `.gitignore` for `squad-cli`.
  - [ ] M0.P2.L2.T4 — Verify credential redaction and absence from serialized diagnostics in `crates/squad-cli/tests/secrets.rs`.
  - [ ] M0.P2.L2.T5 — Test file values, process overrides, defaults, and empty overrides for `SQUAD_BANNERS` and `SQUAD_FX` in `crates/squad-cli/tests/config.rs`.
  - [ ] M0.P2.L2.T6 — Test environment precedence for `SQUAD_DAILY_MAX_USD`, `SQUAD_GRILL_ROUNDS`, `SQUAD_PLAN_WORDS`, `SQUAD_REASONING_EFFORT_<NICK>`, and `SQUAD_FX_DEBUG` in `crates/squad-cli/tests/config.rs`; verify the complete updated `.env.example`.
  - [ ] M0.P2.L2.T7 — Preserve explicit-setting provenance through environment merging in `crates/squad-cli/src/config.rs`; test unset versus explicitly set effects and reasoning values in `crates/squad-cli/tests/config.rs`.
  - [ ] M0.P2.L2.T8 — Load optional `SQUAD_CENSUS_PASSES` through the existing environment precedence path in `crates/squad-cli/src/config.rs`; test defaults, file values, process overrides, and invalid empty overrides in `crates/squad-cli/tests/config.rs`, and assert `.env.example` remains verbatim without this optional variable.

- **M0.P2.L3 — Initialize file logging**
  - [ ] M0.P2.L3.T1 — Install a file-backed JSON tracing subscriber with credential redaction and applicable request, event, work, attempt, contest, and case span identities in `crates/squad-cli/src/main.rs`.
  - [ ] M0.P2.L3.T2 — Verify log-file output, identity propagation, flushing, secret exclusion, and absence of terminal logging while the TUI is active in `crates/squad-cli/tests/logging.rs`.

## Phase M0.P3 — Setup script

- **M0.P3.L1 — Detect platforms and install prerequisites**
  - [ ] M0.P3.L1.T1 — Implement supported OS, architecture, and package-manager detection for `squad-cli` source builds in root `setup.sh`.
  - [ ] M0.P3.L1.T2 — Add checked installation of the C toolchain, `pkg-config`, `git`, and `curl` in root `setup.sh` for `squad-cli`.
  - [ ] M0.P3.L1.T3 — Print exact privilege-escalation commands and enforce unattended `--yes` handling in root `setup.sh` for `squad-cli`.

- **M0.P3.L2 — Prepare Rust and configuration**
  - [ ] M0.P3.L2.T1 — Install or reuse `rustup` and the pinned toolchain with required components in root `setup.sh` for all workspace crates.
  - [ ] M0.P3.L2.T2 — Install checked development tools and implement `--runtime-only` in root `setup.sh` for workspace verification.
  - [ ] M0.P3.L2.T3 — Create `.env` with mode `0600` without overwriting it and print safe missing-key guidance in root `setup.sh` for `squad-cli`.

- **M0.P3.L3 — Build and verify reruns**
  - [ ] M0.P3.L3.T1 — Implement release build, built-binary doctor execution, result reporting, and next-command output in root `setup.sh` for `squad-cli`.
  - [ ] M0.P3.L3.T2 — Implement `--no-build`, `--help`, and repeat-run checks in root `setup.sh` for `squad-cli`.
  - [ ] M0.P3.L3.T3 — Add setup-script fixtures covering preserved configuration, skipped steps, and unsupported platforms in `crates/squad-cli/tests/setup.rs`.
  - [ ] M0.P3.L3.T4 — Verify that root `setup.sh` installs neither `toilet` nor `figlet` in `crates/squad-cli/tests/setup.rs`.

## Phase M0.P4 — CI and developer workflow

- **M0.P4.L1 — Make verification reproducible**
  - [ ] M0.P4.L1.T1 — Configure pinned-toolchain, locked-build, formatting, lint, nextest, and documentation checks for all crates in `.github/workflows/ci.yml`.
  - [ ] M0.P4.L1.T2 — Run `shellcheck` and `setup.sh --yes` on macOS arm64 and Ubuntu x64 for `squad-cli` in `.github/workflows/ci.yml`.
  - [ ] M0.P4.L1.T3 — Configure isolated data directories and controlled OpenRouter fixtures for workspace CI in `.github/workflows/ci.yml`.
  - [ ] M0.P4.L1.T4 — Document workspace commands, all environment variables, setup flags, and source-build instructions for `squad-cli` in `README.md`.
  - [ ] M0.P4.L1.T5 — Record the workspace M0 review and owner check-in in `docs/milestones/M0.md`.
  - [ ] M0.P4.L1.T6 — Document `SQUAD_BANNERS` and `SQUAD_FX` together as optional variables with their defaults and accepted values for `squad-cli` in `README.md`.
  - [ ] M0.P4.L1.T7 — Document daily accounting, grill defaults, plan limits, per-member reasoning settings, effects-debug settings, and the complete updated `.env.example` for `squad-cli` in `README.md`.
  - [ ] M0.P4.L1.T8 — Document the six-crate dependency boundaries, public repository, Apache-2.0 licensing, asset notices, worker dispatch approach, and M7 release-publication decision for all workspace crates in `README.md`.
  - [ ] M0.P4.L1.T9 — Document optional `SQUAD_CENSUS_PASSES`, its persisted room-variable precedence, the four new modes’ fixed judge settings, and D71’s build order in `README.md`, referencing the Contests section for engine scripts and M5 phases.

