# Wave 4 integration verification

Integrated on dev3 from main `820e3b8`, working alone on `integration-wave4`.
All seven source tips are ancestors of the integration branch. Every merge was
followed by a passing `CARGO_BUILD_JOBS=8 cargo build --workspace --all-targets`
before starting the next merge, and its progress was pushed to origin.

| Source branch | Merge commit | Conflict resolution |
| --- | --- | --- |
| m6-ops-security | ff2f7d9 | Retained main fake-provider CLI tests, missing-credential checks and separate quoted-argument checks. |
| mode-tournament | ea350eb | Kept current fake-provider CLI behavior tests in place of older duplicate expectations. |
| mode-census | 2515058 | Combined mode registration, driver dispatch, retry rules, holds, state and reservation paths; assigned migration 0003. |
| mode-crosscheck | bec423d | Combined registration, capability validation, barriers, stop, exports and UI effects; assigned migration 0004. |
| mode-survey | aa6f5f9 | Combined dispatch, authorization, transitions, holds, accounting, exports and UI; assigned migration 0005. |
| mode-grill | c29288f | Retained typed protocol, script, prompts, floor storage, exports and widgets alongside other modes; assigned migration 0006. |
| mode-probe | 1671a75 | Combined registration, scoped dispatch, authorization, transitions, stop, accounting and UI; assigned migration 0007. |

Migration 0002 remains tournament. Later event/work table rebuilds retain earlier
mode allowlists, work constraints and indexes. A new test upgrades populated
schemas from each version, preserving mode events, queued work, indexes and
foreign-key enforcement. Existing migration tests retain their data-preservation
assertions and now expect all registered modes.

The M6 feed recovery test now skips ephemeral worker-presence notifications and
verifies the acknowledged variable-change cursor before exercising overflow and
whole-commit paging. No security assertions were weakened. Ten composed-screen
snapshots were refreshed: their only differences are the six current fixture model
labels at each viewport. Transcript, verdict, accounting and layout content is unchanged.

## Final validation

- `CARGO_BUILD_JOBS=8 cargo test --workspace --no-fail-fast`: **890 passed, 0 failed, 2 ignored**, including documentation tests.
- `CARGO_BUILD_JOBS=8 cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo fmt --all --check`: passed.
- `git diff --check`: passed.
- Tests use synthetic credentials and local fake providers. No real credential file was read and no live OpenRouter call was used.

## Remaining work

Grill remains a foundation checkpoint with admission intentionally disabled.
`docs/grill-implementation-status.md:25` lists the unfinished router admission,
preflight/confirmation, participant holds and reservations, executor/result identity,
capability-bound projection readers, stop/recovery, full-room workflow tests and
TUI state/preflight actions. Its component suites compile and pass in this workspace.
No additional unfinished probe integration work was found; all 13 probe workflow
tests, protocol, export, store and UI suites pass.

The two pre-existing ignored release gates are unchanged:

- `crates/squad-cli/tests/m6_performance.rs:30`: manual first-delta latency gate.
- `crates/squad-core/tests/m6_operational.rs:251`: ordinary dispatch lacks a positive priced reservation when cost is unknown.

These ignored checks do not establish release readiness; the broader M6 deferrals
remain documented in `docs/reports/m6-ops-security.md`.
