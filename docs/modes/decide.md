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

On 2026-11-26, one activation-enabled release smoke used the default six-model roster, `SQUAD_DAILY_MAX_USD=150`, `--env-file /dev/null`, and a fresh private data directory. The branch switch remains off pending integration.

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

## Integration requirements

- `crates/squad-core/src/runtime/judge.rs`, `Executor::execute`: include `grant.work.kind.new_mode_judge()` in the condition routing structured work to `self.crosscheck.execute`. The inner `Runner` already dispatches new modes, but the outer executor sends these work kinds to the legacy adapter, which rejects them before the owned runtime runs. This blocks complete RoomHandle workflows and live smoke. The owned callback/executor fixture bypasses only that outer dispatcher and exercises the real router, store and provider transport.
- `crates/squad-tui/src/app.rs`, `KeyCode::F(7)` handler: in the existing loop over `self.session.contests.values_mut()`, call `contest.decide.toggle()` for a Decide contest. The handler already invalidates composed artifacts through `toggle_verdict()` and marks the frame dirty. The owned view supports both projections; shared input wiring is integrator-owned.
- `crates/squad-core/src/router/executor.rs`, `failed`: classify `MODE_INVALID_OUTPUT` alongside `JUDGE_INVALID_MODE_OUTPUT` as `FailureClass::InvalidOutput`. Otherwise a member's D75 repair is rejected as `PRODUCER_FORBIDDEN`, because a transport retry cannot carry diagnostics.
- `crates/squad-protocol/src/work.rs`, both visible-token match sites: include `DecideScore` in the 4,096-token member set. Until then the owned score executor explicitly requests 4,096 and the stored optional override is absent; reservation bounds already price 4,096. This avoids the scaffold's incorrect 2,048 ceiling without changing a shared file.

Mode activation stays off until the complete RoomHandle and D75 regressions pass after integration. The documented syntax is the intended enabled behavior.

## Verification

On dev2 with four Cargo build jobs, the workspace suite passed with 1,343 passing test executions (1,342 unique), zero failures and six ignored tests. Seventeen new Decide tests pass. Two further Decide integration regressions remain ignored for the shared judge-dispatch and member-repair blockers above. All-target workspace Clippy with warnings denied and the format check passed. Eleven of fourteen M8.P4 tasks are checked; complete runtime acceptance, toggle input wiring and activation remain open.

### Recovery verification on `mode-decide-r2`

The recovery worker on 2026-11-26 reproduced both integration failures against the recovered commits. With `ENABLED` temporarily set to true, `cargo test -p squad-core --test decide_workflows -- --ignored --nocapture` completed all six criteria calls and timed out after 15 seconds with `decide.freeze` still running. The member repair regression failed with `PRODUCER_FORBIDDEN`. The switch was restored to false after this probe; running the RoomHandle regression with the switch off stops at `COMMAND_NOT_IMPLEMENTED` before reaching dispatch.

The owned shortlist validator now rejects empty or whitespace-only option names and descriptions, with a regression covering both fields. No additional live call was made while the reproduced shared dispatcher failure remained unresolved. The earlier live attempt above remains the only live evidence; there is still no complete live decision report. The brief's shared-file ownership restriction remains in force.
