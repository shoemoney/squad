# M1 — Protocol, store and bus

**Goal:** Provide the shared protocol, durable store, router and in-process bus, context-bound readers, recovery, replay, room locking, clear semantics, and one deterministic parser for human submissions used by the TUI, one-shot commands, and router.

**Exit criteria:**

- The protocol, store, bus, router, reader, recovery, lock, replay, and clear phases meet their owning contracts in **Message protocol and bus**; shared contest serde types and validators compile before runtime orchestration is implemented.
- Durable acceptance, history visibility, event cursors, restart recovery, room locking, and clear behavior are verified by the tests assigned to those phases.
- The six-crate dependency layout keeps parser code in `squad-protocol` and executors in `squad-agents`; parser checks require no store, runtime, or TUI-framework dependencies.
- `squad-protocol` exposes the AST, parser, contextual validation helpers, and exact validation strings without persistence, runtime, or TUI-framework dependencies.
- Table-driven cases and `insta` snapshots verify every grammar example and validation-precedence rule.
- A `proptest` parse/format/parse round-trip preserves semantic AST values under unchanged context and metadata.
- Recipient normalization covers configurable rosters, exclusions, reserved roles, and explicit stored recipient lists.
- Assignments work throughout conversational input, use the 1 to 20 `ROUNDS` range and 1 to 10 `GRILL_ROUNDS` range, and rejected input produces no variable updates.
- Default-dependent cases use `ROUNDS=10` and `GRILL_ROUNDS=2`; supplied stored values and staged assignments take precedence as specified.
- Quote metadata, standalone judging, case-follow-up requests, fresh-topic debates, optional direct-debate judging, and every v1 slash command produce the specified AST.
- UTF-8 byte spans, UTF-8 byte limits, serialized field names, update arrays, absent optional slash IDs, and a plain Rust `Result` match this contract.
- M1.P2 checks run against supplied snapshots and record fixtures without requiring model calls or an implemented contest engine.
- Tournament, direct grill, question-first contest, and pick AST variants match every decision-table row and exact validation string.
- Pending picks preserve ordinary-message handling, reject incorrect picks without side effects, and map valid picks to the canonical protocol request.
- Export parsing accepts room, debate, grill, and tournament scopes.
- Census, crosscheck, survey, and probe AST variants preserve their resolved participants, source choices, arguments, metadata, and exact errors without implementing engine moves.
- `CENSUS_PASSES` defaults to 2, accepts 1 to 5, expands using staged values, freezes at admission, and participates in atomic multi-variable rejection.
- Census and crosscheck source fixtures cover member replies, grill plan versions, pasted plans, latest-reply shortcuts, invalid selections, and authoritative revalidation.
- Crosscheck argument fixtures distinguish pasted plans from optional goal-and-constraints briefs without model inference.
- Survey fixtures distinguish new questions from reuse of one stored parallel-answer set; probe fixtures cover four-member minimums, accepted odd selections, and stable sit-out ordering.
- `/set`, `/export`, usage errors, help examples, snapshots, and round-trip generation include all three variables and all seven modes.
- Occupied-room errors accept every active mode kind, and no new mode changes existing debate, tournament, grill, or pending-pick behavior.

## Phase M1.P1 — Envelopes and ordering

- **M1.P1.L1 — Define protocol types**
  - [ ] M1.P1.L1.T1 — Define the exhaustive envelope registry and two-pass schema and serde validation in crate `squad-protocol`, file `crates/squad-protocol/src/envelopes.rs`.
  - [ ] M1.P1.L1.T2 — Define and serialize the complete `ErrorCode` enum in crate `squad-protocol`, file `crates/squad-protocol/src/errors.rs`.
  - [ ] M1.P1.L1.T3 — Add valid and invalid fixtures for every envelope kind, referencing Contests-owned contracts, in crate `squad-protocol`, file `crates/squad-protocol/tests/envelopes.rs`.
  - [ ] M1.P1.L1.T4 — Verify quote fields, actor permissions, presence fields, nullable usage and finish reasons in crate `squad-protocol`, file `crates/squad-protocol/tests/envelopes.rs`.
  - [ ] M1.P1.L1.T5 — Add `debate.round_score`, scored and unscored payload validation, exact debater keys, score and reason bounds, and router-owned tally fields in crate `squad-protocol`, file `crates/squad-protocol/src/envelopes.rs`.
  - [ ] M1.P1.L1.T6 — Test scored ties, malformed scores, unscored payloads and Contests-owned verdict fields `roundTally` and `tallyOverrideReason` in crate `squad-protocol`, file `crates/squad-protocol/tests/envelopes.rs`.
  - [ ] M1.P1.L1.T7 — Implement Contests-owned debate statuses, `DebateState`, `DebateSetup`, every `DebateTransition` variant, `DebateAdjudicationBundle` and `Verdict` in crate `squad-protocol`, file `crates/squad-protocol/src/debate.rs`, moving the serde work from M5.P1.L1.T1.
  - [ ] M1.P1.L1.T8 — Implement the canonical `DebatePreflight`, estimate and confirmation-validation types with debaters, rounds, judge selection, opening-round flag and model-call count in crate `squad-protocol`, file `crates/squad-protocol/src/debate.rs`, moving M5.P1.L1.T3.
  - [ ] M1.P1.L1.T9 — Implement required nullable `tallyOverrideReason` and committed router-computed `roundTally` contracts in crate `squad-protocol`, file `crates/squad-protocol/src/debate.rs`, moving M5.P1.L1.T5.
  - [ ] M1.P1.L1.T10 — Implement Contests-owned verdict validation with `schemars` and `jsonschema`, integer ranges, nick membership, criterion coverage, application-computed totals and winner consistency in crate `squad-protocol`, file `crates/squad-protocol/src/verdict.rs`, moving the validator from M5.P3.L3.T1.
  - [ ] M1.P1.L1.T11 — Implement the canonical round-score validator with exact nick coverage, integer scores from 1 to 10 and reasons at most 200 characters in crate `squad-protocol`, file `crates/squad-protocol/src/verdict.rs`, moving the validator from M5.P3.L3.T4.
  - [ ] M1.P1.L1.T12 — Implement final tally and override-reason validation, including rejection of model-produced authoritative totals and missing required nullable fields, in crate `squad-protocol`, file `crates/squad-protocol/src/verdict.rs` and file `crates/squad-protocol/tests/verdict.rs`, moving the validator from M5.P3.L3.T5.

