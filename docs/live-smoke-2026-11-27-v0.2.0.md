# v0.2.0 final integrated live smoke — 2026-11-27

One release binary, one `squad send` per new mode, run on dev2 in parallel.

- Source `5afff2f1f1e120c04e7c3141dd5359863c24214c`, version 0.2.0,
  `cargo build --release --locked -p squad-cli`.
- Binary SHA-256 `4bdcd35ae163167f938ca49a8bb5ead7b0ff8dd85744d963dde962d9aab8c613`.
- Built-in six-member roster and judge (no `SQUAD_MODEL_*` overrides),
  `SQUAD_DAILY_MAX_USD=150`, a fresh private data directory per mode and
  `--env-file /dev/null`. The credential came from the vault straight into each
  child's environment; no artifact contained it (every file was scanned).
- Prompts are the same realistic ones used in each mode's page (Debug against the
  controlled local fixture in [debug-fixture](reports/v0.2.0-debug-fixture/)).

| Mode | Outcome | Wall | Actual | Peak reserved | Remaining reserved |
|---|---|---:|---:|---:|---:|
| Forge | COMPLETE, original_won | 80.158 s | $0.441333 | $24.736836 | $0 |
| Decide | COMPLETE, complete | 94.179 s | $0.443567 | $16.400080 | $0 |
| Blow It Up | COMPLETE, reported | 197.393 s | $1.765600 | $20.574231 | $0 |
| Readback | COMPLETE, rounds_complete | 44.090 s | $0.420513 | $10.961076 | $0 |
| Forecast | COMPLETE, complete | 127.760 s | $0.471815 | $20.188827 | $0 |
| Tune | COMPLETE, tuned | 114.711 s | $0.552148 | $24.970323 | $0 |
| Bass Ackwards PRD | COMPLETE, saved | 411.352 s | $2.146136 | $50.711781 | $0 |
| Rank | COMPLETE, ranked | 33.562 s | $0.155734 | $18.024576 | $0 |
| Negotiate | COMPLETE, rehearsed | 376.767 s | $2.022003 | $32.021214 | $0 |
| Debug | COMPLETE, isolated | 451.384 s | $1.357046 | $41.175132 | $0 |
| Trace | COMPLETE, complete | 72.140 s | $0.356130 | $23.796411 | $0 |

Actual cost is the ledger (startup probes included) and equals the sum of
OpenRouter generation lookups: 251 of 251 attempts were found, each run within one
microdollar. Every run ended with zero reservations and zero unknown-cost calls; no
run ended `judge_failed`. Readback was answered `1a` through the CLI after its
`AWAITING_HUMAN` return. Debug was answered twice with real fixture probe output
and isolated after round 2. Blow It Up and Debug each had two failed attempts that
were paid and settled ($0.100000 and $0.041460). Offline `export` passed for all
eleven, and the PRD run emitted `export.ready` with its saved path.

Wall times are with eleven runs sharing the host and exclude human think time.
Gates on this source: format, Clippy `-D warnings`, 1,689 Cargo passes (four manual
ignores), 1,688 Nextest passes, doc tests.
