# Decide

Use `$decide` to compare two to four options on a rubric proposed independently by the room. The application computes the result; the judge merges wording and explains sensitivity.

Start with options, one per line after `--`:

```text
@all $decide Choose a database for a small internal service. Prefer low maintenance and reliable backups.
--
Managed PostgreSQL
Self-hosted PostgreSQL
SQLite
```

Or let the room propose options:

```text
@all $decide How should we reduce support response time with a fixed team?
```

The room first generates and shortlists options when needed. Members independently propose criteria and integer weights. The judge merges synonyms while retaining every source. The application keeps criteria supported by at least two members, retains at most seven by mean weight, and renormalizes their weights. Singleton and capped criteria remain in the report.

Each member scores every option on every retained criterion, with a short evidence line. Scorers receive shuffled option orders and no weights. The application uses cell medians and weighted totals, marks cells whose scores differ by at least four, and computes rankings with each criterion removed and the nearest weight changes that flip the leader. A margin below 0.25 on the ten-point scale produces “Too close to call” and no winner. The result is robust only when the leader survives every criterion removal and no flip is within ten percentage points of the current weight.

The judge explains only computed flip conditions and evidence-backed dissents. Its schema has no winner, ranking, score or weight field. These scores describe model judgments about the supplied context, so the evidence gaps matter.

Six members require 14 model calls with supplied options or 21 with generated options. Admission reserves 30 or 45 provider attempts respectively, including repairs. Actual dollar cost depends on frozen model prices, context and output lengths; preflight shows the estimate and conservative reservation. Failed members are skipped after two attempts. Every parallel step requires at least two accepted responses. Judge calls allow three attempts. A failed final explanation leaves the computed matrix exportable. `/stop` cancels unfinished work and keeps committed evidence; restart resumes frozen assignments and option orders.

The report contains options, the frozen rubric, scores, totals and margin, contested cells, leave-one-out rankings, flip thresholds, dissents, dropped criteria, evidence gaps and skipped members. Export it with `/export decide` or `/export decide <contest-id>`. Exports also include each accepted member's original proposal, weights, score sheet and evidence, merge provenance, and unrounded arithmetic.

## Example run

On 2026-11-26, one recovery smoke used the release binary, default six-member roster (`claude`, `fable`, `openai`, `gemini`, `grok`, `muse`), `SQUAD_DAILY_MAX_USD=150`, `--env-file /dev/null`, and a fresh private data directory. It compared SQLite with daily file backups against managed PostgreSQL for a five-person internal issue tracker on one Linux VM, with no dedicated DBA.

The run exited **0** in **236.24 seconds**, including startup, and reached **COMPLETE**. All six criteria lists and all six score sheets were accepted. Grok's first scoring attempt timed out after 90 seconds; its plain retry succeeded. There were **15 contest provider attempts** against **14 planned calls / 30 reserved attempts**, plus four startup alias probes, no schema repairs and no skipped members.

- Preflight estimate: **$3.335259**; initial reservation: **$16.385052**.
- Verified provider cost: **$0.621906**, including **$0.001556** for startup probes and **$0.014226** for the cancelled Grok attempt. Contest-only cost was **$0.620350**.
- Room ledger at exit: **$0.607680** recorded spend, **$0.583480** remaining reservation and **one unresolved attempt**. The runtime's immediate reconciliation did not settle the cancelled generation; a subsequent authenticated, read-only generation lookup verified its $0.014226 charge. No manual ledger edit was performed. The provider total above includes that charge; the retained reservation was subsequently reconciled by the integrated runtime (see below).
- Evidence on dev2: `/tmp/squad-decide-r2-live-te2wydaj/` contains `events.jsonl`, `timing.json`, the private SQLite ledger, the offline `report.md` export and `generation-costs.json`. No credentials are in these artifacts.

The application ranked managed PostgreSQL (`o2`) above SQLite (`o1`), but marked the result fragile. Report excerpt:

