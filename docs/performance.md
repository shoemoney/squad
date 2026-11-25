# M6 performance measurements

## Release rerun on dev3 — 2026-11-25

All summary cells below are **min / median / max across three runs**; each latency
run calculates its own p95 from 100 samples after five warm-ups. Each TUI run
uses the default **300 seconds per viewport**, sequentially at all three sizes,
with assertions enabled. No threshold was relaxed. No live OpenRouter calls or
real credential files were used.

Host: dev3, Linux x86_64, kernel 7.0.0-43-generic, AMD Ryzen 9 8945HX,
16 cores / 32 logical CPUs; Rust/Cargo 1.99.1, optimized release/bench profiles,
`CARGO_BUILD_JOBS=8`. This remains a shared development host without CPU affinity,
not the specified isolated four-core/8-GB machine. Backend: headless Ratatui
TestBackend, full effects, truecolor; no physical terminal or macOS claim.

The first command was `uptime`: **0.41 / 4.19 / 4.74** (1/5/15-minute load).
One-minute load was below 2; longer averages retained earlier activity.
Compilation finished before measurements. The latency runs started at
1.49/3.72/4.49, 1.24/3.30/4.30 and 0.84/2.90/4.12; their ending loads were
1.24/3.30/4.30, 0.84/2.90/4.12 and 3.16/3.21/4.16. The third run therefore
has a transient-load qualification. A separate workspace build subsequently
raised load above 10; the runner waited for one-minute load below 2 before
starting reservation and TUI runs. Raw logs retain every attempt and exit status;
`load.tsv` samples the remaining interval every 30 seconds. Each TUI log records
its own viewport start/end load. These numbers replace the earlier debug/loaded
observations for this release workload, without discarding that evidence.

```sh
CARGO_BUILD_JOBS=8 cargo test --release -p squad-cli --test m6_performance -- --ignored --nocapture
CARGO_BUILD_JOBS=8 cargo bench -p squad-tui --bench composed_streams
CARGO_BUILD_JOBS=8 cargo test --release -p squad-core --test m6_operational ordinary_missing_cost_must_retain_a_positive_priced_reservation -- --ignored --exact --nocapture
```

### Latency, replay and sampled memory

| Workers | Acceptance p95 ms | First delta p95 ms (≤500) | Replay ms (≤500) | Sampled RSS KiB (≤524,288) | Result |
|---|---:|---:|---:|---:|---|
| 6 | 94.080 / 94.813 / 105.650 | 203.921 / 204.788 / 216.170 | 1.158 / 1.344 / 1.579 | 59112 / 59364 / 59392 | PASS 3/3 |
| 12 | 184.672 / 189.183 / 192.834 | 301.917 / 304.778 / 307.108 | 1.474 / 1.540 / 1.596 | 93192 / 93332 / 93808 | PASS 3/3 |

### Real-widget twelve-stream full-effects gate

| Viewport | Draw p95 ms (≤8) | Draw p99 ms (≤16.7) | Key p95 ms (≤50) | CPU % of one core (≤25) | Result |
|---|---:|---:|---:|---:|---|
| 140x42 | 3.656 / 3.798 / 3.947 | 4.540 / 4.641 / 4.734 | 17.310 / 17.598 / 17.735 | 9.443 / 10.337 / 10.757 | PASS 3/3 |
| 80x24 | 3.408 / 3.420 / 3.671 | 4.254 / 4.318 / 4.428 | 17.337 / 17.425 / 17.486 | 8.867 / 9.050 / 9.890 | PASS 3/3 |
| 200x60 | 4.043 / 4.124 / 4.132 | 4.768 / 4.928 / 4.992 | 17.635 / 17.822 / 18.000 | 10.983 / 11.750 / 11.877 | PASS 3/3 |

