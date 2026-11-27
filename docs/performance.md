# M6 performance measurements

## Dirty-card preparation — 2026-11-27

Corrected-fixture manual gates pass on dev3 (30 seconds per viewport) and hueb
(300 seconds per viewport), with the requested Mac workflow still queued.
Measured on dev3 (AMD Ryzen 9 8945HX, Linux x86_64, Rust 1.99.1), optimized
bench, `CARGO_BUILD_JOBS=4`, 30 seconds per viewport. Builds and measurements
ran sequentially. Full effects, 60-Hz scheduling, real widgets, 10,000 retained
history cards and twelve 40-delta/s streams remain enabled. No thresholds changed.

Final Linux results (30 seconds each, all corrected-fixture gates pass):

| Viewport | Before → final draw p95 ms | Final draw p99 ms | Final key p95 ms | Final CPU % one core | Final maximum resize frame ms | Final ending RSS KiB |
|---|---:|---:|---:|---:|---:|---:|
| 140x42 | 3.173 → 0.765 | 0.991 | 16.201 | 3.397 | 40.733 | 194440 |
| 80x24 | 2.815 → 0.472 | 0.563 | 16.236 | 3.488 | 104.629 | 189148 |
| 200x60 | 3.501 → 1.219 | 1.719 | 16.324 | 5.126 | 43.478 | 199596 |

The full final workspace suite passed: **1,302 tests, zero failures, four ignored**.
The four ignored entries are the manual release latency gate, 30-minute streaming
RSS soak, parent-driven PTY child entry point, and manual 500-message RSS audit.
Workspace/all-target Clippy with `-D warnings`, formatting and whitespace checks
pass. All existing insta snapshots remain unchanged. Seven new regressions cover
terminal/retry/supersession publication, pending versus displayed text, no per-frame
history lookups/inserts, follow/unread transitions, clear/restore, replay/paging,
width/anchor stability, fence completion/replacement, and Unicode/tab source rows.
A six-second-per-viewport historical-mode smoke also passed; it only verifies the
preserved comparator switch and is not a sustained acceptance run.

The corrected fixture resizes TestBackend with Terminal and anchors at the verdict
widget's header before emitting its live cue. It asserts verdict visibility during
the painter window. Resize frames remain in all inclusive latency percentiles;
the log also reports resize-call and full resize-frame stalls separately.
`COMPOSED_HISTORICAL=1` retains the original fixture behavior as a comparator;
older results below used that behavior and did not exercise actual width reflow.

| Viewport | Before draw p95 / p99 ms | Dirty-card draw p95 / p99 ms | Before / dirty key p95 ms | Before / dirty CPU % one core | Before / dirty ending RSS KiB |
|---|---:|---:|---:|---:|---:|
| 140x42 | 3.173 / 5.561 | 1.375 / 1.962 | 490.049 / 419.914 | 27.868 / 17.407 | 261976 / 195084 |
| 80x24 | 2.815 / 5.276 | 1.113 / 1.475 | 404.681 / 419.890 | 26.019 / 16.206 | 256680 / 189792 |
| 200x60 | 3.501 / 6.000 | 1.844 / 2.512 | 454.716 / 521.957 | 29.545 / 21.229 | 270272 / 203360 |

These short corrected-fixture windows **fail the key-latency gate**. Each includes
six real width changes. Before maximum resize-frame stalls were 792.389,
789.332 and 791.800 ms; after dirty preparation they were 835.527, 815.787 and
835.472 ms. Removing per-frame history scans improves draw latency and CPU but
does not fix synchronous full-history width reflow. Completed fence parser state
release reduces ending RSS by approximately 65.3 MiB in each viewport.

Dirty publication retains one revision entry per message, rather than an unbounded
chunk journal. Completion, failure, cancellation, retry and supersession publish
identities immediately; pending deltas publish text at the 50-ms flush. Preparation
reads CardState directly from the borrowed card. Activity counts change on actual
insertions and follow transitions; ticks and draws perform no history insertions.
Restore preserves surviving unread identities while older replayed cards count as
seen. Completed Markdown keeps source and styled/wrapped rows and releases its
incremental fence parser/highlighter state.

[Raw before/after logs](reports/perf-dirty-cards-2026-11-27/) include startup,
frame/delta counts, load, inclusive percentiles, resize stalls and RSS.
### Conditional anchor/reflow follow-up