- **M1.P1.L2 — Define durable identities**
  - [ ] M1.P1.L2.T1 — Define `Work`, `Cursor`, `HistoryPosition` and commit fields in crate `squad-protocol`, file `crates/squad-protocol/src/work.rs` and file `crates/squad-protocol/src/cursor.rs`.
  - [ ] M1.P1.L2.T2 — Test integer cursor comparison, decimal serialization and stable generation identities in crate `squad-protocol`, file `crates/squad-protocol/tests/identities.rs`.
  - [ ] M1.P1.L2.T3 — Test distinct event sequences, complete commit groups and delayed history positions in crate `squad-store`, file `crates/squad-store/tests/ordering.rs`.
  - [ ] M1.P1.L2.T4 — Validate member and case context identifiers and conditional work fields in crate `squad-protocol`, file `crates/squad-protocol/tests/work.rs`.
  - [ ] M1.P1.L2.T5 — Define `judge.round` work and validate its judge target, case and debate identities, numbered round, prompt reference, 512-token visible ceiling and separate low reasoning budget in crate `squad-protocol`, file `crates/squad-protocol/src/work.rs` and file `crates/squad-protocol/tests/work.rs`.
  - [ ] M1.P1.L2.T6 — Define `ContestId`, the `DebateId` alias, contest associations and `judge.tournament`, `grill.question`, `grill.answer` and `grill.rebuild` work validation in crate `squad-protocol`, file `crates/squad-protocol/src/work.rs` and file `crates/squad-protocol/tests/work.rs`.
  - [ ] M1.P1.L2.T7 — Define visible ceilings, reasoning budgets, judge deadlines, plan expiry, lease timings and channel capacities as compile-time constants in crate `squad-protocol`, file `crates/squad-protocol/src/constants.rs`.

- **M1.P1.L3 — Define the store and in-process interfaces**
  - [ ] M1.P1.L3.T1 — Implement the complete schema and versioned migration in crate `squad-store`, file `crates/squad-store/src/schema.rs`.
  - [ ] M1.P1.L3.T2 — Define `Request`, `RequestResult` and `FeedItem` in crate `squad-protocol`, file `crates/squad-protocol/src/requests.rs`, and the object-safe `Bus` in crate `squad-core`, file `crates/squad-core/src/bus/mod.rs`.
  - [ ] M1.P1.L3.T3 — Define scoped `HistoryReader`, `WorkReader` and `ExecutorSubmit` traits in crate `squad-protocol`, file `crates/squad-protocol/src/capabilities.rs`, and their store implementations in crate `squad-store`, file `crates/squad-store/src/readers.rs`.
  - [ ] M1.P1.L3.T4 — Define `AgentContext` in crate `squad-agents`, file `crates/squad-agents/src/context.rs`, and construct its scoped readers and submission capabilities in crate `squad-core`, file `crates/squad-core/src/runtime/contexts.rs`.
  - [ ] M1.P1.L3.T5 — Add the `round_scores` table, durable event and work kinds, judge-context constraints and unique debate-round work allocation to the migration in crate `squad-store`, file `crates/squad-store/src/schema.rs`.
  - [ ] M1.P1.L3.T6 — Add the model catalog cache, member and judge model roles, nullable roster order and provenance storage contracts in crate `squad-store`, file `crates/squad-store/src/schema.rs` and file `crates/squad-store/src/models.rs`.
  - [ ] M1.P1.L3.T7 — Add compile-fail tests proving executors cannot import the store or SQLite, construct alternate-context readers, subscribe to the feed or inspect blind label maps in crate `squad-agents`, file `crates/squad-agents/tests/isolation.rs`.
  - [ ] M1.P1.L3.T8 — Define case-bound attempt repair access and verify its trait methods remain object-safe and context-bound in crate `squad-protocol`, file `crates/squad-protocol/src/capabilities.rs`, and crate `squad-store`, file `crates/squad-store/tests/readers.rs`.

- **M1.P1.L4 — Define shared contest and daily spending contracts**
  - [ ] M1.P1.L4.T1 — Implement Contests-owned collection, pick, tournament and grill payload types and their exhaustive envelope registrations in crate `squad-protocol`, file `crates/squad-protocol/src/contest.rs`, file `crates/squad-protocol/src/tournament.rs`, file `crates/squad-protocol/src/grill.rs` and file `crates/squad-protocol/src/envelopes.rs`.
  - [ ] M1.P1.L4.T2 — Add `ContestPick`, shared confirmation and cancellation requests, `ContestPreflight`, `GRILL_ROUNDS` and daily spending projection fields in crate `squad-protocol`, file `crates/squad-protocol/src/requests.rs` and file `crates/squad-protocol/src/envelopes.rs`.
  - [ ] M1.P1.L4.T3 — Add valid and invalid fixtures for pending picks, strict tournament label permutations, individual results, grill participant references, plan versions and daily-max errors in crate `squad-protocol`, file `crates/squad-protocol/tests/contests.rs`.
  - [ ] M1.P1.L4.T4 — Implement `contests`, `contest_events`, collection and pick tables, tournament labels and results, grill plans and turns, repair records, trophies and `spend_daily` in crate `squad-store`, file `crates/squad-store/src/schema.rs`.
  - [ ] M1.P1.L4.T5 — Validate contest-kind references, one active claim, local-day keys, monetary conversion and visible token ceilings independently of provider reasoning allowances in crate `squad-store`, file `crates/squad-store/tests/schema.rs`, and crate `squad-protocol`, file `crates/squad-protocol/tests/work.rs`.

