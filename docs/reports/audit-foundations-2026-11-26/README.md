# Foundations, agents, hardening and launch audit

Scope: the 30 boxes initially unchecked in M0 (11), M1 (1), M2 (7), M6 (10) and M7 (1), on `audit-1-m0-m2-m6-m7`. Only these checklist items changed.

## Checklist disposition

The final disposition is 13 boxes verified from existing evidence, 16 completed with implementation/tests/documentation, and one owner-dependent item left open.

| Milestone | Existing evidence | Implemented or completed | Open |
|---|---:|---:|---:|
| M0 | 3 | 7 | 1 |
| M1 | 1 | 0 | 0 |
| M2 | 3 | 4 | 0 |
| M6 | 6 | 4 | 0 |
| M7 | 0 | 1 | 0 |

Existing-evidence items: M0.P1.L1.T4, M0.P2.L1.T13, M0.P4.L1.T7; M1.P3.L7.T11; M2.P2.L3.T3, M2.P4.L2.T2, M2.P4.L2.T4; M6.P1.L7.T2–T5, M6.P2.L1.T3 and M6.P3.L1.T7. Each checkbox names the owning files/tests.

**Open: M0.P4.L1.T5.** No recorded owner M0 check-in was found. The technical audit and this report do not supply owner approval.

## Changes

The registry correctly prepared fixed-medium structured judges, but the shared streaming helper overwrote census, crosscheck and probe effort with the configured judge effort. Dispatch now preserves medium effort and a 4,096-token visible ceiling for all eight new-mode judge work kinds. Real provider-request capture verifies this with a high configured binding.

The existing accounting attribution helper was uncalled. Contest streams now invoke it with authorized contest, step, execution, work and attempt identities, plus the frozen census pass. Router grant/commit diagnostics carry contest and step identities. Diagnostics exclude conversational bodies and reasoning.

Strict census configuration tests cover defaults, file/process precedence, empty overrides, both boundaries, overflow, fractions, trailing text and exact diagnostics. The canonical `.env.example` remains unchanged. CI isolates application state and schedules full fixture-credential setup plus shellcheck on macOS and Ubuntu. Local shellcheck and the closed-PATH installer checks passed; this Linux audit does not claim a macOS execution.

Default-roster v0.1.2 survey/probe GIFs are linked in README and `docs/demo.md`. Successful runs recorded $1.983427 actual spend. Labelled export continuations show the committed reports and caveats; the stale survey live card is explicitly disclosed. Credential inspection is in [credential-inspection.json](credential-inspection.json).

## New tests

| File | Coverage |
|---|---|
| `crates/squad-protocol/tests/foundations_audit.rs` | Strict census seed syntax, exact diagnostics and precedence |
| `crates/squad-cli/tests/support/canonical_pty_audit.rs` | Full judged question-first flow, commands, recall, search, quote selection, regeneration, stop, flushed export, resize, hangup recovery and terminal restoration |
| `crates/squad-cli/tests/foundations_audit.rs` | Census defaults, file/process overrides and empty override through the binary |
| `crates/squad-agents/tests/support/attempt_tracing_audit.rs` | Eleven actual provider calls; all eight structured judge kinds retain medium/4,096; attribution and secret/body exclusion |
| `crates/squad-core/tests/agent_retry_audit.rs` | Explicit retry and receipt replay execute new work without repeating the human message |
| `crates/squad-core/tests/font_licences_audit.rs` | Both vendored font assets, full licence and notices |
| `crates/squad-core/src/router/observability_tests.rs` | Step/attempt/barrier identities without body/reasoning disclosure |
| `crates/squad-tui/tests/memory_audit.rs` | Isolated 500-message transcript process-RSS measurement |
| `crates/squad-cli/tests/streaming_memory_audit.rs` | Thirty-minute six/twelve-agent streaming high-water RSS soak |

The existing PTY round-trip also now verifies debug JSON reaches the flushed file and stays out of terminal output.

## Production source outside tests

- `crates/squad-agents/src/agents/mod.rs`: dispatch effort/ceiling and accounting-attribution wiring.
- `crates/squad-agents/src/accounting.rs`: body-free attribution diagnostics.
- `crates/squad-core/src/banner/mod.rs`: embeds the full FIGlet licence as `FONT_LICENSE`, available without a filesystem read.
- `crates/squad-core/src/router/mod.rs`: commit/grant contest and step diagnostics, plus the unit-test module declaration.

`crates/squad-core/src/router/observability_tests.rs` is test code under `src`. Other changes are CI, README, scoped milestone evidence, recording tapes/helper, GIFs and audit documentation. The only additive public API is `squad_core::banner::FONT_LICENSE`, needed to expose the embedded asset licence. Protocol schemas are unchanged.

## Validation

- `CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=1 cargo test --workspace --no-fail-fast`: PASS, 1,248 passed, zero failed, three manual tests ignored.
- `cargo fmt --check`: PASS.
- `shellcheck setup.sh scripts/*.sh`: PASS.
- `./scripts/test-setup.sh`: PASS.
- `CARGO_BUILD_JOBS=4 cargo clippy --workspace --all-targets -- -D warnings`: PASS.
- Manual streaming soak: PASS, 1,805.88 seconds, 156 waves / 2,808 answers, 155,588 KiB combined kernel high-water RSS (512-MiB guard).
- Manual 500-message transcript fixture: PASS, 109,084 KiB process RSS (256-MiB guard).
- Both memory measurements are qualified debug test-process results on shared dev2; see `docs/performance.md` for workload and reference-host limitations.

The first manual soak timed out with the character-at-a-time SSE fixture during concurrent compilation; [its failure is retained](streaming-initial-failure.log). A second run exposed fixture retention: disabling Wiremock's global request recording still left cloned requests in each mounted mock. It was stopped after the issue was found; [its samples are retained](streaming-unbounded-fixture-aborted.log). The final harness retains all application history, sends the same 3,712-byte answer in eight SSE deltas, and resets/remounts the provider mock between completed waves. Neither earlier run is claimed as a product pass or a diagnosed application memory defect.
