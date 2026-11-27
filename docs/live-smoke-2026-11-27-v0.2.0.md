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

## Cross-mode follow-up: two longer live runs (2026-11-27, dev3)

Run after the cross-mode checklist work, from a release binary built from the final tree
(`120c0fd`, `cargo build --release --locked -p squad-cli`, SHA-256
`e7bce9c2b856f61dfca3b47ecf814d0c228d7a1cbdbf23e9fa8379ffef47c488`). That tree differs from
`5afff2f` by the three blind-projection fixes recorded in the changelog. Built-in six-member
roster and judge, `SQUAD_DAILY_MAX_USD=150`, a fresh data directory per run and
`--env-file /dev/null`; the key went from the vault into the child environment only. Both
runs were driven through `squad send --yes --json` and ran at the same time.

| Run | Outcome | Wall (active sends) | Actual | Peak reserved | Final reserved / unknown |
|---|---|---:|---:|---:|---|
| `$readback` with `READBACK_ROUNDS=2` | COMPLETE, `rounds_complete` | 384.736 s | $1.356852 | $19.825494 | $0 / 0 |
| `$bass_ackwards_prd` with `PRD_HUMAN_REVIEW=1` | COMPLETE, `saved` | 389.220 s | $2.561021 | $56.142855 | $0 / 0 |

Wall time sums the sends and excludes human think time. Peak is the worst-case
reservation the preflight placed (the largest `reservedUsd` in the CLI's accounting
records); every run released it in full. Actual cost is the ledger total including four
startup probes. Offline `export` exited 0 for both (41,469 and 349,456 bytes).

**Readback, two rounds.** Start 104.373 s to the first `AWAITING_HUMAN` (three entries),
`1a 2a 3a` (148.967 s) answered round 1 and returned at the second pause (the rewritten spec read
back), `1a 2a` (131.396 s) answered round 2 and finished. The run made 24 provider attempts: four
startup probes, 16 planned mode calls, three `readback.interpret` retries and one
`readback.rewrite` retry. The first rewrite attempt failed, was paid ($0.086696) and settled;
its retry completed. Two rewrites and two diff judgments ran. The export states
*"The last rewrite was not read back."* That is the documented behaviour, not a defect: with
two rounds the round-1 rewrite is read back in round 2, and the rewrite produced from the
round-2 answers is the last version and has no further round. Reading that one back needs
`READBACK_ROUNDS=3`.

**PRD with human review.** The start call returned at `LAST LOOK` after 356.281 s with the
document built and no file yet written. `/answer <note>` (0.546 s) recorded one note and
`/continue` (32.393 s) ran the judge's apply step and saved the file. 35 provider attempts: four
probes, 29 planned calls (six each of done, tree, review and must-have, plus freeze, merge,
revise, finalize and apply once) and two repair retries (`review`, `apply`). The saved file is
`exports/prd-self-hosted-status-page-for-a-small-saas-live-wi-<id>.md` under the data
directory, and the `/continue` send emitted `export.ready` with that path.

**Superseded rehearsal.** Both runs were first made on a binary built before the three
blind-projection fixes (`4bdcd35…`, the integrated build). They completed too (Readback
$1.448497; PRD reached `LAST LOOK` at $2.163536 after 405.187 s) and were discarded and redone
on the final binary, so the owner-approved spend for this section is about $7.5 in total.

## Release-candidate smoke: `$negotiate` and `$trace` on the merged tree (2026-11-27, dev2)

Final run on the release build of the merged `release-v0.2.0` tree (fixes and cross-mode
branches merged, `cargo build --release --locked -p squad-cli`, SHA-256
`7e7ebbea17f174fcffb028dbbed2760371e06e93c13d147644082cd9a691bbd1`). Built-in six-member roster and
judge, `SQUAD_DAILY_MAX_USD=150`, a fresh data directory per run, `--env-file /dev/null`, key
from the vault into the child environment only (every output file was scanned for it).

| Run | Outcome | Wall | Actual | Peak reserved | Final reserved / unknown |
|---|---|---:|---:|---:|---|
| `$negotiate` (renewal, weekly deliveries) | COMPLETE, rehearsed, 33 calls | 263.32 s | $1.422555 | $31.902540 | $0 / 0 |
| `$trace` (three-source retention packet) | COMPLETE, 14 calls | 60.32 s | $0.350807 | $23.791536 | $0 / 0 |
