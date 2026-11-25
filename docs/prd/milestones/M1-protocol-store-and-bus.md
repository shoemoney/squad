# M1 — Protocol, store and bus

**Goal:** Build the core and debate protocol, shared API types and deterministic human-input parser; provide the generic contest carriers and tables, durable store, transactional router, in-process bus, executor-bound readers, daily accounting, recovery, replay, locking and clear lifecycle needed by M2 and M3. The six other modes add their typed contracts, storage extensions and visibility tests in their own M5 phases.

**Exit criteria:**

- P1: Core messaging, quotes, standalone cases, Work, generic contest carriers and debate payloads validate and serialize under one shared type owner, with explicit nullable/omitted semantics, typed IDs, executor requests and transition submissions.
- P1: One `0001_initial` migration creates the M1 schema in `<data_dir>/squad.db`; no unbuilt-mode types, validators or tables are required to compile it.
- P1/P3: One member executor can enumerate its ordinary and step-context work through `ExecutorKey`; `HistoryReader::for_work` checks ownership and freezes reads to that work’s context. Case readers remain case-bound. `AgentContext` includes the injected renderer; compile-fail tests preserve the six-crate dependency boundary.
- P2: Table-driven cases, exact ErrorCode/string snapshots, UTF-8 byte spans and `proptest` round trips cover the full grammar and supplied reference/room snapshots without a store or contest runtime.
- P2: All seven mode ASTs, pending picks, `/continue`, `/accept`, standalone judging, quote metadata and direct-debate solution selections map to their canonical requests. Export parsing accepts room, debate, grill, tournament, census, crosscheck, survey and probe scopes.
- P2/P3: All three runtime variables use `u8`, their canonical defaults/ranges and staged expansion; assignment-only and inline updates are atomic and invalid input changes none.
- P3: The single 12-step commit pipeline provides receipt idempotency, complete groups, post-commit notification and atomic result/work/accounting effects. Stop, Retry, debate confirmation/cancellation/expiry and bus diagnostics are implemented.
- P3: Generic steps, assignments, private maps, artifacts, skipped outcomes and barrier allocations persist atomically without admitting unbuilt modes. Four-mode typed arithmetic and visibility fixtures are deferred to M5.P7–P10.
- P3/P4: Core and debate projections match Protocol’s sole visibility table; guessed foreign work IDs, ordinary-to-step history substitution and unauthorized metadata access fail against real-store readers.
- P3/P4: Daily-spend tests cover reported and unknown costs, billed failures/cancellations, duplicate reconciliation, rollback, local midnight, reservations, daily-max refusal and clear preserving the accounting ledger.
- P4: Recovery, claims, leases, fencing and the case-executor hook work with doubles and the M1 OpenMode harness, including dormant unrelated OneShot work; no actual M2 executor or M3 RoomHandle is required.
- P4: Replay preserves commit groups and handles lag, invalid/future/cleared cursors; locks and permissions are verified; clear fences old work, deletes all prior conversational events in foreign-key-safe order, resets variables and scores, preserves monotonic counters/accounting and runs VACUUM after commit.


## Phase M1.P1 — Envelopes and ordering

