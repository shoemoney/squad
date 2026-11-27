# Readback

Status: enabled on `mode-readback-r2`. All fourteen M8.P7 tasks are complete.

Use `$readback` to find requirements that different readers would implement differently. It suits tickets, prompts, specifications and contract clauses. Members independently describe what they would build. A blind judge collects differences that change behaviour, and you choose the intended meanings before anything is rewritten.

```text
@all $readback The service retains records. The operator deletes backups.
@all $readback $READBACK_ROUNDS=1 The service retains records for thirty days.
@claude $readback
```

Paste the source after the command, quote a retained message, or mention a member to use that member's latest reply. The default loop has two rounds; `READBACK_ROUNDS` accepts one through four. The judge is implicit and cannot be mentioned as a participant.

Each round works as follows:

1. Every participant reads only the current spec in a fresh context and returns concrete behaviours, edge cases and assumptions.
2. The judge compares anonymous readings and produces a ledger of behavioural divergences anchored to verbatim passages. The human sees up to ten entries, lettered readings and member support counts.
3. Squad pauses at `READ IT BACK`. Participant holds are released so ordinary chat can continue. The pending questions and partial answers survive a restart and never expire.
4. Choose readings with `1b 2a`, or supply your own meaning with `/answer 3: retain records for thirty days`. Later answers replace earlier answers. Every entry answered closes the step; `/continue` closes it with the remaining entries open. No choice is preselected, and silence makes no choice.
5. The judge rewrites only resolved passages. The application verifies the exact change set, preserves every other character and checks the size bound. A later round reads the new spec independently.

An empty ledger ends with `converged`. Continuing without any answers ends with `no_readings_chosen` and makes no rewrite call. At the round limit the result is `rounds_complete`; the report explicitly says the last rewrite was not read back. Fewer than two accepted readings fails with `insufficient_readings`. Invalid member output gets one repair; exhausted members are skipped. Judge exhaustion keeps the last good spec. `/stop` preserves committed evidence and releases unused reservations.

With six members and two rounds there are at most 16 model calls and 36 reserved attempts. A member has two attempts and a judge three, including transport retries and at most one schema repair. The cost preview prices the frozen source, prompts, schemas and worst-case later evidence against the resolved models. Convergence releases unused rounds. Actual cost depends on model pricing, source length, repairs and how many rounds you choose to continue.

The report includes the final spec, per-round ledgers, reader attribution, chosen meanings, before/after pairs, open entries, skipped members and the application-computed convergence table. `/export readback [id]` also includes every spec version, interpretation and the human's raw answers. `squad send` returns at the human pause; use the same data directory for subsequent `/answer` or `/continue` sends.

## Example run

Measured on dev3 on 2026-11-26 with a release build, the default six-member roster, default two rounds, a fresh private data directory and `SQUAD_DAILY_MAX_USD=150`:

```text
@all $readback The service retains records. The operator deletes backups. The client retries requests.
```

All six interpretations were accepted in each round. Round 1 found four entries. Selecting `1a` and continuing chose the first retry reading; the judge changed only that passage. Round 2 found two entries in the revised spec. Continuing without answers completed with `no_readings_chosen`, leaving those questions open. Fable's second-round reply exceeded the 500-word budget (526 words) and succeeded on its single repair.

The mode made 15 normal calls plus that repair: 16 paid mode attempts costing **$0.629796**. Four startup resolution probes cost **$0.001498**, for a total recorded provider cost of **$0.631294**. Preflight reserved **$18.955638** for 36 worst-case attempts; final reserved cost is **$0**, with **zero unknown-cost calls**. Stored attempt costs sum exactly to the daily ledger.

Execution spans totalled **164.8 seconds**, including the resumed session and its manual continuation; the debugging/rebuild pause is excluded. End-to-end elapsed time was 489.4 seconds including that pause; the first start took 54.23 seconds. This run exposed a headless continuation defect: the old CLI exited before scheduling the rewrite. The fixed build recovered the same contest interactively, completed the rewrite and fresh readings, and the fixed headless `/continue` produced the terminal report. No second readback was started. The independent loopback CLI regression verifies answer → rewrite → fresh round → final report across a restart.

Report excerpt:

```text
Result: no_readings_chosen

## Final spec

The service retains records. The operator deletes backups. The client retries only when an attempt fails, errors, or times out; successful requests are not re-sent.
```

The unchanged retention and backup sentences remain byte-identical. The exported report is `/tmp/squad-readback-r2-live-hb8c211w/report-final.md`; accounting, resolved roster, timings and work statuses are retained alongside it in `metrics-final.json`. Keys came from the vault into process memory, with `--env-file /dev/null`; no key or real environment file was recorded.

