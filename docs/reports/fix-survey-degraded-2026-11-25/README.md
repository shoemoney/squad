# Survey partial participation and revote validation

## Specification

`docs/prd/08-contests-debate-tournament-grill-census-crosscheck-survey-probe.md`,
“Common execution contracts” and “Survey execution”, requires failed member
steps to be skipped and named with their stage. A missing second ballot retains
the first ballot and is marked not re-voted; it is not an unchanged vote or an
observed switch. An accepted report enters `COMPLETE`. Exhausted extraction or
report judges enter `FAILED`. Changing a partial survey to `FAILED` would
contradict that distinction, so this change retains `COMPLETE` and adds explicit
application-owned completeness metadata and a `DEGRADED` warning.

All prose fields together consume `roomWords` for member steps. Survey objects
use strict JSON schemas: unknown fields are rejected, vote values are uppercase
`AGREE`, `DISAGREE`, or `UNSURE`, confidence is an integer 1–5, reasons are
nonempty single lines, and every second-ballot item needs nonempty
`changeEvidence`. Requested claim IDs must occur exactly once.

## Investigation of the original smoke

Read-only inspection of dev2's `/tmp/squad-live-2026-11-25/` found:

- `provider.jsonl` records status, model, reasoning settings and usage, without
  response content. Both revote calls returned HTTP 200 from `openai/gpt-4o-mini`
  and consumed 111 and 117 completion tokens, respectively.
- `survey-cheap.out` contains committed events and generic validation failures,
  without token deltas or rejected ballot JSON.
- The database has zero `judge_repairs` rows. Survey member failures discarded
  rejected output; only judge calls could persist repairs.
- The smoke set `SQUAD_ROOM_WORDS=20`, and its extraction had two claims. The old
  prompt said “Keep generated prose within 20 words” without explicitly saying
  that reasons and change evidence for **all** claims share that budget.

The exact historical rejected shape and failing rule cannot be recovered from
these retained artifacts. Word-budget exhaustion is a plausible explanation,
not a proven diagnosis. No new live provider calls were made to guess at it.

The former adapter erased every validation error into one generic message. It
now preserves safe schema paths/keywords, fixed semantic rules, or exact prose
word totals and limits. Neither JSON parser errors containing model text nor raw
rejected output are published. Future failures distinguish, for example,
`prose word budget exceeded: 26 > 20` from `invalid survey change evidence` or
`/votes/*/vote: enum`.

The revised prompt explicitly divides the total budget across both prose fields
and every supplied claim, permits concise phrases, requires evidence even for
unchanged votes, and names the exact casing and single-line constraints. No
validator relaxation is warranted by the strict specification. Leading/trailing
JSON whitespace, pretty printing, reordered fields, and surrounding prose-field
spaces already pass and remain accepted.

## Result and presentation

New `survey.report` and terminal `survey.state` artifacts contain
`completeness: { complete, missingAnswers, missingVotes, missingRevotes }`.
The router independently recomputes this from frozen sources and accepted
ballots before accepting the report, and computes the terminal summary again at
the report barrier. Missing first votes remain degraded even if a later ballot
fills the final map. Empty claim sets require no ballot calls and can be fully
complete.

The terminal state has a `DEGRADED` reason listing missing members and stages.
The report/export renderer prints the same warning; the CLI's existing text
output calls this renderer, and JSON includes the completeness flag. CLI exit 0
continues to mean the script completed, with a clearly flagged degraded status.
The survey widget puts the warning first even when collapsed or restored from
history. Historical report artifacts without the additive metadata are still
readable and get a degraded warning when their map shows missing participation.

Existing JSON storage persists the additional metadata without a migration.
Ballot retention, final tallies, band rules, retry limits and billing behavior
are unchanged.

## Reproduction and checks

`revote_wire_shapes_preserve_valid_whitespace_and_flag_invalid_cycles` uses the
real runtime, wiremock SSE, router and SQLite store with two claims and a 20-word
budget. Its realistic valid shape is pretty-printed JSON with reordered fields
and surrounding whitespace; each claim has reason ` Tests detect regressions. `
and evidence ` A controlled counterexample `. The full response has 12 prose
words and is accepted. Invalid shapes cover missing evidence, duplicate or
foreign claims, lowercase votes and an extra field. The budget case gives each
claim 13 prose words (26 total), proving the limit applies to the whole ballot.

Every invalid shape exhausts two member attempts, keeps the first ballots,
records no false switches, emits a degraded report and terminal state, releases
unused reservations, and retains that state through reopening. Other checks
cover missing first votes, missing source answers, empty maps, protocol flag
consistency, and collapsed/replayed TUI warnings. Updated snapshots show the
warning on historical partial reports. Only fixture credentials and loopback
provider traffic were used.

Final workspace validation on dev3 used `CARGO_BUILD_JOBS=8`:

- `RUST_TEST_THREADS=1 cargo test --workspace --no-fail-fast`: 919 passed,
  zero failed, two pre-existing ignored tests. All tests ran; none were filtered.
- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.

The default parallel workspace run hit the existing
`grill_workflows::expired_attempts_retry_then_record_timeout_without_late_acceptance`
deadline race; Grill code is outside this change and was left untouched. The
full serialized run passes that test. The survey scoped-reader fixture now
initializes its independent store before starting the live room, avoiding
schema initialization against an active ballot writer.