- **M1.P1.L1 — Define protocol types**
  - [x] M1.P1.L1.T1 — Define the M1 core/debate envelope registry, generic contest carriers and two-pass schema/serde validation; later-mode variants remain unavailable until their M5 phases, in `crates/squad-protocol/src/envelopes.rs`.
  - [x] M1.P1.L1.T2 — Define and serialize the complete `ErrorCode` enum in crate `squad-protocol`, file `crates/squad-protocol/src/errors.rs`.
  - [x] M1.P1.L1.T3 — Add valid/invalid fixtures for every M1 core and debate envelope and generic carrier in `crates/squad-protocol/tests/envelopes.rs`; each later mode owns its fixtures in M5.
  - [x] M1.P1.L1.T4 — Verify quote fields, actor permissions, presence fields, nullable usage and finish reasons in crate `squad-protocol`, file `crates/squad-protocol/tests/envelopes.rs`.
  - [x] M1.P1.L1.T5 — Add `debate.round_score`, scored and unscored payload validation, exact debater keys, score and reason bounds, and router-owned tally fields in crate `squad-protocol`, file `crates/squad-protocol/src/envelopes.rs`.
  - [x] M1.P1.L1.T6 — Test scored ties, malformed scores, unscored payloads and Contests-owned verdict fields `roundTally` and `tallyOverrideReason` in crate `squad-protocol`, file `crates/squad-protocol/tests/envelopes.rs`.
  - [x] M1.P1.L1.T7 — Implement Contests-owned debate statuses, `DebateState`, `DebateSetup`, every `DebateTransition` variant, `DebateAdjudicationBundle` and `Verdict` in crate `squad-protocol`, file `crates/squad-protocol/src/debate.rs`, moving the serde work from M5.P1.L1.T1.
  - [x] M1.P1.L1.T8 — Implement the canonical `DebatePreflight`, estimate and confirmation-validation types with debaters, rounds, judge selection, opening-round flag and model-call count in crate `squad-protocol`, file `crates/squad-protocol/src/debate.rs`, moving M5.P1.L1.T3.
  - [x] M1.P1.L1.T9 — Implement required nullable `tallyOverrideReason` and committed router-computed `roundTally` contracts in crate `squad-protocol`, file `crates/squad-protocol/src/debate.rs`, moving M5.P1.L1.T5.
  - [x] M1.P1.L1.T10 — Implement Contests-owned verdict validation with `schemars` and `jsonschema`, integer ranges, nick membership, criterion coverage, application-computed totals and winner consistency in crate `squad-protocol`, file `crates/squad-protocol/src/verdict.rs`, moving the validator from M5.P3.L3.T1.
  - [x] M1.P1.L1.T11 — Implement the canonical round-score validator with exact nick coverage, integer scores from 1 to 10 and reasons at most 200 characters in crate `squad-protocol`, file `crates/squad-protocol/src/verdict.rs`, moving the validator from M5.P3.L3.T4.
  - [x] M1.P1.L1.T12 — Implement final tally and override-reason validation, including rejection of model-produced authoritative totals and missing required nullable fields, in crate `squad-protocol`, file `crates/squad-protocol/src/verdict.rs` and file `crates/squad-protocol/tests/verdict.rs`, moving the validator from M5.P3.L3.T5.

- **M1.P1.L2 — Define durable identities**
  - [x] M1.P1.L2.T1 — Define `Work`, `Cursor`, `HistoryPosition` and commit fields in crate `squad-protocol`, file `crates/squad-protocol/src/work.rs` and file `crates/squad-protocol/src/cursor.rs`.
  - [x] M1.P1.L2.T2 — Test integer cursor comparison, decimal serialization and stable generation identities in crate `squad-protocol`, file `crates/squad-protocol/tests/identities.rs`.
  - [ ] M1.P1.L2.T3 — Test distinct event sequences, complete commit groups and delayed history positions in crate `squad-store`, file `crates/squad-store/tests/ordering.rs`.
  - [x] M1.P1.L2.T4 — Validate member and case context identifiers and conditional work fields in crate `squad-protocol`, file `crates/squad-protocol/tests/work.rs`.
  - [x] M1.P1.L2.T5 — Define `judge.round` work and validate its judge target, case and debate identities, numbered round, prompt reference, 512-token visible ceiling and separate low reasoning budget in crate `squad-protocol`, file `crates/squad-protocol/src/work.rs` and file `crates/squad-protocol/tests/work.rs`.
  - [x] M1.P1.L2.T6 — Define `ContestId`, the `DebateId` alias, generic contest associations and optional replay metadata in `crates/squad-protocol/src/work.rs`; defer mode work validation to each M5 phase.
  - [x] M1.P1.L2.T7 — Define visible ceilings, reasoning budgets, judge deadlines, plan expiry, lease timings and channel capacities as compile-time constants in crate `squad-protocol`, file `crates/squad-protocol/src/constants.rs`.

- **M1.P1.L3 — Define the store and in-process interfaces**
  - [x] M1.P1.L3.T1 — Implement the complete M1 core/debate schema in one `0001_initial` migration in `crates/squad-store/src/schema.rs`; the mode-specific extensions in 02 land in M5.
  - [x] M1.P1.L3.T2 — Define Protocol’s `Request`, `RequestResult`, `InternalRequest`, `InternalResult`, `ExecutorRequest`, `FeedItem` and `OpenMode` in `crates/squad-protocol/src/requests.rs`, and the object-safe `Bus` in `crates/squad-core/src/bus/mod.rs`.
  - [ ] M1.P1.L3.T3 — Define Protocol’s scoped readers, `ExecutorKey`, `HistoryEntry`, `WorkRecord`, `ReaderError`, `PromptRef` and `PromptRenderer` in `crates/squad-protocol/src/capabilities.rs`, with executor-bound store readers in `crates/squad-store/src/readers.rs`.
  - [ ] M1.P1.L3.T4 — Define the Protocol-owned `AgentContext` with mailbox, work/history readers, `ExecutorSubmit`, `PromptRenderer` and cancellation in `crates/squad-protocol/src/capabilities.rs`; `squad-core/src/runtime/contexts.rs` constructs all ordinary, step and case capabilities.
  - [x] M1.P1.L3.T5 — Add the `round_scores` table, durable event and work kinds, judge-context constraints and unique debate-round work allocation to the migration in crate `squad-store`, file `crates/squad-store/src/schema.rs`.
  - [x] M1.P1.L3.T6 — Add the model catalog cache, member and judge model roles, nullable roster order and provenance storage contracts in crate `squad-store`, file `crates/squad-store/src/schema.rs` and file `crates/squad-store/src/models.rs`.
  - [ ] M1.P1.L3.T7 — Add compile-fail tests proving executors cannot import the store or SQLite, construct alternate-context readers, subscribe to the feed or inspect blind label maps in crate `squad-agents`, file `crates/squad-agents/tests/isolation.rs`.
  - [ ] M1.P1.L3.T8 — Define case-bound attempt repair access and verify its trait methods remain object-safe and context-bound in crate `squad-protocol`, file `crates/squad-protocol/src/capabilities.rs`, and crate `squad-store`, file `crates/squad-store/tests/readers.rs`.

