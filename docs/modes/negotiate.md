# Negotiate

Prepare and rehearse a negotiation with models playing both sides. Explore
interests, fallback (BATNA), possible overlap (ZOPA), conditional concessions and
objections before speaking with the real counterpart. **Hypothetical counterpart,
not a fact:** a rehearsal deal does not predict the other side's behavior.

This branch keeps the public switch off until the integrator connects the shared
judge executor. The owned engine and scoped provider workflows are implemented;
production enable and the paid smoke remain pending that repair.

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
The judge is implicit; do not address `@judge`. After integration, headless use is:

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
about the actual counterpart. Narrow layouts stack tables; the owned
`render_width` supports side-by-side wide layouts.

```text
/export negotiate
/export negotiate <contest-id>
```

Markdown adds every accepted map, briefing, package and turn with round/table
information. Failed/stopped partial contests stay exportable without new calls.
Exports exclude frozen prompt sources and model configuration.

## Example run

A paid production run has **not** been performed. The shared outer judge executor
routes neither `NegotiateBrief` nor `NegotiatePlaybook` to its existing new-mode
runner. Enabling and spending on maps would therefore stop at briefing. Actual
provider cost, wall time, initial reservation versus actual, final holds and a
real report excerpt remain unmeasured. No fixture output is presented as live.

The credential-free six-member R=3 wiremock workflow completes 29 calls, reserves
60 attempts and finishes three tables as `unfinished`. Separate workflows verify
walk-away, all-abandoned, repair, stop and restart. These are test results only.

## Integration handoff

- `crates/squad-core/src/runtime/judge.rs`, `Executor::execute`: include
  `grant.work.kind.new_mode_kind().is_some()` in the condition forwarding to
  `self.crosscheck.execute(...)`. Its `Runner` already dispatches new modes to
  the owned runtime. This is the production blocker.
- `crates/squad-tui/src/composed.rs`: pass the measured contest cell width to
  `widgets::negotiate::render_width` instead of `render`. The existing call uses
  the narrow fallback, so wide layout remains an integration task.
- `crates/squad-core/src/router/executor.rs`: recognize `MODE_INVALID_OUTPUT` as
  `FailureClass::InvalidOutput`. Until then, the owned executor uses the already
  recognized `JUDGE_INVALID_MODE_OUTPUT` for member repairs as well.
- After dispatch integration, enable the owned protocol switch, update shared
  enable-sensitive parser/help/typeahead fixtures as needed, run production
  launcher tests and all gates, then perform one approved paid smoke with fresh
  data, the intended roster and `SQUAD_DAILY_MAX_USD=150`. Fetch credentials into
  process memory without printing them and record the ledger and report here.

Only ownership-map files and the mode's M8.P5 task entries are changed.
