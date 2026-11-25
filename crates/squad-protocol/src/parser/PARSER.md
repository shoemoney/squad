# Parser API and M1.P2 implementation notes

`parser::parse(raw, &ParseContext) -> Result<ParsedInput, ParseError>` is the shared
entry point. Context supplies the configured roster, effective room variables,
submission metadata, pending pick, record summaries, one latest parallel-answer
batch, and active contest kind. It performs no I/O or state changes.

`parser::validate_context(&parsed, &authoritative_context)` repeats roster,
reference, source, pick, and occupied-room checks and returns a fresh validated
AST. Rejected calls cannot partially change the caller's AST or snapshots.
`ParsedInput::contest_pick_request(request_id)` binds a pick to the shared
`Request::ContestPick` variant. The bus milestone will extend `requests.rs` with
the other request variants; this parser implementation does not implement a bus.

`resolve_recipients` handles lexical selectors. All nick creation and serde
validation reuse `config::Nick::new`. `SubmissionMetadata` is a separate serde
value using camelCase (`replyTo`); it is carried in `ParseContext` for parsing.

RoomVariables contains effective values: the caller applies stored-value versus
environment-seed precedence before supplying it. Defaults are 10, 2, and 2.
Record eligibility is supplied by the record owner; the parser never reads source
text, chooses execution roles, assembles cases, estimates calls or spends money.

## Specification choices

1. A single backslash protects the next sigil, backtick, or backslash as defined
   by the grammar. Decoded text is never scanned again.
2. Example IDs `d1`, `m1`, `t1`, `c1`, and `p1` are explicitly described as symbolic
   valid identifiers. Fixtures substitute deterministic UUIDv7 values. Production
   slash IDs and typed record IDs require UUIDv7, following the shared protocol.
3. The snapshot owner supplies pick eligibility in roster order; diagnostics print
   that supplied order unchanged. Fixtures use claude, gemini, grok.
4. Explicit solution selections match distinct debaters by recorded authorship,
   independent of attachment order. The spec does not bind selection array indices
   to debater indices. Missing or invalid records report the first debater still
   lacking a valid selection; duplicate-author selections cannot supply both sides.
5. Question-first `replyTo` rejection uses the specific collected-answers error
   for either mode, before the general non-debate selection prohibition. The
   examples cover question-first debate; the prose explicitly covers both modes.
6. A pending pick implies an occupied contest even if `active_contest` is omitted
   from the supplied snapshot. It does not block ordinary messages or controls.
7. Stale picks use `CONTEST_PICK_INVALID` and the shared diagnostic
   `The pending pick is no longer available.`
8. A second `$verb`, including an unknown second verb, reports the multiple-command
   error. A second `@all` reports the recipient-combination error.
9. Token removal collapses whitespace only when both sides have whitespace.
   Protected text stays byte-identical, and a fence-closing newline is retained.
10. `@judge` overrides recipient-combination errors only; other recipient errors
    compete in token order.
11. `/export room` accepts no ID: there is a single persistent room. Other export
    scopes accept an optional UUIDv7 ID.
12. Grill selector/count errors use `GRILL_PARTICIPANTS_REQUIRED`. A census source
    conflict is an argument failure and uses `INVALID_COMMAND`.
13. Survey answer batches carry answer IDs. Authoritative validation checks each
    usable answer's eligibility, authorship, direction, and shared question ID.

## Verification coverage

All 162 original example-table rows plus four review regressions have explicit AST or exact-error expectations in
`examples.json`; symbolic IDs are the only input substitutions.
Accepted insta snapshots contain every example's complete `Result`, AST,
attachment order, update array, error code, message, and span. Additional fixtures
cover lexical boundaries, protected code, byte limits, slash arguments,
precedence, all decision-table boundaries, every active contest kind, stale
sources and picks, and shared request conversion.

Three proptests cover arbitrary Unicode input, syntax-heavy fragment input, and
parse/format/parse semantic round trips. The formatter receives unchanged
context and metadata and verifies each parse's own raw input. All 43 generator
branches also run deterministically, covering all modes, source forms, picks,
variables, export scopes, and slash actions.

Only parser-needed shared identifiers, the canonical complete ErrorCode enum,
ContestKind, and the ContestPick request variant are added here. Full bus request,
envelope, store, case execution, spending admission, and contest-engine registries
belong to other milestones. No parser task requires those execution paths.