- **M1.P1.L4 — Define shared contest and daily spending contracts**
  - [x] M1.P1.L4.T1 — Implement generic selection/pick/setup/state/step/artifact/report carriers, `ContestState` and `ContestTransitionSubmission` plus debate transition normalization in `crates/squad-protocol/src/contest.rs`; mode-specific variants are added in M5.
  - [x] M1.P1.L4.T2 — Add `ContestPick`, shared confirmation/cancellation, `ContestPreflight`, `ContestPreflightResult`, `PendingPreflight` and daily spending projections in `crates/squad-protocol/src/requests.rs` and `envelopes.rs`; non-debate admission stays disabled until its M5 implementation.
  - [x] M1.P1.L4.T3 — Add core fixtures for pending picks, generic preflight/result carriers, daily-max errors and frontend `ParsedInput` to `Request` mapping including atomic `SetVars` in `crates/squad-protocol/tests/contests.rs`.
  - [x] M1.P1.L4.T4 — Implement generic contest, event-association, collection, pick, step, assignment, artifact and private-map tables, repair records, scores and `spend_daily` in the M1 initial schema; mode tables land with each mode in M5.
  - [x] M1.P1.L4.T5 — Validate contest-kind references, one active claim, local-day keys, monetary conversion and visible token ceilings independently of provider reasoning allowances in crate `squad-store`, file `crates/squad-store/tests/schema.rs`, and crate `squad-protocol`, file `crates/squad-protocol/tests/work.rs`.

- **M1.P1.L5 — Define generic shared-move contracts**
  - [x] M1.P1.L5.T1 — Implement the shared `ContestMove`, `ContestStep`, `ContestAssignment`, `StepId`, seven-kind `ContestKind` and generic `ContestPreflight` contracts in `crates/squad-protocol/src/contest.rs`; no non-debate typed payload is required in M1.
  - [x] M1.P1.L5.T6 — Validate generic step-bound context syntax and assignment ownership through `ExecutorKey`, plus core/debate work fields in `crates/squad-protocol/src/work.rs`; register each later mode’s work kinds and schemas in its M5 phase.
  - [x] M1.P1.L5.T7 — Add the new ErrorCode variants, `CENSUS_PASSES`, `CensusContinue`, `CrosscheckAccept` and step-associated deltas in crate `squad-protocol`, file `crates/squad-protocol/src/errors.rs`, file `crates/squad-protocol/src/requests.rs` and file `crates/squad-protocol/src/envelopes.rs`.
  - [x] M1.P1.L5.T8 — Add only generic step, assignment, artifact and restricted-map tables to `0001_initial` in `crates/squad-store/src/schema.rs`; defer all six modes’ typed tables and CHECK extensions to M5.


## Phase M1.P2 — Parser

- **M1.P2.L1 — Implement lexical rules**
  - [x] M1.P2.L1.T1 — Add scanning with UTF-8 byte spans in crate `squad-protocol`, file `crates/squad-protocol/src/parser/lexer.rs`.
  - [x] M1.P2.L1.T2 — Verify boundaries, email text, escapes, inline code, fenced code, and Unicode spans with table-driven cases in crate `squad-protocol`, file `crates/squad-protocol/src/parser/lexer_tests.rs`.
  - [x] M1.P2.L1.T3 — Test contiguous `@all` exclusions and assignments at every content position in crate `squad-protocol`, file `crates/squad-protocol/src/parser/lexer_tests.rs`.