| Viewport | Reducer + draw p95 ms | Reducer + draw p99 ms | Startup ms | RSS KiB | VmHWM KiB |
|---|---:|---:|---:|---:|---:|
| 140x42 | 3.718 / 4.180 / 4.395 | 4.983 / 5.086 / 5.188 | 1027.119 / 1064.847 / 1151.615 | 254892 / 255212 / 255236 | 254892 / 255212 / 255236 |
| 80x24 | 3.484 / 3.485 / 4.125 | 4.713 / 4.782 / 4.891 | 1135.428 / 1161.956 / 1178.778 | 249644 / 249948 / 249984 | 253204 / 253892 / 254496 |
| 200x60 | 4.100 / 4.237 / 4.253 | 5.229 / 5.409 / 5.458 | 980.741 / 1118.578 / 1160.252 | 260764 / 261104 / 261148 | 260764 / 261104 / 261148 |

Startup and memory are observations, not additional benchmark assertions. VmHWM is the kernel-reported counter. Its reported value decreased between the first two viewports, so the raw values are retained without claiming an independently validated, monotonic process peak.

| Run | Viewport | Start load (1/5/15 min) | End load (1/5/15 min) |
|---|---|---|---|
| 1 | 140x42 | 0.64/3.00/4.00 | 0.23/1.24/2.94 |
| 1 | 80x24 | 0.23/1.24/2.94 | 0.12/0.61/2.20 |
| 1 | 200x60 | 0.12/0.61/2.20 | 0.13/0.32/1.63 |
| 2 | 140x42 | 0.13/0.32/1.63 | 0.27/0.23/1.21 |
| 2 | 80x24 | 0.33/0.24/1.21 | 0.12/0.17/0.90 |
| 2 | 200x60 | 0.12/0.17/0.90 | 0.05/0.12/0.68 |
| 3 | 140x42 | 0.05/0.12/0.68 | 0.20/0.18/0.54 |
| 3 | 80x24 | 0.20/0.18/0.54 | 0.16/0.19/0.44 |
| 3 | 200x60 | 0.22/0.20/0.45 | 0.18/0.20/0.37 |

### Missing-cost reservation: FAIL 3/3, still a release blocker

All three explicit release executions failed the unchanged assertion
`state.daily_spend.reserved_usd > 0`, at load 1.46/3.72/4.29. Ordinary dispatch
has no reservation row to retain: `router::executor::start` calls
`accounting::start`, whose `start_inner` splits an existing plan reserve,
transfers existing unstarted work reserves, and admits **zero new micros**.
It records an unresolved provider attempt and increments `unknown_calls`, but
never prices or reserves an ordinary request. A missing-cost completion thus
leaves no positive reserved amount. The fake catalog does provide positive
prices, so this is not an absent-price fixture artifact.

The ignore is intentional: enabling the test now makes normal workspace tests
fail. It can be enabled after ordinary/standalone dispatch atomically reserves
priced input, visible output and supported reasoning for permitted attempts,
refuses unavailable pricing with `COST_UNKNOWN`, and correctly releases unused
retry reserves while retaining uncertain started calls. Provider start currently
precedes request preparation in the worker. This requires coordinated pricing,
dispatch and retry-lifecycle work, rather than a small safe assertion or ledger
patch. No arbitrary positive reserve or pricing fallback was introduced.

### Harness repair and retained failures

The original TUI harness failed all three initial release attempts before any
steady-state measurement: `(2, height - 5)` now contains the editor title's `I`,
not typed input. It also resized 80 columns to 79, which invokes the production
minimum-size screen. The contained repair derives the editor position from the
current layout, verifies **every** typed character, and alternates 80/81 columns
for compact resize stress (other viewports still use width/width−1). The
production renderer and performance thresholds are unchanged. A six-second smoke
at every viewport passed, including resize; this is harness validation, not a
replacement for the nine full windows.

The latency test stays manual; its ignore text now describes the release
invocation instead of claiming a current target miss.

Raw evidence: [all attempts, load samples and structured summary](reports/perf-gates-2026-11-25/). Trailing whitespace and empty final lines in two validation logs were normalized; measured values are unchanged.

### Final workspace validation: blocked by operational-test stall