A 30-second-per-viewport smoke on hueb (Apple M3 Ultra, 28 logical CPUs, 96 GiB,
macOS 27.2.1, Rust 1.99.1, four Cargo jobs) confirmed the remaining failure after
dirty preparation: draw p95 5.050/4.342/5.670 ms, key p95
341.814/341.260/345.524 ms, CPU 21.484/18.827/27.666%, and maximum resize
stalls 636.398/644.792/646.991 ms. This triggered the conditional rank-4 work.

Anchor preparation now uses the already parsed block's actual row count and caches
its source-grapheme count. Its lightweight source-row mapping preserves the existing
Unicode/tab/control wrapping policy without allocating display cells or starting a
second Markdown renderer/highlighter. Width changes reuse wrapped blocks when all
styled lines fit both old and new widths, with a conservative tab-width bound.
Narrowing still performs real wrapping; there is no viewport-specific shortcut or
alternate renderer. Retry, completion, reference reconciliation and width changes
keep the canonical source anchors.

The subsequent Mac smoke passed at all three viewports:

| Viewport | Draw p95 / p99 ms | Key p95 ms | CPU % one core | Maximum resize frame ms |
|---|---:|---:|---:|---:|
| 140x42 | 2.593 / 3.153 | 18.256 | 10.807 | 72.756 |
| 80x24 | 1.541 / 1.812 | 18.155 | 8.263 | 133.793 |
| 200x60 | 3.780 / 5.146 | 19.700 | 14.098 | 75.313 |

These Mac measurements ran manually under the runner's `squad-ci` account in an
isolated source/build directory with a clone of its compiled dependency cache.
The unrelated CI job remained untouched and idle in its stalled PTY test.
[Environment](reports/perf-dirty-cards-2026-11-27/mac-environment.txt) and raw logs
identify this shared-host qualification. They are separate from workflow acceptance.