Interactive `/export readback [id]` can choose the latest run. The standalone CLI requires its contest ID:

```sh
squad --data-dir /path/to/room export readback 01a21a4f-a678-740e-9b94-a64bf5f7146d
```

## Integration notes

The worker branch includes the required shared fixes:

- `crates/squad-core/src/runtime/judge.rs` routes new-mode judge work to the structured runner; `crates/squad-core/src/router/executor.rs` classifies `MODE_INVALID_OUTPUT` as invalid output, allowing member repair diagnostics.
- `crates/squad-core/src/router/mod.rs` adds a human answer's contest to the one-shot engine scope. `crates/squad-cli/src/adapter.rs` waits for the successor script transition after a completed human answer. The CLI unit and real-room regression in `crates/squad-cli/tests/real_room.rs` cover the dispatch gap and restart continuation.
- `crates/squad-core/tests/human_step.rs` seeds the mode-owned run and ledger. The protocol parser examples and export-usage snapshot, protocol contest registry test, core disabled-mode fixture, and TUI typeahead/actions/help/export fixtures account for readback being enabled. `crates/squad-tui/src/typeahead.rs` supplies its parseable TIP example.
- `crates/squad-tui/tests/m4b_tournament_runtime_audit.rs` waits for the champion event, rather than a per-round result followed by a later state read, removing a pre-existing event/state race.

Readback's owned stop callback leaves queued/running work visible to the shared stop handler, which emits cancellation events, fences grants and releases each unused work reservation atomically. Other mode switches remain unchanged. No shared changes remain outstanding for readback; the integrator can merge this branch and assemble its mode page into the global README.

## Validation evidence

Source head `6719836`, checked on dev3 with `CARGO_BUILD_JOBS=4`:

- `cargo test --workspace --no-fail-fast`: 1,354 unique tests pass (1,355 passing summaries include the repeated WAL subprocess), zero failures, four existing manual/PTY entries ignored. All readback tests are active.
- Twenty-seven owned mode tests pass across protocol (3), agents (1), store (4), tally (4), export (1), workflows (11) and TUI views (3), plus two CLI continuation regressions and the existing shared contract suites.
- Wiremock workflows cover real `RoomHandle` dispatch, fresh second-round input, blind judge messages, the stored 36-attempt reservation, partial/custom/replaced answers, a year-old pending answer, ledger cuts, invalid quote and rewrite repair, skipped members, quorum failure, judge exhaustion, exact untouched passages, round limits, convergence, no-choice completion and export. Stop coverage claims unstarted work in interpretation, diff and rewrite, verifies cancellation events and zero unused reservation, and rejects late callbacks.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo fmt --all -- --check`: passed.
- `cargo build --release -p squad-cli`: passed. The single paid contest above completed and exported; no live provider calls run in tests.

## Integrated release smoke (2026-11-26)

Release 0.2.0 source `c7c20ff` ran once on dev2 with the built-in six-member
roster. CLI exit 0, `AWAITING_HUMAN`, choices required; 8 planned mode calls and
12 total provider attempts including startup. Spend was $0.428807, observed
wall time 57.114 seconds, and initial/sample-peak reservation
$10.961076/$10.961076. Final reserved funds were $6.724861 and unknown
calls 0. The report-only offline export is not available before the human continuation;
it currently reports `contest not found` at this pause. Evidence: `/tmp/squad-v020-readback-qpox1sy9/`;
see the [integration record](../live-smoke-2026-11-26-v0.2.0.md).

The reservation is for unstarted continuation work, not an unknown charge.
The human choices are intentionally left open; this invocation tests the documented
headless pause. One interpretation required private repair. Six interpretations
and the diff were accepted. No judge failure occurred.

## Final integrated run (2026-11-27)

One `squad send` from the v0.2.0 release binary (SHA-256 `4bdcd35ae163167f938ca49a8bb5ead7b0ff8dd85744d963dde962d9aab8c613`, source `5afff2f`) on the built-in six-member roster, `SQUAD_DAILY_MAX_USD=150`, a fresh data directory and `--env-file /dev/null` reached **COMPLETE / rounds_complete**, with no `judge_failed`. Wall time 44.090 s (39.079 s start + 5.011 s `1a` continuation); actual cost **$0.420513** across 13 provider attempts including four startup probes; peak reservation **$10.961076**; final reservation **$0** with zero unknown-cost calls. Offline `export` succeeded. The run used `$READBACK_ROUNDS=1` on a short clamp specification and stopped at its human pause; answering `1a` produced the rewrite and terminal report. Measurements for all eleven new modes are in [docs/costs.md](../costs.md).