`CARGO_BUILD_JOBS=8 cargo test --workspace --no-fail-fast` returned **101**.
The `squad-core --test m6_operational` process stopped making progress with five
tests still running. After **4 minutes 31 seconds**, it was explicitly terminated
with SIGTERM so `--no-fail-fast` could finish all other targets. Completed test
harnesses reported **883 passed, zero failed, one ignored**; the aborted target
has no final test count and is not a pass. Five test/router threads waited on the
same futex address, mapped into the bundled SQLite static mutex array; this
identifies a synchronization boundary, not a proven lock cycle or root cause.

A direct retry of the same debug test executable with `--test-threads=1` timed
out after 90 seconds (exit 124), stalled at the targeted-stop test. It also
reported the credential test as FAILED before stalling, without releasing its
captured failure detail. An isolated `--exact --nocapture` credential-test retry
passed in 0.15 seconds. These results remain unresolved; serialization is not
claimed as a fix, and the production code used by these tests is unchanged.

- `CARGO_BUILD_JOBS=8 cargo clippy --workspace --all-targets -- -D warnings`: **PASS**.
- `cargo fmt --all --check`: **PASS**.
- `git diff --check`: **PASS**.

Validation output, serial/isolated retries and the bounded futex observation are
retained with the [raw gate evidence](reports/perf-gates-2026-11-25/). No overall
workspace or release pass is claimed.

## Historical debug measurements (retained)

Measured on 2026-11-25, dev3, Linux x86_64, AMD Ryzen 9 8945HX, 32 logical CPUs visible, shared development host. Debug build, `CARGO_BUILD_JOBS=8`. This is not the specified isolated four-core/8-GB acceptance machine.

Run:

```sh
export CARGO_BUILD_JOBS=8
cargo test -p squad-cli --test m6_performance -- --ignored --nocapture
```

The fixture uses the real `RoomHandle`, router, SQLite writer and member executors with the M2 loopback OpenRouter fake. Each roster receives five warm-up requests and 100 measured `@all` requests. The fake delays each completion response by 100 ms. Requests retain their durable history as the run progresses. The test process contains both the application runtime and fake provider; `/proc/self/status` RSS is sampled after every completed request wave. The numbers below are the second full run, including both rosters.

| Workers | Acceptance p95 | First delta p95 | 100-event replay | Maximum observed process RSS |
|---|---:|---:|---:|---:|
| 6 | 544.742 ms | 666.926 ms | 6.656 ms | 69,240 KiB |
| 12 | 1,081.485 ms | 1,221.659 ms | 6.682 ms | 102,700 KiB |

**The historical debug run failed its 500-ms deterministic first-delta assertion.** Both rosters missed. Acceptance dominates the observed latency; this test does not isolate the cause or attribute it to a specific router operation. The earlier six-worker-only run also missed: acceptance p95 557.957 ms, first delta p95 683.069 ms, replay 6.810 ms and sampled RSS 69,220 KiB. Do not loosen the target to make these measurements green.

At the historical debug measurement, the test was explicitly ignored as a manual gate with a known target miss. The release follow-up above passes; the gate remains manual. Its assertion remains executable. The short test adds repeatable latency and process-RSS evidence; it does not complete the release performance gate.

## Limits and outstanding measurements

- First delta means consumption from the room feed by the CLI test process, before rendering. It is not a measurement of visible TUI pixels.
- The 100-ms provider delay is configured separately. This fixture does not timestamp provider chunk receipt inside the client, so it does not independently measure application streaming overhead against the 100-ms requirement.
- Acceptance latency is not equivalent to six-mailbox fan-out latency.
- Replay measures a 100-event target, expanding to complete commit groups; it does not specifically seed 100 final messages.
- RSS samples are whole test-process resident memory, including wiremock and captured requests. They are not continuous peak RSS, allocator usage, or an isolated shipped `squad` process.
- There is no 30-minute sustained-stream result or dedicated 500-displayed-message TUI memory result. The release full-effects TestBackend workload is measured above; it does not cover the entire M6 acceptance scope. `M6.P2.L1.T7` and `M6.P2.L1.T8` remain unchecked.
- TUI governor shedding and idle disarming are deterministic functional checks in `crates/squad-tui/tests/m6_effects.rs`; they do not substitute for the remaining performance acceptance scope.
- Live-provider first-token objectives were not measured. No live credential is needed or used by the fixture.
