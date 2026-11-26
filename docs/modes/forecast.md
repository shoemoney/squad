# Forecast

`$forecast` produces a dated probability, the spread of the room, cruxes and
observable signposts. Use it for a yes/no event with a deadline. Use `$survey`
for a map of claims.

**Status: enabled.** Forecast uses the production room runtime, parser,
completion, help and rotating examples.

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

The default-roster live smoke is pending the final gate run. No live cost or
wall time is claimed until it completes.

## Integration and verification

With explicit scope approval on 2026-11-26, `mode-forecast-r2` changes four
shared files required for enablement:

- `crates/squad-core/src/runtime/judge.rs`: route new-mode judge grants to the
  structured runner, which already dispatches to the forecast executor.
- `crates/squad-core/src/router/executor.rs`: classify member
  `MODE_INVALID_OUTPUT` as invalid output so its private repair is accepted.
- `crates/squad-protocol/src/parser/examples.json`: replace row 176's disabled
  forecast expectation with an enabled parse fixture.
- `crates/squad-protocol/tests/contests.rs`: check forecast availability against
  its enable switch instead of requiring it to remain disabled.

The owned forecast executor now reports `MODE_INVALID_OUTPUT` for members and
`JUDGE_INVALID_MODE_OUTPUT` for judges. All workflow fixtures use the production
launcher and public parser/room submission; the wrapper captures only the store
handle for assertions. No injected executor or second driver remains. Coverage
includes 14 normal calls/30 reserved attempts, captured revision privacy, member
repair, missing revisions, insufficient quorum, framing refusal, failed-crux
export, stop fencing and restart with frozen frame, labels and bundles.

The TUI discovery test verifies completion inventory/ranking, help description,
a parseable forecast example and its appearance in the rotating TIP. Strict
calendar validation also rejects signed numeric date components.
