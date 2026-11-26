# v0.1.3 integration

Release branch: `release-v0.1.3`, based on main `9093b19`.

## Merge order and conflicts

1. `audit-2-m3`: `ce98510`, no conflicts.
2. `audit-5-m5a`: `9f451af`, no conflicts.
3. `audit-6-m5b`: `11c5fa9`, conflict only in the M5 checklist. Retained both audits' ticks and evidence notes.
4. `audit-3-m4a`: `778d6b1`, no conflicts.
5. `audit-4-m4b`: `df69ccd`, conflicts in `src/ui.rs`, `src/widgets/preflight.rs` under `squad-tui`, and the M4 checklist. Kept the full-height source-preview panel and pinned confirm/cancel controls, survey boolean counts, nested grill setup/plan handling and sanitisation, source IDs and debug diagnostics. Retained both branches' tests and checklist evidence. Expanded round reasons merged automatically.

The merged TUI suite passed 363 tests, zero failures, one ignored subprocess entry point. Each merge passed `CARGO_BUILD_JOBS=4 cargo build --workspace` before its push; the second M4 merge also passed the full TUI suite before pushing.

6. `audit-1-m0-m2-m6-m7`: `1b58ac1`, no conflicts. Fetched final head `42b474f` after 913 seconds without commits; closing report includes soak, GIF and licence evidence. Merged last.

7. `fix-empty-content`: `3ec7045`. Conflict only at the end of `squad-agents/tests/agents_integration.rs`; retained the audit tracing module and both new empty-answer accounting/retry tests.
8. `demo-gif`: `704fa25`, no conflicts. Adds the README hero GIF, raw recording, tape and documentation.
9. `origin/main` at `75013a1`: manual macOS performance workflow, no conflicts.

These additional merges also built with four Cargo workers before each push. The earlier release CI run was cancelled so final CI covers the combined release.

## Independent checklist review

- M4.P2.L8.T2 remains open. `m4b_session_audit.rs` proves crosscheck audit/acceptance restore and probe roles, but does not test census pass/plan-reference restore.
- M4.P3.L8.T6 remains open: D71 ordering covers preflight/cancel, without integrating completed results.
- M4.P4.L2.T2 remains open: round 0, queued-debater display and manual judging are not demonstrated in the TUI.
- M4.P1.L6.T2 is consistently open for the unsupported standalone bare `$CENSUS_PASSES` lookup; partial implementation is identified without claiming full coverage.
- M4.P4.L8.T5 is now checked: merged report search/copy/quote preparation preserves the draft and leaves `app.pending` empty. Shared report inspection is local; delivery requires normal submission.
- M4.P4.L8.T2 remains open for large-report navigation/visible-row coverage across report types. The new census search/copy/anchored-resize evidence is cited.
- M4.P4.L8.T7 remains open for a combined seven-mode completed-result gate. Report navigation is implemented, and is no longer cited as missing.
- M5.P3.L6.T1 retains its implementation tick and explicitly notes the PRD inconsistency of 150 versus 200 grill-question words. Behavior is unchanged.

## Validation

All integration provider tests use fixture credentials and local fake/loopback providers. No live model API calls were made during integration. The sixth audit's pre-existing recording and soak evidence is described separately in its own report.

## Checklist counts

Checked means implemented or verified against cited evidence; it does not close the separately noted grill-budget inconsistency.

| Milestone | Total | Checked | Newly checked since main | Open |
|---|---:|---:|---:|---:|
| M0 | 51 | 50 | 10 | 1 |
| M1 | 167 | 167 | 1 | 0 |
| M2 | 136 | 136 | 7 | 0 |
| M3 | 95 | 94 | 22 | 1 |
| M4 | 236 | 223 | 51 | 13 |
| M5 | 361 | 361 | 89 | 0 |
| M6 | 150 | 150 | 10 | 0 |
| M7 | 35 | 35 | 1 | 0 |
| Total | 1231 | 1216 | 191 | 15 |

Initial six-audit version 0.1.3 local gates on dev3 (before the additional transport, hero and main merges):

- `CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=1 cargo test --workspace --no-fail-fast`: 1,286 unique tests passed, zero failures, four ignored entries. Raw Cargo summaries show 1,287 passes because the WAL regression prints its child-process result a second time. The ignored entries are the manual latency, thirty-minute streaming and transcript-memory gates, plus the PTY subprocess entry point exercised by its parent.
- `CARGO_BUILD_JOBS=4 cargo clippy --workspace --all-targets --locked -- -D warnings`: passed.
- `cargo fmt --all -- --check`: passed.

The sixth audit's prior soak and transcript measurements are retained in `docs/reports/audit-foundations-2026-11-26/`; they were not rerun during integration.

Combined release gates after `fix-empty-content`, `demo-gif` and main `75013a1`:

- Full workspace tests: 1,292 unique tests passed, zero failures, four ignored entries (1,293 raw passes including the duplicated WAL subprocess). The newly merged empty-answer accounting, retry, captured reasoning-only truncation and typed content-part tests all pass alongside the audit tracing tests.
- All-target locked workspace Clippy with `-D warnings`: passed.
- `cargo fmt --all -- --check` and diff whitespace check: passed.
