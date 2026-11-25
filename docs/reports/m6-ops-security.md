# M6 operational and security verification

Scope: existing core room, ordinary replies, standalone judge cases and debates, based on runtime merge `f2debf9`. The six new contest modes are outside this unit. No real credential files are read; provider integration uses the M2 loopback fake. Tests run sequentially on dev3 with `CARGO_BUILD_JOBS=8`.

## Changes

Production source changes are limited to:

- `crates/squad-cli/src/output.rs`: sanitize human final replies, errors, preflight warnings and judge model labels. Escape Unicode C1 controls in JSON records and formatted JSON payloads without changing decoded data.
- `crates/squad-cli/src/export.rs`: sanitize markdown written to the terminal; saved export artifacts retain original content.

No shared enum, match-arm registration, module declaration or Cargo manifest edits.

New tests are `m6_*.rs` under CLI, core, store and TUI, with `crates/squad-core/tests/support/m6_room.rs` constructing real `RoomHandle` instances. CLI `tests/commands.rs` replaces obsolete runtime-stub expectations with offline credential and argument checks; runtime behavior is covered against the loopback provider instead.

## Evidence

| Area | New checks and existing owning checks |
|---|---|
| Terminal safety | CLI human replies, errors, JSON C0/C1 escaping and displayed exports; TUI every UTF-8 prefix of OSC 52, CSI, DCS and C1 variants in prose/fences/quotes/links; decoded entities; literal paste; model replies cannot synthesize input |
| Credentials and reasoning | Synthetic key absent from configuration/client Debug, real room feed, state, exports and database files; private provider reasoning absent from feed/store/export; file JSON logger redaction and retry/case identity fields; offline doctor; no real completion endpoint |
| Failure persistence | SQLite busy, real SQLITE_FULL via max_page_count, corrupt-file preservation; failed commits create no receipts or accounting mutations; concurrent reservations serialize at the exact capacity boundary |
| Runtime recovery | Commit-only ordinary/case/debate admission, interactive restart, receipt replay, frozen paid-call counts and daily totals; no retroactive round scoring for manual judging |
| Costs | 1/10/20-round preflight counts; cancellation without calls; actual round requests use low effort, 1,024 reasoning tokens and 512 visible tokens; daily max exact refusal, raised max, unknown-cost restart, `/clear` preserves spending |
| Failures and stop | 429, 503 and partial stream retry; independent member progress after another exhausts attempts; targeted cancellation excludes late final output and preserves another member's completion |
| Delivery | Real room queue boundary, feed overflow and paging from last cursor, exact room-lock message with concurrent read-only export; existing capacity-one mailbox/ingress tests |
| TUI controls | Off/NO_COLOR idle timer disarming; full-effects governor shedding; existing event-loop input priority/256-item batches/60Hz cap and budget rendering tests |
| Setup | Exact Foundations `.env.example`; mocked `scripts/test-setup.sh` checks creation/preservation/permissions/no secret echo/build/doctor/platform branches |

Full workspace gates and the remaining milestone checklist are recorded in the completion report. Short performance evidence and its limits are in `docs/performance.md`.

## Release follow-up (2026-11-25)

The three-run optimized dev3 follow-up is recorded in
[performance.md](../performance.md): latency/replay/RSS and all nine full TUI
viewport windows pass their harness thresholds. The missing-cost ordinary
reservation check failed 3/3 in that rerun; it is fixed by commit `2b040e9` and
the check is no longer ignored (see performance.md). The follow-up workspace
run stalled in `m6_operational` and was terminated after 4m31s (overall exit 101);
a bounded serial retry also stalled. All other targets, Clippy and formatting
passed. This does not supersede that unresolved operational validation failure.
The wiki is now available
on dev3; the earlier unavailable-wiki note below describes the original unit.

## Release blockers and unchecked work

- Missing-cost ordinary reservation: fixed in commit `2b040e9`. Ordinary dispatch now sizes its reservation from the assembled prompt, and `m6_operational::ordinary_missing_cost_must_retain_a_positive_priced_reservation` is no longer `#[ignore]`d. Coverage for the reservation fix is in `crates/squad-core/tests/dispatch_reservations.rs`, which also has a test that unavailable prices refuse dispatch with `COST_UNKNOWN`.
- `--bus-log` is still rejected as unimplemented. F12 uses a local debug projection rather than a shared request/commit/wake diagnostic source. Logger tests verify its sink/redaction/identity behavior, not end-to-end provider span propagation or the absent bus-log writer.
- No 1-GiB storage-ceiling enforcement path was found. Transactional clear/VACUUM and history preservation have owning tests; the storage-capacity milestone remains unchecked.
- Performance misses and unmeasured 30-minute/full-TUI gates remain explicit in `docs/performance.md`.
- Work-lease expiry has owning tests, but a new integrated timed 20-second renewal test is still needed. Actual killed-process commit/barrier crash coverage and PTY close-during-stream coverage remain outstanding; transaction rollback and restart fixtures do not claim to prove all process-crash boundaries.
- Machine-wide multi-room accounting is not verified: the current product admits only room `main`. Local-day transfer tests use the accounting API's explicit date rather than changing the host clock.
- `/data/wiki` is absent on dev3, so the required wiki read/update/lint/commit workflow could not run. No host paths or credentials were guessed.