- **M1.P2.L2 — Implement AST and validation strings**
  - [x] M1.P2.L2.T1 — Define the Rust AST and serde representations using shared identifiers and `ErrorCode`, with a plain Rust `Result`, in crate `squad-protocol`, file `crates/squad-protocol/src/parser/types.rs`.
  - [x] M1.P2.L2.T2 — Add the exact validation table with a Code column and placeholder substitution, including the 1 to 20 `ROUNDS` error, in crate `squad-protocol`, file `crates/squad-protocol/src/parser/errors.rs`.
  - [x] M1.P2.L2.T3 — Implement message, standalone judge, debate, and assignment classification in crate `squad-protocol`, file `crates/squad-protocol/src/parser/parser.rs`.
  - [x] M1.P2.L2.T4 — Implement roster expansion, exclusion validation, and recipient deduplication in crate `squad-protocol`, file `crates/squad-protocol/src/parser/parser.rs`.
  - [x] M1.P2.L2.T5 — Implement staged assignments, effective variable snapshots, and instruction extraction in crate `squad-protocol`, file `crates/squad-protocol/src/parser/parser.rs`.
  - [x] M1.P2.L2.T6 — Preserve submission quotes and direct-debate-only solution selections in crate `squad-protocol`, file `crates/squad-protocol/src/parser/parser.rs`.
  - [x] M1.P2.L2.T7 — Implement the complete v1 slash-command argument table, including `/set ROUNDS <1-20>`, `/continue ["<plan>"]` and `/accept <cellId…> -- <reason>`, in crate `squad-protocol`, file `crates/squad-protocol/src/parser/parser.rs`.

- **M1.P2.L3 — Verify acceptance boundaries**
  - [x] M1.P2.L3.T1 — Encode every example and error-precedence rule as table-driven cases with `insta` snapshots in crate `squad-protocol`, file `crates/squad-protocol/src/parser/parser_tests.rs`.
  - [x] M1.P2.L3.T2 — Add pure reference and source validation against supplied record summaries in crate `squad-protocol`, file `crates/squad-protocol/src/parser/validate_context.rs`.
  - [x] M1.P2.L3.T3 — Test the legal standalone and debate `@judge` contexts, implied tournament judging, prohibited grill judging, and quoted-text non-execution in crate `squad-protocol`, file `crates/squad-protocol/src/parser/validate_context_tests.rs`.
  - [x] M1.P2.L3.T4 — Test missing inferred solutions as valid opening-round candidates and invalid explicit selections as errors in crate `squad-protocol`, file `crates/squad-protocol/src/parser/validate_context_tests.rs`.
  - [x] M1.P2.L3.T5 — Test the active-contest error separately from busy-member admission using fixtures in crate `squad-protocol`, file `crates/squad-protocol/src/parser/validate_context_tests.rs`.
  - [x] M1.P2.L3.T6 — Test UTF-8 byte limits, attachment limits, invalid assignments, reserved commands, default 10, and accepted range endpoints 1 and 20 in crate `squad-protocol`, file `crates/squad-protocol/src/parser/parser_tests.rs`.
  - [x] M1.P2.L3.T7 — Declare Protocol’s `SubmissionMeta`, `ParseContext`, `RecordSummary` and `parse(raw, meta, ctx) -> ParseResult`, verify shared-consumer fixture results and expose the parser API in crate `squad-protocol`, files `crates/squad-protocol/src/parser/parser_tests.rs` and `crates/squad-protocol/src/parser/mod.rs`.
  - [x] M1.P2.L3.T8 — Add the `proptest` parse/format/parse round-trip with unchanged context and metadata in crate `squad-protocol`, file `crates/squad-protocol/src/parser/round_trip_tests.rs`.
  - [x] M1.P2.L3.T9 — Snapshot serde representations for every AST variant, absent optional slash IDs, update arrays, and plain Rust `Result` success and error values in crate `squad-protocol`, file `crates/squad-protocol/src/parser/parser_tests.rs`.
  - [x] M1.P2.L3.T10 — Test UTF-8 byte spans for multibyte characters, removed tokens, whole-input metadata errors, and character-boundary endpoints in crate `squad-protocol`, file `crates/squad-protocol/src/parser/lexer_tests.rs`.
  - [x] M1.P2.L3.T11 — Test both variable defaults, range endpoints, independent final-assignment ordering, mixed-variable expansion, and atomic rejection of staged updates in crate `squad-protocol`, file `crates/squad-protocol/src/parser/parser_tests.rs`.
  - [x] M1.P2.L3.T12 — Encode every contest decision-table row, duplicate-member case, required-question case, and tournament minimum as fixtures in crate `squad-protocol`, file `crates/squad-protocol/src/parser/parser_tests.rs`.
  - [x] M1.P2.L3.T13 — Test direct-grill prior-plan and quoted-plan validation, missing plans, ambiguous selections, and restricted griller lists in crate `squad-protocol`, file `crates/squad-protocol/src/parser/validate_context_tests.rs`.
  - [x] M1.P2.L3.T14 — Test pending-pick cardinality, eligibility, duplicate mentions, exact list formatting, unchanged pending state on rejection, and stale authoritative snapshots in crate `squad-protocol`, file `crates/squad-protocol/src/parser/validate_context_tests.rs`.
  - [x] M1.P2.L3.T15 — Test ordinary messages during pending picks, mentions-only input without a pending pick, and protected, quoted, or expanded text remaining non-executable in crate `squad-protocol`, file `crates/squad-protocol/src/parser/parser_tests.rs`.
  - [x] M1.P2.L3.T16 — Snapshot contest-busy, insufficient-answer, and formatted daily-max strings against the canonical protocol code assignments in crate `squad-protocol`, file `crates/squad-protocol/src/parser/validate_context_tests.rs`.

