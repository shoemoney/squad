# Changelog

All notable changes to Squad will be documented in this file, following
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

No release has been published. The workspace version is 0.1.0. This record is
sourced from git history on `m7-release-tests`; publication, tags, signing and
macOS arm64 installation evidence remain pending the owner's decision.

### Added

- Six-crate Rust workspace (protocol, store, agents, core, TUI, CLI), licensed
  Apache-2.0, with a single `squad` binary.
- CLI surface: `squad` (TUI), `send` (text and `--json` records, `--yes`
  pre-confirmation, `--no-wait` commit-only acceptance with an accepted cursor,
  `--quote`, `--verbose`), read-only `export` for `room` and every contest
  scope, `models`, and `doctor` (`--env`, `--online`). Exit codes: 0 success,
  1 failure or refused confirmation, 2 usage error, 130 interrupt.
- Persistent room with crash recovery, same-room locking, restart persistence,
  `/clear`, regeneration and offline markdown export that works while the room
  is locked.
- All seven contest modes run over the shared engine, built in D71 order:
  debate, grill, census, crosscheck, tournament, survey and probe, each with
  typed contracts, blind steps where specified, preflight estimates, durable
  recovery, stop handling and a committed report:
  - `$census` with persistent `CENSUS_PASSES` (seed `SQUAD_CENSUS_PASSES`,
    default 2, range 1 to 5), discovery curve and singleton hints;
  - `$crosscheck` with plan-blind inventory and human risk acceptance;
  - `$survey` with blind claims, votes, revotes and five report bands;
  - `$probe` with committed predictions, a decision tree and unresolved
    explanations;
  - blind `$tournament` judging and `$grill` with a shared floor and plan
    versions; question-first picks for debate and grill.
- Daily spending ledger with a priced reservation taken before every dispatch,
  `SQUAD_DAILY_MAX_USD`, unknown-cost accounting that is never reported as zero,
  and `DAILY_MAX_REACHED` rejections that report spent, cap and the required
  reservation.
- Durable model-alias probe cache so paid `~...-latest` alias probes are not
  repeated across starts; offline commands never probe.
- OpenRouter streaming SSE transport, retry and cancellation handling, valid
  reasoning budgets, a fake provider for offline tests, and sanitized provider
  status and message detail on executor failures.
- TUI with streamed Markdown and highlighted code, contest banners built from
  embedded FIGlet fonts, effects and motion with a frame-budget governor, and
  terminal colour fallbacks.
- `setup.sh` (pinned toolchain, reruns, private `.env`), CI gates, a prompt pack
  of 46 embedded templates, cargo-dist configuration for macOS arm64 and Linux
  x64 archives with SHA-256 checksums, and a Homebrew formula template.
- Release test suite `crates/squad-cli/tests/release.rs`, which drives the
  optimized `squad` binary against a loopback OpenRouter fixture.

### Changed

- SQLite upgraded to 3.51.3 through `rusqlite` 0.39 to prevent WAL lifecycle
  deadlocks.
- Surveys report a `DEGRADED` status for degraded cycles, with clearer revote
  validation messages.
- Usage lines are coalesced per call in CLI output; `--verbose` prints raw
  accounting updates.

### Fixed

- Startup credential exposure through non-UTF-8 environment errors.
- Doctor configuration reporting and setup script toolchain handling.
- Terminal control sequences are stripped from CLI replies and exports.

### Known issues (found by the release tests, not fixed here)

- `--bus-log` and `--bus-log-bodies` are accepted but write nothing for `send`;
  `doctor --bus-log` still reports "not implemented yet".
- A noninteractive refusal ("Re-run with --yes") leaves the pending preflight
  stored, so the re-run is rejected with `CONTEST_BUSY` or `DEBATE_BUSY` until
  the plan expires.
- (fixed) the prompt pack now lives in `crates/squad-core/prompts/`, inside the
  `squad-core` package, so a registry package of `squad-core` carries every
  embedded prompt.

### Planned after v1

- `$lattice`, `$delphi`, `$assemble`, and a spending-controlled meta-mode that
  loops `$grill` and `$census` until quiet (the human-directed `$grill`,
  `$census`, patch, `$crosscheck`, `$census` pipeline is the v1 route).
- Additional architecture targets, signing and notarisation, multi-room
  operation, remote attachment, tools, and image and file uploads, as tracked in
  the launch PRD.
