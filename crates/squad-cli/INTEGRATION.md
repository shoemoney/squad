# CLI integration handoff

## Runtime binding (wave 1)

`adapter::Runtime` now opens `squad_core::room::RoomHandle` in Interactive or
OneShot mode. Plain `squad` launches `squad_tui::run(config, room)`; `send` uses
the real RoomApi implementation. `--no-wait` calls `submit_commit_only` and never
wakes work. The concrete state serializes RoomState directly, while durable
history supplies parser references, causal relevance, and addressed execution
progress. Offline export delegates to `squad_core::room::export_offline`; models
and doctor retain their credential-free, read-only inspection paths.

`tests/real_room.rs` runs both injected-runtime boundary tests and the three
production-launcher tests: `send_history_live_and_offline_export`,
`no_wait_does_not_dispatch_and_interactive_restart_resumes`, and
`real_room_feed_reduces_into_tui_session`. The real runtime is bound; these tests
are no longer ignored. All provider calls use loopback fixtures. Executable
command tests set `OPENROUTER_BASE_URL` to the shared wiremock fake's API URL;
the default remains `https://openrouter.ai/api/v1/`.
The TUI adapter also has a standalone snapshot/reducer test.

The earlier handoff below describes the original implementation seams and
milestone deferrals; the runtime binding above supersedes its stub instructions.


`RoomApi` in `src/room_api.rs` is the temporary RoomHandle adapter requested for
this worker. `lifecycle::Runtime` owns open and TUI launch. Its `open` must honor
`OpenPolicy { mode: OneShot, commit_only: true }` by suppressing dispatch before
submission. `send::run` owns shutdown. Never start and cancel no-wait work.

Bind these in `main.rs` where the explicit integration stub remains:

- Interactive: `lifecycle::interactive(runtime, config)`; implement `Runtime::tui`
  using `squad_tui::run(room).await` once that function exists.
- Send: `lifecycle::send(runtime, config, args, output, confirmation_callback)`.
  A noninteractive callback returns `None`; an interactive callback prompts on
  stderr and returns the user's answer. `--yes` only confirms this submission.
- `RoomApi::State` must serialize the actual RoomState. `parse_context` must supply
  the complete authoritative parser snapshot, including eligible references,
  pending picks, active contests, and effective persisted variables.
- `progress` must cover the submitted request's addressed executions and requested
  judging, and retain partial failures until every addressed execution is terminal.
  Unscored round judging alone must not make this projection fail.
- `relevant` must include causally related contest, accounting, supersession and
  error events, including those whose correlation differs from the request ID.
  Do not infer membership from correlation alone in the production adapter.

The CLI subscribes before reading its initial watermark. It pages durable history
in ascending cursor order, recovers broadcast lag through history, ignores deltas,
prints preflights before acknowledgements, and does not infer completion from
silence. `run_until` provides an injectable interrupt future for cleanup tests.

`models` and standalone `export` are wired into the executable and bypass room
opening. `export` also bypasses credential/config validation, using only the
resolved data path. `models` permits a missing credential but rejects other
configuration errors.

The current core renderer accepts room/debate/tournament/grill, and explicitly
rejects census/crosscheck/survey/probe. All eight CLI scopes are parsed and
delegated. The offline adapter uses squad-store's transactional public-event
reader on a READ_ONLY SQLite connection, preserving a concurrent room lock.
Typed public tournament/grill artifact projections are still needed from core/store;
the CLI deliberately does not decode unrestricted artifact JSON into those types.
Add that adapter before claiming identical complete live/offline mode exports.

Doctor now reports daily accounting, reservations, effective max, unknown-call
counts, cached roster and judge identities, and recorded response-model mismatches
without opening the room. Provider reachability/catalog validation and full
`--env` effective-value/source reporting remain deferred. The current protocol
configuration has no census_passes field; its seed/precedence requires the config
worker's extension. No bus diagnostics or PTY TUI integration is claimed here.

Tests never read a developer environment or contact OpenRouter. They cover output
snapshots, shared payload preservation, parser errors, confirmation/cancellation,
request-scoped replay, partial failure, no-wait receipts, interruption cleanup,
locked offline export, deterministic banners, read-only cache/accounting, and
sanitized storage failures. Actual no-dispatch/restart execution and concrete
RoomHandle lock/recovery behavior must be checked after binding the adapter.

## Milestone accounting

Ready as CLI implementations with fake-RoomApi coverage:

- M3.P3.L1.T1 and T5: complete argument/global-flag set and all export choices.
- M3.P3.L2.T1, T2, T8 and T10: shared parser/request mapping, sequence quotes,
  confirmation, request-scoped waiting, labeled/JSON output, receipt shapes and
  typed preflight estimates. Production progress/relevance requires the binding.
- M3.P3.L3.T7: offline deterministic OPENING/ROUND/FINAL ROUND/VERDICT snapshots.
- CLI delegation portions of M3.P3.L3.T1, T3, T4, T10 and T14 are implemented.
  Full artifact exports and failed-resolution metadata still require owner APIs.
- M3.P3.L5.T1: human and JSON output snapshots are included.

Do not mark these broader integration items complete yet:

- M3.P3.L1.T2–T4: real RoomHandle startup/lock/recovery and executable TUI/send
  binding remain for the coordinator. The lifecycle seam and fake checks exist.
- M3.P3.L2.T3–T6, T9, T11–T12: component tests cover portions, but actual debate
  completion, every contextual slash outcome, all typed mode artifacts, no-wait
  provider non-dispatch and execution on restart require integrated runtime tests.
- M3.P3.L3.T2, T5, T9, T11–T19: full live/offline exports require the owner’s typed
  artifact snapshot source and the new-mode core renderer. T6/T8 are pre-existing
  core work outside this territory.
- M3.P3.L4: accounting and mismatch/cache reporting are added, but this worker does
  not claim complete doctor reachability/catalog/env or bus diagnostic milestones.
- M3.P3.L5.T2–T8: PTY, full state/mode snapshots, top-level documentation and review
  remain outside this worker’s completed work.

Validation: workspace build, 504 workspace tests (56 CLI tests), Clippy for all
workspace targets with warnings denied, and workspace format checking pass. Cargo
regenerates seven dependency-list entries in the root Cargo.lock; the coordinator
must regenerate and commit it after merging because it is outside this territory.
Milestone checkbox edits are likewise left to the coordinator.

## Bound CLI regression validation (2026-11-25)

On dev3, the bound-runtime regression update passed `cargo build --workspace`,
`cargo test --workspace --no-fail-fast` (114 test targets, 704 passed, 0 failed,
0 ignored, 0 filtered out), `cargo clippy --workspace --all-targets -- -D warnings`,
and `cargo fmt --check`, with `CARGO_BUILD_JOBS=8` and sequential gates.
The 21 focused executable/room tests include all three formerly ignored runtime
checks. No milestone checkbox or shared registration files changed.
