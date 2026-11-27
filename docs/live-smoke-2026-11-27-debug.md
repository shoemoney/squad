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

Pending the final gate run. The six-member default roster will investigate an
isolated local black-box zero-rate bug. The initial observed batch returns
`[110.0, 110.0]`, while the stated numeric contract expects `[110.0, 100.0]`.
Actual operator probes will supply human outcome evidence within one contest.
