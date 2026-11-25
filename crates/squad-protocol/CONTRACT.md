# M1 shared contract

Source of truth: `docs/prd/02-message-protocol-store-and-bus.md` and the debate declarations in section 08. All types are available from the `squad_protocol` root; modules also expose their owned declarations.

| Module | Contract |
| --- | --- |
| `identities` | Distinct UUIDv7 EventId, MessageId, WorkId, ContestId, CaseId, PlanId and StepId; UUIDv4 CellId; DebateId aliases ContestId; Actor, Recipient, Target, ContextId and nonempty Text |
| `config` | Nick remains the sole nick type; MemberNick aliases it |
| `cursor` | Positive signed SQLite integers serialized as decimal strings; Cursor and HistoryPosition compare numerically |
| `envelopes` | All 36 M1 payload kinds, producer and committed envelopes, quotes, standalone cases, operational/accounting payloads |
| `debate` | State/status, setup, all 29 transitions, preflights/estimates, adjudication bundle, round scores and verdict value types |
| `verdict` | Schema-first provider round/verdict validators, exact debater coverage, rubric arithmetic, winner/override checks and committed total checks |
| `contest` | Seven ContestKind names, generic selection, setup/state/preflight, moves, steps, assignments, artifacts and reports; debate transition normalization |
| `work` | Sole executable Work schema and M1 work kinds; executor context, frozen assignment and visible token ceiling validation |
| `requests` | Request, RequestResult, FeedItem, executor/internal operations, OpenMode and frontend AST mapping |
| `capabilities` | ExecutorKey, PromptRef, safe history/work records, grants, repairs, AgentContext, object-safe readers, renderer, submit and Bus traits |
| `constants` | Execution constants; reexports configuration-owned `limits` constants rather than duplicating their values |
| `validation` | Strict JSON Schema construction and schema-then-serde decoding |

Rust fields use snake_case; wire fields follow the PRD spelling. DebateTransition's reference fields retain its explicitly declared snake_case (`outcome_event_id`, `verdict_event_id`, `score_event_id`, `work_id`). Request enum discriminants retain Rust variant names, with camelCase fields; explicitly tagged wire enums follow their PRD tags.

Use `ProducerEnvelope::from_value` / `CommittedEnvelope::from_value` at envelope boundaries. These perform schema validation before serde and semantic checks. `validation::decode<T>` does the structural two-pass check for other contracts; run the type's semantic validator afterward where applicable. Direct serde derives are available for trusted internal round trips and do not replace boundary validation. Flattened verdicts deliberately do not combine `flatten` with `deny_unknown_fields`.

Omitted Option fields use `skip_serializing_if`. Required nullable fields remain present with null: usage, costUsd, responseModel, terminalReason, DebateState.caseId, DebateRoundScore.scores and tallyOverrideReason. Schema generation explicitly adds these required entries. Dynamic settings/data and nick-keyed maps retain their intended open, typed schemas while complete declared objects reject unknown fields.

`verdict::validate_judge_verdict` rejects model-produced weightedTotals and roundTally, then returns an application-assembled Verdict. `validate_committed_verdict` takes the canonical debaters and authoritative round tally; the router must supply these from stored state. `validate_round_response` accepts only model scores; `validate_round_score` validates the committed carrier against admitted debaters and total rounds. Ledger recomputation and result fencing belong to the transactional router.

`ExecutorKey::owns` verifies target/context syntax. `Work::validate_assignment` additionally checks the frozen assignment's target, context and work identity. Concrete store readers must still enforce ownership, current execution boundary, eligibility, case-bound repair access and the current room epoch; these traits expose no caller-selected history context. Agents receive AgentContext, never Bus or SQLite.

Generic carriers accept common structure for all seven kind names. `ContestKind::is_built()` returns true only for debate. A generic carrier is not admission authority for an unimplemented mode. No later-mode envelope/work payload variants or typed tables are compiled in M1.

The existing parser keeps its API and shared VariableName, VariableUpdate and ExportScope types. `Request::from_parsed` maps every existing parser variant, preserves the successful AST for conversational requests and retains complete atomic SetVars vectors. The later grammar worker must add Continue/Accept AST actions and their authoritative snapshot mapping; CensusContinue and CrosscheckAccept transport variants already exist.

The initial migration lives in `crates/squad-store/migrations/0001_initial.sql`; `schema::migrate` applies it transactionally once, under WAL/FULL/foreign_keys, using SQLite user_version 1. The 25 generic/core/debate tables include provenance, repairs and daily accounting, with no later-mode table groups. `models::save_model` validates member/judge cache roles. Accounting helpers convert reported amounts once and round reservations upward.

Deferred integration: real executor-bound store readers; runtime context construction; concrete Bus/router; atomic commit-group and delayed-history behavior; real-reader repair/ownership isolation and agent compile-fail tests. Those require M1.P3/P4 runtime implementations. No parser source edits were necessary for this contract.