- **M1.P1.L5 — Define shared moves and four-mode protocol contracts**
  - [ ] M1.P1.L5.T1 — Implement the Contests-owned shared-move and step contracts, `StepId`, seven-kind `ContestKind` and four new `ContestPreflight` variants in crate `squad-protocol`, file `crates/squad-protocol/src/contest.rs`.
  - [ ] M1.P1.L5.T2 — Implement the census setup, state, plan, hunt, ledger, rating, report and case-material types with exhaustive envelope registration in crate `squad-protocol`, file `crates/squad-protocol/src/census.rs` and file `crates/squad-protocol/src/envelopes.rs`.
  - [ ] M1.P1.L5.T3 — Implement the crosscheck setup, state, goal, inventory, grid, review, audit, ruling, acceptance, report and case-material types in crate `squad-protocol`, file `crates/squad-protocol/src/crosscheck.rs` and file `crates/squad-protocol/src/envelopes.rs`.
  - [ ] M1.P1.L5.T4 — Implement the survey setup, state, claims, vote, selection, revote, report and case-material types in crate `squad-protocol`, file `crates/squad-protocol/src/survey.rs` and file `crates/squad-protocol/src/envelopes.rs`.
  - [ ] M1.P1.L5.T5 — Implement the probe setup, state, explanation, check, prediction, report and case-material types in crate `squad-protocol`, file `crates/squad-protocol/src/probe.rs` and file `crates/squad-protocol/src/envelopes.rs`.
  - [ ] M1.P1.L5.T6 — Register all new work kinds, step-bound context validation, judge case requirements, prompt references, schema references and medium-effort 4,096-token judge settings in crate `squad-protocol`, file `crates/squad-protocol/src/work.rs`.
  - [ ] M1.P1.L5.T7 — Add the new ErrorCode variants, `CENSUS_PASSES`, `CensusContinue`, `CrosscheckAccept` and step-associated deltas in crate `squad-protocol`, file `crates/squad-protocol/src/errors.rs`, file `crates/squad-protocol/src/requests.rs` and file `crates/squad-protocol/src/envelopes.rs`.
  - [ ] M1.P1.L5.T8 — Add the step, assignment, artifact, restricted-map, census, crosscheck, survey and probe tables and extended CHECK constraints through a versioned migration in crate `squad-store`, file `crates/squad-store/src/schema.rs`.
  - [ ] M1.P1.L5.T9 — Add valid and invalid fixtures for artifact references, assigned-item coverage, anonymity, missing work, required nullable fields and model attempts to supply authoritative tallies or risk acceptance in crate `squad-protocol`, file `crates/squad-protocol/tests/modes.rs`.
  - [ ] M1.P1.L5.T10 — Verify the migration preserves existing debate, tournament and grill records and readers while accepting all new durable kinds in crate `squad-store`, file `crates/squad-store/tests/schema.rs`.

## Phase M1.P2 — Parser

- **M1.P2.L1 — Implement lexical rules**
  - [ ] M1.P2.L1.T1 — Add scanning with UTF-8 byte spans in crate `squad-protocol`, file `crates/squad-protocol/src/parser/lexer.rs`.
  - [ ] M1.P2.L1.T2 — Verify boundaries, email text, escapes, inline code, fenced code, and Unicode spans with table-driven cases in crate `squad-protocol`, file `crates/squad-protocol/src/parser/lexer_tests.rs`.
  - [ ] M1.P2.L1.T3 — Test contiguous `@all` exclusions and assignments at every content position in crate `squad-protocol`, file `crates/squad-protocol/src/parser/lexer_tests.rs`.

- **M1.P2.L2 — Implement AST and validation strings**
  - [ ] M1.P2.L2.T1 — Define the Rust AST and serde representations using shared identifiers and `ErrorCode`, with a plain Rust `Result`, in crate `squad-protocol`, file `crates/squad-protocol/src/parser/types.rs`.
  - [ ] M1.P2.L2.T2 — Add the exact validation table and placeholder substitution, including the 1 to 20 `ROUNDS` error, in crate `squad-protocol`, file `crates/squad-protocol/src/parser/errors.rs`.
  - [ ] M1.P2.L2.T3 — Implement message, standalone judge, debate, and assignment classification in crate `squad-protocol`, file `crates/squad-protocol/src/parser/parser.rs`.
  - [ ] M1.P2.L2.T4 — Implement roster expansion, exclusion validation, and recipient deduplication in crate `squad-protocol`, file `crates/squad-protocol/src/parser/parser.rs`.
  - [ ] M1.P2.L2.T5 — Implement staged assignments, effective variable snapshots, and instruction extraction in crate `squad-protocol`, file `crates/squad-protocol/src/parser/parser.rs`.
  - [ ] M1.P2.L2.T6 — Preserve submission quotes and direct-debate-only solution selections in crate `squad-protocol`, file `crates/squad-protocol/src/parser/parser.rs`.
  - [ ] M1.P2.L2.T7 — Implement the complete v1 slash-command argument table, including `/set ROUNDS <1-20>`, in crate `squad-protocol`, file `crates/squad-protocol/src/parser/parser.rs`.

- **M1.P2.L3 — Verify acceptance boundaries**
  - [ ] M1.P2.L3.T1 — Encode every example and error-precedence rule as table-driven cases with `insta` snapshots in crate `squad-protocol`, file `crates/squad-protocol/src/parser/parser_tests.rs`.
  - [ ] M1.P2.L3.T2 — Add pure reference and source validation against supplied record summaries in crate `squad-protocol`, file `crates/squad-protocol/src/parser/validate_context.rs`.
  - [ ] M1.P2.L3.T3 — Test the legal standalone and debate `@judge` contexts, implied tournament judging, prohibited grill judging, and quoted-text non-execution in crate `squad-protocol`, file `crates/squad-protocol/src/parser/validate_context_tests.rs`.
  - [ ] M1.P2.L3.T4 — Test missing inferred solutions as valid opening-round candidates and invalid explicit selections as errors in crate `squad-protocol`, file `crates/squad-protocol/src/parser/validate_context_tests.rs`.
  - [ ] M1.P2.L3.T5 — Test the active-contest error separately from busy-member admission using fixtures in crate `squad-protocol`, file `crates/squad-protocol/src/parser/validate_context_tests.rs`.
  - [ ] M1.P2.L3.T6 — Test UTF-8 byte limits, attachment limits, invalid assignments, reserved commands, default 10, and accepted range endpoints 1 and 20 in crate `squad-protocol`, file `crates/squad-protocol/src/parser/parser_tests.rs`.
  - [ ] M1.P2.L3.T7 — Verify shared-consumer fixture results and expose the parser API in crate `squad-protocol`, files `crates/squad-protocol/src/parser/parser_tests.rs` and `crates/squad-protocol/src/parser/mod.rs`.
  - [ ] M1.P2.L3.T8 — Add the `proptest` parse/format/parse round-trip with unchanged context and metadata in crate `squad-protocol`, file `crates/squad-protocol/src/parser/round_trip_tests.rs`.
  - [ ] M1.P2.L3.T9 — Snapshot serde representations for every AST variant, absent optional slash IDs, update arrays, and plain Rust `Result` success and error values in crate `squad-protocol`, file `crates/squad-protocol/src/parser/parser_tests.rs`.
  - [ ] M1.P2.L3.T10 — Test UTF-8 byte spans for multibyte characters, removed tokens, whole-input metadata errors, and character-boundary endpoints in crate `squad-protocol`, file `crates/squad-protocol/src/parser/lexer_tests.rs`.
  - [ ] M1.P2.L3.T11 — Test both variable defaults, range endpoints, independent final-assignment ordering, mixed-variable expansion, and atomic rejection of staged updates in crate `squad-protocol`, file `crates/squad-protocol/src/parser/parser_tests.rs`.
  - [ ] M1.P2.L3.T12 — Encode every contest decision-table row, duplicate-member case, required-question case, and tournament minimum as fixtures in crate `squad-protocol`, file `crates/squad-protocol/src/parser/parser_tests.rs`.
  - [ ] M1.P2.L3.T13 — Test direct-grill prior-plan and quoted-plan validation, missing plans, ambiguous selections, and restricted griller lists in crate `squad-protocol`, file `crates/squad-protocol/src/parser/validate_context_tests.rs`.
  - [ ] M1.P2.L3.T14 — Test pending-pick cardinality, eligibility, duplicate mentions, exact list formatting, unchanged pending state on rejection, and stale authoritative snapshots in crate `squad-protocol`, file `crates/squad-protocol/src/parser/validate_context_tests.rs`.
  - [ ] M1.P2.L3.T15 — Test ordinary messages during pending picks, mentions-only input without a pending pick, and protected, quoted, or expanded text remaining non-executable in crate `squad-protocol`, file `crates/squad-protocol/src/parser/parser_tests.rs`.
  - [ ] M1.P2.L3.T16 — Snapshot contest-busy, insufficient-answer, and formatted daily-max strings against the canonical protocol code assignments in crate `squad-protocol`, file `crates/squad-protocol/src/parser/validate_context_tests.rs`.