[Run 45927912562](https://github.com/shoemoney/squad/actions/runs/45927912562)
was the initial dirty-only dispatch and was cancelled while queued after the
conditional fix. Its replacement, [run 45928957488](https://github.com/shoemoney/squad/actions/runs/45928957488),
uses product head `659700128025546c20e3e2d5024ccaa678cbc3cb` and remains queued
behind unrelated CI, which the user requested be left running. The final manual
300-second-per-viewport run passes at all sizes; workflow acceptance remains
pending. Original supplied macOS draw p95 was 10.8–11.6 ms in
[run 45912463641](https://github.com/shoemoney/squad/actions/runs/45912463641).

### Final manual macOS gate — 300 seconds per viewport

All three corrected-fixture windows pass, with draw p95 below the 6-ms target.
The 60 real width changes per viewport remain in inclusive draw, work and key
latency samples; dedicated resize p95/max still expose the synchronous tails.

| Viewport | Draw p95 / p99 ms | Key p95 ms | CPU % one core | Resize frame p95 / max ms | Frames / deltas | Ending RSS KiB |
|---|---:|---:|---:|---:|---:|---:|
| 140x42 | 2.376 / 3.058 | 18.638 | 9.708 | 73.723 / 74.991 | 17765 / 144012 | 179216 |
| 80x24 | 1.564 / 1.809 | 19.851 | 8.343 | 135.834 / 136.529 | 17535 / 144012 | 220128 |
| 200x60 | 3.514 / 4.196 | 19.474 | 14.487 | 74.436 / 75.318 | 17765 / 144012 | 233568 |

Product source is `659700128025546c20e3e2d5024ccaa678cbc3cb`.
[Source hashes](reports/perf-dirty-cards-2026-11-27/source-sha256.txt) were checked
against dev3. Linux's later benchmark-only Clippy spelling uses
`is_multiple_of(8)` instead of `% 8 == 0`; production code and workload are identical.
[Raw sustained Mac log](reports/perf-dirty-cards-2026-11-27/mac-final-300s.log),
[machine-readable summary](reports/perf-dirty-cards-2026-11-27/summary.json) and
[Linux comparison chart](reports/perf-dirty-cards-2026-11-27/draw-p95.html)
retain the measurements. Shared-host manual results are separate from the queued
workflow's acceptance status. The workflow still needs to run on the dedicated
runner; its stalled unrelated CI job was left untouched as requested.

Reproduction from the product checkout (builds finish before benchmark sampling):

```sh
CARGO_BUILD_JOBS=4 COMPOSED_SECS=30 cargo bench -p squad-tui --locked --bench composed_streams
CARGO_BUILD_JOBS=4 COMPOSED_SECS=300 cargo bench -p squad-tui --locked --bench composed_streams
CARGO_BUILD_JOBS=4 cargo test --workspace --no-fail-fast
CARGO_BUILD_JOBS=4 cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

## Scoped memory audit on dev2 — 2026-11-26

Host: shared dev2, Linux x86_64, kernel 7.0.0-43-generic, AMD Ryzen 9 8945HX (32 logical CPUs), Rust 1.99.1. Debug test binaries, built with `CARGO_BUILD_JOBS=4`. These measurements use library/runtime test processes and do not certify the isolated four-core/8-GB reference-machine profile or an integrated physical-terminal/full-effects run. No live provider or real credential is used.

| Workload | Whole test-process memory | Limit | Result |
|---|---:|---:|---|
| TUI transcript: 500 messages, no generation | 109,084 KiB RSS | 262,144 KiB | PASS |
| Combined six- and twelve-agent streaming rooms | 155,588 KiB kernel high-water RSS | 524,288 KiB | PASS, 1805 s |

The TUI fixture uses 500 complete 900-byte Markdown messages, a 140×42 viewport, and visits every viewport to populate layout/render caches before reading `/proc/self/status`. Its isolated process includes the TUI library and test harness; it does not launch the shipped CLI. Source: `crates/squad-tui/tests/memory_audit.rs::five_hundred_displayed_messages_process_rss`.

The completed streaming run lasted 1,805.88 seconds and accepted 156 waves (2,808 member answers), with no failures.

The streaming fixture runs the real `RoomHandle`, router, SQLite writer and agent executors in two concurrent rooms (six and twelve configurable fixture members). Each wave sends `@all` to both, accepts a 3,712-byte answer in eight SSE deltas per member after a 100-ms fake-provider delay, waits for all durable finals, and pauses ten seconds before the next wave. All application history is retained. It reads kernel `VmHWM` so allocations between wave samples count; the 512-MiB assertion covers the combined runtime/fake-provider process. There is no TUI consumer in this process, and the 100-ms delay is configured rather than a streaming-overhead measurement. Source: `crates/squad-cli/tests/streaming_memory_audit.rs::six_and_twelve_agent_streaming_for_thirty_minutes`.

Two preliminary runs are retained. The character-at-a-time SSE fixture timed out during concurrent compilation. The next run revealed test-fixture retention: Wiremock keeps matching request clones in each mounted mock even with global recording disabled. The final harness resets and remounts the mock between completed waves, freeing those captured prompts while retaining application histories. Neither preliminary run is a product pass or a proven application leak.

Commands:

```sh
CARGO_BUILD_JOBS=4 cargo test -p squad-tui --test memory_audit -- --ignored --nocapture
CARGO_BUILD_JOBS=4 cargo test -p squad-cli --test streaming_memory_audit -- --ignored --nocapture
```

[Raw memory evidence and audit validation](reports/audit-foundations-2026-11-26/).

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

### Missing-cost reservation: failed 3/3 before the fix, now resolved

The three explicit release executions in this rerun failed the unchanged assertion
`state.daily_spend.reserved_usd > 0`, at load 1.46/3.72/4.29. At that time ordinary
dispatch had no reservation row to retain: `router::executor::start` admitted zero new
micros for an ordinary request, so a missing-cost completion left no positive reserved
amount.

The fix is commit `2b040e9`. It sizes the ordinary dispatch reservation from the
assembled prompt, and its coverage is in `crates/squad-core/tests/dispatch_reservations.rs`.
The check `m6_operational::ordinary_missing_cost_must_retain_a_positive_priced_reservation`
is no longer `#[ignore]`d, so it runs in the normal test suite. The pre-fix FAIL result
above is kept as the record of that run.

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

### Perf-gates branch validation: operational-test stall (later resolved)

The integration-wave6 follow-up identified and repaired the bundled SQLite
open/close deadlock. See the [root cause, regression proof and repeated gates](reports/integration-wave6-2026-11-25/README.md). The evidence below records the original branch run.

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
- The scoped debug audit above records a 30-minute runtime streaming soak and a dedicated 500-displayed-message TUI fixture. These library/runtime measurements do not replace an integrated physical-terminal/full-effects memory run on the four-core/8-GB reference host. The release full-effects TestBackend workload is measured separately above. `M6.P2.L1.T8` records three optimized runs per viewport with `SQUAD_FX=full`; `M6.P2.L1.T7` records the qualified memory results above.
- TUI governor shedding and idle disarming are deterministic functional checks in `crates/squad-tui/tests/m6_effects.rs`; they do not substitute for the remaining performance acceptance scope.
- Live-provider first-token objectives were not measured. No live credential is needed or used by the fixture.
