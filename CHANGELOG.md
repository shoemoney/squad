# Changelog

All notable changes to Squad will be documented in this file, following
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.1] - 2026-11-25

Test-coverage and keyboard-reachability release: six gap branches merged on top
of 0.1.0, plus the fixes their tests exposed.

### Added

- Prompt pack: grill focus, already-asked list and shared floor plans; census,
  survey and probe shared-move compositions; composition provenance hashes and
  prompt refs; named constants for the new modes' medium judge effort and
  4,096-token visible ceiling; expanded prompt-pack guide.
- Step, pass and execution attribution on `UsageRecord` for the new modes.
- `squad doctor` reports the effective value, source and default for
  `SQUAD_BANNERS` and `SQUAD_FX`.
- TUI: retry, regenerate and per-target stop through `RoomHandle`; source
  selection for debate solutions and grill plans with previews; per-mode
  preflight breakdowns and post-pick context; pick routing, judge validation
  and contest progress cards.
- TUI: Ctrl+F transcript search and OSC 52 clipboard copy (Ctrl+Y); anchored
  PageUp/PageDown and wheel paging with a new-activity count; presentation
  expansion for tournament rankings and grill plans; incomplete and superseded
  replies rendered in the transcript.
- TUI keys with the room focused: `r` regenerates the focused reply (or the
  latest), `t` retries its failed or cancelled work item, `x` stops only its
  member. Search and copy keys added to the README key table.
- Tests: judge request capture and provenance for the new modes; router
  revalidation and contest error-code mapping; contest engine scripts, one
  active contest across seven modes, storage limits at maximum census passes,
  12-member probe and 25 survey claims; privacy injection; 20 s work-lease
  renewal; grill isolation and restart; room-level and CLI tests across modes
  (requests, actions, judge, history, state, feed, clear, export, send,
  output); PTY end-to-end with streamed round trip, terminal restore and SIGHUP
  recovery; banner snapshots at 120/80/40 columns, completion popups and render
  budget layouts.

### Fixed

- Explicit debate solution selections map reply events to their own debate.
- Judge case follow-ups use their own invocation.
- Replayed preflight receipts compare equal (`serde_json` `float_roundtrip`).
- Terminal control bytes in model text are neutralised in export.
- A started attempt cancelled before its provider call (stop or `/clear`)
  releases its spend reservation instead of leaking it.
- Contest plan reservations are sized from what each planned step can actually
  receive (frozen templates, response schema, frozen inputs and accepted
  artifacts bounded by their word limits and output ceilings) instead of assuming
  prompts fill the model's context window. Every permitted attempt is still
  reserved, each started attempt draws a frozen per-step share, and a census plan
  that grows between passes needs a revised preflight. A six-member grill no
  longer fails with `CONTEXT_TOO_LARGE` on 256k-context models.
- Debate reservations use the same bounds: prior turns are bounded by their word
  limit or four bytes per visible token (not one byte per token), selected
  solutions and the opening round count toward the judge's input, and per-round
  scores are included. The default-roster debate reserves $7.62 (was $5.58).
- Structured-output schemas are sanitised per provider (`wire_schema`), map
  schemas fall back to prompt-only JSON, survey, probe and census live-run
  failures are fixed, and OpenAI and xAI models receive `reasoning.effort`.
- One usage record is committed per attempt change, not per streamed chunk (a
  single call used to commit about 1,300), and `squad send` waits on the room feed
  instead of re-reading the whole history every 50 ms, so it exits promptly once
  a contest ends.

### Changed

- Workspace version is 0.1.1.

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