- **M1.P2.L4 — Add contest grammar, variables, and picks**
  - [ ] M1.P2.L4.T1 — Add `GRILL_ROUNDS`, independent staged update arrays, tournament and grill export scopes, and the new contest AST variants in crate `squad-protocol`, file `crates/squad-protocol/src/parser/types.rs`.
  - [ ] M1.P2.L4.T2 — Implement `$tournament` entrant resolution, prompt extraction, minimum count, and implied judging in crate `squad-protocol`, file `crates/squad-protocol/src/parser/parser.rs`.
  - [ ] M1.P2.L4.T3 — Implement direct `$grill` leader and griller resolution, focus extraction, quoted-plan selection, and effective grill rounds in crate `squad-protocol`, files `crates/squad-protocol/src/parser/parser.rs` and `crates/squad-protocol/src/parser/validate_context.rs`.
  - [ ] M1.P2.L4.T4 — Implement the full direct-versus-question-first decision table and required-question validation in crate `squad-protocol`, file `crates/squad-protocol/src/parser/parser.rs`.
  - [ ] M1.P2.L4.T5 — Add supplied pending-pick context, mentions-only classification, distinct-count checks, eligibility checks, and binding to the shared `ContestPick` request contract in crate `squad-protocol`, files `crates/squad-protocol/src/parser/types.rs` and `crates/squad-protocol/src/parser/validate_context.rs`.
  - [ ] M1.P2.L4.T6 — Add all tournament, grill, question-first, pick, active-contest, insufficient-answer, and daily-max strings with exact placeholder formatting in crate `squad-protocol`, file `crates/squad-protocol/src/parser/errors.rs`.
  - [ ] M1.P2.L4.T7 — Extend `/set`, `/export`, and help examples for both variables, all contest modes, and pending picks in crate `squad-protocol`, file `crates/squad-protocol/src/parser/parser.rs`.
  - [ ] M1.P2.L4.T8 — Extend generated round-trip submissions to both variables, tournament, direct grill, question-first forms, and picks under unchanged supplied context in crate `squad-protocol`, file `crates/squad-protocol/src/parser/round_trip_tests.rs`.

- **M1.P2.L5 — Add census, crosscheck, survey, and probe grammar**
  - [ ] M1.P2.L5.T1 — Add the four executable verbs, AST variants, `PlanSource`, and `SurveySource` in crate `squad-protocol`, files `crates/squad-protocol/src/parser/types.rs` and `crates/squad-protocol/src/parser/parser.rs`.
  - [ ] M1.P2.L5.T2 — Add `CENSUS_PASSES`, its default 2 and range 1 to 5, effective-value expansion, final-assignment ordering, and atomic staged updates in crate `squad-protocol`, files `crates/squad-protocol/src/parser/types.rs` and `crates/squad-protocol/src/parser/parser.rs`.
  - [ ] M1.P2.L5.T3 — Implement census and crosscheck participant resolution, sole-member source shortcuts, minimum counts, quoted-plan validation, pasted plans, and latest-reply source validation against supplied summaries in crate `squad-protocol`, files `crates/squad-protocol/src/parser/parser.rs` and `crates/squad-protocol/src/parser/validate_context.rs`.
  - [ ] M1.P2.L5.T4 — Implement census source-conflict checks and crosscheck goal-and-constraints extraction, including the first unprotected separator line and empty-brief normalization, in crate `squad-protocol`, file `crates/squad-protocol/src/parser/parser.rs`.
  - [ ] M1.P2.L5.T5 — Implement survey new-question and latest-parallel-answer forms, with contextual validation against one supplied answer-batch summary, in crate `squad-protocol`, files `crates/squad-protocol/src/parser/parser.rs` and `crates/squad-protocol/src/parser/validate_context.rs`.
  - [ ] M1.P2.L5.T6 — Implement probe brief extraction, the four-member minimum, accepted odd selections, and preservation of resolved member order in crate `squad-protocol`, file `crates/squad-protocol/src/parser/parser.rs`.
  - [ ] M1.P2.L5.T7 — Add every new exact error string, usage string, active-mode placeholder, and canonical protocol-code mapping without defining a second enum in crate `squad-protocol`, file `crates/squad-protocol/src/parser/errors.rs`.
  - [ ] M1.P2.L5.T8 — Extend `/set`, `/export`, implied `@judge` handling, and help examples for all three variables and all seven modes in crate `squad-protocol`, files `crates/squad-protocol/src/parser/types.rs` and `crates/squad-protocol/src/parser/parser.rs`.
  - [ ] M1.P2.L5.T9 — Snapshot every new input example, source form, participant boundary, exact error, UTF-8 span, AST variant, and optional brief representation in crate `squad-protocol`, files `crates/squad-protocol/src/parser/parser_tests.rs` and `crates/squad-protocol/src/parser/validate_context_tests.rs`.
  - [ ] M1.P2.L5.T10 — Test all three variable defaults and range endpoints, invalid decimal forms, expansion before assignments, independent final-assignment order, and rejection of all staged updates when any variable is invalid in crate `squad-protocol`, file `crates/squad-protocol/src/parser/parser_tests.rs`.
  - [ ] M1.P2.L5.T11 — Test quoted member replies, stored grill plan versions, ambiguous and invalid sources, stale source summaries, and survey batches that must not combine unrelated questions in crate `squad-protocol`, file `crates/squad-protocol/src/parser/validate_context_tests.rs`.
  - [ ] M1.P2.L5.T12 — Test separator recognition before expansion, protected separator lines, first-separator behavior, empty arguments, and literal syntax inside resolved sources in crate `squad-protocol`, files `crates/squad-protocol/src/parser/parser_tests.rs` and `crates/squad-protocol/src/parser/lexer_tests.rs`.
  - [ ] M1.P2.L5.T13 — Test new modes against every occupied-room kind, pending picks, multiple commands, solution-selection restrictions, implied judging, and unchanged existing contest classification in crate `squad-protocol`, files `crates/squad-protocol/src/parser/parser_tests.rs` and `crates/squad-protocol/src/parser/validate_context_tests.rs`.
  - [ ] M1.P2.L5.T14 — Extend generated parse/format/parse submissions to all three variables, all four new modes, plan-source forms, optional crosscheck briefs, survey reuse, and odd probe selections under unchanged supplied context in crate `squad-protocol`, file `crates/squad-protocol/src/parser/round_trip_tests.rs`.