- **M1.P2.L4 — Add contest grammar, variables, and picks**
  - [x] M1.P2.L4.T1 — Add `GRILL_ROUNDS`, independent staged update arrays, tournament and grill export scopes, and the new contest AST variants in crate `squad-protocol`, file `crates/squad-protocol/src/parser/types.rs`.
  - [x] M1.P2.L4.T2 — Implement `$tournament` entrant resolution, prompt extraction, minimum count, and implied judging in crate `squad-protocol`, file `crates/squad-protocol/src/parser/parser.rs`.
  - [x] M1.P2.L4.T3 — Implement direct `$grill` leader and griller resolution, focus extraction, quoted-plan selection, and effective grill rounds in crate `squad-protocol`, files `crates/squad-protocol/src/parser/parser.rs` and `crates/squad-protocol/src/parser/validate_context.rs`.
  - [x] M1.P2.L4.T4 — Implement the full direct-versus-question-first decision table and required-question validation in crate `squad-protocol`, file `crates/squad-protocol/src/parser/parser.rs`.
  - [x] M1.P2.L4.T5 — Add supplied pending-pick context, mentions-only classification, distinct-count checks, eligibility checks, and binding to the shared `ContestPick` request contract in crate `squad-protocol`, files `crates/squad-protocol/src/parser/types.rs` and `crates/squad-protocol/src/parser/validate_context.rs`.
  - [x] M1.P2.L4.T6 — Add all tournament, grill, question-first, pick, active-contest, insufficient-answer, and daily-max strings with exact placeholder formatting in crate `squad-protocol`, file `crates/squad-protocol/src/parser/errors.rs`.
  - [x] M1.P2.L4.T7 — Extend `/set`, `/export`, and help examples for both variables, all contest modes, and pending picks in crate `squad-protocol`, file `crates/squad-protocol/src/parser/parser.rs`.
  - [x] M1.P2.L4.T8 — Extend generated round-trip submissions to both variables, tournament, direct grill, question-first forms, and picks under unchanged supplied context in crate `squad-protocol`, file `crates/squad-protocol/src/parser/round_trip_tests.rs`.