### Deferred six-mode todo IDs

Leave these unchecked until the corresponding modes exist:

- `M6.P2.L1.T14`, `M6.P2.L1.T15`
- `M6.P2.L2.T14`, `M6.P2.L2.T16`, `M6.P2.L2.T17`, `M6.P2.L2.T18`, `M6.P2.L2.T19`
- `M6.P3.L1.T11`, `M6.P3.L1.T13`, `M6.P3.L1.T14`, `M6.P3.L1.T15`, `M6.P3.L1.T16`
- `M6.P3.L2.T8`

All other unchecked P2/P3 IDs retain their owning acceptance scope; partial checks in this unit do not turn those gates green.

## Completed todo IDs

These combine the new M6 checks with the existing owning suites. The final workspace checks passed:

- `M6.P2.L1.T6`, `M6.P2.L1.T9`, `M6.P2.L1.T10`, `M6.P2.L1.T12`
- `M6.P2.L2.T1`, `M6.P2.L2.T3`, `M6.P2.L2.T5`, `M6.P2.L2.T7`, `M6.P2.L2.T8`, `M6.P2.L2.T9`, `M6.P2.L2.T13`, `M6.P2.L2.T15`
- `M6.P3.L1.T2`, `M6.P3.L1.T4`, `M6.P3.L1.T5`, `M6.P3.L1.T6`, `M6.P3.L1.T8`, `M6.P3.L1.T9`, `M6.P3.L1.T10`, `M6.P3.L1.T12`
- `M6.P3.L2.T1`, `M6.P3.L2.T2`, `M6.P3.L2.T3`, `M6.P3.L2.T4`, `M6.P3.L2.T5`, `M6.P3.L2.T6`, `M6.P3.L2.T7`

Existing-scope todo IDs still unchecked:

- `M6.P2.L1.T1`–`T5`, `M6.P2.L1.T7`, `M6.P2.L1.T11`, `M6.P2.L1.T13`
- `M6.P2.L2.T2`, `M6.P2.L2.T4`, `M6.P2.L2.T6`, `M6.P2.L2.T10`–`T12`
- `M6.P3.L1.T1`, `M6.P3.L1.T3`, `M6.P3.L1.T7`

The remaining IDs are the six-mode deferrals listed above. In particular, no whole phase is declared complete.

## Final validation

- `cargo build --workspace`: passed.
- `cargo test --workspace --no-fail-fast`: passed, 734 tests passed, zero failed, five ignored. Three ignored CLI integration tests predate this unit; the other two are the explicitly documented manual performance and reservation release checks.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo fmt --check`: passed.
- `bash scripts/test-setup.sh`: passed, also run by `m6_setup`.
- Explicit manual performance check: failed its unchanged latency threshold after recording both rosters.
- Explicit ordinary missing-cost reservation check, before the fix in `2b040e9`: failed because `reserved_usd` was not positive.
- Confirmed the milestone diff changes only checkbox characters.

### File manifest

Production files:

- `crates/squad-cli/src/export.rs`
- `crates/squad-cli/src/output.rs`

Test files:

- `crates/squad-cli/tests/commands.rs`
- `crates/squad-cli/tests/m6_credential_safety.rs`
- `crates/squad-cli/tests/m6_observability.rs`
- `crates/squad-cli/tests/m6_output_safety.rs`
- `crates/squad-cli/tests/m6_performance.rs`
- `crates/squad-cli/tests/m6_setup.rs`
- `crates/squad-core/tests/m6_debate_recovery.rs`
- `crates/squad-core/tests/m6_limits.rs`
- `crates/squad-core/tests/m6_operational.rs`
- `crates/squad-core/tests/support/m6_room.rs`
- `crates/squad-store/tests/m6_failures.rs`
- `crates/squad-tui/tests/m6_effects.rs`
- `crates/squad-tui/tests/m6_terminal_safety.rs`

Documentation:

- `docs/prd/milestones/M6-hardening.md`
- `docs/performance.md`
- `docs/reports/m6-ops-security.md`
