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

Measured 2026-11-26 on dev3, release code `a2dadf9`, with one real
`squad send`, a fresh temporary data directory, the default six-model roster,
`--env-file /dev/null`, `--yes --json` and `SQUAD_DAILY_MAX_USD=150`.
The question was a fictional planning scenario: whether Project Cedar, with two
full-time engineers, a working prototype and 60 percent of planned features
complete, would publish a non-prerelease v1.0.0 tag by 2027-06-30.

The run finished **COMPLETE** with exit code 0 in **261.103 seconds**. All six
first forecasts and all six revisions were accepted. There were 14 successful
forecast calls, one Grok transport timeout followed by a successful retry, no
schema repairs, and four startup identity probes: 19 provider generations total.
Resolved members were Claude Opus 5.5, Claude Fable 5.2, GPT-6.2 Sol, Gemini 4
Neon, Grok 4.7 and Muse Wonder; the judge was Claude Fable 5.2.

OpenRouter lookups found **all 19 generations**, confirming total cost
**$0.507324**, including **$0.001532** for startup probes. Forecast execution
itself cost **$0.505792**. Preflight estimated $4.483936 and reserved 30 attempts
with a **$20.205552** hold; the sampled peak matched that amount.

The terminal ledger records **$0.493022** and retains **$0.510112** reserved for
one unresolved timeout charge. A later read-only OpenRouter lookup confirmed
that charge as **$0.014302**, making the provider-confirmed total $0.507324.
The immediate runtime reconciliation had not resolved it; no ledger rows were
manually changed. The hold was later reconciled by the integrated runtime, as recorded below.

Excerpt from the exported report:

> Pooled probability: **45.5%**
>
> Spread of the room: 38.2%–52.9%; individual range: 34%–56%.
> The pool is not extremised. This spread is not a confidence interval.

Under **Update if you see**, the judge included an upward signpost for a release
candidate or beta tag with a published stable-release date by 2027-03-31.
The Markdown export succeeded and retained both unrounded aggregates and all
six delivered bundles. This scenario is a smoke test, not an empirical
calibration result.

Local evidence: `/tmp/squad-forecast-r2-live-6os3lna4/` contains `prompt.txt`,
`stdout.jsonl`, `summary.json`, `provider-costs.json`, `report.md` and the temporary
SQLite data directory. Credentials were injected in process memory; no key or
real env file was written into these artifacts.

## Integration and verification

With explicit scope approval on 2026-11-26, `mode-forecast-r2` changes shared runtime code and fixtures required for enablement:

- `crates/squad-core/src/runtime/judge.rs`: route new-mode judge grants to the
  structured runner, which already dispatches to the forecast executor.
- `crates/squad-core/src/router/executor.rs`: classify member
  `MODE_INVALID_OUTPUT` as invalid output so its private repair is accepted.
- `crates/squad-protocol/src/parser/examples.json` and its `all_examples`
  snapshot: enable row 176 and include forecast in export usage diagnostics.
- `crates/squad-protocol/tests/contests.rs`: check forecast availability against
  its enable switch instead of requiring it to remain disabled.
- `crates/squad-core/tests/router_validation.rs`: skip enabled forecast in the
  disabled-mode rejection fixture.
- `crates/squad-tui/tests/typeahead.rs`, `actions_shell.rs` and
  `contest_actions.rs`: reflect enabled forecast in starter/example counts,
  disabled-mode filtering, help and export inventories.

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


## Gates (2026-11-26)

Tested code: `a2dadf969660ade4073e998c2ebdb4d76f71401b`, dev3,
`CARGO_BUILD_JOBS=4`.

- `cargo test --workspace --no-fail-fast -- --test-threads=2`: 1,350 unique
  tests passed, zero failures, four existing ignored entries. The 1,351 passing
  executions include one repeated SQLite WAL subprocess test.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo fmt --all -- --check`: passed.
- `cargo build --release --locked -p squad-cli`: passed inside the release
  suite; the resulting binary performed the live smoke above.
- All 14 M8.P3 tasks are checked. The 25 forecast tests include seven production
  workflows and enabled completion/help/TIP discovery. No forecast test is ignored.

The initial default-parallel run exposed stale disabled-mode fixtures and a
regeneration-focus timeout in the shared PTY audit. Fixtures were updated; the
PTY audit passed in isolation and in the final complete two-thread rerun.

## Integrated delayed-receipt reconciliation

On 2026-11-26, the tested 0.2.0 binary from `c7c20ff` reopened the original dev3
smoke room with `/who`. The canonical writer applied the $0.014302 receipt:
recorded spend became **$0.507324**, reservations became zero and unknown calls
became zero. Attempt count stayed 19. The command exited zero in 0.818 seconds,
with no new provider generation or manual ledger edit. The before-state backup
and summary are at `/tmp/squad-v020-reconcile-forecast-cvbelkrb/` on dev3; the
integration has a local summary in its gate evidence directory.

## Integrated release smoke (2026-11-26)

Release 0.2.0 source `c7c20ff` ran once on dev2 with the built-in six-member
roster. CLI exit 0, `COMPLETE`, `complete`; 14 planned mode calls and
18 total provider attempts including startup. Spend was $0.470614, observed
wall time 105.725 seconds, and initial/sample-peak reservation
$20.188827/$20.188827. Final reserved funds were $0.000000 and unknown
calls 0. Credential-free offline export passed. Evidence: `/tmp/squad-v020-forecast-7k6pluyb/`;
see the [integration record](../live-smoke-2026-11-26-v0.2.0.md).

## Final integrated run (2026-11-27)

One `squad send` from the v0.2.0 release binary (SHA-256 `4bdcd35ae163167f938ca49a8bb5ead7b0ff8dd85744d963dde962d9aab8c613`, source `5afff2f`) on the built-in six-member roster, `SQUAD_DAILY_MAX_USD=150`, a fresh data directory and `--env-file /dev/null` reached **COMPLETE / complete**, with no `judge_failed`. Wall time 127.760 s; actual cost **$0.471815** across 18 provider attempts including four startup probes; peak reservation **$20.188827**; final reservation **$0** with zero unknown-cost calls. Offline `export` succeeded. Measurements for all eleven new modes are in [docs/costs.md](../costs.md).