```text
Winner: o2
fragile · margin 0.2773 · 4 members rank the leader first

| Option | Total |
|---|---:|
| o2 | 7.9227 |
| o1 | 7.6455 |

| Removed criterion | Ranking |
| backup_reliability | o1 (8.1290) > o2 (7.7258) |
| backup_and_recovery_combined | o1 (7.9462) > o2 (7.9086) |

| Criterion | Rival | Overtakes when weight |
| total_cost | o1 | above 10.4220% |
```

The report also flags overlapping backup/recovery criteria and missing provider-specific backup guarantees as evidence gaps. The explanation did not choose the winner or calculate totals.

An earlier, pre-fix attempt stopped at the shared judge dispatcher after six criteria calls. Its separate database `/tmp/squad-decide-live-Yz1Yfl/` retains $0.084077 known spend and $14.003520 reservation with one unresolved judge slot after a 150.01-second watchdog stop. It produced no final decision report and was not modified or used as the recovery smoke room.

## Runtime and controls

Decide is enabled. Typeahead and help list `$decide`, and the composer TIP rotation includes a parser-checked example. Press F7 to switch between the score matrix and leave-one-criterion-out table.

The shared judge dispatcher now routes new-mode judge work through its structured runner. Invalid member output enters the diagnostic repair path, and DecideScore uses the canonical 4,096-token allowance in both protocol validation and durable work. Older frozen work without an optional allowance falls back to that canonical limit. The F7 input handler invokes the owned Decide view's toggle.

## Verification

The supplied-option and generated-option RoomHandle workflows exercise six members, exact 14/21 call counts and 30/45 attempt reservations, hidden weights, distinct option orders, persisted 4,096-token scoring allowances and restart without duplicate calls. The member repair regression uses RoomHandle and verifies one diagnostic repair per member, 20 total calls and reconciled accounting. Additional fixtures cover skips, quorum failures, judge exhaustion with exportable arithmetic, stop fencing, startup recovery, rubric provenance, medians, close calls and sensitivity arithmetic.

Protocol fixtures reject blank shortlist names and descriptions. The TUI tests exercise F7 through the app input handler and verify completion, help, TIP discovery and a public parser example. The two integration regressions inherited from the original worker are now normal tests.

The original 166 parser fixture rows and their snapshot remain unchanged. A test-only activation override freezes that legacy grammar while separate tests verify enabled Decide parsing and export discovery.

Final gates on dev2 with `CARGO_BUILD_JOBS=4`: `cargo test --workspace --no-fail-fast` passed with **1,347 passing test executions, zero failures and four inherited ignores**. All **21 Decide tests** pass with none ignored. `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check` and the explicit release build passed. Offline export of the live report succeeded without another provider call.

## Integrated delayed-receipt reconciliation

On 2026-11-26, release 0.2.0 from integration checkpoint `c7c20ff` reopened the
original smoke room with `/who`. The canonical accounting writer applied the
late $0.014226 receipt: spend changed from $0.607680 to **$0.621906**, the
$0.583480 hold became zero, and unknown calls became zero. Attempt count stayed
19, so no additional provider generation occurred. The command exited zero in
0.916 seconds. A before-state SQLite backup and summary are retained on dev2 at
`/tmp/squad-v020-reconcile-decide-jmsxp55j/`. No ledger row was manually edited.

## Integrated release smoke (2026-11-26)

Release 0.2.0 source `c7c20ff` ran once on dev2 with the built-in six-member
roster. CLI exit 0, `COMPLETE`, `complete`; 14 planned mode calls and
18 total provider attempts including startup. Spend was $0.400975, observed
wall time 69.135 seconds, and initial/sample-peak reservation
$16.400079/$16.400080. Final reserved funds were $0.000000 and unknown
calls 0. Credential-free offline export passed. Evidence: `/tmp/squad-v020-decide-v5zzu1x4/`;
see the [integration record](../live-smoke-2026-11-26-v0.2.0.md).
