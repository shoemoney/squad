# Negotiate

Prepare and rehearse a negotiation with models playing both sides. Explore
interests, fallback (BATNA), possible overlap (ZOPA), conditional concessions and
objections before speaking with the real counterpart. **Hypothetical counterpart,
not a fact:** a rehearsal deal does not predict the other side's behavior.

The mode is enabled on the integration branch. The shared judge executor, TIP
example and measured compositor width are connected; integrated validation and
the paid smoke are recorded below as they complete.

## Start

Select four to twelve members. Put our goal, fallback and private limits before
`--`, and known counterpart information after it:

```text
@all $negotiate Renew a supplier contract. Need reliable weekly deliveries.
Fallback: retain our current supplier. Red line: no exclusive commitment.
-- The supplier wants predictable volume and faster payment.
```

For a shorter rehearsal, set the room variable first:

```text
NEGOTIATE_ROUNDS=1
@all $negotiate Ask for a paid trial with no exclusivity -- Buyer has a tight launch date
```

The default is three rounds per table; the allowed range is one through six.
Counterpart text is optional. Without it, inferences are especially uncertain.
The judge is implicit; do not address `@judge`. Headless use is:

```sh
squad send '@all $negotiate Renew with weekly deliveries and no exclusivity -- Supplier wants faster payment'
```

## What happens

1. Admission freezes inputs, participants, models, prompts, limits and rounds.
   The app randomly draws disjoint negotiator/counterpart pairs and shows them.
   Four or five members make two tables; six through twelve make three.
   Restart reuses the draw.
2. Every member independently maps interests, BATNAs, tentative ZOPA and tactics.
   Counterpart interests are marked `supplied` or `inferred`. Fewer than two
   accepted maps fails with `insufficient_maps`.
3. The judge reads anonymous maps and creates our strategy sheet, a safe public
   scenario and one hidden hypothetical brief per counterpart.
4. Everyone except counterpart players prepares a package from our side and the
   strategy sheet. A negotiator whose package failed opens from the strategy.
5. Tables rehearse in parallel waves. A round is a negotiator move and counterpart
   reply. Each sees its own transcript. Negotiators never receive hidden briefs;
   counterparts receive only the public scenario, their own brief and transcript.
   Negotiators are instructed to keep private fallback and limits out of messages.
6. `accept` ends a table with `deal`; `walk_away` with `no_deal`. At the round
   ceiling it is `unfinished`. Exhausting either player's attempts means
   `abandoned`; a failed reply is never a concession. Impossible future turns
   release their reservations immediately.
7. The judge writes a playbook, even if all tables were abandoned: opening,
   anchors, conditional concession ladder, objections, walk-away line, ZOPA,
   questions and assumptions. Table notes flag unrealistic accepted concessions
   against hidden reservations. All-abandoned means no rehearsal evidence.

Members have two attempts, judges three. A schema-invalid first member answer
gets one repair with diagnostics; transport failures retry normally. Missing moves
are never fabricated. Judge exhaustion fails while preserving committed evidence.
Stop fences late replies; restart reuses committed inputs and turns. Interrupted
unknown charges remain held until shared accounting reconciles them.

## Cost

For `m` members, `P = min(3, floor(m/2))` tables and `R` rounds, ceilings are
`2m - P + 2PR + 2` calls and `4m - 2P + 4PR + 6` reserved attempts:

| Members | Rounds | Maximum calls | Reserved attempts |
|---|---|---|---|
| 4 | 3 | 20 | 42 |
| 6 | 3 | 29 | 60 |
| 12 | 3 | 41 | 84 |

These are counts, not measured dollar prices. Reservations cover worst permitted
retry/repair inputs and outputs. Admission shows the cost plan and follows normal
confirmation and daily-budget policy. Context overflow fails admission instead of
dropping evidence. Early table endings reduce work.

## Report and export

The report contains roles, supplied/inferred interests, BATNAs, ZOPA, packages,
table transcripts and outcomes, playbook, assumptions and skipped work. The human
sees all briefs with the hypothetical warning. Outcome counts are application
computed. Breach flags come from the judge's `unrealisticConcessions`, not a claim
about the actual counterpart. Wide layouts show tables side by side; narrow layouts stack them.

```text
/export negotiate
/export negotiate <contest-id>
```

Markdown adds every accepted map, briefing, package and turn with round/table
information. Failed/stopped partial contests stay exportable without new calls.
Exports exclude frozen prompt sources and model configuration. A failure or stop
after work has begun also commits an assembled partial report, retaining skipped
members and committed table outcomes even when no playbook was completed.

## Example run

The integrated release smoke completed on source `7753b71`: one deal and two
unfinished tables after three rounds, with no abandoned tables or judge failure.
Measured cost and reservation details are recorded below.

The credential-free six-member R=3 wiremock workflow completes 29 calls, reserves
60 attempts and finishes three tables as `unfinished`. Separate workflows verify
walk-away, all-abandoned, repair, stop and restart. These are test results only.

## Historical worker handoff

- `crates/squad-core/src/runtime/judge.rs`, `Executor::execute`: include
  `grant.work.kind.new_mode_kind().is_some()` in the condition forwarding to
  `self.crosscheck.execute(...)`. Its `Runner` already dispatches new modes to
  the owned runtime. This is the production blocker.