## Phase M1.P3 — Router and private histories

- **M1.P3.L1 — Validate and resolve submissions**
  - [ ] M1.P3.L1.T1 — Implement producer authorization and startup-roster recipient resolution in crate `squad-core`, file `crates/squad-core/src/router/authorization.rs`.
  - [ ] M1.P3.L1.T2 — Integrate the M1.P2 parser and verify its error-code and exact-string mappings in crate `squad-core`, file `crates/squad-core/tests/router_validation.rs`.
  - [ ] M1.P3.L1.T3 — Resolve and freeze attributed quotes, reject invalid references and enforce debate-only `replyTo` in crate `squad-core`, file `crates/squad-core/src/router/references.rs`.
  - [ ] M1.P3.L1.T4 — Verify `@all` exclusions and explicit stored recipients for rosters of 1 through 12 members in crate `squad-core`, file `crates/squad-core/tests/router_recipients.rs`.
  - [ ] M1.P3.L1.T5 — Verify that direct debate and standalone judge submissions allocate no ordinary reply work, while question-first collection allocates independent ordinary replies, in crate `squad-core`, file `crates/squad-core/tests/router_work.rs`.
  - [ ] M1.P3.L1.T6 — Commit parser-produced inline variable changes with the associated operation in crate `squad-core`, file `crates/squad-core/src/router/variables.rs`.
  - [ ] M1.P3.L1.T7 — Authorize round-score submissions only for their fenced `judge.round` work and derive `me` as the sole recipient in crate `squad-core`, file `crates/squad-core/src/router/authorization.rs`.
  - [ ] M1.P3.L1.T8 — Reject quote or reference projections that would expose a debate’s round scores to member histories before its verdict in crate `squad-core`, file `crates/squad-core/src/router/references.rs` and file `crates/squad-core/tests/router_visibility.rs`.
  - [ ] M1.P3.L1.T9 — Map shared contest conflicts, invalid picks, insufficient answers, tournament entrant minimums, missing grill plans and daily-max refusals to their canonical codes and strings in crate `squad-core`, file `crates/squad-core/tests/router_validation.rs`.

- **M1.P3.L2 — Commit visible events**
  - [ ] M1.P3.L2.T1 — Implement atomic event, history, work, receipt and state writes on the dedicated writer thread in crate `squad-store`, file `crates/squad-store/src/commit.rs`.
  - [ ] M1.P3.L2.T2 — Test identical retries and conflicting payloads before mutable source resolution in crate `squad-core`, file `crates/squad-core/tests/router_idempotency.rs`.
  - [ ] M1.P3.L2.T3 — Test ordinary visibility destinations and deliberate quote sharing in crate `squad-core`, file `crates/squad-core/tests/router_visibility.rs`.
  - [ ] M1.P3.L2.T4 — Preserve source event IDs and sequences on delayed projections in crate `squad-store`, file `crates/squad-store/src/history.rs`.
  - [ ] M1.P3.L2.T5 — Verify independent per-recipient admission errors without dropping accepted work in crate `squad-core`, file `crates/squad-core/tests/router_work.rs`.
  - [ ] M1.P3.L2.T6 — Commit scored and unscored round events, associations, judge-history projections, tally arithmetic and terminal work atomically in crate `squad-store`, file `crates/squad-store/src/round_scores.rs`.
  - [ ] M1.P3.L2.T7 — Test duplicate, late and out-of-order round-score commits, unscored rounds, rollback and tally arithmetic in crate `squad-store`, file `crates/squad-store/tests/round_scores.rs`.
  - [ ] M1.P3.L2.T8 — Prove no debater context contains a round score before the verdict and both receive round outcomes with the verdict announcement in crate `squad-core`, file `crates/squad-core/tests/router_visibility.rs`.

