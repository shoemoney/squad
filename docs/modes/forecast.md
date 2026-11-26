# Forecast

`$forecast` produces a dated probability, the spread of the room, cruxes and
observable signposts. Use it for a yes/no event with a deadline. Use `$survey`
for a map of claims.

**Status: disabled pending shared judge dispatch integration.** The commands
below describe the mode after that integration.

```text
@all $forecast Will the project release version 1 by 2027-06-30?
@claude @openai @gemini $forecast Will the migration finish by 2027-03-31? -- The prototype works; production testing has not started.
```

Select three to twelve members. The judge is implicit; do not include `@judge`.
The usual cost confirmation and daily budget apply before work starts.

1. The judge freezes resolution criteria, date, source and a void rule. An
   unforecastable question stops the run before any member call.
2. Members independently give a probability from 1% to 99%, a reference class,
   three factors and a rationale. Fewer than three accepted forecasts fails.
3. Each accepted forecaster sees its own first estimate and anonymous peer
   reasons. Peer probabilities/base-rate fields are absent, and numeric
   probability/odds forms in prose are withheld. Members revise exactly once.
   A missing revision carries the first estimate forward as `not revised`.
4. The app pools forecasts using a trimmed mean of log-odds, trimming
   `floor(n / 5)` entries at each end. It shows one sample standard deviation
   of all log-odds around that pool as the **spread of the room**. This is not
   a confidence interval. A large, balanced gap produces a split view showing
   each side before the pool. The pool is not extremised: models share training
   data and blind spots. Moves toward the initial pool without citations are
   flagged as possible herding and still count.
5. A blind judge explains cruxes and signposts under **Update if you see**.
   It cannot change the app's probability. If this step fails, the aggregate
   remains visible and exportable.

The report includes the frame, pool/spread or split view, first/final table,
missing revisions, uncited moves, cruxes, signposts, shared assumptions, weak
evidence and skipped members. Export with `/export forecast` or
`/export forecast <contest-id>`. The Markdown audit adds timestamps, accepted
forecasts, each delivered bundle, raw probabilities, sorted and trimmed log-odds,
`k`, `L` and `s`. Partial failed/stopped runs also export. `/stop` fences late
results; restart reuses frozen inputs and label maps.

Six members make 14 normal calls: one frame, six first estimates, six revisions
and one crux explanation. Admission reserves 30 attempts: two per member call,
three per judge call. Estimates/revisions allow 2,048 visible tokens; judges
4,096. Member deadlines are 180 seconds; judge deadlines 300 seconds. Invalid
output receives one repair; transport failures receive plain retries. Actual
cost depends on roster token prices. Reservation is a worst-case hold, not the
bill; unused holds are released.

## Example run

No live example is claimed. The approved smoke is blocked before any forecast
provider call by the shared legacy judge gate. Live cost/time are **not measured**.
The fake-provider six-member fixture verifies 14 calls, 30 reserved attempts and
a stored/exported 55.0% pool; this is test data, not forecasting evidence.

## Integration requirements

Only forecast-owned files were changed. In
`crates/squad-core/src/runtime/judge.rs`, `Executor::execute` must route
`grant.work.kind.new_mode_judge()` to `self.crosscheck.execute` alongside the
existing structured judge kinds. That runner already routes new-mode work to
`runtime::forecast::execute`. The wrapper currently uses the legacy
`squad_agents::judge::JudgeExecutor` allowlist, which rejects forecast work with
`ReaderError::Forbidden`. Default-room testing reproduced `forecast.frame`
stuck running without a forecast provider call.

In `crates/squad-core/src/router/executor.rs`, classify `MODE_INVALID_OUTPUT`
as `FailureClass::InvalidOutput` alongside `JUDGE_INVALID_MODE_OUTPUT`.
Forecast currently uses the recognized latter diagnostic for member validation
failures so D75 repairs work; normalize the member diagnostic after this fix.

The workflow suite directly admits through the owned admission entry point and
uses the public runtime injection seam. It tests the full owned engine with the
public switch off, including captured revision privacy, repair, quorum, missing
revisions, crux failure with retained tally, stop fencing and restart. It does
not establish that the production judge wrapper works.

After the shared dispatch fix: set `forecast::ENABLED = true`, verify the parser,
typeahead/TIP/help, rerun default-launcher workflows and all gates, then run the
approved single live `squad send` in a fresh temporary data directory with the
default six-model roster and `SQUAD_DAILY_MAX_USD=150`. Record actual cost,
wall time and reservations here.

## Worker gates (2026-11-26)

Final tested code head: `00a83cae89a3384e68e6624bbed00091519026ca` on `mode-forecast`, dev3,
`CARGO_BUILD_JOBS=4`.

- `cargo test --workspace --no-fail-fast`: 1,348 unique tests passed, zero
  failures, four existing ignored entries. Cargo prints 1,349 passing summaries
  because the SQLite WAL subprocess repeats one test.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo fmt --all -- --check`: passed.
- `cargo build --release --locked -p squad-cli`: passed inside the release
  suite, which also exercises private binary snapshots and packaged sources.
- 23 added tests span protocol, store, agents, core workflows/tallies/export
  and TUI. The six-member workflow exports both rounds' unrounded aggregates;
  the current TUI bar uses the latest pool.

Twelve of fourteen M8.P3 tasks are checked. Default-launcher verification and
public enable/discovery/live verification remain open for the shared dispatch
fix. No real API calls were made: live spend is $0; real run cost, duration and
reserved-versus-actual amounts remain unmeasured.
