//! Squad agent executors: OpenRouter client, model registry and context-bound workers.
//!
//! This crate depends on `squad-protocol` and the reader/submit traits only. It
//! never links `squad-core`, `squad-store` or `rusqlite`: the router leases
//! work, renders prompts through the opaque `PromptRenderer` capability and
//! persists everything an executor submits. An executor makes exactly one
//! provider call per durable attempt it is granted; retries, replays, tallies,
//! scheduling and contest transitions belong to the router and orchestrator.
//!
//! # Environment configuration
//!
//! Configuration is loaded once by `squad-protocol::config::SquadConfig` from
//! the single `.env` file and the process environment (the process wins). The
//! executors read only that resolved config:
//!
//! | Variable | Meaning |
//! |---|---|
//! | `OPENROUTER_API_KEY` | Required. Never logged, serialized or placed in an event. |
//! | `OPENROUTER_BASE_URL` | Optional endpoint override (tests point it at a fake server). |
//! | `SQUAD_MODEL_<NICK>` | Member roster: one concrete or alias slug per member, in order. |
//! | `SQUAD_PERSONA_<NICK>` | Optional member persona; never applied to a judge. |
//! | `SQUAD_REASONING_EFFORT_<NICK>` | Optional member effort (`low`, `medium`, `high`). |
//! | `SQUAD_JUDGE_MODEL`, `SQUAD_JUDGE_REASONING_EFFORT`, `SQUAD_JUDGE_FALLBACKS` | Final judge binding. |
//! | `SQUAD_ALIAS_CACHE_TTL_SECONDS` | How long a resolved alias stays valid. |
//! | `SQUAD_DAILY_MAX_USD`, `SQUAD_DEBATE_CONFIRM_USD` | Daily cap and preflight confirmation. |
//!
//! # Effort and output ceilings
//!
//! Ceilings count visible tokens only. A model that lists reasoning support is
//! sent `max_tokens = visible + reasoning` and an explicit `reasoning.max_tokens`
//! (low 1,024, medium 4,096, high 8,192); other models get the visible ceiling
//! alone, and only an explicitly configured effort for an unsupported member
//! is an error. `judge.round` is always low with a 512 ceiling. The census,
//! crosscheck, survey and probe judge calls are always medium with a 4,096
//! ceiling (`constants::NEW_MODE_JUDGE_REASONING_EFFORT` and
//! `NEW_MODE_JUDGE_VISIBLE_TOKENS`), whatever the judge binding says.
//!
//! # Timeouts
//!
//! The transport enforces `constants::CONNECT_TIMEOUT_MS` (15 s), first token
//! `FIRST_TOKEN_TIMEOUT_MS` (90 s), idle `IDLE_TIMEOUT_MS` (30 s) and total
//! `TOTAL_TIMEOUT_MS` (300 s) for ordinary replies. Contest work carries a
//! durable deadline class instead: debate turns 180 s, the final judge 300 s,
//! `judge.round` 120 s and tournament judging 120 s. A deadline authorized by
//! the router is the upper bound for the attempt.
//!
//! # Resolution diagnostics
//!
//! [`resolver`] pins every alias to a concrete model at startup from the
//! OpenRouter catalog (with bounded transient retries) and probes it with a
//! 16-token request, retrying once with a larger allowance after an HTTP 400.
//! Failures are stable, human-readable diagnostics that name the variable:
//! `MODEL_RESOLUTION_FAILED <variable>: alias <slug> has no validated concrete
//! target`, `MODEL_RESOLUTION_FAILED catalog: ...`, and
//! `MODEL_IDENTITY_MISMATCH <slug>: pinned ..., response ...` when a response
//! names a different model than the one pinned.
//!
//! # Accounting and uncertain-charge recovery
//!
//! [`accounting::AttemptAccounting`] snapshots one record per provider call and
//! upserts it by `(work_id, attempt)` before, during and after the stream, so a
//! crash leaves a durable, unresolved record. Cost and token counts stay
//! `None` until a provider reports them; an unknown value is never rounded to
//! zero and never invented. After a crash, a cancellation or a missing usage
//! block, [`accounting::AttemptAccounting::from_record`] rehydrates the original
//! identity and [`accounting::AttemptAccounting::reconcile`] looks the
//! generation up by its provider request ID and merges the real figures into
//! the same record, keeping the original billing day. Repeated reconciliation
//! cannot double-charge. Billed failures and cancellations count toward the
//! daily cap. The census, crosscheck, survey and probe calls are further
//! attributed to their contest, step, pass and execution identity through
//! [`accounting::AttemptAttribution`].
pub mod accounting;
pub mod agents;
pub mod context;
pub mod judge;
pub mod openrouter;
pub mod registry;
pub mod resolver;
pub mod wire_schema;
pub mod worker;