- **M1.P3.L3 — Notify accepted traffic**
  - [ ] M1.P3.L3.T1 — Implement capacity-256 ingress, oneshot results, capacity-4,096 broadcast feed delivery and capacity-1 `try_send` wake-up mailboxes in crate `squad-core`, file `crates/squad-core/src/bus/in_process.rs`.
  - [ ] M1.P3.L3.T2 — Implement fenced delta forwarding and in-memory presence updates in crate `squad-core`, file `crates/squad-core/src/router/ephemeral.rs`.
  - [ ] M1.P3.L3.T3 — Test duplicate chunks and obsolete streaming attempts through ingress in crate `squad-core`, file `crates/squad-core/tests/router_streaming.rs`.
  - [ ] M1.P3.L3.T4 — Persist held-work, deferred-history and execution-boundary metadata required by agent and contest scheduling in crate `squad-store`, file `crates/squad-store/src/work.rs`.
  - [ ] M1.P3.L3.T5 — Verify that no result or notification indicates durable acceptance before commit in crate `squad-core`, file `crates/squad-core/tests/bus_commit_boundary.rs`.
  - [ ] M1.P3.L3.T6 — Persist judge-case turn projections and pending round-scoring allocation metadata at numbered-round barriers, supporting idempotent allocation after commit in crate `squad-store`, file `crates/squad-store/src/work.rs` and file `crates/squad-store/src/history.rs`.
  - [ ] M1.P3.L3.T7 — Defer held ordinary-message projections until contest release and preserve source order and identities on delivery in crate `squad-store`, file `crates/squad-store/src/history.rs` and file `crates/squad-store/tests/held_history.rs`.
  - [ ] M1.P3.L3.T8 — Verify that mid-debate messages and quoted current-round content cannot enter either attempt’s frozen round context in crate `squad-core`, file `crates/squad-core/tests/router_visibility.rs`.
  - [ ] M1.P3.L3.T9 — Saturate ingress and mailboxes to prove wake-up coalescing cannot deadlock the router or lose durable work in crate `squad-core`, file `crates/squad-core/tests/bus_backpressure.rs`.

- **M1.P3.L4 — Implement controlled room mutations**
  - [ ] M1.P3.L4.T1 — Commit supersession markers and replacement reply work atomically in crate `squad-core`, file `crates/squad-core/src/router/regen.rs`.
  - [ ] M1.P3.L4.T2 — Implement case creation, scoped case-history writes and follow-up associations in crate `squad-store`, file `crates/squad-store/src/cases.rs`.
  - [ ] M1.P3.L4.T3 — Commit verdict-linked scoreboard changes once per accepted verdict in crate `squad-core`, file `crates/squad-core/src/router/verdict.rs`.
  - [ ] M1.P3.L4.T4 — Implement idempotent attempt-ledger, reservation and daily spending projection writes in crate `squad-store`, file `crates/squad-store/src/accounting.rs`.
  - [ ] M1.P3.L4.T5 — Implement `/clear` fencing, content deletion, variable reset and scoreboard reset while preserving daily accounting in crate `squad-core`, file `crates/squad-core/src/router/clear.rs`.
  - [ ] M1.P3.L4.T6 — Expose consistent room and contest read boundaries for M3 markdown export in crate `squad-store`, file `crates/squad-store/src/export.rs`.
  - [ ] M1.P3.L4.T7 — Compute final `roundTally`, validate required override justification and project per-round outcomes with the verdict announcement without changing win-loss accounting for round scores in crate `squad-core`, file `crates/squad-core/src/router/verdict.rs` and file `crates/squad-core/tests/router_verdict.rs`.
  - [ ] M1.P3.L4.T8 — Support the accumulating debate judge case and fresh explicit-judge cases containing retained round scores without duplicate source projections in crate `squad-store`, file `crates/squad-store/src/cases.rs`.
  - [ ] M1.P3.L4.T9 — Persist rejected judge output and validation errors per work attempt, expose them only through the case-bound reader and delete them with conversational content in crate `squad-store`, file `crates/squad-store/src/judge_repairs.rs` and file `crates/squad-store/tests/judge_repairs.rs`.
  - [ ] M1.P3.L4.T10 — Store all contest case IDs, prior terminal status and prompt invocation provenance without replacing sealed cases in crate `squad-store`, file `crates/squad-store/src/cases.rs` and file `crates/squad-store/tests/cases.rs`.

- **M1.P3.L5 — Commit shared contest projections**
  - [ ] M1.P3.L5.T1 — Implement transactional active-contest claims and kind-checked event associations for debate, tournament and grill in crate `squad-store`, file `crates/squad-store/src/contests.rs`.
  - [ ] M1.P3.L5.T2 — Commit question-collection outcomes, frozen eligible answers, pending picks and idempotent accepted selections in crate `squad-store`, file `crates/squad-store/src/contest_picks.rs`.
  - [ ] M1.P3.L5.T3 — Implement router-only tournament label-map storage and restricted anonymised judge projections in crate `squad-store`, file `crates/squad-store/src/tournaments.rs`, and crate `squad-core`, file `crates/squad-core/src/router/tournaments.rs`.
  - [ ] M1.P3.L5.T4 — Commit accepted rankings, individual result histories, one elimination and final placements with an idempotent trophy increment in crate `squad-store`, file `crates/squad-store/src/tournaments.rs`.
  - [ ] M1.P3.L5.T5 — Commit grill setup, questions, answers, passes and plan versions to exactly the participant histories and retain appendix references in crate `squad-store`, file `crates/squad-store/src/grills.rs`.
  - [ ] M1.P3.L5.T6 — Implement authorised grill working-context selection using the latest plan, current-round exchanges and prior-question list in crate `squad-store`, file `crates/squad-store/src/readers.rs`.

- **M1.P3.L6 — Commit daily spending**
  - [ ] M1.P3.L6.T1 — Implement shared `spend_daily` updates with deterministic micro-dollar arithmetic and per-attempt local-day attribution in crate `squad-store`, file `crates/squad-store/src/accounting.rs`.
  - [ ] M1.P3.L6.T2 — Capture actual reported costs for successful, failed and cancelled calls in the result transaction, applying reconciliation differences once in crate `squad-store`, file `crates/squad-store/src/accounting.rs`.
  - [ ] M1.P3.L6.T3 — Persist unknown-cost calls, retain unresolved reservations and emit daily projection fields for the pending segment and `~` indicator in crate `squad-core`, file `crates/squad-core/src/router/accounting.rs`.
  - [ ] M1.P3.L6.T4 — Recheck daily admission before provider start and contest reservation, reject with `DAILY_MAX_REACHED`, and allow in-flight completion and late accounting in crate `squad-core`, file `crates/squad-core/src/router/accounting.rs`.
  - [ ] M1.P3.L6.T5 — Transfer unstarted reservations across local midnight and preserve prior-day actual costs and unresolved attempts in crate `squad-store`, file `crates/squad-store/src/accounting.rs` and file `crates/squad-store/tests/daily_spend.rs`.

