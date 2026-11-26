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

On 2026-11-26, one activation-enabled release smoke used the default six-model roster, `SQUAD_DAILY_MAX_USD=150`, `--env-file /dev/null`, and a fresh private data directory. This earlier attempt preceded the shared dispatcher fix.

The decision compared SQLite with daily file backups against managed PostgreSQL for a five-person internal issue tracker. All six criteria calls returned accepted output on their first attempt. The run then stalled at `FREEZING` because the outer shared judge dispatcher rejected the work before the owned judge executor ran. A 150-second watchdog ended the process (exit 137). No decision report or winner was produced.

- Wall time: **150.01 seconds**, including startup.
- Preflight: **14 calls**, **30 reserved attempts**, estimated **$3.328140**, reserved **$16.358688**.
- Recorded actual spend: **$0.084077**, including startup alias probes. The ledger retains one unresolved judge attempt with no provider request ID, so this is recorded known spend rather than a claim that every accounting entry settled.
- Remaining reservation at shutdown: **$14.003520**. The private smoke database is retained for reconciliation; no later run or manual ledger edit was performed.
- Local evidence: `/tmp/squad-decide-live-Yz1Yfl/` on dev2 (`events.jsonl`, timing and SQLite ledger). No credentials are in those artifacts.

Accepted criteria excerpt:

> Low maintenance burden — weight 30: Effort required for setup, upgrades, monitoring, patching, and routine administration on a single server.
>
> Backup reliability and recoverability — weight 30: How reliably data can be backed up, verified, and restored to a recent consistent state after failure or corruption.

## Runtime and controls

Decide is enabled. Typeahead and help list `$decide`, and the composer TIP rotation includes a parser-checked example. Press F7 to switch between the score matrix and leave-one-criterion-out table.

The shared judge dispatcher now routes new-mode judge work through its structured runner. Invalid member output enters the diagnostic repair path, and DecideScore uses the canonical 4,096-token allowance in both protocol validation and durable work. Older frozen work without an optional allowance falls back to that canonical limit. The F7 input handler invokes the owned Decide view's toggle.

## Verification

The supplied-option and generated-option RoomHandle workflows exercise six members, exact 14/21 call counts and 30/45 attempt reservations, hidden weights, distinct option orders, persisted 4,096-token scoring allowances and restart without duplicate calls. The member repair regression uses RoomHandle and verifies one diagnostic repair per member, 20 total calls and reconciled accounting. Additional fixtures cover skips, quorum failures, judge exhaustion with exportable arithmetic, stop fencing, startup recovery, rubric provenance, medians, close calls and sensitivity arithmetic.

Protocol fixtures reject blank shortlist names and descriptions. The TUI tests exercise F7 through the app input handler and verify completion, help, TIP discovery and a public parser example. The two integration regressions inherited from the original worker are now normal tests.
