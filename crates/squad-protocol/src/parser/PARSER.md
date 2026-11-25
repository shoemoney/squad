# Parser API and M1.P2 implementation notes

`parser::parse(raw, &SubmissionMeta, &ParseContext) -> ParseResult` is the shared,
deterministic frontend entry point. It performs no I/O or state changes. Context
uses the protocol's roster, `BTreeMap<VariableName, u8>`, pending pick, full
`RecordSummary` values and optional `ContestState`. Missing variable entries use
defaults 10, 2 and 2; callers supply effective stored/environment values.
`SubmissionMetadata` remains an alias of `SubmissionMeta` for existing consumers.

`parser::validate_context(&parsed, &meta, &authoritative_context)` repeats reference,
source, pick and occupied-room validation. It returns a fresh AST and cannot
partially mutate the caller's snapshots. Internal `ParserSnapshot` and
`SourceSummary` are private implementation details used by the scanner and old
fixtures; they are not alternate public protocol contracts. All original parser
fixtures pass through the public entry point using a test-only snapshot adapter.

`Request::from_parsed_with_context` owns frontend AST-to-request conversion,
including waiting census and completed crosscheck binding. It delegates existing
context-free mappings to `Request::from_parsed`, which retains compatibility for
those inputs and rejects controls requiring a snapshot. There is no independent
pick-to-request helper. Assignment-only `SetVars` vectors and inline conversational
updates remain intact for the router's atomic commit. This parser never commits
variables or performs a contest transition.

`/continue` decodes one JSON string without expansion, or uses one eligible quote,
or keeps the current plan. It binds the waiting census ID and state version.
M1's generic state has no admitted-pass counter; checking the remaining pass
limit, draining work and reacquiring holds require the M5 census execution state.
`/accept` preserves the reason after argument-separating whitespace, including
internal/trailing whitespace, and binds the latest retained COMPLETE crosscheck's
plan, grid, cells and version. Context records use durable kinds such as
`user.message`, `message.final`, `grill.plan` and `crosscheck.state`. They are ordered
by numeric sequence rather than input vector position. Superseded or ineligible
sources cannot supply plans, answers or acceptance targets.

Survey reuse selects one latest eligible parallel-answer question from supplied
human-message and final-reply summaries. It never combines answers to different
questions. The router freezes the selected batch and revalidates all mutable
sources and state versions before acceptance.

## Verification

The 166 existing `examples.json` cases and their complete AST/error snapshots are
unchanged. Symbolic example identifiers are replaced by deterministic UUIDv7
values in fixtures. Export IDs now use `ContestId` directly in the AST; their JSON
representation is unchanged. Cell IDs require UUIDv4.

`validation_strings.json` and `validation_codes.json` independently freeze every
Rule's exact text and canonical ErrorCode, including the different ordinary and
survey codes for the same missing-recipient string. Busy errors select their code
from the requested mode and print the currently active kind.

Tests cover all seven modes, both pick kinds, quoting, standalone judging,
explicit solution selections, source eligibility, all eight exports, atomic
three-variable staging, Unicode spans, lexical precedence and contextual controls.
The original generated round trips use unchanged context and metadata; additional
properties preserve arbitrary literal continuation/acceptance text. Request tests
exercise the public parser boundary and shared mapping without a store or engine.

## Existing grammar decisions

- Protected text and decoded escapes are never scanned again.
- Pick eligibility diagnostics preserve the supplied roster order. Stale picks
  use `CONTEST_PICK_INVALID` and `The pending pick is no longer available.`
- Explicit solution selections bind by recorded authorship, not array position.
- Question-first solution selections use the collected-answers error for both modes.
- Pending picks occupy the contest claim while ordinary messages and controls remain legal.
- Token removal collapses whitespace only when whitespace exists on both sides.
- Standalone `@judge` overrides recipient-combination errors; other recipient errors
  compete by their token positions.
- `/export room` accepts no ID; the other scopes accept an optional UUIDv7 ID.