- **M1.P3.L7 — Commit four-mode assignments and artifacts**
  - [ ] M1.P3.L7.T1 — Extend active-contest claims, shared confirmation and stale-plan validation to the four new kinds without changing existing behavior in crate `squad-core`, file `crates/squad-core/src/router/contests.rs`.
  - [ ] M1.P3.L7.T2 — Persist frozen script steps, per-member payload references, context identities, random assignments and barrier delivery records in crate `squad-store`, file `crates/squad-store/src/contest_steps.rs`.
  - [ ] M1.P3.L7.T3 — Implement step-bound history readers that exclude ordinary member history and restricted metadata, including fresh audit contexts, in crate `squad-store`, file `crates/squad-store/src/readers.rs`.
  - [ ] M1.P3.L7.T4 — Commit census plan versions, hunt artifacts, finder sets, ledgers, ratings and reports with distinct initial-find and addition provenance in crate `squad-store`, file `crates/squad-store/src/census.rs`.
  - [ ] M1.P3.L7.T5 — Commit crosscheck inventories, grids, origin-aware cell assignments, reviews, fixed audit samples, rulings and version-bound human risk acceptances in crate `squad-store`, file `crates/squad-store/src/crosscheck.rs`.
  - [ ] M1.P3.L7.T6 — Commit survey source snapshots, claim provenance, both ballots, app-selected claims and report projections without exposing authorship or counts to voters in crate `squad-store`, file `crates/squad-store/src/survey.rs`.
  - [ ] M1.P3.L7.T7 — Commit probe role splits, sitting-out notices, blind initial artifacts, locked predictions and report references in crate `squad-store`, file `crates/squad-store/src/probe.rs`.
  - [ ] M1.P3.L7.T8 — Authorize new artifact producers against work, step, assignment and case identities; derive safe recipients and projections in crate `squad-core`, file `crates/squad-core/src/router/authorization.rs`.
  - [ ] M1.P3.L7.T9 — Persist skipped-member outcomes and recoverable next-step allocations atomically with barriers, work completion and accounting in crate `squad-store`, file `crates/squad-store/src/contest_steps.rs`.
  - [ ] M1.P3.L7.T10 — Integrate Parser’s four-mode validation strings and `CENSUS_PASSES` effects without duplicating grammar rules in crate `squad-core`, file `crates/squad-core/tests/router_validation.rs`.
  - [ ] M1.P3.L7.T11 — Expose consistent four-mode artifact and report reads for M3 export and M4 rendering without exposing raw private maps in crate `squad-store`, file `crates/squad-store/src/export.rs`.

## Phase M1.P4 — Recovery, replay and storage

- **M1.P4.L1 — Recover requests and agent work**
  - [ ] M1.P4.L1.T1 — Restore committed receipts and re-wake unfinished work according to `OpenMode`, leaving unrelated work dormant for one-shot invocations, in crate `squad-core`, file `crates/squad-core/src/router/recovery.rs`.
  - [ ] M1.P4.L1.T2 — Implement private work claims, lease fencing and terminal tombstones in crate `squad-store`, file `crates/squad-store/src/work.rs`.
  - [ ] M1.P4.L1.T3 — Test stopped workers and crashes between commit, notification and result delivery in crate `squad-core`, file `crates/squad-core/tests/recovery.rs`.
  - [ ] M1.P4.L1.T4 — Verify that recovery cannot bypass provider-attempt accounting in crate `squad-store`, file `crates/squad-store/tests/attempt_recovery.rs`.
  - [ ] M1.P4.L1.T5 — Test stale result rejection after cancellation, supersession and room clear in crate `squad-core`, file `crates/squad-core/tests/router_fencing.rs`.
  - [ ] M1.P4.L1.T6 — Implement the advisory room lock and exact conflict message in crate `squad-store`, file `crates/squad-store/src/room_lock.rs`.
  - [ ] M1.P4.L1.T7 — Recover pending barrier allocations, unfinished `judge.round` work and scored or unscored outcomes without duplicate work or tally contributions in crate `squad-core`, file `crates/squad-core/src/router/recovery.rs` and file `crates/squad-core/tests/recovery.rs`.
  - [ ] M1.P4.L1.T8 — Test round-score result fencing after stop and clear, preservation of the two-attempt limit, and final-judge admission only after round work settles in crate `squad-core`, file `crates/squad-core/tests/router_fencing.rs` and file `crates/squad-core/tests/router_work.rs`.
  - [ ] M1.P4.L1.T9 — Create and reopen the database, lock, WAL, shared-memory and auxiliary files with the required permissions in crate `squad-store`, file `crates/squad-store/src/open.rs` and file `crates/squad-store/tests/permissions.rs`.
  - [ ] M1.P4.L1.T10 — Spawn and retire case-bound judge executors for eligible unfinished work, including recovery and follow-up summons, in crate `squad-core`, file `crates/squad-core/src/runtime/contexts.rs`.
  - [ ] M1.P4.L1.T11 — Test one-shot isolation, committed `--no-wait` work remaining dormant until interactive open, and repair-record recovery without resetting attempt budgets in crate `squad-core`, file `crates/squad-core/tests/recovery.rs`.
  - [ ] M1.P4.L1.T12 — Initialise JSON file logging and propagate correlation, causation, request and work trace identities through room startup and router operations in crate `squad-cli`, file `crates/squad-cli/src/main.rs`, and crate `squad-core`, file `crates/squad-core/src/router/mod.rs`.

- **M1.P4.L2 — Expose replay primitives**
  - [ ] M1.P4.L2.T1 — Implement bounded ordered event reads, complete groups, replay floors and high-water marks for M3 in crate `squad-store`, file `crates/squad-store/src/events.rs`.
  - [ ] M1.P4.L2.T2 — Verify that broadcast lag is repaired from durable events without replaying ephemeral traffic in crate `squad-core`, file `crates/squad-core/tests/feed_recovery.rs`.
  - [ ] M1.P4.L2.T3 — Test malformed, future and cleared-room cursors with their specified errors in crate `squad-store`, file `crates/squad-store/tests/event_pages.rs`.
  - [ ] M1.P4.L2.T4 — Verify durable replay of late round-score events and state transitions sufficient to reconstruct banners without persisted banner events in crate `squad-store`, file `crates/squad-store/tests/event_pages.rs`.
  - [ ] M1.P4.L2.T5 — Verify replay of collection, persistent picks, tournament results, trophies, grill plan versions and daily spending projections without exposing private label maps in crate `squad-store`, file `crates/squad-store/tests/event_pages.rs`.