- **M1.P2.L5 — Add census, crosscheck, survey, and probe grammar**
  - [x] M1.P2.L5.T1 — Add the four executable verbs, AST variants, `PlanSource`, and `SurveySource` in crate `squad-protocol`, files `crates/squad-protocol/src/parser/types.rs` and `crates/squad-protocol/src/parser/parser.rs`.
  - [x] M1.P2.L5.T2 — Add `CENSUS_PASSES`, its default 2 and range 1 to 5, effective-value expansion, final-assignment ordering, and atomic staged updates in crate `squad-protocol`, files `crates/squad-protocol/src/parser/types.rs` and `crates/squad-protocol/src/parser/parser.rs`.
  - [x] M1.P2.L5.T3 — Implement census and crosscheck participant resolution, sole-member source shortcuts, minimum counts, quoted-plan validation, pasted plans, and latest-reply source validation against supplied summaries in crate `squad-protocol`, files `crates/squad-protocol/src/parser/parser.rs` and `crates/squad-protocol/src/parser/validate_context.rs`.
  - [x] M1.P2.L5.T4 — Implement census source-conflict checks and crosscheck goal-and-constraints extraction, including the first unprotected separator line and empty-brief normalization, in crate `squad-protocol`, file `crates/squad-protocol/src/parser/parser.rs`.
  - [x] M1.P2.L5.T5 — Implement survey new-question and latest-parallel-answer forms, with contextual validation against one supplied answer-batch summary, in crate `squad-protocol`, files `crates/squad-protocol/src/parser/parser.rs` and `crates/squad-protocol/src/parser/validate_context.rs`.
  - [x] M1.P2.L5.T6 — Implement probe brief extraction, the four-member minimum, accepted odd selections, and preservation of resolved member order in crate `squad-protocol`, file `crates/squad-protocol/src/parser/parser.rs`.
  - [x] M1.P2.L5.T7 — Add every new exact error string, usage string, active-mode placeholder, and canonical protocol-code mapping without defining a second enum in crate `squad-protocol`, file `crates/squad-protocol/src/parser/errors.rs`.
  - [x] M1.P2.L5.T8 — Extend `/set`, `/export`, implied `@judge` handling, and help examples for all three variables and all seven modes in crate `squad-protocol`, files `crates/squad-protocol/src/parser/types.rs` and `crates/squad-protocol/src/parser/parser.rs`.
  - [x] M1.P2.L5.T9 — Snapshot every new input example, source form, `/continue` and `/accept` form, participant boundary, exact code/error, UTF-8 span, AST variant, and optional brief representation in crate `squad-protocol`, files `crates/squad-protocol/src/parser/parser_tests.rs` and `crates/squad-protocol/src/parser/validate_context_tests.rs`.
  - [x] M1.P2.L5.T10 — Test all three variable defaults and range endpoints, invalid decimal forms, expansion before assignments, independent final-assignment order, and rejection of all staged updates when any variable is invalid in crate `squad-protocol`, file `crates/squad-protocol/src/parser/parser_tests.rs`.
  - [x] M1.P2.L5.T11 — Test quoted member replies, stored grill plan versions, ambiguous and invalid sources, stale source summaries, and survey batches that must not combine unrelated questions in crate `squad-protocol`, file `crates/squad-protocol/src/parser/validate_context_tests.rs`.
  - [x] M1.P2.L5.T12 — Test separator recognition before expansion, protected separator lines, first-separator behavior, empty arguments, and literal syntax inside resolved sources in crate `squad-protocol`, files `crates/squad-protocol/src/parser/parser_tests.rs` and `crates/squad-protocol/src/parser/lexer_tests.rs`.
  - [x] M1.P2.L5.T13 — Test new modes against every occupied-room kind, pending picks, multiple commands, solution-selection restrictions, implied judging, and unchanged existing contest classification in crate `squad-protocol`, files `crates/squad-protocol/src/parser/parser_tests.rs` and `crates/squad-protocol/src/parser/validate_context_tests.rs`.
  - [x] M1.P2.L5.T14 — Extend generated parse/format/parse submissions to all three variables, all four new modes, plan-source forms, optional crosscheck briefs, survey reuse, and odd probe selections under unchanged supplied context in crate `squad-protocol`, file `crates/squad-protocol/src/parser/round_trip_tests.rs`.

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

  - [ ] M1.P3.L4.T11 — Implement Stop handlers with fencing, NOTHING_TO_STOP, pending-pick cancellation, hold release and terminal arbitration in `crates/squad-core/src/router/stop.rs`; test with the real store.
  - [ ] M1.P3.L4.T12 — Implement ordinary Retry allocation, failed-work eligibility and idempotency in `crates/squad-core/src/router/retry.rs`.
  - [ ] M1.P3.L4.T13 — Implement debate confirm, cancel and expiry handlers with durable `debate.preflight.result`, stale-plan checks and judge-only restoration in `crates/squad-core/src/router/preflight.rs`; persist generic non-debate preflight results without admitting unbuilt modes.
  - [ ] M1.P3.L4.T14 — Implement the bus-log tap for request, commit and ExecutorKey mailbox diagnostics, preserving trace identities and default body redaction, in `crates/squad-core/src/bus/diagnostics.rs`.

- **M1.P3.L5 — Commit shared contest projections**
  - [ ] M1.P3.L5.T1 — Implement transactional active-contest claims and generic kind-checked event associations in `crates/squad-store/src/contests.rs`; M1 admits debate only.
  - [ ] M1.P3.L5.T2 — Commit question-collection outcomes, frozen eligible answers, pending picks and idempotent accepted selections in crate `squad-store`, file `crates/squad-store/src/contest_picks.rs`.

