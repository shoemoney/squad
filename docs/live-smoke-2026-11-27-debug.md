# Debug integration evidence — 2026-11-27

## Original worker ledger

The stopped worker room `/tmp/squad-debug-smoke.e5wzca1q/data` reopened with
`/who` under the integrated accounting fix. The two Grok generation receipts
settled automatically. Spent became $0.151144, leaving one $0.787385 hold for
the old judge attempt with no generation ID. The attempt count stayed twelve.

Old source `30e7407` routes DebugMerge to ordinary JudgeExecutor, which rejects
that work kind before its provider runner. A source-scoped zero-cost Account
transaction was dry-run on a SQLite backup, then applied to the exact cancelled
attempt in the original room with a system audit notice. Final spent is
$0.151144, reserved $0 and unknown calls zero, still twelve attempts. The
canonical writer preserves the original billing day. Generic ID-less attempts
remain conservative; the absence of an ID alone is not grounds for zero cost.

[Audit source and summary](reports/v0.2.0-debug-reconciliation/) are committed.
Private before-state backups and dry-run results are in
`/tmp/squad-v020-reconcile-debug-hvquuycu/`. No new generation was started.

## Integrated full-contest smoke

The six-member default roster investigates an
isolated local black-box zero-rate bug. The initial observed batch returns
`[110.0, 110.0]`, while the stated numeric contract expects `[110.0, 100.0]`.
Actual operator probes supply human outcome evidence within one contest.

Source `d4f0225c5aa1ff2b4892217343c7efb31aeec644` passed formatting,
Clippy, 1,658 Cargo passing executions, 1,657 unique Nextest passes with four
inherited skips, doc tests and the release build. The snapshot is
`/tmp/squad-v020-build-d4f0225/squad`, SHA-256
`df6d472888c98a6ec275133078dc647e3d4fe6d118540afb6a551ce3e9bae817`.

Source `d4f0225c5aa1ff2b4892217343c7efb31aeec644` passed formatting,
Clippy, 1,658 Cargo passing executions, 1,657 unique Nextest passes with four
inherited skips, doc tests and the release build. Snapshot:
`/tmp/squad-v020-build-d4f0225/squad`, SHA-256
`df6d472888c98a6ec275133078dc647e3d4fe6d118540afb6a551ce3e9bae817`.

The one contest completed `COMPLETE`, `isolated`, exit 0. It used the default
six members and R=3. The plan bounded twenty-nine calls and sixty-three attempts,
estimated $9.350834 and initially reserved $41.175132. Sampled peak remained
$41.175132. Actual spend including startup was $1.940283, observed end-to-end
wall time 971.115 seconds (including operator observation time), and final
reservation/unknown calls were zero. Offline export passed.

| Round | Actual input list | Actual stdout | Submitted label |
|---|---|---|---|
| 1 | `[{"amount":100,"rate":0}]` | `[110.0]` | O1 |
| 2 | `[{"amount":100,"rate":0.20},{"amount":200,"rate":0}]` | `[120.0, 220.0]` | O1 |
| 3 | `[{"amount":100,"rate":0.5},{"amount":100,"rate":0}]` | `[150.0, 110.0]` | O1 |

Each selected procedure was invoked against the trusted local fixture through
a subprocess argv, without executing model shell text. All exits were zero.
The three selected instructions, outcome maps, actual argv and stdout are
committed in [fixture evidence](reports/v0.2.0-debug-fixture/). Each `/answer`
continued the same contest; no second Debug start occurred.

All six hypothesis tables were accepted, with one Grok transport retry. Grok
exhausted both ninety-second attempts for its proposal in each of the three
rounds; the other five proposals per round were accepted. All seven failed
attempts' receipts reconciled automatically. Thirty-three mode attempts plus
four startup probes settled. Twenty-six of twenty-nine work items completed;
the three exhausted proposal slots were skipped and all five judge calls
(merge, three selections and report) passed first try. No `judge_failed` occurred.

The application eliminated six hypotheses and retained H1 with three consistent
results. The final report proposes replacing a truthiness-based default with
`item.get("rate", 0.10)` (or an explicit key-presence check), describes it as
illustrative because the source was hidden, and says it was not applied.
Actual evidence remains under `/tmp/squad-v020-debug-c6s5um2l/`, including
`summary.json`, the initial summary, all three CLI continuations and `report.md`.