- **M1.P4.L3 — Enforce the storage lifecycle**
  - [ ] M1.P4.L3.T1 — Verify that accepted events, histories, receipts and terminal work records remain until clear in crate `squad-store`, file `crates/squad-store/tests/storage_lifecycle.rs`.
  - [ ] M1.P4.L3.T2 — Test that ordinary operation preserves referenced case material and active contest anchors in crate `squad-store`, file `crates/squad-store/tests/storage_lifecycle.rs`.
  - [ ] M1.P4.L3.T3 — Verify WAL settings and rollback or recovery across interrupted transactions in crate `squad-store`, file `crates/squad-store/tests/crash_recovery.rs`.
  - [ ] M1.P4.L3.T4 — Enforce storage admission limits while preserving completion, cleanup, reconciliation and export reads in crate `squad-store`, file `crates/squad-store/src/admission.rs`.
  - [ ] M1.P4.L3.T5 — Verify that case histories and room-lifetime receipts survive restart and follow the clear lifecycle in crate `squad-store`, file `crates/squad-store/tests/clear.rs`.
  - [ ] M1.P4.L3.T6 — Verify monotonic counters, the new replay floor, retained unresolved accounting and post-commit `VACUUM` in crate `squad-store`, file `crates/squad-store/tests/clear.rs`.
  - [ ] M1.P4.L3.T7 — Verify round-score retention across restart and foreign-key-safe deletion of round scores, case projections and round work during clear while preserving required accounting in crate `squad-store`, file `crates/squad-store/tests/storage_lifecycle.rs` and file `crates/squad-store/tests/clear.rs`.
  - [ ] M1.P4.L3.T8 — Verify clear deletes pending picks, tournament maps and results, grill plans and turns, judge repair content and trophies while retaining daily spending and unknown-cost reconciliation in crate `squad-store`, file `crates/squad-store/tests/clear.rs`.

- **M1.P4.L4 — Verify contest and daily accounting boundaries**
  - [ ] M1.P4.L4.T1 — Test competing contest claims, persistent pending picks, rejected picks preserving state, ordinary routing while pending and `pick_cancelled` retention in crate `squad-core`, file `crates/squad-core/tests/contest_storage.rs`.
  - [ ] M1.P4.L4.T2 — Prove tournament entrant histories contain no other entrant’s answers or individual results, and blind judge reads contain no entrant nick, model name or label map, in crate `squad-core`, file `crates/squad-core/tests/router_visibility.rs`, and crate `squad-agents`, file `crates/squad-agents/tests/isolation.rs`.
  - [ ] M1.P4.L4.T3 — Test duplicate ranking delivery, round replay records, final-placement projections and exactly one trophy increment in crate `squad-store`, file `crates/squad-store/tests/tournaments.rs`.
  - [ ] M1.P4.L4.T4 — Prove grill shared-floor delivery reaches exactly the participants and that working reads exclude full earlier-round answers while preserving the no-repeat question list in crate `squad-store`, file `crates/squad-store/tests/grills.rs`.
  - [ ] M1.P4.L4.T5 — Test reported costs, billed failures and cancellations, unknown costs, duplicate reconciliation, transaction rollback, local midnight and clear preserving daily totals in crate `squad-store`, file `crates/squad-store/tests/daily_spend.rs`.
  - [ ] M1.P4.L4.T6 — Test daily-max refusal before new provider calls and contest admission, in-flight completion after the cap, and admission after a new day or raised configured max in crate `squad-core`, file `crates/squad-core/tests/daily_admission.rs`.

- **M1.P4.L5 — Verify four-mode storage and visibility**
  - [ ] M1.P4.L5.T1 — Verify all seven kinds compete for one active claim and that new artifact commits preserve complete event groups and one committed effect in crate `squad-core`, file `crates/squad-core/tests/contest_storage.rs`.
  - [ ] M1.P4.L5.T2 — Prove census hunters cannot read peer finds, raters cannot read finder identities or counts, and later additions cannot alter initial finder sets in crate `squad-store`, file `crates/squad-store/tests/census.rs`.
  - [ ] M1.P4.L5.T3 — Prove crosscheck inventory contexts cannot read a plan already present in ordinary history, reviewers cannot read unassigned cells or review self-origin items, and fresh auditors cannot read reviewer histories or identities in crate `squad-store`, file `crates/squad-store/tests/crosscheck.rs`.
  - [ ] M1.P4.L5.T4 — Prove survey voters cannot read whole peer answers, claim authorship, vote counts or attributed reasons, and duplicate ballots cannot change tallies in crate `squad-store`, file `crates/squad-store/tests/survey.rs`.
  - [ ] M1.P4.L5.T5 — Prove probe initial roles remain mutually blind, explainers receive only their own explanation and frozen checks for prediction, and committed predictions cannot change before reporting in crate `squad-store`, file `crates/squad-store/tests/probe.rs`.
  - [ ] M1.P4.L5.T6 — Test guessed work IDs, alternate-context access and metadata leaks across every new step-bound reader in crate `squad-store`, file `crates/squad-store/tests/readers.rs`, and crate `squad-agents`, file `crates/squad-agents/tests/isolation.rs`.
  - [ ] M1.P4.L5.T7 — Crash and recover at assignment, artifact, barrier and next-work allocation boundaries without rerolling samples, repeating tallies or losing skipped-member outcomes in crate `squad-core`, file `crates/squad-core/tests/mode_recovery.rs`.
  - [ ] M1.P4.L5.T8 — Test human-only version-bound risk acceptance, stale acceptance rejection and rulings preserving every retained crosscheck cell in crate `squad-store`, file `crates/squad-store/tests/crosscheck.rs`.
  - [ ] M1.P4.L5.T9 — Verify four-mode report and artifact replay supports consistent export and banner reconstruction without model calls, raw private maps or provisional chunks in crate `squad-store`, file `crates/squad-store/tests/event_pages.rs`.
  - [ ] M1.P4.L5.T10 — Test stop and clear fencing for every new work kind, foreign-key-safe deletion of all new conversational records, `CENSUS_PASSES` reset and retained daily accounting in crate `squad-store`, file `crates/squad-store/tests/clear.rs`, and crate `squad-core`, file `crates/squad-core/tests/router_fencing.rs`.