- **M1.P3.L6 — Commit daily spending**
  - [ ] M1.P3.L6.T1 — Implement shared `spend_daily` updates with deterministic micro-dollar arithmetic and per-attempt local-day attribution in crate `squad-store`, file `crates/squad-store/src/accounting.rs`.
  - [ ] M1.P3.L6.T2 — Capture actual reported costs for successful, failed and cancelled calls in the result transaction, applying reconciliation differences once in crate `squad-store`, file `crates/squad-store/src/accounting.rs`.
  - [ ] M1.P3.L6.T3 — Persist unknown-cost calls, retain unresolved reservations and emit daily projection fields for the pending segment and `~` indicator in crate `squad-core`, file `crates/squad-core/src/router/accounting.rs`.
  - [ ] M1.P3.L6.T4 — Recheck daily admission before provider start and contest reservation, reject with `DAILY_MAX_REACHED`, and allow in-flight completion and late accounting in crate `squad-core`, file `crates/squad-core/src/router/accounting.rs`.
  - [ ] M1.P3.L6.T5 — Transfer unstarted reservations across local midnight and preserve prior-day actual costs and unresolved attempts in crate `squad-store`, file `crates/squad-store/src/accounting.rs` and file `crates/squad-store/tests/daily_spend.rs`.

- **M1.P3.L7 — Commit generic assignments and artifacts**
  - [ ] M1.P3.L7.T1 — Implement generic active-contest claims, confirmation/result persistence and stale-plan checks without interpreting unbuilt mode payloads in `crates/squad-core/src/router/contests.rs`.
  - [ ] M1.P3.L7.T2 — Persist frozen script steps, per-member payload references, context identities, random assignments and barrier delivery records in crate `squad-store`, file `crates/squad-store/src/contest_steps.rs`.
  - [ ] M1.P3.L7.T3 — Implement step-bound history readers that exclude ordinary member history and restricted metadata, including fresh audit contexts, in crate `squad-store`, file `crates/squad-store/src/readers.rs`.
  - [ ] M1.P3.L7.T8 — Authorize generic artifact producers against work, step, assignment and case identity, leaving typed coverage checks to mode M5 validators, in `crates/squad-core/src/router/authorization.rs`.
  - [ ] M1.P3.L7.T9 — Persist skipped-member outcomes and recoverable next-step allocations atomically with barriers, work completion and accounting in crate `squad-store`, file `crates/squad-store/src/contest_steps.rs`.
  - [ ] M1.P3.L7.T10 — Revalidate parsed AST snapshots, staged `CENSUS_PASSES` and generic source references without duplicating grammar; reject runtime admission of unbuilt modes in `crates/squad-core/tests/router_validation.rs`.
  - [ ] M1.P3.L7.T11 — Expose consistent generic artifact/report reads without raw private maps for M3 export and M4 rendering in `crates/squad-store/src/export.rs`; typed readers land in M5.

## Phase M1.P4 — Recovery, replay and storage

- **M1.P4.L1 — Recover requests and agent work**
  - [ ] M1.P4.L1.T1 — Restore receipts and unfinished work through an `OpenMode` runtime harness, leaving unrelated work dormant in OneShot; do not depend on M3 `RoomHandle::open`, in `crates/squad-core/src/router/recovery.rs`.
  - [ ] M1.P4.L1.T2 — Implement private work claims, lease fencing and terminal tombstones in crate `squad-store`, file `crates/squad-store/src/work.rs`.
  - [ ] M1.P4.L1.T3 — Test stopped workers and crashes between commit, notification and result delivery in crate `squad-core`, file `crates/squad-core/tests/recovery.rs`.
  - [ ] M1.P4.L1.T4 — Verify that recovery cannot bypass provider-attempt accounting in crate `squad-store`, file `crates/squad-store/tests/attempt_recovery.rs`.
  - [ ] M1.P4.L1.T5 — Test stale result rejection after cancellation, supersession and room clear in crate `squad-core`, file `crates/squad-core/tests/router_fencing.rs`.
  - [ ] M1.P4.L1.T6 — Implement the advisory room lock and exact conflict message in crate `squad-store`, file `crates/squad-store/src/room_lock.rs`.
  - [ ] M1.P4.L1.T7 — Recover pending barrier allocations, unfinished `judge.round` work and scored or unscored outcomes without duplicate work or tally contributions in crate `squad-core`, file `crates/squad-core/src/router/recovery.rs` and file `crates/squad-core/tests/recovery.rs`.
  - [ ] M1.P4.L1.T8 — Test round-score result fencing after stop and clear, preservation of the two-attempt limit, and final-judge admission only after round work settles in crate `squad-core`, file `crates/squad-core/tests/router_fencing.rs` and file `crates/squad-core/tests/router_work.rs`.
  - [ ] M1.P4.L1.T9 — Create and reopen the database, lock, WAL, shared-memory and auxiliary files with the required permissions in crate `squad-store`, file `crates/squad-store/src/open.rs` and file `crates/squad-store/tests/permissions.rs`.
  - [ ] M1.P4.L1.T10 — Define the case-executor spawn/retire hook and test recovery and follow-up summons with a double in `crates/squad-core/src/runtime/contexts.rs`; M2 supplies the actual executor.
  - [ ] M1.P4.L1.T11 — Use the M1 runtime harness to test OneShot isolation, dormant committed no-wait work until Interactive recovery, and repair recovery without resetting attempt budgets in `crates/squad-core/tests/recovery.rs`.
  - [ ] M1.P4.L1.T12 — Initialise JSON file logging and propagate correlation, causation, request and work trace identities through room startup and router operations in crate `squad-cli`, file `crates/squad-cli/src/main.rs`, and crate `squad-core`, file `crates/squad-core/src/router/mod.rs`.