- `crates/squad-tui/src/composed.rs`: pass the measured contest cell width to
  `widgets::negotiate::render_width(&contest.negotiate, usize::from(body_width))`
  instead of `render`. The existing call uses
  the narrow fallback, so wide layout remains an integration task.
- `crates/squad-tui/src/typeahead.rs`, `examples`: add
  `"@all $negotiate renew our supplier contract -- Supplier wants faster payment"`
  to the example candidates. The existing parser filter suppresses it when the
  mode is disabled or the roster is too small. The temporary-enable probe passes
  parser, mode completion and help assertions, but fails the TIP-example assertion
  in `negotiate_view.rs:93` because no negotiate example is registered.
- `crates/squad-core/src/router/executor.rs`: recognize `MODE_INVALID_OUTPUT` as
  `FailureClass::InvalidOutput`. Until then, the owned executor uses the already
  recognized `JUDGE_INVALID_MODE_OUTPUT` for member repairs as well.
- After dispatch integration, enable the owned protocol switch, update shared
  enable-sensitive parser/help/typeahead fixtures as needed, run production
  launcher tests and all gates, then perform one approved paid smoke with fresh
  data, the intended roster and `SQUAD_DAILY_MAX_USD=150`. Fetch credentials into
  process memory without printing them and record the ledger and report here.

The worker changed only ownership-map files and its M8.P5 task entries.
The integrator has since connected the shared judge dispatch, existing TIP
example and compositor width, enabled the mode and added public-launcher and
composed-layout regressions.

## Worker verification (2026-11-26)

There are 35 owned tests: five protocol, seven store, one agents validator,
fourteen core workflow, three tally, two export and three TUI view tests.
The workflow harness uses the actual scoped readers, owned runtime, fake
OpenRouter transport and acknowledged router/accounting boundary. It bypasses
the shared outer judge executor; it is not proof of production launcher dispatch.

The disabled-branch full workspace gate passes 1,360 unique tests, zero failures
and four existing ignored tests (1,361 passing summary entries include the WAL
subprocess repeated inside its parent test). The release CLI build and its
behavior/package checks pass. All-target workspace Clippy with warnings denied
and formatting pass. Commands on dev3:

```sh
export CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=1
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0
cargo test --workspace --no-fail-fast
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

Serial fixture execution avoids overlapping expensive test builds; disabling
debug information and incremental compilation keeps the shared box's memory
use lower. The original debug-profile run was interrupted with exit 143, without
a reported assertion failure. The completed serial run is the gate evidence.

A separate temporary-enable probe passes all five protocol tests, including a
public parser example and the three-member rejection. In the view target, two
tests pass and the registration test passes mode-menu/help assertions before
failing the absent TIP candidate at `negotiate_view.rs:93`. The public switch is
restored to false. This failure is intentionally reported as an integration gap,
not hidden by weakening the future enable test.

The shared repair classifier workaround, production judge blocker, TIP example
and compositor width call are listed above. M8.P5 has eleven tasks complete and
two open: the production wide-layout binding and public enable/live verification.
No real provider call was made; live cost, wall time, reserved versus actual and
a real report excerpt remain pending.

## Integrated release smoke (2026-11-26)

Release 0.2.0 source `7753b71` ran once on dev2 with the built-in six-member
roster. CLI exit 0, `COMPLETE`, `rehearsed`; 29 planned mode calls and
39 total provider attempts including startup. Spend was $1.591324, observed
wall time 274.577 seconds, and initial/sample-peak reservation
$32.021214/$32.021214. Final reserved funds were $0.000000 and unknown
calls 0. Credential-free offline export passed. Evidence: `/tmp/squad-v020-negotiate-iocxpej9/`;
see the [integration record](../live-smoke-2026-11-26-v0.2.0.md).

All twenty-nine work items completed. Six first member outputs exceeded their
word budgets and received one private repair each: one map, two moves and three
counterpart replies. Both judge stages passed first try. The full run used
thirty-five mode attempts plus four startup probes. The initial preflight
estimate was $12.431808, with sixty reserved attempts; all unknown calls and
reservations settled to zero. One table reached a deal and two ended unfinished
at the three-round ceiling. The exported role/outcome table and full build
provenance are in the integration record.

## Integrated validation

All thirty-seven Negotiate checks pass, including the public OneShot production
launcher and composed wide–narrow–wide viewport test. The full group passes fmt,
all-target Clippy, 1,631 Cargo passing executions (one repeated WAL child),
1,630 unique Nextest tests with four inherited skips in 190.567 seconds, doc
tests and the explicit release build. The claim fixture uses four members so
Negotiate's grammar participates in all 162 enabled-mode/active-kind cases.

## Final integrated run (2026-11-27)

One `squad send` from the v0.2.0 release binary (SHA-256 `4bdcd35ae163167f938ca49a8bb5ead7b0ff8dd85744d963dde962d9aab8c613`, source `5afff2f`) on the built-in six-member roster, `SQUAD_DAILY_MAX_USD=150`, a fresh data directory and `--env-file /dev/null` reached **COMPLETE / rehearsed**, with no `judge_failed`. Wall time 376.767 s; actual cost **$2.022003** across 41 provider attempts including four startup probes; peak reservation **$32.021214**; final reservation **$0** with zero unknown-cost calls. Offline `export` succeeded. Measurements for all eleven new modes are in [docs/costs.md](../costs.md).
