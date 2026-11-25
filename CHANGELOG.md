# Changelog

All notable changes to Squad will be documented in this file, following
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-11-25

First public release: a single-binary terminal room for six models over
OpenRouter with seven contest modes. Binaries for macOS arm64 and Linux x64
are attached to the GitHub Release with SHA-256 checksums.

### Added

- Census estimate range: Chao (1987) log-normal plausible range around the Chao1 estimate, `Nothing found yet.` when empty, and a real `Census is quiet. Good place to stop.` condition (upper bound below 2, no new gap, full participation, no unresolved ratings).
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

- `--bus-log` / `--bus-log-bodies` now write request, commit and wake-up JSON lines (bodies only with the explicit flag); `doctor --bus-log` no longer errors.
- A refused noninteractive preflight is cancelled, so the `--yes` re-run is admitted instead of failing with `CONTEST_BUSY` / `DEBATE_BUSY`.
- Ordinary dispatch reservations are sized from the prompt actually assembled
  for the request (plus the output ceiling and reasoning budget) instead of the
  model's whole context window, so an Opus-class reply no longer exceeds the
  default `$20` cap with nothing spent. Retries re-price the attempts that
  remain, and the attempt limits come from one shared table.
- Calls that fail or are stopped before OpenRouter returns a generation id
  release their reservation immediately instead of holding it until midnight;
  only calls with an id and a failed cost lookup stay reserved as unknown.
- A cached model alias whose target has left the fresh catalog is re-resolved
  once instead of failing every call with `COST_UNKNOWN` for 24 hours.
- Alias repair now triggers on any HTTP 404 from OpenRouter, including 404s
  with a JSON error body.
- `--bus-log` refuses symlinks and non-regular files (`O_NOFOLLOW`) rather than
  truncating or changing the permissions of whatever they point to.
- The storage ceiling guard fails closed when the database size cannot be read.
- Removed raw run logs carrying local home paths from `docs/reports`.
- Startup credential exposure through non-UTF-8 environment errors.
- Doctor configuration reporting and setup script toolchain handling.
- Terminal control sequences are stripped from CLI replies and exports.

### Known issues (found by the release tests, not fixed here)

- (fixed) `--bus-log` and `--bus-log-bodies` now write for `send`, and `doctor --bus-log` no longer errors.
- (fixed) a noninteractive refusal ("Re-run with --yes") now cancels the pending preflight, so the re-run starts clean.
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