- **M1.P4.L2 — Expose replay primitives**
  - [ ] M1.P4.L2.T1 — Implement bounded ordered event reads, complete groups, replay floors and high-water marks for M3 in crate `squad-store`, file `crates/squad-store/src/events.rs`.
  - [ ] M1.P4.L2.T2 — Verify that broadcast lag is repaired from durable events without replaying ephemeral traffic in crate `squad-core`, file `crates/squad-core/tests/feed_recovery.rs`.
  - [ ] M1.P4.L2.T3 — Test malformed, future and cleared-room cursors with their specified errors in crate `squad-store`, file `crates/squad-store/tests/event_pages.rs`.
  - [ ] M1.P4.L2.T4 — Verify durable replay of late round-score events and state transitions sufficient to reconstruct banners without persisted banner events in crate `squad-store`, file `crates/squad-store/tests/event_pages.rs`.
  - [ ] M1.P4.L2.T5 — Verify replay of generic collection/pick/step/artifact/report and daily spending events without private-map leaks in `crates/squad-store/tests/event_pages.rs`; typed mode replay tests land in M5.

- **M1.P4.L3 — Enforce the storage lifecycle**
  - [ ] M1.P4.L3.T1 — Verify that accepted events, histories, receipts and terminal work records remain until clear in crate `squad-store`, file `crates/squad-store/tests/storage_lifecycle.rs`.
  - [ ] M1.P4.L3.T2 — Test that ordinary operation preserves referenced case material and active contest anchors in crate `squad-store`, file `crates/squad-store/tests/storage_lifecycle.rs`.
  - [ ] M1.P4.L3.T3 — Verify WAL settings and rollback or recovery across interrupted transactions in crate `squad-store`, file `crates/squad-store/tests/crash_recovery.rs`.
  - [ ] M1.P4.L3.T4 — Enforce storage admission limits while preserving completion, cleanup, reconciliation and export reads in crate `squad-store`, file `crates/squad-store/src/admission.rs`.
  - [ ] M1.P4.L3.T5 — Verify that case histories and room-lifetime receipts survive restart and follow the clear lifecycle in crate `squad-store`, file `crates/squad-store/tests/clear.rs`.
  - [ ] M1.P4.L3.T6 — Verify monotonic counters, the new replay floor, retained unresolved accounting and post-commit `VACUUM` in crate `squad-store`, file `crates/squad-store/tests/clear.rs`.
  - [ ] M1.P4.L3.T7 — Verify round-score retention across restart and foreign-key-safe deletion of round scores, case projections and round work during clear while preserving required accounting in crate `squad-store`, file `crates/squad-store/tests/storage_lifecycle.rs` and file `crates/squad-store/tests/clear.rs`.
  - [ ] M1.P4.L3.T8 — Verify clear deletes generic contest/step/map/pick records and judge repair content while retaining daily spending and unknown-cost reconciliation in `crates/squad-store/tests/clear.rs`; mode-table deletion tests land in M5.

- **M1.P4.L4 — Verify contest and daily accounting boundaries**
  - [ ] M1.P4.L4.T1 — Test competing contest claims, persistent pending picks, rejected picks preserving state, ordinary routing while pending and `pick_cancelled` retention in crate `squad-core`, file `crates/squad-core/tests/contest_storage.rs`.
  - [ ] M1.P4.L4.T5 — Test reported costs, billed failures and cancellations, unknown costs, duplicate reconciliation, transaction rollback, local midnight and clear preserving daily totals in crate `squad-store`, file `crates/squad-store/tests/daily_spend.rs`.
  - [ ] M1.P4.L4.T6 — Test daily-max refusal before new provider calls and contest admission, in-flight completion after the cap, and admission after a new day or raised configured max in crate `squad-core`, file `crates/squad-core/tests/daily_admission.rs`.



  - [ ] M1.P4.L4.T7 — Verify core/debate visibility and ExecutorKey ownership across ordinary, synthetic step and case work with real SQLite readers in `crates/squad-core/tests/isolation.rs`; reject guessed IDs and metadata leakage.
