# M6 performance measurements

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

**The manual performance test fails its 500-ms deterministic first-delta assertion.** Both rosters miss. Acceptance dominates the observed latency; this test does not isolate the cause or attribute it to a specific router operation. The earlier six-worker-only run also missed: acceptance p95 557.957 ms, first delta p95 683.069 ms, replay 6.810 ms and sampled RSS 69,220 KiB. Do not loosen the target to make these measurements green.

The test remains explicitly ignored in routine workspace runs because it is a manual measurement gate with a known target miss. Its assertion remains executable. The short test adds repeatable latency and process-RSS evidence; it does not complete the release performance gate.

## Limits and outstanding measurements

- First delta means consumption from the room feed by the CLI test process, before rendering. It is not a measurement of visible TUI pixels.
- The 100-ms provider delay is configured separately. This fixture does not timestamp provider chunk receipt inside the client, so it does not independently measure application streaming overhead against the 100-ms requirement.
- Acceptance latency is not equivalent to six-mailbox fan-out latency.
- Replay measures a 100-event target, expanding to complete commit groups; it does not specifically seed 100 final messages.
- RSS samples are whole test-process resident memory, including wiremock and captured requests. They are not continuous peak RSS, allocator usage, or an isolated shipped `squad` process.
- There is no 30-minute sustained-stream result, no 500-displayed-message TUI memory result and no 12-stream `SQUAD_FX=full` renderer/latency/memory result. `M6.P2.L1.T7` and `M6.P2.L1.T8` remain unchecked.
- TUI governor shedding and idle disarming are deterministic functional checks in `crates/squad-tui/tests/m6_effects.rs`; they do not substitute for the missing full-effects performance measurement.
- Live-provider first-token objectives were not measured. No live credential is needed or used by the fixture.
