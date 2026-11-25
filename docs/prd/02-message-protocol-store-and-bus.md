# Message protocol, store and bus

### Part 1: Specification

Squad is one self-contained Rust binary running one process, with one front end per room. The persistent room is `main`. Live delivery uses in-process tokio channels. Durable state lives in `<data_dir>/squad.db`, accessed through `rusqlite` with bundled SQLite. Room state and the machine-wide daily spending ledger share this database so their updates commit atomically. The daily ledger is not namespaced by room.

This section owns envelope kinds and fields, `Work`, `Cursor`, `Request`, `RequestResult`, the complete `ErrorCode` enum, the SQL schema, transactional commit semantics and the bus interfaces.

| Responsibility | Location |
|---|---|
| Envelopes, validation, `Work`, `Cursor`, `Request`, `RequestResult` and `ErrorCode` | `crates/squad-protocol/src/` |
| Reader and executor-submission traits | `crates/squad-protocol/src/capabilities.rs` |
| Schema, migrations, transactional writes and scoped readers | `crates/squad-store/src/` |
| Bus trait and in-process implementation | `crates/squad-core/src/bus/` |
| Authorization and routing | `crates/squad-core/src/router/` |
| Executors, OpenRouter client and model registry | `crates/squad-agents/src/` |

The following contracts retain their named owners:

| Contract | Owner |
|---|---|
| Parser AST, grammar, command validation and exact user-facing validation strings | Command grammar and parser |
| `SquadConfig`, environment variables and `setup.sh` | Foundations |
| `RoomHandle`, CLI behavior and markdown export contract | Room API and CLI |
| Ordinary-reply execution, queue scheduling, context assembly and model resolution | Agents and OpenRouter |
| Contest engine, shared moves, all seven mode scripts and states, mode payloads, adjudication bundles, verdict schemas, admission, deadlines and attempt policy | Contests |
| Prompt templates, placeholders and rendering rules | Prompt pack |
| Spending reconciliation and storage admission policy | Hardening: Cost guard |

The protocol references those contracts by name. It does not define alternate shapes or state machines. Contests (08) owns each mode’s typed payloads. Once a mode is built, its Rust types in `squad-protocol` are the normative contract. M1 implements core messaging, quotes, standalone cases, Work, generic contest carriers and tables, and debate typed payloads only. Grill, census, crosscheck, tournament, survey and probe typed payloads, statuses, tables, validators and visibility tests are added in that mode’s M5 phase. Future-mode definitions and schema extensions below describe those later contracts; they are not M1 deliverables.

The workspace has six crates: `squad-protocol`, `squad-store`, `squad-agents`, `squad-core`, `squad-tui` and `squad-cli`. `squad-agents` depends on `squad-protocol` and its reader and submission traits, never on `squad-store` or `rusqlite`. `squad-core` constructs context-bound capabilities and passes them to executors. Export, prompt rendering, banner text and embedded fonts live in `squad-core`.

### Canonical numbers and configuration

| Thing | Value |
|---|---|
| Members | 1 to 12; nick pattern `^[a-z][a-z0-9_]{0,31}$`; reserved `me`, `all`, `judge`, `system`, `squad` |
| `ROUNDS` | Default 10, min 1, max 20 |
| `GRILL_ROUNDS` | Default 2, min 1, max 10 |
| Word limits, prompt-enforced | Owned by [Prompt pack](09-prompt-pack.md#word-limit-defaults); no numeric copy here |
| Visible output token ceilings | 2,048 for room replies, tournament entries, debate turns, grill questions and grill answers; 4,096 for final judge calls, `judge.tournament` and grill rebuilds; 512 for `judge.round` |
| Reasoning effort | Medium for members, low for `judge.round`, high for final judge calls and `judge.tournament`; reasoning text never forwarded |
| Reasoning token budgets | Low 1,024; medium 4,096; high 8,192 |
| Ordinary reply | Timeouts 15 s connect / 90 s first token / 30 s idle / 300 s total; 2 attempts, including one automatic retry, then an error event with a retry action |
| Debate turn | Deadline and attempts owned by [Contests execution limits](08-contests-debate-tournament-grill-census-crosscheck-survey-probe.md#execution-limits) |
| Final judge call | Deadline owned by [Contests execution limits](08-contests-debate-tournament-grill-census-crosscheck-survey-probe.md#execution-limits) |
| `judge.round` and `judge.tournament` | Deadlines owned by [Contests execution limits](08-contests-debate-tournament-grill-census-crosscheck-survey-probe.md#execution-limits) |
| Judge call | 3 attempts including schema-repair attempts, except `judge.round` |
| Per-agent queue | 20 items, then `AGENT_QUEUE_FULL` |
| User message size | 1 to 32,768 UTF-8 bytes |
| Daily spending | `SQUAD_DAILY_MAX_USD` = 20; debate confirmation threshold `SQUAD_DEBATE_CONFIRM_USD` = 2 |
| Quotes per message | Max 12 |
| Round judge call | 2 attempts including schema-repair attempts; then record the round as unscored |
| Round scores | Integer 1 to 10 per debater; reason at most 200 characters; equal round scores allowed |
| Preflight plan expiry | 10 min |
| Work lease | 60 s, renewed every 20 s |
| Ingress channel capacity | 256 |
| Mailbox capacity | 1 pending wake-up, using synchronous `try_send` |
| Feed broadcast capacity | 4,096; lag is repaired from `events` after the last applied cursor |
| Tournament | At least 3 entrants; one elimination per round; k entrants require k−1 rounds |
| Tournament judge recovery | 3 attempts including repairs; replay the round once after exhaustion, then fail if judging exhausts again |
| Grill | At least one leader and one griller; at most 3 questions per griller turn |
| `CENSUS_PASSES` | Default 2, min 1, max 5 |
| Census | At least 3 members; at most 8 initial gaps per member per pass |
| Crosscheck | At most 200 retained grid cells; 2 fresh-context auditors, each sampling 5 closures |
| Survey | At most 25 claims; at most 5 claims selected for a second ballot; confidence integers from 1 to 5 |
| Probe | At least 4 active members, split evenly; with an odd addressed count, the last member sits out |
| Census, crosscheck, survey and probe member calls | `room_words`; 2,048 visible tokens; deadline and 2-attempt plain-retry policy owned by [Contests execution limits](08-contests-debate-tournament-grill-census-crosscheck-survey-probe.md#execution-limits) |
| Census, crosscheck, survey and probe judge calls | `plan_words`; medium reasoning effort and 4,096-token reasoning budget; 4,096 visible output tokens; strict task-specific JSON schema; deadline and 3-attempt policy with at most one repair owned by [Contests execution limits](08-contests-debate-tournament-grill-census-crosscheck-survey-probe.md#execution-limits) |
| Grill calls | Deadline and attempts owned by [Contests execution limits](08-contests-debate-tournament-grill-census-crosscheck-survey-probe.md#execution-limits) |
| Crosscheck minimum and inventory | At least 3 members; 3 concrete items per member |
| Active contests | One active claim |
| Alias probe | 16 visible tokens; one retry on HTTP 400 with 2,048 tokens; room-open only |

Every output ceiling limits visible output. For a model whose catalog entry lists reasoning support, the request sends `max_tokens = visible ceiling + reasoning budget` and explicit `reasoning.max_tokens` equal to that budget. `reasoning.effort` is sent only for supported models. An explicitly configured effort for an unsupported model raises `MODEL_PARAMETER_UNSUPPORTED`; the default effort does not reject a model without reasoning support.

The alias probe sends `max_tokens: 16` with the lowest reasoning setting the model accepts. On a 400 response it retries once with `max_tokens: 2048`. The response `model` field supplies the concrete slug.

`SquadConfig` is the Foundations-owned type in `crates/squad-protocol/src/config.rs`. Configuration comes from one `.env` file, loaded with `dotenvy`; process environment variables take precedence. File lookup follows `--env-file <path>`, `./.env`, then `<OS config dir>/squad/.env`.

`SQUAD_ROUNDS` seeds `ROUNDS` only when the room has no stored value. Its default is 10, with range 1 to 20. A stored runtime value wins until `/clear`, which restores configured defaults.

`SQUAD_GRILL_ROUNDS` seeds `GRILL_ROUNDS` by the same rule, with default 2 and range 1 to 10. `SQUAD_PLAN_WORDS` optionally changes the rebuilt-plan word limit defined by Prompt pack. `SQUAD_REASONING_EFFORT_<NICK>` optionally overrides a member’s reasoning effort under the Foundations configuration contract.

`SQUAD_CENSUS_PASSES` optionally seeds `CENSUS_PASSES` by the same rule, with default 2 and range 1 to 5. It is not included in `.env.example`. Foundations owns the environment field; Parser owns assignment, expansion and exact validation strings.

The startup roster comes from `SQUAD_MODEL_<NICK>` variables under the Foundations configuration contract. Model versions are resolved by Agents and OpenRouter and cached in `models`, fixed until restart. Personas and the user name come from their optional environment variables. This section does not introduce configuration settings.

Timeouts, attempt counts, queue depth, reasoning budgets, output token ceilings, plan expiry, lease timing, channel capacities and input size limits are compile-time constants in `squad-protocol`. Credentials never enter the database.

Foundations owns `setup.sh`, including creating `.env` with mode 0600, preserving an existing file, installing build dependencies and building the single binary.

### Envelope primitives

Every boundary uses serde types with validation. Unknown fields are rejected before semantic checks.

Rust declarations below retain the contract field spellings. Implementations may use Rust field naming conventions with serde renames. Repeated derives are omitted. Validation uses two passes: first validate JSON against a schema with `additionalProperties: false` across the complete object and required entries for every required nullable field; then deserialize with serde. Flattened structures do not rely on `deny_unknown_fields` derives. An `Option` field is omitted when `None` unless marked `// nullable`; implementations use `#[serde(default, skip_serializing_if = "Option::is_none")]` for omitted fields. A field marked `// nullable` is required and serializes as `null` when `None`. This rule applies to every Rust declaration in the specification, including Parser, Foundations and Contests. Required nullable fields, including `tallyOverrideReason`, must be present even when their value is `null`.

- `EventId`, `MessageId`, `WorkId`, `ContestId`, `CaseId`, `PlanId`, correlation IDs and judge invocation IDs are UUIDv7 values. `DebateId` is the debate-specific alias of `ContestId`; it is not an unrelated event identity.
- `roomId` is the literal `main`.
- Member nicks follow the canonical pattern, exclude reserved names and exist in the startup registry.
- Dates are UTC ISO 8601 strings.
- Daily spending keys are local calendar dates formatted `YYYY-MM-DD`.
- IDs establish identity. They do not establish delivery order.
- `Cursor` wraps a positive SQLite integer. Its JSON representation remains a decimal string matching `^[1-9][0-9]*$`. Comparisons use integer arithmetic.
- Each durable event has its own `events.seq`, represented by `Cursor`.
- `HistoryPosition` wraps a positive `history.pos`. It identifies private-history delivery order, independently of event acceptance order. Its JSON representation is a decimal string.
- `Text` is a nonempty string. User-input byte limits apply before parsing. Parser validation determines whether the conversational body contains required text.
- `StepId` is UUIDv7. Anonymous artifact/item IDs (gap, inventory item, cell, claim, explanation and check) are router-minted UUIDv4 values at a barrier, or shuffled opaque short labels. They never encode source time or order, authorship or counts.

```rust
pub struct EventId(pub uuid::Uuid);
pub struct MessageId(pub uuid::Uuid);
pub struct WorkId(pub uuid::Uuid);
pub struct ContestId(pub uuid::Uuid);
pub type DebateId = ContestId;
pub struct CaseId(pub uuid::Uuid);
pub struct PlanId(pub uuid::Uuid);
pub struct StepId(pub uuid::Uuid);
pub struct CellId(pub uuid::Uuid); // anonymous UUIDv4
pub struct MemberNick(pub String);
pub struct ContextId(pub String);
pub struct Text(pub String);
pub type DateTime = chrono::DateTime<chrono::Utc>;

// The sole valid wire value is "main".
#[serde(rename_all = "lowercase")]
pub enum MainRoom { Main }

#[serde(tag = "kind", content = "id", rename_all = "lowercase")]
pub enum ExecutorKey {
    Member(MemberNick),
    Case(CaseId),
}

pub struct Cursor(i64);
pub struct HistoryPosition(i64);

pub enum Actor {
    Member(MemberNick),
    Me,
    System,
    Judge,
}

pub enum Recipient {
    Member(MemberNick),
    Me,
    Judge,
}

pub enum Target {
    Member(MemberNick),
    Judge,
}

#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    Stop,
    Length,
    ContentFilter,
}

pub struct Usage {
    pub inputTokens: u64,
    pub outputTokens: u64,
    pub costUsd: Option<f64>, // nullable
}

pub struct EnvelopeBase {
    pub version: u8,
    pub id: EventId,
    pub roomId: MainRoom,
    pub correlationId: EventId,
    pub causationId: Option<EventId>,
    pub sender: Actor,
    pub recipients: Vec<Recipient>,
    pub createdAt: DateTime,
}

pub struct CommitFields {
    pub seq: Cursor,
    pub acceptedAt: DateTime,
    pub commitIndex: u32,
    pub commitSize: u32,
}

pub struct ProducerEnvelope {
    #[serde(flatten)]
    pub base: EnvelopeBase,
    #[serde(flatten)]
    pub content: EnvelopePayload,
}

pub struct CommittedEnvelope {
    #[serde(flatten)]
    pub base: EnvelopeBase,
    #[serde(flatten)]
    pub content: EnvelopePayload,
    #[serde(flatten)]
    pub commit: CommitFields,
}
```

`Actor`, `Recipient` and `Target` serialize as their nick or reserved literal, without an additional tag. `MainRoom` validates the literal `main`. `version` must equal 1. Recipients must be nonempty and unique. Token counts are nonnegative integers; present monetary values are finite and nonnegative.

Producer envelopes cannot supply `CommitFields`. `CommittedEnvelope` accepts only canonical durable kinds. `delta` and `presence` are ephemeral and never contain commit fields.

The human submission capability establishes identity `me`. Separate internal capabilities establish worker, judge executor, orchestrator, accounting and router identities. A supplied `sender` or recipient list never grants authority.

The router derives canonical recipients. `all` and exclusion syntax never appear in stored recipient lists.

### Content and generation fields

Optional fields are omitted when absent. Nullable fields remain present with `null` when unavailable.

```rust
pub struct ResolvedQuote {
    pub eventId: EventId,
    pub author: Actor,
    pub text: String,
}

pub struct UserMessageInput {
    pub messageId: MessageId,
    pub text: Text,
    pub quote: Option<Vec<EventId>>,
    pub replyTo: Option<Vec<EventId>>,
}

pub struct UserMessage {
    #[serde(flatten)]
    pub input: UserMessageInput,
    pub body: String,
    pub resolvedQuotes: Vec<ResolvedQuote>,
    pub debateId: Option<DebateId>,
    pub caseId: Option<CaseId>,
    pub contestId: Option<ContestId>,
}

pub struct GenerationIdentity {
    pub messageId: MessageId,
    pub workId: WorkId,
    pub triggerEventId: EventId,
    pub attempt: u32,
}

pub struct CompletedGeneration {
    #[serde(flatten)]
    pub generation: GenerationIdentity,
    pub model: Text,
    pub finishReason: FinishReason,
    pub usage: Option<Usage>, // nullable
}

pub struct Completion {
    #[serde(flatten)]
    pub completed: CompletedGeneration,
    pub inReplyToMessageId: MessageId,
    pub content: String,
}
```

`text` retains the original input, including command tokens. `body` is the parser-produced body or retained debate instruction.

`quote` contains unique IDs, at most 12. `replyTo` contains unique IDs, at most 2, and is valid only for debate solution selection.

For a user message, `messageId` equals its envelope ID. Generated content uses a stable `messageId` throughout its work, including automatic attempts. The router allocates that identity before execution. Generation attempts are positive integers counting provider calls.

`model` is the concrete model reported by the response. Every completion request uses the startup-probed concrete slug. Each attempt records the response model separately; a mismatch with the requested concrete slug logs a warning and is reported by `squad doctor`. `usage: null` means token usage was unavailable. A present `Usage` retains the original numeric token fields and nullable numeric cost. Missing usage or cost is never replaced with zero.

An empty completed answer is permitted only when `finishReason` is `content_filter`. Truncation and filtering remain visible in accepted finals and completed debate turns.

`inReplyToMessageId` identifies the original user message answered by an ordinary reply, including a regenerated reply. It is distinct from the current work’s triggering control event.

`contestId` associates accepted question collection, tournament, grill, census, crosscheck, survey and probe activity with its contest. On debate-associated records carrying both fields, `contestId` and `debateId` identify the same contest. The association grants no additional visibility.

### Complete envelope kind registry

The registry is exhaustive for each built mode. Ungated variants are M1; `cfg(feature = "mode-…")` lines describe additions made only in the corresponding M5 phase, not feature implementations required by M1. Every available payload has strict validation; a generic carrier cannot bypass mode admission.

```rust
#[serde(tag = "kind", content = "payload")]
pub enum EnvelopePayload {
    #[serde(rename = "user.message")]
    UserMessage(UserMessage),
    #[serde(rename = "agent.reply")]
    AgentReply(Completion),
    #[serde(rename = "message.final")]
    MessageFinal(Completion),
    #[serde(rename = "delta")]
    Delta(Delta),
    #[serde(rename = "presence")]
    Presence(Presence),
    #[serde(rename = "system.notice")]
    SystemNotice(SystemNotice),
    #[serde(rename = "error")]
    Error(ErrorPayload),
    #[serde(rename = "var.change")]
    VarChange(VariableChange),
    #[serde(rename = "work.state")]
    WorkState(WorkStateEvent),
    #[serde(rename = "usage.record")]
    UsageRecord(UsageRecord),
    #[serde(rename = "budget.state")]
    BudgetState(BudgetStateEvent),
    #[serde(rename = "debate.preflight")]
    DebatePreflight(DebatePreflight),
    #[serde(rename = "debate.preflight.result")]
    DebatePreflightResult(DebatePreflightResult),
    #[serde(rename = "debate.opening")]
    DebateOpening(DebateOpening),
    #[serde(rename = "debate.setup")]
    DebateSetup(DebateSetup),
    #[serde(rename = "contest.transition")]
    ContestTransition(ContestTransitionSubmission),
    #[serde(rename = "debate.state")]
    DebateState(DebateState),
    #[serde(rename = "debate.turn")]
    DebateTurn(DebateTurn),
    #[serde(rename = "verdict")]
    Verdict(DebateVerdict),
    #[serde(rename = "case.opened")]
    CaseOpened(CaseOpened),
    #[serde(rename = "case.followup")]
    CaseFollowup(CaseFollowup),
    #[serde(rename = "case.ruling")]
    CaseRuling(CaseRuling),
    #[serde(rename = "message.superseded")]
    MessageSuperseded(MessageSuperseded),
    #[serde(rename = "scores.changed")]
    ScoresChanged(ScoresChanged),
    #[serde(rename = "room.cleared")]
    RoomCleared(RoomCleared),
    #[serde(rename = "debate.round_score")]
    DebateRoundScore(DebateRoundScore),
    #[serde(rename = "contest.setup")]
    ContestSetup(ContestSetup),
    #[serde(rename = "contest.state")]
    ContestState(ContestState),
    #[serde(rename = "contest.preflight")]
    ContestPreflight(ContestPreflight),
    #[serde(rename = "contest.preflight.result")]
    ContestPreflightResult(ContestPreflightResult),
    #[serde(rename = "contest.step")]
    ContestStep(ContestStep),
    #[serde(rename = "contest.artifact")]
    ContestArtifact(ContestArtifact),
    #[serde(rename = "contest.report")]
    ContestReport(ContestReport),
    #[serde(rename = "contest.collecting")]
    ContestCollecting(ContestCollecting),
    #[serde(rename = "contest.pick_requested")]
    ContestPickRequested(ContestPickRequested),
    #[serde(rename = "contest.picked")]
    ContestPicked(ContestPicked),
    #[cfg(feature = "mode-tournament")] // added in this mode’s M5 phase
    #[serde(rename = "tournament.setup")]
    TournamentSetup(TournamentSetup),
    #[cfg(feature = "mode-tournament")] // added in this mode’s M5 phase
    #[serde(rename = "tournament.state")]
    TournamentState(TournamentState),
    #[cfg(feature = "mode-tournament")] // added in this mode’s M5 phase
    #[serde(rename = "tournament.entry")]
    TournamentEntry(TournamentEntry),
    #[cfg(feature = "mode-tournament")] // added in this mode’s M5 phase
    #[serde(rename = "tournament.ranking")]
    TournamentRanking(TournamentRanking),
    #[cfg(feature = "mode-tournament")] // added in this mode’s M5 phase
    #[serde(rename = "tournament.result")]
    TournamentResult(TournamentResult),
    #[cfg(feature = "mode-tournament")] // added in this mode’s M5 phase
    #[serde(rename = "tournament.champion")]
    TournamentChampion(TournamentChampion),
    #[cfg(feature = "mode-grill")] // added in this mode’s M5 phase
    #[serde(rename = "grill.setup")]
    GrillSetup(GrillSetup),
    #[cfg(feature = "mode-grill")] // added in this mode’s M5 phase
    #[serde(rename = "grill.state")]
    GrillState(GrillState),
    #[cfg(feature = "mode-grill")] // added in this mode’s M5 phase
    #[serde(rename = "grill.question")]
    GrillQuestion(GrillQuestion),
    #[cfg(feature = "mode-grill")] // added in this mode’s M5 phase
    #[serde(rename = "grill.answer")]
    GrillAnswer(GrillAnswer),
    #[cfg(feature = "mode-grill")] // added in this mode’s M5 phase
    #[serde(rename = "grill.pass")]
    GrillPass(GrillPass),
    #[cfg(feature = "mode-grill")] // added in this mode’s M5 phase
    #[serde(rename = "grill.plan")]
    GrillPlan(GrillPlan),
    #[cfg(feature = "mode-grill")] // added in this mode’s M5 phase
    #[serde(rename = "grill.complete")]
    GrillComplete(GrillComplete),
    #[cfg(feature = "mode-census")] // added in this mode’s M5 phase
    #[serde(rename = "census.setup")]
    CensusSetup(CensusSetup),
    #[cfg(feature = "mode-census")] // added in this mode’s M5 phase
    #[serde(rename = "census.state")]
    CensusState(CensusState),
    #[cfg(feature = "mode-census")] // added in this mode’s M5 phase
    #[serde(rename = "census.plan")]
    CensusPlan(CensusPlan),
    #[cfg(feature = "mode-census")] // added in this mode’s M5 phase
    #[serde(rename = "census.hunt")]
    CensusHunt(CensusHunt),
    #[cfg(feature = "mode-census")] // added in this mode’s M5 phase
    #[serde(rename = "census.ledger")]
    CensusLedger(CensusLedger),
    #[cfg(feature = "mode-census")] // added in this mode’s M5 phase
    #[serde(rename = "census.rating")]
    CensusRating(CensusRating),
    #[cfg(feature = "mode-census")] // added in this mode’s M5 phase
    #[serde(rename = "census.report")]
    CensusReport(CensusReport),
    #[cfg(feature = "mode-crosscheck")] // added in this mode’s M5 phase
    #[serde(rename = "crosscheck.setup")]
    CrosscheckSetup(CrosscheckSetup),
    #[cfg(feature = "mode-crosscheck")] // added in this mode’s M5 phase
    #[serde(rename = "crosscheck.state")]
    CrosscheckState(CrosscheckState),
    #[cfg(feature = "mode-crosscheck")] // added in this mode’s M5 phase
    #[serde(rename = "crosscheck.goal")]
    CrosscheckGoal(CrosscheckGoal),
    #[cfg(feature = "mode-crosscheck")] // added in this mode’s M5 phase
    #[serde(rename = "crosscheck.inventory")]
    CrosscheckInventory(CrosscheckInventory),
    #[cfg(feature = "mode-crosscheck")] // added in this mode’s M5 phase
    #[serde(rename = "crosscheck.grid")]
    CrosscheckGrid(CrosscheckGrid),
    #[cfg(feature = "mode-crosscheck")] // added in this mode’s M5 phase
    #[serde(rename = "crosscheck.review")]
    CrosscheckReview(CrosscheckReview),
    #[cfg(feature = "mode-crosscheck")] // added in this mode’s M5 phase
    #[serde(rename = "crosscheck.audit")]
    CrosscheckAudit(CrosscheckAudit),
    #[cfg(feature = "mode-crosscheck")] // added in this mode’s M5 phase
    #[serde(rename = "crosscheck.ruling")]
    CrosscheckRuling(CrosscheckRuling),
    #[cfg(feature = "mode-crosscheck")] // added in this mode’s M5 phase
    #[serde(rename = "crosscheck.acceptance")]
    CrosscheckAcceptance(CrosscheckAcceptance),
    #[cfg(feature = "mode-crosscheck")] // added in this mode’s M5 phase
    #[serde(rename = "crosscheck.report")]
    CrosscheckReport(CrosscheckReport),
    #[cfg(feature = "mode-survey")] // added in this mode’s M5 phase
    #[serde(rename = "survey.setup")]
    SurveySetup(SurveySetup),
    #[cfg(feature = "mode-survey")] // added in this mode’s M5 phase
    #[serde(rename = "survey.state")]
    SurveyState(SurveyState),
    #[cfg(feature = "mode-survey")] // added in this mode’s M5 phase
    #[serde(rename = "survey.claims")]
    SurveyClaims(SurveyClaims),
    #[cfg(feature = "mode-survey")] // added in this mode’s M5 phase
    #[serde(rename = "survey.vote")]
    SurveyVote(SurveyVote),
    #[cfg(feature = "mode-survey")] // added in this mode’s M5 phase
    #[serde(rename = "survey.selection")]
    SurveySelection(SurveySelection),
    #[cfg(feature = "mode-survey")] // added in this mode’s M5 phase
    #[serde(rename = "survey.revote")]
    SurveyRevote(SurveyRevote),
    #[cfg(feature = "mode-survey")] // added in this mode’s M5 phase
    #[serde(rename = "survey.report")]
    SurveyReport(SurveyReport),
    #[cfg(feature = "mode-probe")] // added in this mode’s M5 phase
    #[serde(rename = "probe.setup")]
    ProbeSetup(ProbeSetup),
    #[cfg(feature = "mode-probe")] // added in this mode’s M5 phase
    #[serde(rename = "probe.state")]
    ProbeState(ProbeState),
    #[cfg(feature = "mode-probe")] // added in this mode’s M5 phase
    #[serde(rename = "probe.explanation")]
    ProbeExplanation(ProbeExplanation),
    #[cfg(feature = "mode-probe")] // added in this mode’s M5 phase
    #[serde(rename = "probe.check")]
    ProbeCheck(ProbeCheck),
    #[cfg(feature = "mode-probe")] // added in this mode’s M5 phase
    #[serde(rename = "probe.prediction")]
    ProbePrediction(ProbePrediction),
    #[cfg(feature = "mode-probe")] // added in this mode’s M5 phase
    #[serde(rename = "probe.report")]
    ProbeReport(ProbeReport),
}
```

| Kind | Producer and durable effect |
|---|---|
| `user.message` | Human submission, router normalization. Accepts only `UserMessageInput` from the human. Durable human message and authorized content projections. |
| `agent.reply` | Ordinary worker submission. Converted to `message.final`; never persisted as a second event. |
| `message.final` | Router canonical ordinary reply. Durable room event and author’s private-history entry. |
| `delta` | Worker or judge executor through ingress. Ephemeral human output after fencing checks. |
| `presence` | Worker through ingress, or router-derived offline observation. Ephemeral snapshot and notification. |
| `system.notice` | Authorized internal producer. Durable notice with router-derived visibility. |
| `error` | Router canonical execution failure. Durable when associated with accepted room work. |
| `var.change` | Router. Durable variable update. |
| `work.state` | Authorized executor observations or router transitions. Durable operational event; never prompt history. |
| `usage.record` | Accounting adapter through router. Durable attempt accounting and daily spending reconciliation. |
| `budget.state` | Router. Durable daily spending projection after a reservation or reconciliation change. |
| `debate.preflight` | Orchestrator proposal through router. Durable estimate visible to the human. |
| `debate.preflight.result` | Router. Durable confirmation, cancellation, expiry or rejection result. |
| `debate.opening` | Orchestrator through router. Durable addressed input for round 0. |
| `debate.setup` | Orchestrator through router. Durable setup delivered to both debaters; payload owned by Contests. |
| `contest.transition` | Orchestrator submission. Validated and normalized into `debate.state` or the matching kind’s state; not separately persisted. |
| `debate.state` | Router. Durable state snapshot visible to the human; payload owned by Contests. |
| `debate.turn` | Executor submits a completed outcome; orchestrator submits a forfeit. Durable room and debate event, with private delivery deferred to the barrier. |
| `verdict` | Judge executor through router. Durable validated debate verdict, case decision and scoreboard update. |
| `case.opened` | Router. Durable creation of an isolated adjudication case and its initial material. |
| `case.followup` | Router. Durable association of a human follow-up with an existing case. |
| `case.ruling` | Judge executor through router. Durable free-form standalone ruling or case follow-up answer. |
| `message.superseded` | Router. Durable replacement marker and new ordinary-work allocation. |
| `scores.changed` | Router. Durable scoreboard projection from an accepted debate verdict. |
| `room.cleared` | Router. Durable reset marker after `/clear`. |
| `debate.round_score` | Judge executor submits validated scores through router; authorized exhaustion records an unscored round. Durable event addressed only to `me`, stored in `round_scores` and the judge case history. No member-history delivery before the verdict. |
| `contest.setup`, `contest.state`, `contest.step`, `contest.artifact`, `contest.report` | Router-normalized generic carriers for frozen inputs, lifecycle state, steps, artifacts and reports. M1 validates common identities and JSON structure, never a future mode’s schema. |
| `contest.preflight` | Durable non-debate estimate with common admission fields and generic kind settings. |
| `contest.preflight.result` | Durable non-debate confirmation, cancellation, expiry or rejection outcome. |
| `contest.collecting` | Router. Durable active-contest claim and question-collection state; referenced answers are ordinary reply work. |
| `contest.pick_requested` | Router. Durable `AWAITING_PICK` state and frozen eligible-answer references, addressed only to `me`. |
| `contest.picked` | Router. Durable accepted selection and frozen sources for subsequent preflight; not a model message. |
| `tournament.setup` | Orchestrator through router. Durable prompt and tournament notice, projected separately to each entrant. |
| `tournament.state` | Router. Durable state under the Contests contract; preflight outcomes use `contest.preflight.result`; no automatic member-history entry. |
| `tournament.entry` | Executor through router. Durable entrant answer or orchestrator-authorized forfeit; only its author receives an entrant-history projection. |
| `tournament.ranking` | Judge executor through router. Durable validated full ranking with router-resolved nicks, addressed only to `me`. |
| `tournament.result` | Router. Durable per-round result addressed to one entrant and `me`; only that entrant’s rank, score, critique and survival outcome enter its history. |
| `tournament.champion` | Router. Durable champion, final placing list and one trophy increment; every entrant receives final placements only. |
| `grill.setup` | Orchestrator through router. Durable initial plan, focus and participant list, delivered to exactly the grill participants. |
| `grill.state` | Router. Durable state under the Contests contract; preflight outcomes use `contest.preflight.result`; no automatic member-history entry. |
| `grill.question` | Executor through router. Durable question turn, delivered to every grill participant. |
| `grill.answer` | Leader executor through router. Durable answers and plan changes, delivered to every grill participant. |
| `grill.pass` | Executor through router. Durable pass, delivered to every grill participant as a notice. |
| `grill.plan` | Leader executor through router. Durable complete plan version, delivered to every grill participant. |
| `grill.complete` | Router. Durable completion reason and references to the final plan and appendix; no judge or scoreboard mutation. |
| `census.setup`, `crosscheck.setup`, `survey.setup`, `probe.setup` | Orchestrator through router. Durable frozen sources, participants and script settings; only script-authorized per-member projections enter working histories. |
| `census.state`, `crosscheck.state`, `survey.state`, `probe.state` | Router. Durable state, current step; preflights use `contest.preflight` and `contest.preflight.result`; no automatic prompt entry. |
| `census.plan` | Router. Durable accepted plan version for a subsequent pass, retaining its source event and immutable text. |
| `census.hunt` | Member executor through router. Durable assigned-lens gap list; no delivery to another hunter. |
| `census.ledger` | Judge executor through router. Durable merged or finalized ledger, distinguished by pass and stage; member delivery uses an anonymised projection. |
| `census.rating` | Member executor through router. Durable ratings, covered citations and proposed additions; no cross-member rating delivery. |
| `census.report` | Router. Durable pass report and final report references, including app-computed counts, estimate range, discovery curve and skipped members. |
| `crosscheck.goal` | Judge executor through router when extraction is needed. Durable extracted goal and constraints; does not deliver the plan to inventory writers. |
| `crosscheck.inventory` | Member executor through router. Durable lens inventory produced without the plan or other inventories. |
| `crosscheck.grid` | Router from validated normalization output. Durable retained cells, trim provenance and assignments; identity-bearing provenance remains outside reviewer and auditor projections. |
| `crosscheck.review` | Member executor through router. Durable outcomes for exactly that member’s assigned cells. |
| `crosscheck.audit` | Fresh-context member executor through router. Durable sampled-closure findings and proposed missing case, without reviewer identities. |
| `crosscheck.ruling` | Judge executor through router. Durable rulings on challenged closures and additions; cannot silently delete retained cells or accept risks for the human. |
| `crosscheck.acceptance` | Router from an explicit human operation. Durable acceptance of named residual risks against the current grid and plan version. |
| `crosscheck.report` | Router. Durable coverage-ledger references, residual risks, audit outcome, bounded-stop status and skipped members. |
| `survey.claims` | Judge executor through router. Durable neutral claim list and restricted source provenance; members receive only unattributed claims. |
| `survey.vote` | Member executor through router. Durable first ballot, one-line reasons and confidence values. |
| `survey.selection` | Router. Durable app-computed selection of claims for a second ballot; counts are not projected to members. |
| `survey.revote` | Member executor through router. Durable second ballot and evidence that would change the member’s mind. |
| `survey.report` | Judge executor through router, with router-validated stored tallies. Durable five-band map, switches, settling questions and skipped members; no winner or scoreboard mutation. |
| `probe.explanation` | Explainer executor through router. Durable explanation and key assumption; no delivery to other explainers or checkers. |
| `probe.check` | Checker executor through router. Durable proposed check, measurements and outcomes; anonymised checks are delivered only to explainers at the prediction step. |
| `probe.prediction` | Explainer executor through router. Durable predictions for the frozen check set, including explicit inability to predict. |
| `probe.report` | Judge executor through router. Durable decision tree, prediction matrix, inseparable explanations, unexpected-result branch and skipped members; no declaration of a true explanation. |

`debate.confirm`, `debate.cancel`, `contest.confirm`, `contest.cancel`, `contest.pick`, `regen` and retry are requests, not additional envelope kinds. Their accepted effects use this registry.

Round banners are derived from state transitions. Pick banners are derived from `contest.pick_requested`. They introduce no envelope kind or stored banner event.

The tournament, grill and collection payload declarations are the canonical contracts owned by Contests and implemented in `crates/squad-protocol/src/contest.rs`, `tournament.rs` and `grill.rs`. The registry does not introduce alternative payload shapes.

The census, crosscheck, survey and probe payload declarations follow the same ownership rule and live in `census.rs`, `crosscheck.rs`, `survey.rs` and `probe.rs`. Each artifact carries `contestId` and `stepId`; pass-specific census artifacts also carry the pass number. Generated artifacts retain `CompletedGeneration`. Router-derived artifacts retain source event IDs rather than inventing generation metadata. Setup and state types retain the Contests-owned script version, state version, settings and frozen source references. State payloads can represent `PREFLIGHT`, `WAITING_FOR_PARTICIPANTS`, each script step, `COMPLETE`, `STOPPED` and `FAILED`.

Report schemas retain incomplete-step and skipped-member records with their work IDs and error codes. Reports do not silently treat missing work as a vote, a closure, a successful audit or a prediction. App-computed report fields cannot be supplied authoritatively by a model. The exact step schemas, state transitions, tally formulas and preflight call counts remain owned by Contests.

### Ephemeral payloads

```rust
pub struct Delta {
    #[serde(flatten)]
    pub generation: GenerationIdentity,
    pub chunkIndex: u64,
    pub text: String,
    pub debateId: Option<DebateId>,
    pub round: Option<u32>,
    pub caseId: Option<CaseId>,
    pub contestId: Option<ContestId>,
    pub stepId: Option<StepId>,
}

#[serde(rename_all = "lowercase")]
pub enum PresenceStatus {
    Idle,
    Thinking,
    Streaming,
    Error,
    Offline,
}

pub struct Presence {
    pub nick: MemberNick,
    pub status: PresenceStatus,
    pub workId: Option<WorkId>,
    pub queueDepth: u32,
    pub errorCode: Option<ErrorCode>,
    pub observedAt: DateTime,
}
```

`debateId` and `round` occur together on debate deltas. The round is an integer from 0 through the debate’s `totalRounds`. Judge deltas carry `caseId`; debate judging also carries `debateId`. Tournament and grill deltas carry `contestId` and their applicable round. These fields are derived from fenced work.

Census, crosscheck, survey and probe deltas carry `contestId` and `stepId` from their fenced work. Deltas remain human-only and cannot expose a partial artifact to another member before a script barrier.

`(messageId, attempt, chunkIndex)` identifies a delta. Duplicate chunks are ignored. A higher attempt replaces the previous attempt’s provisional text. There is no separate reset event. An accepted final replaces all provisional text, so missing chunks cannot prevent completion.

Workers submit deltas through ingress. Only the router forwards them to the human feed. Reasoning text is never included.

Presence is an in-memory room projection. It is not persisted or used as execution authority.

### Operational and error payloads

```rust
pub enum RetryKind {
    #[serde(rename = "retry")]
    Retry,
}

pub struct RetryAction {
    pub kind: RetryKind,
    pub workId: WorkId,
}

pub struct ErrorPayload {
    pub code: ErrorCode,
    pub text: Text,
    pub retryable: bool,
    pub relatedEventId: Option<EventId>,
    pub workId: Option<WorkId>,
    pub debateId: Option<DebateId>,
    pub caseId: Option<CaseId>,
    pub earliestCursor: Option<Cursor>,
    pub retryAction: Option<RetryAction>,
    pub contestId: Option<ContestId>,
}

pub struct SystemNotice {
    pub code: Text,
    pub text: Text,
    pub workId: Option<WorkId>,
    pub debateId: Option<DebateId>,
    pub caseId: Option<CaseId>,
    pub contestId: Option<ContestId>,
}

pub enum VariableName {
    #[serde(rename = "ROUNDS")]
    Rounds,
    #[serde(rename = "GRILL_ROUNDS")]
    GrillRounds,
    #[serde(rename = "CENSUS_PASSES")]
    CensusPasses,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VariableUpdate {
    pub name: VariableName,
    pub value: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportScope {
    Room,
    Debate,
    Grill,
    Tournament,
    Census,
    Crosscheck,
    Survey,
    Probe,
}

pub struct VariableChange {
    pub name: VariableName,
    pub value: u8,
    pub previousValue: u8,
}

#[serde(rename_all = "lowercase")]
pub enum WorkStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

pub struct WorkStateEvent {
    pub workId: WorkId,
    pub triggerEventId: EventId,
    pub target: Target,
    pub contextId: ContextId,
    pub attempt: u32,
    pub status: WorkStatus,
    pub error: Option<ErrorPayload>,
    pub debateId: Option<DebateId>,
    pub caseId: Option<CaseId>,
    pub contestId: Option<ContestId>,
}

#[serde(rename_all = "lowercase")]
pub enum UsageOutcome {
    Completed,
    Failed,
    Cancelled,
    Unknown,
}

pub struct UsageRecord {
    pub workId: WorkId,
    pub attempt: u32,
    pub target: Target,
    pub contextId: ContextId,
    pub providerRequestId: Option<String>,
    pub model: Text,
    pub startedAt: DateTime,
    pub endedAt: Option<DateTime>,
    pub outcome: UsageOutcome,
    pub usage: Option<Usage>, // nullable
    pub estimatedCostUsd: Option<f64>,
    pub debateId: Option<DebateId>,
    pub caseId: Option<CaseId>,
    pub contestId: Option<ContestId>,
    pub responseModel: Option<Text>, // nullable
    pub day: String,
}

pub struct BudgetStateEvent {
    pub spentUsd: f64,
    pub reservedUsd: f64,
    pub limitUsd: f64,
    pub estimatedUsd: f64,
    pub unresolvedAttempts: u32,
    pub day: String,
    pub unknownCalls: u32,
}
```

`SystemNotice.code` is an informational notice identifier, not an alternative error code. `RetryAction.workId` identifies failed ordinary work eligible for a human-requested retry.

A work-state attempt is zero before any provider call and positive afterward. Usage-record attempts are positive. Money values are finite and nonnegative. `estimatedUsd` is labelled estimated and is not added again to reported spending. Unresolved or possibly billed attempts retain reservations under Hardening: Cost guard.

An attempt has one accounting identity, `(workId, attempt)`. Later usage reconciliation updates that record atomically; it does not add its full cost to totals a second time.

`UsageRecord.model` is the concrete slug requested for that attempt. `responseModel` is nullable until a response supplies its model identity. It is retained even when it differs from the requested slug.

`BudgetStateEvent` is the daily spending projection. `day` identifies its local calendar day, `spentUsd` contains reported actual cost, `reservedUsd` contains pending reservations, and `limitUsd` is the current `SQUAD_DAILY_MAX_USD`. `unknownCalls` counts calls without a reported cost. The TUI displays `~` when this count is nonzero and shows reservations as a lighter pending segment. Colour thresholds are 75%, 90% and 100%.

Executor permissions restrict reported work transitions. Only a router-accepted result can complete work. Only the router or authorized cancellation path can cancel it.

An exhausted ordinary reply emits an `error` with a retry action. A retry request allocates new work linked to the original request. Debate retries remain orchestrator-controlled; judge recovery uses the operations defined by Contests.

Member work in census, crosscheck, survey and probe follows the script’s retry policy. Exhaustion records a skipped member for that step and preserves the error for the report; it does not create an ordinary-reply retry action. Judge recovery and terminal failure remain script-owned.

Validation failures before acceptance return a rejected `RequestResult`. They create no conversation message, private-history entry or generation work.

### Debate payloads and admission effects

```rust
pub struct DebateEstimate {
    pub estimatedUsd: f64,
    pub reservedUsd: f64,
    pub remainingUsd: f64,
    pub modelCalls: u32,
}

pub struct DebatePreflight {
    pub judgeOnly: bool,
    pub debateId: DebateId,
    pub planId: PlanId,
    pub expiresAt: DateTime,
    pub estimate: DebateEstimate,
    pub judgeModel: Option<String>,
    pub warning: Option<Text>,
    pub confirmationRequired: bool,
    pub debaters: [MemberNick; 2],
    pub rounds: u32,
    pub judgeRequested: bool,
    pub openingRoundRequired: bool,
}

#[serde(rename_all = "lowercase")]
pub enum PreflightStatus {
    Confirmed,
    Cancelled,
    Expired,
    Rejected,
}

#[serde(rename_all = "lowercase")]
pub enum Confirmation {
    Automatic,
    Explicit,
    Preconfirmed,
}

pub struct DebatePreflightResult {
    pub debateId: DebateId,
    pub planId: PlanId,
    pub status: PreflightStatus,
    pub confirmation: Option<Confirmation>,
    pub error: Option<ErrorPayload>,
}

pub struct DebateOpening {
    pub debateId: DebateId,
    pub debaters: [MemberNick; 2],
    pub topicMessageId: MessageId,
    pub topicText: Text,
    pub instruction: String,
    pub rounds: u32,
    pub round: u32,
}

#[serde(rename_all = "lowercase")]
pub enum ForfeitReason {
    Timeout,
    Error,
}

#[serde(tag = "status", rename_all = "lowercase")]
pub enum DebateTurnOutcome {
    Completed {
        content: String,
        model: Text,
        finishReason: FinishReason,
        usage: Option<Usage>, // nullable
    },
    Forfeit {
        reasonCode: ForfeitReason,
        text: Text,
    },
}

pub struct DebateTurn {
    #[serde(flatten)]
    pub generation: GenerationIdentity,
    pub debateId: DebateId,
    pub round: u32,
    pub speaker: MemberNick,
    pub outcome: DebateTurnOutcome,
}

pub struct DebateVerdict {
    #[serde(flatten)]
    pub completed: CompletedGeneration,
    pub debateId: DebateId,
    pub caseId: CaseId,
    pub judgeInvocationId: EventId,
    pub rubricVersion: Text,
    #[serde(flatten)]
    pub verdict: Verdict,
}

pub struct RoundScore {
    pub score: u8,
    pub reason: String,
}

pub struct DebateRoundScore {
    pub debateId: DebateId,
    pub caseId: CaseId,
    pub round: u32,
    pub scores: Option<std::collections::BTreeMap<MemberNick, RoundScore>>, // nullable
    pub tally: std::collections::BTreeMap<MemberNick, u32>,
    pub reason: Option<Text>,
}
```

`DebateOpening.round` must equal 0. `rounds` is the admitted `ROUNDS` snapshot. Debate-turn rounds range from 0 through `totalRounds`.

`DebateSetup`, `DebateState`, `DebateTransition`, `DebateAdjudicationBundle` and `Verdict` remain owned by Contests. `DebateTransition` has one variant per row of that section’s transition table. `DebateAdjudicationBundle` contains that section’s seven prescribed material items. `Verdict` combines `JudgeVerdict` with `weightedTotals: Vec<{nick,total}>` and `roundTally: BTreeMap<MemberNick,u32>`. Their declarations and validators are implemented in M1 rather than deferred to M5.

Envelope decoding performs structural and payload self-consistency checks only. Before commit, the router must call `DebateVerdict::validate_against(&DebateState, &authoritative_tally)` or `DebateRoundScore::validate_against(&DebateState)` with stored admitted state. These gates check debate/case identity, admitted debaters and round limits; verdict validation also checks the accepted round tally.

The verdict envelope includes that contract’s nested scores, non-null winner, `whatToStealFromLoser`, router-computed weighted totals, router-computed `roundTally` and nullable `tallyOverrideReason`. The worker cannot supply authoritative totals. `roundTally` contains exactly the two debater nicks and their integer sums of accepted round scores. When the winner differs from the unique round-tally leader, a nonempty `tallyOverrideReason` is required; missing justification triggers the verdict repair path. A tied round tally does not permit a tied final verdict.

`DebateRoundScore.round` is from 1 through the debate’s `totalRounds`. A scored payload contains exactly the two debater nicks in `scores`, with integer scores from 1 to 10 and one sentence of reason per debater, at most 200 characters each. Equal scores are valid. `reason` is omitted for a scored round. An exhausted round has `scores: null` and `reason: "unscored"`. Its sender may be `system`; a scored round requires `judge`. A `debate.turn` normally uses the speaker as sender; an orchestrator-recorded forfeit may use `system`.

`tally` always contains exactly the two debater nicks. The router computes it from all scored rounds accepted so far for the debate; unscored rounds contribute nothing. It is an acceptance-time running sum, not an assumption that all earlier numbered rounds have arrived. The model produces only `scores`; the router supplies the debate, case and round identities from the fenced work and computes `tally`. Duplicate results cannot add a round twice.

`judgeOnly` is required on debate preflights. When true it requires `judgeRequested` and exactly one estimated model call; otherwise the estimate must equal the full planned call count.

Every successful preflight emits `debate.preflight` with its estimate before admission. The threshold comparison uses `estimate.estimatedUsd`. The reservation includes every permitted attempt and the opening round when required. For a debate with `@judge`, the estimate adds one `judge.round` call per numbered round plus the final judge call, and the reservation covers their permitted attempts. Round 0 is not scored. The estimate presentation states the planned round count and number of model calls using the debate-owned plan. `remainingUsd` is the remaining daily amount after actual spending and outstanding reservations. A plan that cannot fit is rejected with `DAILY_MAX_REACHED`.

At or below the configured confirmation threshold, the router auto-confirms. Above it, admission waits for `debate.confirm` or `debate.cancel`, unless `squad send --yes` preconfirmed the request. Expired or changed plans cannot authorize admission. Each outcome emits `debate.preflight.result`.

The pending direct command remains request and planning state until confirmed. Cancellation, expiry or rejection does not create its conversational `user.message`. A question-first command has already committed its ordinary question and answers; cancelling its later preflight does not delete them.

At admission:

- A direct `$debate` command creates no ordinary `reply` work. Question-first answer collection creates ordinary `reply` work before the pick.
- With existing solutions, the router commits the command and `debate.setup` together.
- When opening positions are needed, the router commits the command and `debate.opening`, then schedules round 0. Its accepted positions become the solutions in the subsequent `debate.setup`.
- The router stores referenced material once in each effective prompt context. Setup and barrier projections preserve source identities so opening positions and other anchors are not duplicated as separate prompt content.

Round 0 is outside `ROUNDS`. Its work, deltas, outcomes, accounting and barrier use round number zero.

For a debate with `@judge`, the judge case context is initialized with the topic, solutions, instruction, rubric and judge identity before numbered-round scoring. It does not require future turns, scores or a final work ID. Each numbered-round barrier appends that round’s two turns to this case context. After the barrier commit, the orchestrator enqueues exactly one `judge.round` work item for that round on `judge:{caseId}`. Durable scheduling metadata makes this allocation recoverable and idempotent. The next debate round starts at the barrier without waiting for scoring.

Accepted round scores append to the same judge case context as the judge’s own output and are addressed only to `me`. They never enter a debater’s private history before the verdict, including through quotes or other content projections. Scores may arrive late or out of order; consumers associate them by `debateId` and `round`. The final `judge` work is enqueued only after all `judge.round` work for the debate has completed or been given up. Exhausting two attempts commits the unscored event and allows progression.

A debate admitted without `@judge` has no round-scoring work or round-score events and ends `COMPLETE_UNJUDGED`. A later explicit `/judge` does not backfill round scores.

The active-contest claim and transitions follow Contests, including `WAITING_FOR_DEBATERS` and `COMPLETE_UNJUDGED`. A second active contest is rejected with the applicable admission code and `A {kind} is already running. Stop it before starting another.`

When waiting for debaters, work already ahead of the debate drains first. Ordinary work accepted afterward for either debater is held until the debate reaches a terminal state. Its user-message event commits immediately, but its private-history projection is deferred until the hold is released. Other members continue normally. Debate-turn attempts reuse the round’s frozen boundary.

Lifecycle submissions use the standard envelope `id`. They become `debate.state` snapshots rather than introducing separate lifecycle envelope kinds. State-version checks and terminal transitions remain owned by the contest engine.

Explicit `/judge` on `STOPPED`, `COMPLETE_UNJUDGED` or `FAILED` follows the Contests-owned judge-only transitions through `PREPARING`, optional `AWAITING_CONFIRMATION` and `JUDGING`. Cancellation or expiry restores the stored prior status. The contest retains all of its case IDs rather than overwriting the original case.

### Shared contest protocol

```rust
#[serde(rename_all = "lowercase")]
pub enum ContestKind {
    Debate,
    Tournament,
    Grill,
    Census,
    Crosscheck,
    Survey,
    Probe,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ContestSelectionStatus {
    CollectingAnswers,
    AwaitingPick,
    Failed, // not enough answers
    Stopped, // pick_cancelled
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContestAnswer {
    pub nick: MemberNick,
    pub message_id: MessageId,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContestCollecting {
    pub contest_id: ContestId,
    pub kind: ContestKind,
    pub question_message_id: MessageId,
    pub question: String,
    pub addressed: Vec<MemberNick>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContestPickRequested {
    pub contest_id: ContestId,
    pub kind: ContestKind,
    pub question_message_id: MessageId,
    pub eligible: Vec<MemberNick>,
    pub required_picks: u8,
    pub banner: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContestPicked {
    pub contest_id: ContestId,
    pub kind: ContestKind,
    pub nicks: Vec<MemberNick>,
    pub source_message_ids: Vec<MessageId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContestSelectionState {
    pub contest_id: ContestId,
    pub kind: ContestKind,
    pub state: ContestSelectionStatus,
    pub state_version: u64,
    pub question_message_id: MessageId,
    pub addressed: Vec<MemberNick>,
    pub answers: Vec<ContestAnswer>,
    pub required_picks: u8,
}

pub struct ContestSetup {
    pub contestId: ContestId,
    pub kind: ContestKind,
    pub scriptVersion: Text,
    pub participants: Vec<MemberNick>,
    pub sourceEventIds: Vec<EventId>,
    pub settings: serde_json::Value, // validated JSON object; mode schema arrives in M5
}

pub struct GenericContestState {
    pub contestId: ContestId,
    pub kind: ContestKind,
    pub stateVersion: u64,
    pub status: Text,
    pub stepId: Option<StepId>,
    pub artifactEventIds: Vec<EventId>,
    pub terminalReason: Option<Text>, // nullable
}

#[serde(tag = "kind", content = "state", rename_all = "lowercase")]
pub enum ContestState {
    Generic(GenericContestState),
    Selection(ContestSelectionState),
    Debate(DebateState),
    #[cfg(feature = "mode-tournament")]
    Tournament(TournamentState),
    #[cfg(feature = "mode-grill")]
    Grill(GrillState),
    #[cfg(feature = "mode-census")]
    Census(CensusState),
    #[cfg(feature = "mode-crosscheck")]
    Crosscheck(CrosscheckState),
    #[cfg(feature = "mode-survey")]
    Survey(SurveyState),
    #[cfg(feature = "mode-probe")]
    Probe(ProbeState),
}

pub struct ContestPreflight {
    pub contestId: ContestId,
    pub kind: ContestKind,
    pub planId: PlanId,
    pub expiresAt: DateTime,
    pub estimate: DebateEstimate,
    pub participants: Vec<MemberNick>,
    pub confirmationRequired: bool,
    pub warning: Option<Text>,
    pub settings: serde_json::Value, // object; typed mode settings added by its M5 phase
}

pub struct ContestPreflightResult {
    pub contestId: ContestId,
    pub planId: PlanId,
    pub status: PreflightStatus,
    pub confirmation: Option<Confirmation>,
    pub error: Option<ErrorPayload>,
}

#[serde(tag = "kind", content = "preflight", rename_all = "lowercase")]
pub enum PendingPreflight {
    Debate(DebatePreflight),
    Contest(ContestPreflight),
}

pub struct ContestStep {
    pub contestId: ContestId,
    pub stepId: StepId,
    pub ordinal: u32,
    pub scriptVersion: Text,
    pub moveName: ContestMove,
    pub inputs: Vec<EventId>,
    pub assignments: Vec<ContestAssignment>,
}

#[serde(rename_all = "lowercase")]
pub enum ContestMove { Collect, Pick, Fanout, Turns, Judge, Tally, Barrier, Report }

pub struct ContestAssignment {
    pub target: Target,
    pub contextId: ContextId,
    pub payloadRefs: Vec<EventId>,
    pub workId: Option<WorkId>,
}

pub struct ContestArtifact {
    pub contestId: ContestId,
    pub stepId: StepId,
    pub artifactKind: Text,
    pub workId: Option<WorkId>,
    pub sourceRefs: Vec<EventId>,
    pub data: serde_json::Value,
}

pub struct ContestReport {
    pub contestId: ContestId,
    pub stepId: StepId,
    pub artifactEventIds: Vec<EventId>,
    pub skippedWorkIds: Vec<WorkId>,
    pub data: serde_json::Value,
}

pub struct ContestTransitionSubmission {
    pub contestId: ContestId,
    pub kind: ContestKind,
    pub expectedStateVersion: u64,
    pub transition: ContestTransition,
}

#[serde(tag = "kind", content = "transition", rename_all = "lowercase")]
pub enum ContestTransition {
    Debate(DebateTransition),
    #[cfg(feature = "mode-tournament")]
    Tournament(TournamentTransition),
    #[cfg(feature = "mode-grill")]
    Grill(GrillTransition),
    #[cfg(feature = "mode-census")]
    Census(CensusTransition),
    #[cfg(feature = "mode-crosscheck")]
    Crosscheck(CrosscheckTransition),
    #[cfg(feature = "mode-survey")]
    Survey(SurveyTransition),
    #[cfg(feature = "mode-probe")]
    Probe(ProbeTransition),
}

```

`ContestPreflight` is the shared base for the six non-debate modes. `TournamentPreflight`, `GrillPreflight`, `CensusPreflight`, `CrosscheckPreflight`, `SurveyPreflight` and `ProbePreflight` are declared by Contests in their respective M5 phases as typed specializations of that base plus kind settings. Until then, M1 stores settings as an object and validates only common admission fields. A generic carrier does not admit an unbuilt mode. Debate uses `DebatePreflight`; `PendingPreflight` unifies room-state and request-result presentation without changing either durable event. Non-debate preflights and their outcomes always commit as `contest.preflight` and `contest.preflight.result`, never hidden inside a state event.

`contests` and `contest_events` are the single storage model for all seven kinds. Their active claim covers question collection, a pending pick, preflight, queue draining and execution. Only one row may be active. Terminal transitions release participant holds atomically with their deferred history projections. Census also releases holds on entering `WAITING_FOR_PLAN`; continuation reacquires holds and drains earlier ordinary work before hunting. Its active-contest claim remains held during the wait.

Mode status enums and allowed transitions are owned by [Contests](08-contests-debate-tournament-grill-census-crosscheck-survey-probe.md); generic carriers do not duplicate those enums or admit unbuilt modes.

All seven modes are scripts over the shared `collect`, `pick`, `fanout`, `turns`, `judge`, `tally`, `barrier` and `report` moves. Their declarations and validated step contracts live in `squad-protocol`; the engine runs in `squad-core`. Build order is debate, grill, census, crosscheck, tournament, survey, probe. This section stores step identity, assignments, input references, committed artifacts and delivery decisions; Contests owns move behavior, each script, preflight call counts, tally formulas and M5.

A step record freezes its script version, move, inputs and assignments before work becomes runnable. Per-member payload references are separate assignments, not a shared room broadcast. A barrier records its committed outcome and delivers exactly the projections named by the script in one transaction. `pick`, `tally`, `barrier` and an app-assembled `report` allocate no provider work. A model-assisted report allocates its named judge work. Existing debate, tournament and grill envelope kinds and behavior remain unchanged when represented by these moves.

Question-first collection commits `contest.collecting`, the addressed question and independent ordinary `reply` work. Collection waits until each addressed work item has answered, failed or been stopped. It freezes eligible answer event IDs and emits `contest.pick_requested` to `me`. These replies remain mutually invisible and remain ordinary transcript and private-history content after cancellation. Their actual costs count toward daily spending; they are excluded from the subsequent contest estimate.

`ContestPick` selects exactly two eligible nicks for a debate or exactly one eligible leader for a grill. For a grill, all other eligible answerers become grillers. The router validates the pending contest and frozen answer references, then commits `contest.picked` before preparing the selected contest’s preflight. A picked debate requests judging by default and uses the original question and selected answers as its topic and solutions.

A pending pick has no expiry and survives restart. It holds the active-contest claim without freezing the room or holding ordinary member histories. Non-pick messages are handled normally. Invalid picks leave the pending selection intact and use the parser-owned strings:

- `Pick exactly two of: @a, @b, @c.`
- `Pick exactly one of: @a, @b, @c.`
- `@{nick} did not answer, so it cannot be picked.`

Fewer than two eligible answers ends either question-first mode as `FAILED` with `Not enough answers to start. {n} of {m} members answered.` Stopping a pending pick commits `STOPPED` with reason `pick_cancelled`; the collected answers remain.

Tournament entries use `reply` work with a contest ID and round, but commit as `tournament.entry`. Each entrant receives only its own prompt, answers and per-round result. The router creates a fresh shuffled label map for every round or replay, stores it outside history and presents only anonymised labels and answers to the case-bound judge. The judge output is a strict permutation of the labels, ordered best first, with integer scores from 1 to 10 and one-line critiques. No ties, missing labels, duplicate labels or unknown labels are accepted.

The router maps accepted labels back to nicks for the human ranking and individual result projections. A forfeiting entrant is ranked last under the Contests-owned forfeit policy. Exactly one entrant is eliminated per accepted round. A ranking commit, individual results, elimination and round state commit atomically. A champion commit increments that member’s trophy count once and delivers the final placing list, containing nicks and placements only, to every entrant. Tournament judging remains blind and never swaps the judge model.

Grill events implement an explicit shared floor. Setup, questions, answers, passes and plan versions are projected to exactly the leader and grillers as they commit. No non-participant receives them automatically. Each question and answer records its round, turn, speaker and referenced plan version under the Contests payload contract. Plan versions are immutable and retained. The final output references the last good plan and every question, answer and plan change for the appendix.

Grill working-context selection uses the latest plan, the current round’s questions and answers, and the compact list of earlier questions. Full prior answers remain durable for viewing and export but are excluded from the next round’s working context. Griller order, rotating angles, `PASS`, early completion and leader-failure behavior follow Contests.

### Four-mode artifact and delivery contracts

A census or crosscheck plan source is frozen from the selected quote, selected grill plan version, pasted plan or applicable latest-reply shortcut under Parser and Contests rules. Selecting a source never grants its author’s history. A census continuation freezes the current plan for the next pass; a replacement creates `census.plan` and does not rewrite earlier passes.

Census storage retains pass number, assigned lens, initial finds, merged gap IDs, finder membership, ratings, citations, additions and finalized status. Initial-find membership remains distinct from later additions so the app can compute the Contests-owned estimate from the correct evidence. Finder counts, singleton flags, the member-by-gap table, estimate bounds and discovery curve are derived from stored data. Member ledger projections contain no finder names, model names or counts. A report retains both estimate bounds and the shared-blind-spots caveat; no model-supplied point estimate replaces them.

Crosscheck storage retains the goal and constraints, their explicit or extracted provenance, inventory-item origins, normalized items, retained grid and any pre-grid trimming decisions. Each retained cell has stable item references, an assigned reviewer and its citation, justified exclusion or open gap with proposed fix. A reviewer cannot receive any cell containing an inventory item it originated, including a normalized item merged from several members’ inventories. Auditor selection and closure samples are persisted before calls and reused on recovery. Audit projections remove reviewer identities. Rulings retain challenged closures and additions without removing retained cells. Human risk acceptance records name exact cells and the plan and grid versions accepted; a judge output cannot create acceptance.

Survey storage freezes one question and its source answer set. Question-first collection uses ordinary `reply` work and `message.final` without a human pick. Reusing answers freezes eligible, unsuperseded answers to the same question. Claim provenance remains available to the router and authorized judge material for lone-insight and outlier classification, but not to voting members. First and second ballots are immutable separate records. Missing votes remain missing. The app computes tallies, selected claim IDs and switches from those records; the report cannot invent votes, authorship or confidence values.

Probe setup retains the addressed order, any sitting-out member and the persisted random assignment to explainer or checker. Explanation and check artifacts are independent blind outputs. Prediction work receives the explainer’s own explanation and the complete frozen, anonymised check set. Its output covers every supplied check with a prediction or an explicit inability to predict. Predictions are immutable before report work starts. The report retains the matrix and references every kept or dropped check and every inseparable explanation group under the Contests schema.

The four reports are human-facing artifacts. They are not automatically projected into member histories. Explicit later sharing follows quote rules and any active blind-step restrictions. None of the four modes updates wins, losses or trophies.

### Judge cases

```rust
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum CaseMaterial {
    Standalone {
        messages: Vec<ResolvedQuote>,
        question: Text,
    },
    Debate {
        setup: DebateSetup,
        rubricVersion: Text,
    },
    #[cfg(feature = "mode-tournament") ]
    Tournament {
        contestId: ContestId,
        material: TournamentCaseMaterial,
    },
    #[cfg(feature = "mode-census") ]
    Census {
        contestId: ContestId,
        material: CensusCaseMaterial,
    },
    #[cfg(feature = "mode-crosscheck") ]
    Crosscheck {
        contestId: ContestId,
        material: CrosscheckCaseMaterial,
    },
    #[cfg(feature = "mode-survey") ]
    Survey {
        contestId: ContestId,
        material: SurveyCaseMaterial,
    },
    #[cfg(feature = "mode-probe") ]
    Probe {
        contestId: ContestId,
        material: ProbeCaseMaterial,
    },
}

pub struct CaseOpened {
    pub caseId: CaseId,
    pub judgeInvocationId: EventId,
    pub workId: Option<WorkId>,
    pub judgeModel: Text,
    pub material: CaseMaterial,
    pub debateId: Option<DebateId>,
    pub contestId: Option<ContestId>,
}

pub struct CaseFollowup {
    pub caseId: CaseId,
    pub userEventId: EventId,
    pub judgeInvocationId: EventId,
    pub workId: WorkId,
}

pub struct CaseRuling {
    #[serde(flatten)]
    pub completed: CompletedGeneration,
    pub caseId: CaseId,
    pub judgeInvocationId: EventId,
    pub inReplyToEventId: EventId,
    pub content: Text,
}
```

The router creates each case with context ID `judge:{caseId}`. A judge executor in `crates/squad-agents/src/` builds requests only from its assigned case history and the judge prompt. Its scoped readers expose neither the room events nor another context, including the room member `fable`’s history. The orchestrator requests adjudication through source references and state transitions; it never assembles prompt text.

The router spawns an executor for each case with unfinished work, including during recovery. It retires the executor when that case has no unfinished work; a later follow-up can spawn another executor bound to the same retained case.

A new standalone `@judge` message requires the judge as sole recipient and at least one quote. Its initial context contains exactly the selected quoted text with attribution and the user’s question, beneath the judge prompt. It produces free-form `case.ruling`, not a scored debate verdict. Missing quoted material uses `JUDGE_QUOTE_REQUIRED` and `Quote at least one message for @judge to rule on.`

A debate adjudication uses the exact `DebateAdjudicationBundle` owned by Contests. For an automatically judged debate, `CaseOpened.workId` is absent and the case starts with the setup’s topic, solutions and instruction, the rubric and judge identity. It accumulates the authorized turns and round scores without duplicating source material. Final `judge` work uses the complete bundle assembled from that case after all round-scoring work has settled, with unscored rounds identified in its prompt. For an explicit `/judge`, the router commits the retained transcript and any existing round scores into a fresh case and enqueues final `judge` work without retroactive scoring. A valid result commits `verdict`.

Judge conflict selection uses resolved concrete model identity, not nick spelling. Debate and standalone cases skip fallbacks that are unconfigured, are participating sources, or resolve to a conflicting model, under the Agents and Contests contracts. Only explicitly configured fallbacks require configuration validation. A debate swap uses `Judge model changed to {judge_model_slug} because @{nick} is debating.` Blind tournaments never swap.

Census, crosscheck, survey and probe judge work uses the judge system prompt, the mode-specific task prompt and its strict JSON schema. Each uses medium reasoning effort and a 4,096-token visible ceiling. The mode-specific case-material types are Contests-owned. Their source projections strip member nick and model attribution where the script requires anonymity; router-only provenance maps resolve source handles for storage and human reports. A step’s frozen material controls its read scope even when the case retains earlier invocations.

A follow-up quoting a case’s accepted verdict or ruling reopens that case. Replying to a verdict card is encoded as a quote, not ordinary-message `replyTo`. The request uses that case’s material, accepted decision and prior follow-ups, plus the current addressed question and explicit quotes. It does not load another case implicitly. References selecting different cases in one follow-up return `CASE_REFERENCE_AMBIGUOUS`.

The original material and decision remain sealed. Follow-up questions and answers append to the case; they do not rewrite the decision or update debate scores.

Each explicit `/judge` summon on an eligible stopped or finished debate opens a fresh case from its existing transcript. Following up on an existing decision instead uses its case reference.

`case.opened` and `case.followup` are audit events. Their addressed content is projected into case history once; audit metadata does not become duplicate prompt text.

Judge invocation IDs identify calls within cases. They do not replace `caseId` in context identifiers. Case histories survive successful judging so follow-ups work after restart.

Rejected judge output and validation errors are stored in attempt-scoped repair records keyed by `(work_id, attempt)`. They never enter conversation or case history. Only the case-bound `WorkReader` can expose the preceding failed attempt’s repair data to an authorised subsequent attempt. Final judging, round scoring and tournament judging retain their separate attempt budgets.

For judge work, context assembly sends the judge system prompt plus one rendered user message containing the authorised case material and any repair data. It does not also send the case history as separate provider messages.

### Quotes, supersession, scoreboard and clearing

```rust
pub struct MessageSuperseded {
    pub supersededEventId: EventId,
    pub supersededMessageId: MessageId,
    pub member: MemberNick,
    pub inReplyToMessageId: MessageId,
    pub replacementWorkId: WorkId,
    pub replacementMessageId: MessageId,
}

pub struct Score {
    pub nick: MemberNick,
    pub wins: u64,
    pub losses: u64,
    pub trophies: u64,
}

pub struct ScoresChanged {
    pub verdictEventId: EventId,
    pub scores: Vec<Score>,
}

pub struct RoomCleared {
    pub epoch: u64,
    pub previousThrough: Option<Cursor>,
}
```

For `quote`, the router resolves each referenced retained event to `{author, text}`, preserves the reference ID and freezes that text in the accepted message. The labelled text becomes part of the addressed user input in every recipient’s private history. Missing or ineligible references reject the submission; the router never silently omits them.

Quoting is the only human-triggered path for sharing another member’s words outside an admitted contest. A quote does not grant access to the source member’s history. Model-generated mentions, commands and quote-like text remain ordinary output. Tournament isolation and the prohibition on delivering debate round scores before the verdict remain enforced when validating projections.

A quote or setup cannot inject hidden material into an active census, crosscheck, survey or probe step. The original user event may retain the plan for the human while the inventory assignment receives only the authorized goal, constraints and lens. Anonymised working projections remove attribution and restricted metadata from both content and source metadata.

`replyTo` is valid only on `$debate` for solution selection. Its references are not ordinary-message quotes. Solution delivery follows `DebateSetup`.

A regeneration request must identify an eligible ordinary member reply. The router atomically commits `message.superseded`, marks the old reply excluded from that member’s future context and creates new `reply` work for the original request. The replacement uses a new generation identity. The old answer remains available in the human audit transcript as superseded.

Regeneration does not edit sent user messages, rewrite sealed cases, change frozen contest sources or mutate quote snapshots already delivered. A failed replacement leaves the supersession visible and offers the appropriate recovery action.

A verdict commit increments the winner’s wins and the loser’s losses in `scores` and emits `scores.changed` in the same transaction. Duplicate verdict delivery cannot increment scores twice. Round scores, standalone rulings and case follow-ups do not affect the scoreboard.

A tournament champion commit increments `scores.trophies` once. Its `tournament.champion` event carries the resulting trophy projection. Grill completion does not change wins, losses or trophies.

`/clear` is the room deletion operation. It cancels and fences work before deleting conversation history, contest and case content, round scores, tournament rankings and label maps, grill plans and turns, pending picks, supersession state, scoreboard entries and obsolete delivery state. Room variables return to configured defaults. It is not a local display operation.

Clearing also deletes script steps, per-member assignments, isolated step histories, four-mode artifacts, census finder records, crosscheck cell and audit records, risk acceptances, survey provenance and ballots, and probe assignments and predictions. `CENSUS_PASSES` returns to its configured default.

Clearing advances the room epoch and retains monotonic ordering metadata so old results cannot enter the cleared room. Actual daily spending, unknown-cost counts and outstanding billing reconciliation required by Cost guard remain accounted for without retaining deleted conversational content. The reset commits `room.cleared`, invalidates old replay positions and resets the current TUI state.

Room, debate, tournament and grill markdown exports use the Room API and CLI export contract and store read primitives. Export reads a consistent durable boundary, includes supersession and forfeit labels, and contains no provisional chunks or credentials. Export does not delete history. Markdown export and banner rendering live in `crates/squad-core/src/export/` and `crates/squad-core/src/banner/`. The public contract is `RoomHandle::export(scope, id: Option<ContestId>) -> Result<String, RoomError>`, with `export_offline(data_dir, room, scope, id)` for offline reads.

Census, crosscheck, survey and probe exports use the same consistent boundary and their Foundations-owned scopes. They retain committed reports, source-version references, incomplete-step labels and the human-visible evidence needed by each report. Human-visible census finder data is exportable; restricted router maps and auditor reviewer-identity maps are not exposed as raw storage metadata.

### Complete ErrorCode enum

Every parser error, envelope error, request rejection, executor failure, presence error, and contest or case failure uses this enum. Exact validation strings remain owned by Command grammar and parser.

```rust
#[allow(non_camel_case_types)]
pub enum ErrorCode {
    INVALID_ENVELOPE,
    UNSUPPORTED_VERSION,
    NOT_READY,
    INVALID_COMMAND,
    COMMAND_NOT_IMPLEMENTED,
    RECIPIENT_REQUIRED,
    UNKNOWN_RECIPIENT,
    MESSAGE_TOO_LARGE,
    INVALID_REFERENCE,
    JUDGE_QUOTE_REQUIRED,
    ROOM_NOT_FOUND,
    ROOM_CLEARED,
    PRODUCER_FORBIDDEN,
    IDEMPOTENCY_CONFLICT,
    CURSOR_EXPIRED,
    CURSOR_INVALID,
    CONFIG_INVALID,
    CREDENTIALS_MISSING,
    MODEL_CATALOG_UNAVAILABLE,
    MODEL_UNAVAILABLE,
    MODEL_PARAMETER_UNSUPPORTED,
    AGENT_QUEUE_FULL,
    CONTEXT_TOO_LARGE,
    PROVIDER_TIMEOUT,
    PROVIDER_NETWORK_ERROR,
    PROVIDER_ERROR,
    RATE_LIMITED,
    DEBATE_SOURCE_MISSING,
    DEBATE_TOPIC_MISMATCH,
    DEBATE_BUSY,
    DEBATE_NOT_FOUND,
    DEBATE_PLAN_EXPIRED,
    DEBATE_PLAN_STALE,
    INVALID_STATE_TRANSITION,
    JUDGE_INVALID_VERDICT,
    CASE_NOT_FOUND,
    CASE_REFERENCE_AMBIGUOUS,
    REGEN_NOT_ALLOWED,
    NOTHING_TO_STOP,
    STALE_WORK_ATTEMPT,
    WORK_CANCELLED,
    COST_UNKNOWN,
    DAILY_MAX_REACHED,
    STORAGE_LIMIT,
    UNAVAILABLE,
    INTERNAL_ERROR,
    CONTEST_BUSY,
    CONTEST_NOT_FOUND,
    CONTEST_PLAN_EXPIRED,
    CONTEST_PLAN_STALE,
    CONTEST_PICK_INVALID,
    CONTEST_NOT_ENOUGH_ANSWERS,
    TOURNAMENT_ENTRANTS_REQUIRED,
    JUDGE_INVALID_RANKING,
    GRILL_SOURCE_MISSING,
    GRILL_PARTICIPANTS_REQUIRED,
    CENSUS_SOURCE_MISSING,
    CENSUS_PARTICIPANTS_REQUIRED,
    CROSSCHECK_SOURCE_MISSING,
    CROSSCHECK_PARTICIPANTS_REQUIRED,
    CROSSCHECK_ASSIGNMENT_INVALID,
    CROSSCHECK_ACCEPTANCE_STALE,
    SURVEY_SOURCE_MISSING,
    SURVEY_PARTICIPANTS_REQUIRED,
    PROBE_INPUT_REQUIRED,
    PROBE_PARTICIPANTS_REQUIRED,
    MODE_INVALID_OUTPUT,
    JUDGE_INVALID_MODE_OUTPUT,
}
```

Each variant serializes as its exact uppercase name.

| Codes | Use |
|---|---|
| `INVALID_ENVELOPE`, `UNSUPPORTED_VERSION`, `PRODUCER_FORBIDDEN` | Invalid envelope structure, unsupported version or unauthorized producer operation. |
| `INVALID_COMMAND`, `COMMAND_NOT_IMPLEMENTED` | Parser syntax, argument, variable, command-support and recipient-combination failures. Reserved future commands use `COMMAND_NOT_IMPLEMENTED`. |
| `RECIPIENT_REQUIRED`, `UNKNOWN_RECIPIENT` | Missing effective recipients and unknown member names. |
| `MESSAGE_TOO_LARGE`, `INVALID_REFERENCE` | Input byte limit, invalid quote or solution reference, excessive references or prohibited ordinary-message `replyTo`. |
| `JUDGE_QUOTE_REQUIRED` | Standalone judge addressing without required quoted material. |
| `DEBATE_SOURCE_MISSING`, `DEBATE_TOPIC_MISMATCH` | Missing explicit or frozen solutions, and incompatible selected topics. Implicit missing solutions take the round-0 path. |
| `DEBATE_BUSY`, `DEBATE_NOT_FOUND` | Active-contest conflict for a debate request or unavailable target debate. |
| `DEBATE_PLAN_EXPIRED`, `DEBATE_PLAN_STALE` | Expired plan or changed material, settings, pricing or reservation assumptions requiring new preflight. |
| `INVALID_STATE_TRANSITION`, `STALE_WORK_ATTEMPT`, `WORK_CANCELLED` | Invalid lifecycle transition, obsolete execution authority or cancelled work. |
| `PROVIDER_TIMEOUT`, `PROVIDER_NETWORK_ERROR`, `PROVIDER_ERROR`, `RATE_LIMITED` | Sanitized provider failure categories. Retry eligibility follows the owning execution policy. |
| `JUDGE_INVALID_VERDICT` | Judge output still invalid after the permitted repair policy. |
| `CASE_NOT_FOUND`, `CASE_REFERENCE_AMBIGUOUS` | Unavailable case or references selecting different cases. |
| `REGEN_NOT_ALLOWED`, `NOTHING_TO_STOP` | Ineligible regeneration or no eligible work to stop. |
| `COST_UNKNOWN`, `DAILY_MAX_REACHED`, `STORAGE_LIMIT` | Unavailable pricing, daily spending guard and durable-storage admission guard. |
| `CONFIG_INVALID`, `CREDENTIALS_MISSING`, model codes | Configuration, startup resolution, supported parameters and model availability failures. |
| `CURSOR_INVALID`, `CURSOR_EXPIRED`, `ROOM_CLEARED` | Invalid event boundary or a boundary invalidated by clearing. |
| `CONTEST_BUSY`, `CONTEST_NOT_FOUND` | Active-contest conflict for tournament or grill admission, or unavailable target contest. |
| `CONTEST_PLAN_EXPIRED`, `CONTEST_PLAN_STALE` | Expired or changed tournament or grill preflight. |
| `CONTEST_PICK_INVALID` | Invalid pick cardinality, duplicate selection, ineligible answerer or wrong pending contest. |
| `CONTEST_NOT_ENOUGH_ANSWERS` | Collection settled with fewer eligible answers than required to continue. |
| `TOURNAMENT_ENTRANTS_REQUIRED` | Fewer than three tournament entrants. |
| `JUDGE_INVALID_RANKING` | Tournament ranking fails validation after the permitted repair policy; round replay and terminal behavior follow Contests. |
| `GRILL_SOURCE_MISSING`, `GRILL_PARTICIPANTS_REQUIRED` | Missing leader plan or no eligible griller. |
| `CENSUS_SOURCE_MISSING`, `CENSUS_PARTICIPANTS_REQUIRED` | No usable census plan or fewer than three resolved census participants. |
| `CROSSCHECK_SOURCE_MISSING`, `CROSSCHECK_PARTICIPANTS_REQUIRED` | No usable crosscheck plan or participants insufficient under the Contests-owned assignment and audit rules. |
| `CROSSCHECK_ASSIGNMENT_INVALID` | A cell is assigned to an originator of one of its inventory items, an assignment is incomplete or duplicated, or audit assignments violate the script. |
| `CROSSCHECK_ACCEPTANCE_STALE` | Human acceptance references an obsolete plan, grid or cell outcome. |
| `SURVEY_SOURCE_MISSING`, `SURVEY_PARTICIPANTS_REQUIRED` | No question or eligible parallel answer set, or an invalid participant set under the survey script. |
| `PROBE_INPUT_REQUIRED`, `PROBE_PARTICIPANTS_REQUIRED` | Missing required diagnostic brief or fewer than four active members after applying the odd-count sit-out rule. |
| `MODE_INVALID_OUTPUT` | A census, crosscheck, survey or probe member artifact fails its task schema or assignment coverage validation. Retry and skip behavior follow Contests. |
| `JUDGE_INVALID_MODE_OUTPUT` | A judge artifact in one of the four modes remains invalid after its repair policy. |
| Remaining codes | Named readiness, room, idempotency, availability or internal failure. |

`CONTEST_BUSY`, `CONTEST_NOT_FOUND`, `CONTEST_PLAN_EXPIRED` and `CONTEST_PLAN_STALE` also apply to census, crosscheck, survey and probe. Existing reference, state-transition, provider, accounting and fencing errors are reused. Parser owns the four verbs’ exact error strings and their mappings to these codes; this section does not introduce competing strings.

The model error-code names remain part of the shared enum; they do not introduce additional configuration mechanisms.

`reasonCode` on a forfeit remains the separate `"timeout" | "error"` category used by the forfeit prompt. Detailed execution failures use `ErrorCode`.

`DAILY_MAX_REACHED` uses `Daily max of $20.00 reached. Raise SQUAD_DAILY_MAX_USD or wait until tomorrow.` with the configured max formatted to two decimal places. It rejects new generation and contest admission without cancelling in-flight provider calls.

Contest admission uses the parser-owned strings `$tournament needs at least three entrants.` and `@{nick} has no plan to grill yet. Ask it something first.` where applicable.

Errors contain sanitized text and IDs. They never contain credentials, authorization headers, provider reasoning text or unrelated private content.

### Work record and execution authority

```rust
pub enum WorkKind {
    #[serde(rename = "reply")]
    Reply,
    #[serde(rename = "debate.turn")]
    DebateTurn,
    #[serde(rename = "judge")]
    Judge,
    #[serde(rename = "judge.round")]
    JudgeRound,
    #[cfg(feature = "mode-tournament")]
    #[serde(rename = "judge.tournament")]
    JudgeTournament,
    #[cfg(feature = "mode-grill")] // added in this mode’s M5 phase
    #[cfg(feature = "mode-grill")]
    #[serde(rename = "grill.question")]
    GrillQuestion,
    #[cfg(feature = "mode-grill")] // added in this mode’s M5 phase
    #[cfg(feature = "mode-grill")]
    #[serde(rename = "grill.answer")]
    GrillAnswer,
    #[cfg(feature = "mode-grill")] // added in this mode’s M5 phase
    #[cfg(feature = "mode-grill")]
    #[serde(rename = "grill.rebuild")]
    GrillRebuild,
    #[cfg(feature = "mode-census")] // added in this mode’s M5 phase
    #[cfg(feature = "mode-census")]
    #[serde(rename = "census.hunt")]
    CensusHunt,
    #[cfg(feature = "mode-census")] // added in this mode’s M5 phase
    #[cfg(feature = "mode-census")]
    #[serde(rename = "census.merge")]
    CensusMerge,
    #[cfg(feature = "mode-census")] // added in this mode’s M5 phase
    #[cfg(feature = "mode-census")]
    #[serde(rename = "census.rate")]
    CensusRate,
    #[cfg(feature = "mode-census")] // added in this mode’s M5 phase
    #[cfg(feature = "mode-census")]
    #[serde(rename = "census.final")]
    CensusFinal,
    #[cfg(feature = "mode-crosscheck")] // added in this mode’s M5 phase
    #[cfg(feature = "mode-crosscheck")]
    #[serde(rename = "crosscheck.goal")]
    CrosscheckGoal,
    #[cfg(feature = "mode-crosscheck")] // added in this mode’s M5 phase
    #[cfg(feature = "mode-crosscheck")]
    #[serde(rename = "crosscheck.inventory")]
    CrosscheckInventory,
    #[cfg(feature = "mode-crosscheck")] // added in this mode’s M5 phase
    #[cfg(feature = "mode-crosscheck")]
    #[serde(rename = "crosscheck.normalize")]
    CrosscheckNormalize,
    #[cfg(feature = "mode-crosscheck")] // added in this mode’s M5 phase
    #[cfg(feature = "mode-crosscheck")]
    #[serde(rename = "crosscheck.cell")]
    CrosscheckCell,
    #[cfg(feature = "mode-crosscheck")] // added in this mode’s M5 phase
    #[cfg(feature = "mode-crosscheck")]
    #[serde(rename = "crosscheck.audit")]
    CrosscheckAudit,
    #[cfg(feature = "mode-crosscheck")] // added in this mode’s M5 phase
    #[cfg(feature = "mode-crosscheck")]
    #[serde(rename = "crosscheck.rule")]
    CrosscheckRule,
    #[cfg(feature = "mode-survey")] // added in this mode’s M5 phase
    #[cfg(feature = "mode-survey")]
    #[serde(rename = "survey.extract")]
    SurveyExtract,
    #[cfg(feature = "mode-survey")] // added in this mode’s M5 phase
    #[cfg(feature = "mode-survey")]
    #[serde(rename = "survey.vote")]
    SurveyVote,
    #[cfg(feature = "mode-survey")] // added in this mode’s M5 phase
    #[cfg(feature = "mode-survey")]
    #[serde(rename = "survey.revote")]
    SurveyRevote,
    #[cfg(feature = "mode-survey")] // added in this mode’s M5 phase
    #[cfg(feature = "mode-survey")]
    #[serde(rename = "survey.report")]
    SurveyReport,
    #[cfg(feature = "mode-probe")] // added in this mode’s M5 phase
    #[cfg(feature = "mode-probe")]
    #[serde(rename = "probe.explain")]
    ProbeExplain,
    #[cfg(feature = "mode-probe")] // added in this mode’s M5 phase
    #[cfg(feature = "mode-probe")]
    #[serde(rename = "probe.check")]
    ProbeCheck,
    #[cfg(feature = "mode-probe")] // added in this mode’s M5 phase
    #[cfg(feature = "mode-probe")]
    #[serde(rename = "probe.predict")]
    ProbePredict,
    #[cfg(feature = "mode-probe")] // added in this mode’s M5 phase
    #[cfg(feature = "mode-probe")]
    #[serde(rename = "probe.report")]
    ProbeReport,
}

pub struct Work {
    pub workId: WorkId,
    pub messageId: MessageId,
    pub inReplyToMessageId: Option<MessageId>,
    pub kind: WorkKind,
    pub target: Target,
    pub contextId: ContextId,
    pub triggerEventId: EventId,
    pub historyThrough: HistoryPosition,
    pub attempt: u32,
    pub debateId: Option<DebateId>,
    pub round: Option<u32>,
    pub replay: Option<u8>,
    pub caseId: Option<CaseId>,
    pub promptRef: Option<PromptRef>,
    pub maxOutputTokens: Option<u32>,
    pub contestId: Option<ContestId>,
    pub stepId: Option<StepId>,
}
```

`messageId` is allocated by the router before execution and survives retries. Reply work requires `inReplyToMessageId`, including regenerated replies whose trigger is a supersession event. The grant exposes both through `work`; `binding` freezes the resolved model slug and reasoning effort for this execution.

This is the sole execution-work schema. Its durable representation is in `work`. Status, leases, scheduling flags, receipts and completion tombstones are operational records and never enter prompt history.

For ordinary replies and member work in debate, tournament and grill, `contextId` equals the member nick. A generic step-bound reply may instead use `contest:{contestId}:{stepId}:{nick}` when its frozen assignment authorizes that context; M1 reader fixtures use this generic form without admitting an unbuilt mode. For `judge`, `judge.round` and `judge.tournament` work, it is `judge:{caseId}` and `caseId` is required. Debate-turn work requires `debateId` and `round`. Judge work associated with a debate also carries `debateId`. `judge.round` requires `debateId` and a round from 1 through `totalRounds`, uses `judge.round.md` under the judge system prompt, has a 512-token visible output ceiling and low reasoning effort with a 1,024-token reasoning budget when supported, and permits two provider attempts including schema repair.

Tournament entry work reuses `reply` with `contestId`, round, required `replay` (0 or 1) and `tournament.entrant.md`; it does not gain access to another entrant’s history. `judge.tournament` requires a tournament contest, case and round, uses `tournament.judge.md`, and permits three attempts including schema repairs. Tournament judge work also requires `replay`; result acceptance checks the current round execution’s replay index. A replay creates new round-execution work under the Contests-owned replay policy rather than resetting consumed attempts.

Grill work requires a grill contest and round. `grill.question` targets the scheduled griller, `grill.answer` and `grill.rebuild` target the leader. Their prompt references are `grill.question.md`, `grill.answer.md` and `grill.rebuild.md`. Question and answer work use the 2,048-token visible ceiling; rebuild work uses 4,096. The Contests contract determines deadlines, retries, skip behavior and leader failure.

Question-first collection also uses ordinary `reply` work associated with its contest, without a contest round. Its completion remains `message.final`, and it follows the ordinary reply policy.

| New work kinds | Target and prompt |
|---|---|
| `census.hunt`, `census.rate` | Assigned member; `census.hunt.md`, `census.rate.md` |
| `census.merge`, `census.final` | Case-bound judge; `census.merge.md`, `census.final.md` |
| `crosscheck.goal` | Case-bound judge; optional goal-and-constraints extraction task under the Contests and Prompt pack contracts |
| `crosscheck.inventory`, `crosscheck.cell`, `crosscheck.audit` | Assigned member; `crosscheck.inventory.md`, `crosscheck.cell.md`, `crosscheck.audit.md` |
| `crosscheck.normalize`, `crosscheck.rule` | Case-bound judge; `crosscheck.normalize.md`, `crosscheck.rule.md` |
| `survey.extract`, `survey.report` | Case-bound judge; `survey.extract.md`, `survey.report.md` |
| `survey.vote`, `survey.revote` | Assigned member; `survey.vote.md`, `survey.revote.md` |
| `probe.explain`, `probe.check`, `probe.predict` | Assigned explainer or checker; `probe.explain.md`, `probe.check.md`, `probe.predict.md` |
| `probe.report` | Case-bound judge; `probe.report.md` |

Every new work kind requires matching `contestId`, `stepId`, assignment, prompt reference and output schema. Census pass identity is resolved through the step record rather than overloading debate `round`. Judge-targeted kinds require `caseId` and `contextId = judge:{caseId}`; they use medium reasoning effort and 4,096 visible output tokens. Their repair records follow the case-bound judge contract.

Member-targeted work in the four new modes uses `contextId = contest:{contestId}:{stepId}:{nick}`. These are isolated private working histories, populated only by that step’s authorized deliveries and its own output. They do not inherit ordinary member history or another step’s history. The script explicitly delivers any needed earlier artifact into the new context. This prevents a crosscheck inventory writer from seeing a plan already present in its ordinary history and keeps census hunts, survey ballots and probe predictions blind.

A `crosscheck.audit` assignment always gets a new step-bound context with no earlier member or reviewer conversation, even when the selected model participated in inventory or review. Retrying that work retains the same frozen audit inputs and sample; it does not import the model’s other contexts. `squad-core` builds every context; Agents executes its assigned work.

Work carries references and execution metadata, not copies of topic text, solutions, instructions or previous turns. Those inputs must already exist in the authorized private history. `historyThrough` is an inclusive private-history position, frozen before the attempt runs. Attempt-scoped judge repair data is obtained separately through the case-bound work reader.

Ordinary queued work follows Agents and OpenRouter: it includes completed earlier exchanges while excluding user requests accepted after its trigger. The router fixes its execution boundary when it becomes runnable. A history position alone does not replace that eligibility rule. Scoped history reads apply the authoritative work-selection metadata before context assembly. Debate work uses the fixed boundaries required by the debate engine. Debate-turn attempts reuse the round’s frozen boundary.

An executable `Work.attempt` is positive. A queued storage record may have attempt zero and no execution boundary before its first runnable attempt.

`attempt` counts provider calls, not wake-ups, lease renewals or delivery claims. A lease has a separate increasing fence and owner identity. Internal submission authority includes that fence and the room epoch; these are not human-supplied routing fields.

A recovered delivery may reuse an unstarted attempt. Once a provider call may have started, recovery consumes that attempt and follows the owning retry policy. Each attempt makes one provider call. The orchestrator issues the next debate attempt. Judge attempts, including repair calls, use the judge policy.

A result is accepted only when its work, target, context, attempt, lease fence, room epoch and applicable contest or case state are current. Completing work, committing its result and capturing its reported actual cost are atomic.

For new mode work, acceptance also verifies the step and assignment identities, current script state and exact artifact-item coverage. A result cannot write another member’s ballot, review an unassigned cell, change a frozen prediction or advance an unrelated step.

Duplicate provider execution remains possible after an uncertain crash. The protocol guarantees one committed effect, not exactly-once provider billing.

### Request, result and feed interfaces

`Request` carries operations into the router. It does not add envelope kinds. Each durable operation has a UUIDv7 request identity. For a human message, that identity is its `messageId`.

```rust
pub enum Request {
    UserMessage {
        input: UserMessageInput,
        preconfirmed: bool,
        parsed: ParsedInput,
    },
    Stop {
        requestId: EventId,
        target: Option<MemberNick>,
    },
    Regen {
        requestId: EventId,
        messageId: Option<MessageId>,
    },
    DebateConfirm {
        requestId: EventId,
        debateId: DebateId,
        planId: PlanId,
    },
    DebateCancel {
        requestId: EventId,
        debateId: DebateId,
        planId: PlanId,
    },
    Judge {
        requestId: EventId,
        debateId: Option<DebateId>,
    },
    SetVars {
        requestId: EventId,
        updates: Vec<VariableUpdate>,
    },
    Clear {
        requestId: EventId,
    },
    Retry {
        requestId: EventId,
        workId: WorkId,
    },
    ContestPick {
        requestId: EventId,
        contestId: ContestId,
        nicks: Vec<MemberNick>,
    },
    ContestConfirm {
        requestId: EventId,
        contestId: ContestId,
        planId: PlanId,
    },
    ContestCancel {
        requestId: EventId,
        contestId: ContestId,
        planId: PlanId,
    },
    CensusContinue {
        requestId: EventId,
        contestId: ContestId,
        expectedStateVersion: u64,
        planEventId: Option<EventId>,
        replacementText: Option<Text>,
    },
    CrosscheckAccept {
        requestId: EventId,
        contestId: ContestId,
        expectedStateVersion: u64,
        planEventId: EventId,
        gridEventId: EventId,
        cellIds: Vec<CellId>,
        reason: Text,
    },
    Export { scope: ExportScope, id: Option<ContestId> },
    Who,
    Help,
    Internal(InternalRequest),
}

pub enum RequestResult {
    Accepted {
        requestId: EventId,
        eventIds: Vec<EventId>,
        cursor: Option<Cursor>,
    },
    PendingConfirmation {
        requestId: EventId,
        preflight: PendingPreflight,
        cursor: Cursor,
    },
    Rejected {
        requestId: EventId,
        error: ErrorPayload,
    },
    Internal(InternalResult),
}

pub enum FeedItem {
    Committed(CommittedEnvelope),
    Delta(ProducerEnvelope),
    Presence(ProducerEnvelope),
}
```

`FeedItem::Delta` accepts only a `delta` envelope. `FeedItem::Presence` accepts only a `presence` envelope. Neither contains commit fields. Live-versus-replay origin is a TUI-internal tag, not a protocol field.

`Stop.target: None` selects all generating work and cancels a pending pick; a nick selects that member. Optional regeneration and judge targets retain the parser-owned selection rules. `preconfirmed` carries the CLI’s `--yes` choice without adding a field to `UserMessageInput`.

`ContestPick` is accepted only against the current `AWAITING_PICK` state. It has its own request identity and creates no conversational user message. `ContestCancel` always requires a plan ID and cancels only that preflight. Cancelling a pending pick uses `Stop { target: None }` exclusively. Existing debate confirmation and cancellation requests retain their debate-specific contracts.

`CensusContinue` is accepted only at the script’s between-pass continuation point and before its admitted pass limit. An absent `planEventId` and `replacementText` keeps the current plan; exactly one may supply an eligible quoted replacement or pasted replacement text. Both together are rejected. Changed material uses the normal stale-plan and remaining-cost checks before new calls. Starting a grill while census holds the active-contest claim requires stopping or completing census first.

`CrosscheckAccept` is allowed on a `COMPLETE` crosscheck and requires the human capability, nonempty unique cell IDs and a nonempty reason. It records acceptance against the current plan, grid and state version through `crosscheck.acceptance`; it does not close a cell by invented evidence. Invalid or stale acceptance leaves the ledger unchanged. These requests define transport contracts; user-facing syntax and interaction remain owned by Parser and TUI.

An accepted result identifies its committed effects and the last event cursor of the transaction, when present. A pending-confirmation result acknowledges the committed preflight, not admission of the conversational message. Consumers use `requestId`; there is no `commandId`. Consumers requiring a cursor handle the absent case explicitly rather than treating it as a required field or inventing cursor zero.

Internal requests cover authorized producer submissions, work claims, provider-call start records, lease renewals and barrier commits. Their shared request and result types live in `squad-protocol`; producer capabilities and router handlers remain private to `squad-core`. Human submission capabilities cannot invoke them. Claim and start results carry the scoped work, lease owner, fence, epoch and applicable deadline. These are execution metadata, not additional conversation payloads.

Each ingress packet carries a `Request`, a bound producer capability and a `tokio::sync::oneshot::Sender<RequestResult>`. The packet capability and epoch are established by the room runtime, never trusted from envelope JSON.

The ingress channel is a bounded `tokio::sync::mpsc` channel with capacity 256. Sending into it is not durable acceptance. The caller receives durable acceptance only after the relevant SQLite transaction commits.

```rust
pub type FeedReceiver =
    tokio::sync::broadcast::Receiver<FeedItem>;

pub trait Bus: Send + Sync {
    fn send(
        &self,
        request: Request,
    ) -> std::pin::Pin<
        Box<dyn Future<Output = RequestResult> + Send + '_>
    >;

    fn subscribe(&self) -> FeedReceiver;

    fn wake_mailbox(
        &self,
        executor: &ExecutorKey,
    );
}
```

The one v1 implementation is in-process. The trait is object-safe and permits a later implementation without changing router, agent or TUI contracts; no later implementation is specified here.

The TUI and one-shot commands use the Room API and CLI-owned `RoomHandle`. Agents receive neither a `Bus` nor a `RoomHandle`.

Feed delivery uses `tokio::sync::broadcast` with capacity 4,096. Durable events are recoverable from `events`; deltas and presence are ephemeral. Mailboxes are capacity-1 `mpsc` channels carrying wake-ups only. `wake_mailbox` uses synchronous `try_send`; `Full` means a wake-up is already pending. Coalescing a wake-up is safe because work remains in SQLite. The router never awaits a channel owned by an agent.

### Parser-to-request mapping

Front ends parse once, allocate a stable request identity, and map by this exhaustive table. `UserMessage` carries both the original `UserMessageInput` (including attachments) and the successful `ParsedInput`; the router verifies they describe the same submission and revalidates authoritative references, settings and recipients without classifying raw text again. Inline `variable_updates` commit with that message; `SetVars` applies the whole update vector or none. Parser support for a later mode never admits that mode before its M5 implementation exists.

| ParsedInput variant or SlashAction | Exactly one Request variant |
|---|---|
| `Message` | `UserMessage` |
| `Judge` | `UserMessage` (standalone or case-follow-up routing) |
| `Debate` | `UserMessage` |
| `QuestionFirst` | `UserMessage` |
| `Tournament` | `UserMessage` |
| `Grill` | `UserMessage` |
| `Census` | `UserMessage` |
| `Crosscheck` | `UserMessage` |
| `Survey` | `UserMessage` |
| `Probe` | `UserMessage` |
| `Assignment` | `SetVars` with the complete `updates` vector |
| `ContestPick` | `ContestPick` with its pending contest ID and nicks |
| `Slash::Set` | `SetVars` with one update |
| `Slash::Stop` | `Stop` with the optional member target; pending-pick cancellation uses only `target: None` |
| `Slash::Judge` | `Judge` |
| `Slash::Regen` | `Regen` |
| `Slash::Clear` | `Clear` |
| `Slash::Continue` | `CensusContinue`; freeze waiting census/state version and optional attachment or replacement text |
| `Slash::Accept` | `CrosscheckAccept`; freeze COMPLETE crosscheck, plan/grid/state version, cells and reason |
| `Slash::Export` | `Export`; uses the same core export renderer and typed ID contract as `RoomHandle::export` |
| `Slash::Who` | `Who` |
| `Slash::Help` | `Help` |

Read-only `Export`, `Who` and `Help` return `InternalResult::Text` without events, receipts or model work. Other typed controls such as confirmation and retry originate from UI/CLI actions rather than new grammar. They use their one named Request variant.

### Shared API value types

These declarations live in `squad-protocol`; parser-owned AST types (`ParsedInput`, `ParseResult`, `PendingPick`) are imported from its parser module. `parse` is implemented there with the signature below. Repeated standard derives and serde camelCase renames are omitted as elsewhere in this section.

```rust
pub struct SubmissionMeta {
    pub quote: Vec<EventId>,
    pub replyTo: Vec<EventId>,
}

pub struct ParseContext {
    pub roster: Vec<MemberNick>,
    pub vars: std::collections::BTreeMap<VariableName, u8>,
    pub pending_pick: Option<PendingPick>,
    pub records: Vec<RecordSummary>,
    pub activeContest: Option<ContestState>,
}

pub struct RecordSummary {
    pub eventId: EventId,
    pub messageId: Option<MessageId>,
    pub seq: Cursor,
    pub kind: Text,
    pub author: Actor,
    pub recipients: Vec<Recipient>,
    pub content: String,
    pub inReplyToMessageId: Option<MessageId>,
    pub contestId: Option<ContestId>,
    pub caseId: Option<CaseId>,
    pub planEventId: Option<EventId>,
    pub gridEventId: Option<EventId>,
    pub cellIds: Vec<CellId>,
    pub stateVersion: Option<u64>,
    pub superseded: bool,
    pub eligible: bool,
}

pub fn parse(raw: &str, meta: &SubmissionMeta, ctx: &ParseContext) -> ParseResult;

#[serde(rename_all = "lowercase")]
pub enum OpenMode { Interactive, OneShot }

pub struct PromptRef {
    pub templateId: Text,
    pub packVersion: Text,
    pub templateHash: Text,
}

pub trait PromptRenderer: Send + Sync {
    fn render(
        &self,
        prompt: &PromptRef,
        values: &std::collections::BTreeMap<String, String>,
    ) -> Result<String, ReaderError>;
}

pub struct Model {
    pub nick: Actor, // Member or Judge only
    pub requestedSlug: Text,
    pub resolvedSlug: Text,
    pub rosterOrder: Option<u32>,
    pub contextLength: u64,
    pub pricing: serde_json::Value,
    pub resolvedAt: DateTime,
}

#[serde(rename_all = "lowercase")]
pub enum HistoryRole { System, User, Assistant }

// Serializes as true; rejects false during deserialization.
pub struct True;

#[serde(untagged, deny_unknown_fields)]
pub enum HistoryMetadata {
    Attributed {
        originEventId: EventId,
        originSeq: Cursor,
        author: Actor,
        projectionKey: Text,
    },
    Anonymous { anonymous: True }, // marker accepts/serializes only literal true; no additional fields
}

pub struct HistoryEntry {
    pub position: HistoryPosition,
    pub role: HistoryRole,
    pub name: Option<String>,
    pub content: String,
    pub meta: HistoryMetadata,
}

pub struct WorkRecord {
    pub workId: WorkId,
    pub messageId: MessageId,
    pub inReplyToMessageId: Option<MessageId>,
    pub kind: WorkKind,
    pub target: Target,
    pub contextId: ContextId,
    pub status: WorkStatus,
    pub attempt: u32,
    pub runnable: bool,
    pub executable: Option<Work>, // absent until historyThrough is frozen
    pub leaseUntil: Option<DateTime>,
    pub deadline: Option<DateTime>,
    pub error: Option<ErrorPayload>,
}

pub enum ReaderError {
    Forbidden,
    NotFound,
    NotRunnable,
    StaleWork,
    StoreUnavailable { message: String },
}

pub struct ExecutorBinding {
    pub model: Text,
    pub effort: ReasoningEffort,
}

pub struct ExecutionGrant {
    pub binding: ExecutorBinding,
    pub work: Work,
    pub owner: ExecutorKey,
    pub fence: u64,
    pub epoch: u64,
    pub leaseUntil: DateTime,
    pub deadline: Option<DateTime>,
}

pub enum ExecutorRequest {
    Claim { workId: WorkId },
    StartAttempt { workId: WorkId, attempt: u32, fence: u64 },
    RenewLease { workId: WorkId, fence: u64 },
    Delta { fence: u64, epoch: u64, delta: Delta },
    Presence(Presence),
    Result {
        workId: WorkId,
        attempt: u32,
        fence: u64,
        epoch: u64,
        output: EnvelopePayload,
    },
}

pub struct HistoryProjection {
    pub contextId: ContextId,
    pub originEventId: EventId,
    pub projectionKey: Text,
    pub role: HistoryRole,
    pub name: Option<String>,
    pub content: String,
    pub meta: HistoryMetadata,
}

pub struct BarrierCommit {
    pub contestId: ContestId,
    pub stepId: StepId,
    pub expectedStateVersion: u64,
    pub barrierEvent: ProducerEnvelope,
    pub projections: Vec<HistoryProjection>,
    pub nextWork: Vec<Work>,
}

pub enum InternalRequest {
    Executor(ExecutorRequest),
    Produce { envelope: ProducerEnvelope },
    Transition { envelopeId: EventId, submission: ContestTransitionSubmission },
    Barrier { requestId: EventId, commit: BarrierCommit },
}

pub enum InternalResult {
    Claimed(ExecutionGrant),
    Started(ExecutionGrant),
    LeaseRenewed { workId: WorkId, fence: u64, leaseUntil: DateTime },
    NoWork,
    Text { text: String }, // non-durable export, roster or help response
}
```

Executor results carry only `EnvelopePayload`; the router supplies the envelope identity, timestamp, recipients and correlation ID from the fenced work and admission state.

`ExecutorSubmit` binds requests to one `ExecutorKey` and the current room epoch; callers never supply a producer identity. `StartAttempt` and `RenewLease` use that bound epoch, and the router checks the grant, attempt and fence. Claim/start replies use `RequestResult::Internal`; durable producer and barrier replies use `Accepted` or `Rejected`. Deltas and presence return `Accepted` with no cursor or event IDs. `HistoryMetadata::Anonymous` serializes directly as `{anonymous:true}` (untagged on the wire), while attributed metadata contains only its declared fields. Anonymous readers expose neither the barrier event ID nor contributor source IDs.

### Structural isolation types

```rust
pub struct AgentContext {
    pub executor: ExecutorKey,
    pub mailbox: tokio::sync::mpsc::Receiver<Wakeup>,
    pub history: std::sync::Arc<dyn HistoryReader>,
    pub work: std::sync::Arc<dyn WorkReader>,
    pub submit: std::sync::Arc<dyn ExecutorSubmit>,
    pub render: std::sync::Arc<dyn PromptRenderer>,
    pub cancel: tokio_util::sync::CancellationToken,
}

#[derive(Debug, Clone, Copy)]
pub struct Wakeup;

pub struct JudgeRepairData {
    pub attempt: u32,
    pub rejectedOutput: String,
    pub validationErrors: Vec<String>,
}

pub trait HistoryReader: Send + Sync {
    fn for_work(
        &self,
        work_id: WorkId,
    ) -> std::pin::Pin<
        Box<
            dyn Future<
                Output = Result<Vec<HistoryEntry>, ReaderError>
            > + Send + '_
        >
    >;
}

pub trait WorkReader: Send + Sync {
    fn unfinished(
        &self,
    ) -> std::pin::Pin<
        Box<
            dyn Future<
                Output = Result<Vec<WorkRecord>, ReaderError>
            > + Send + '_
        >
    >;

    fn get(
        &self,
        work_id: WorkId,
    ) -> std::pin::Pin<
        Box<
            dyn Future<
                Output = Result<Option<WorkRecord>, ReaderError>
            > + Send + '_
        >
    >;

    fn repair_for_attempt(
        &self,
        work_id: WorkId,
        attempt: u32,
    ) -> std::pin::Pin<
        Box<
            dyn Future<
                Output = Result<Option<JudgeRepairData>, ReaderError>
            > + Send + '_
        >
    >;
}

pub trait ExecutorSubmit: Send + Sync {
    fn submit(
        &self,
        request: ExecutorRequest,
    ) -> std::pin::Pin<
        Box<dyn Future<Output = RequestResult> + Send + '_>
    >;
}
```

`HistoryReader`, `WorkReader`, `ExecutorSubmit` and their shared value types live in `squad-protocol`. `AgentContext` is declared here and lives in `squad-protocol`. Concrete scoped reader implementations live in `squad-store`; `squad-core` constructs them and the capability-bound submission adapter. Executors cannot name store constructors because `squad-agents` has no store or SQLite dependency. Compile-fail isolation tests live in `squad-agents`.

`HistoryEntry` exposes authorized role, content and safe projection metadata only; anonymous entries expose no source IDs or attribution. `WorkRecord` exposes this executor’s work and safe scheduling metadata. Neither exposes a database connection or unrestricted query interface. Neither exposes tournament label maps or another entrant’s identity-bearing metadata to a tournament judge.

For the four new modes, readers likewise expose no census finder map, survey source-author map, crosscheck inventory-origin map or auditor reviewer identity unless the script explicitly authorizes that projection. Anonymity applies to metadata as well as rendered text. Human feed envelopes and database source records are not reusable prompt messages without this projection.

Both readers are bound to one `ExecutorKey`. `ExecutorKey::Member(nick)` authorizes that member’s work across its nick context and all assigned step contexts; `ExecutorKey::Case(caseId)` authorizes only that case’s work. `WorkReader::unfinished` returns work across those authorized contexts, so one member task can run runtime-created step work while enforcing one active generation across all its contexts.

`HistoryReader::for_work` first verifies executor ownership and the frozen execution boundary, then binds the read to that work’s own `contextId` and eligibility rules. It accepts no caller-selected context. Guessing a foreign work ID returns `ReaderError::Forbidden`. A member cannot substitute its ordinary history for a step history or inherit prior audit inputs. `squad-core` constructs every context and the executor-keyed mailbox; wake-ups use `ExecutorKey`, never a `ContextId`.

`repair_for_attempt` verifies case ownership and the current authorised attempt, then returns only the preceding failed attempt’s repair data for the same work. Member work cannot read judge repair records.

An agent task receives exactly this capability set. It cannot query `events`, enumerate histories or acquire a feed receiver. Its `ExecutorSubmit` can submit only operations authorized for its executor key and selected work context; the router still validates current execution authority.

A judge executor receives the same shape bound to `judge:{caseId}`. It cannot read member history or another case. The orchestrator receives reference and lifecycle interfaces and never assembles prompt text.

`squad_store::open` configures WAL, synchronous=FULL and foreign_keys=ON on every connection, including readers. Read-only connections may serve typed queries under WAL mode. Only the router can request mutations from the dedicated writer thread. Workers never update leases, attempts or status directly.

### SQL schema

Room state and the machine-wide daily ledger share `<data_dir>/squad.db`. Production v1 opens only room `main`. A future room uses the same daily ledger rather than creating an independent spending allowance.

The writer enables these settings before migrations:

```sql
PRAGMA journal_mode = WAL;
PRAGMA synchronous = FULL;
PRAGMA foreign_keys = ON;
```

M1 ships one `0001_initial` migration containing the core and debate schema below. The six mode-specific table groups are declared separately under M5 schema extensions; they are not part of `0001_initial`. Generic carriers are validated only as common structure until their mode is built.

```sql
CREATE TABLE room_meta (
    room_id TEXT PRIMARY KEY CHECK (room_id = 'main'),
    schema_version INTEGER NOT NULL CHECK (schema_version >= 1),
    epoch INTEGER NOT NULL CHECK (epoch >= 1),
    next_event_seq INTEGER NOT NULL CHECK (next_event_seq >= 1),
    next_history_pos INTEGER NOT NULL CHECK (next_history_pos >= 1),
    replay_floor INTEGER CHECK (replay_floor >= 1)
) STRICT;

CREATE TABLE events (
    seq INTEGER PRIMARY KEY CHECK (seq >= 1),
    id TEXT NOT NULL UNIQUE,
    room_id TEXT NOT NULL CHECK (room_id = 'main'),
    epoch INTEGER NOT NULL CHECK (epoch >= 1),
    version INTEGER NOT NULL CHECK (version = 1),
    kind TEXT NOT NULL CHECK (kind IN (
        'user.message',
        'message.final',
        'system.notice',
        'error',
        'var.change',
        'work.state',
        'usage.record',
        'budget.state',
        'debate.preflight',
        'debate.preflight.result',
        'debate.opening',
        'debate.setup',
        'debate.state',
        'debate.turn',
        'verdict',
        'case.opened',
        'case.followup',
        'case.ruling',
        'message.superseded',
        'scores.changed',
        'room.cleared',
        'debate.round_score',
        'contest.setup',
        'contest.state',
        'contest.preflight',
        'contest.preflight.result',
        'contest.step',
        'contest.artifact',
        'contest.report',
        'contest.collecting',
        'contest.pick_requested',
        'contest.picked'
    )),
    correlation_id TEXT NOT NULL,
    causation_id TEXT,
    sender TEXT NOT NULL,
    recipients TEXT NOT NULL CHECK (
        json_valid(recipients)
        AND json_type(recipients) = 'array'
        AND json_array_length(recipients) >= 1
    ),
    payload TEXT NOT NULL CHECK (
        json_valid(payload) AND json_type(payload) = 'object'
    ),
    created_at TEXT NOT NULL,
    accepted_at TEXT NOT NULL,
    commit_start_seq INTEGER NOT NULL CHECK (commit_start_seq >= 1),
    commit_index INTEGER NOT NULL CHECK (commit_index >= 0),
    commit_size INTEGER NOT NULL CHECK (commit_size >= 1),
    CHECK (commit_index < commit_size),
    CHECK (seq = commit_start_seq + commit_index),
    UNIQUE (commit_start_seq, commit_index)
) STRICT;

CREATE INDEX events_kind_seq_idx
    ON events(kind, seq);

CREATE INDEX events_correlation_seq_idx
    ON events(correlation_id, seq);

CREATE TABLE history (
    pos INTEGER PRIMARY KEY CHECK (pos >= 1),
    context_id TEXT NOT NULL,
    event_seq INTEGER NOT NULL REFERENCES events(seq),
    origin_event_id TEXT NOT NULL REFERENCES events(id),
    projection_key TEXT NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('system', 'user', 'assistant')),
    name TEXT,
    content TEXT NOT NULL,
    meta TEXT NOT NULL CHECK (
        json_valid(meta) AND json_type(meta) = 'object'
    ),
    excluded INTEGER NOT NULL DEFAULT 0 CHECK (excluded IN (0, 1)),
    UNIQUE (context_id, origin_event_id, projection_key)
) STRICT;

CREATE INDEX history_context_pos_idx
    ON history(context_id, pos);

CREATE INDEX history_event_idx
    ON history(event_seq, context_id);

CREATE TABLE contests (
    contest_id TEXT PRIMARY KEY,
    kind TEXT NOT NULL CHECK (kind IN (
        'debate', 'tournament', 'grill',
        'census', 'crosscheck', 'survey', 'probe'
    )),
    epoch INTEGER NOT NULL CHECK (epoch >= 1),
    state_version INTEGER NOT NULL CHECK (state_version >= 0),
    state_json TEXT NOT NULL CHECK (
        json_valid(state_json) AND json_type(state_json) = 'object'
    ),
    active INTEGER NOT NULL CHECK (active IN (0, 1)),
    plan_id TEXT UNIQUE,
    plan_json TEXT CHECK (
        plan_json IS NULL OR json_valid(plan_json)
    ),
    pending_request_json TEXT CHECK (
        pending_request_json IS NULL OR json_valid(pending_request_json)
    ),
    expires_at TEXT,
    updated_seq INTEGER REFERENCES events(seq)
) STRICT;

CREATE UNIQUE INDEX contests_one_active_idx
    ON contests(active) WHERE active = 1;

CREATE TABLE contest_events (
    contest_id TEXT NOT NULL REFERENCES contests(contest_id),
    event_seq INTEGER NOT NULL REFERENCES events(seq),
    PRIMARY KEY (contest_id, event_seq)
) STRICT;

CREATE INDEX contest_events_event_idx
    ON contest_events(event_seq);

CREATE TABLE contest_answers (
    contest_id TEXT NOT NULL REFERENCES contests(contest_id),
    nick TEXT NOT NULL,
    work_id TEXT NOT NULL UNIQUE REFERENCES work(work_id),
    answer_event_seq INTEGER UNIQUE REFERENCES events(seq),
    outcome TEXT NOT NULL CHECK (outcome IN (
        'pending', 'answered', 'failed', 'stopped'
    )),
    PRIMARY KEY (contest_id, nick),
    CHECK (
        (outcome = 'answered' AND answer_event_seq IS NOT NULL)
        OR (outcome <> 'answered' AND answer_event_seq IS NULL)
    )
) STRICT;

CREATE TABLE contest_picks (
    contest_id TEXT PRIMARY KEY REFERENCES contests(contest_id),
    requested_event_seq INTEGER NOT NULL UNIQUE REFERENCES events(seq),
    eligible_json TEXT NOT NULL CHECK (
        json_valid(eligible_json)
        AND json_type(eligible_json) = 'array'
    ),
    required_count INTEGER NOT NULL CHECK (required_count IN (1, 2)),
    picked_json TEXT CHECK (
        picked_json IS NULL
        OR (
            json_valid(picked_json)
            AND json_type(picked_json) = 'array'
        )
    ),
    picked_event_seq INTEGER UNIQUE REFERENCES events(seq),
    CHECK (
        (picked_json IS NULL AND picked_event_seq IS NULL)
        OR (picked_json IS NOT NULL AND picked_event_seq IS NOT NULL)
    )
) STRICT;

CREATE TABLE cases (
    case_id TEXT PRIMARY KEY,
    context_id TEXT NOT NULL UNIQUE,
    epoch INTEGER NOT NULL CHECK (epoch >= 1),
    contest_id TEXT REFERENCES contests(contest_id),
    judge_model TEXT NOT NULL,
    material_json TEXT NOT NULL CHECK (
        json_valid(material_json) AND json_type(material_json) = 'object'
    ),
    opened_event_seq INTEGER NOT NULL REFERENCES events(seq),
    decision_event_seq INTEGER UNIQUE REFERENCES events(seq),
    state_json TEXT NOT NULL CHECK (
        json_valid(state_json) AND json_type(state_json) = 'object'
    ),
    CHECK (context_id = 'judge:' || case_id)
) STRICT;

CREATE INDEX cases_contest_idx
    ON cases(contest_id);

CREATE TABLE case_events (
    case_id TEXT NOT NULL REFERENCES cases(case_id),
    event_seq INTEGER NOT NULL REFERENCES events(seq),
    PRIMARY KEY (case_id, event_seq)
) STRICT;

CREATE INDEX case_events_event_idx
    ON case_events(event_seq);

CREATE TABLE work (
    work_id TEXT PRIMARY KEY,
    kind TEXT NOT NULL CHECK (kind IN ('reply', 'debate.turn', 'judge', 'judge.round')),
    target TEXT NOT NULL,
    context_id TEXT NOT NULL,
    trigger_event_seq INTEGER NOT NULL REFERENCES events(seq),
    trigger_event_id TEXT NOT NULL REFERENCES events(id),
    message_id TEXT NOT NULL UNIQUE,
    in_reply_to_message_id TEXT,
    history_through INTEGER CHECK (history_through >= 1),
    attempt INTEGER NOT NULL DEFAULT 0 CHECK (attempt >= 0),
    status TEXT NOT NULL CHECK (status IN (
        'queued', 'running', 'completed', 'failed', 'cancelled'
    )),
    epoch INTEGER NOT NULL CHECK (epoch >= 1),
    lease_owner TEXT,
    lease_fence INTEGER NOT NULL DEFAULT 0 CHECK (lease_fence >= 0),
    lease_until TEXT,
    execution_deadline TEXT,
    provider_may_have_started INTEGER NOT NULL DEFAULT 0
        CHECK (provider_may_have_started IN (0, 1)),
    runnable INTEGER NOT NULL DEFAULT 0 CHECK (runnable IN (0, 1)),
    held_by_contest_id TEXT REFERENCES contests(contest_id),
    scheduling_meta TEXT NOT NULL DEFAULT '{}'
        CHECK (json_valid(scheduling_meta)),
    contest_id TEXT REFERENCES contests(contest_id),
    debate_id TEXT REFERENCES contests(contest_id),
    round INTEGER CHECK (round >= 0),
    replay INTEGER CHECK (replay IN (0,1)),
    case_id TEXT REFERENCES cases(case_id),
    prompt_ref TEXT CHECK (
        prompt_ref IS NULL OR json_valid(prompt_ref)
    ),
    max_output_tokens INTEGER CHECK (max_output_tokens >= 1),
    error_json TEXT CHECK (
        error_json IS NULL OR json_valid(error_json)
    ),
    completed_event_seq INTEGER REFERENCES events(seq),
    step_id TEXT REFERENCES contest_steps(step_id),
    CHECK (kind <> 'reply' OR in_reply_to_message_id IS NOT NULL),
    CHECK (
        kind <> 'debate.turn'
        OR (debate_id IS NOT NULL AND round IS NOT NULL)
    ),
    CHECK (
        debate_id IS NULL
        OR (contest_id IS NOT NULL AND contest_id = debate_id)
    ),
    CHECK (
        round IS NULL OR contest_id IS NOT NULL
    ),
    CHECK (
        (kind IN ('judge', 'judge.round') AND target = 'judge'
            AND case_id IS NOT NULL AND context_id = 'judge:' || case_id)
        OR (kind IN ('reply', 'debate.turn') AND target <> 'judge'
            AND case_id IS NULL AND (
                context_id = target
                OR (kind = 'reply' AND contest_id IS NOT NULL AND step_id IS NOT NULL
                    AND context_id =
                        'contest:' || contest_id || ':' || step_id || ':' || target)
            ))
    ),
    CHECK (
        status <> 'running'
        OR (
            attempt >= 1
            AND history_through IS NOT NULL
            AND lease_owner IS NOT NULL
            AND lease_until IS NOT NULL
        )
    ),
    CHECK (
        provider_may_have_started = 0
        OR attempt >= 1
    ),
    CHECK (
        kind <> 'judge.round'
        OR (
            debate_id IS NOT NULL
            AND round IS NOT NULL
            AND round BETWEEN 1 AND 20
            AND attempt <= 2
            AND max_output_tokens IS NOT NULL
            AND max_output_tokens = 512
        )
    )
) STRICT;

CREATE INDEX work_context_queue_idx
    ON work(context_id, status, runnable, trigger_event_seq, work_id);

CREATE INDEX work_unfinished_lease_idx
    ON work(status, lease_until);

CREATE INDEX work_debate_idx
    ON work(debate_id, round, target);

CREATE INDEX work_contest_idx
    ON work(contest_id, round, target);

CREATE INDEX work_held_contest_idx
    ON work(held_by_contest_id) WHERE held_by_contest_id IS NOT NULL;

CREATE INDEX work_case_idx
    ON work(case_id);

CREATE UNIQUE INDEX work_round_judge_idx
    ON work(debate_id, round) WHERE kind = 'judge.round';

CREATE INDEX work_step_idx
    ON work(step_id, target);

CREATE TABLE judge_repairs (
    work_id TEXT NOT NULL REFERENCES work(work_id),
    attempt INTEGER NOT NULL CHECK (attempt >= 1),
    case_id TEXT NOT NULL REFERENCES cases(case_id),
    rejected_output TEXT NOT NULL,
    validation_errors_json TEXT NOT NULL CHECK (
        json_valid(validation_errors_json)
        AND json_type(validation_errors_json) = 'array'
    ),
    PRIMARY KEY (work_id, attempt)
) STRICT;

CREATE TABLE round_scores (
    debate_id TEXT NOT NULL REFERENCES contests(contest_id),
    case_id TEXT NOT NULL REFERENCES cases(case_id),
    round INTEGER NOT NULL CHECK (round BETWEEN 1 AND 20),
    work_id TEXT NOT NULL UNIQUE REFERENCES work(work_id),
    event_seq INTEGER NOT NULL UNIQUE REFERENCES events(seq),
    scores_json TEXT CHECK (
        scores_json IS NULL
        OR (
            json_valid(scores_json)
            AND json_type(scores_json) = 'object'
        )
    ),
    tally_json TEXT NOT NULL CHECK (
        json_valid(tally_json) AND json_type(tally_json) = 'object'
    ),
    reason TEXT,
    PRIMARY KEY (debate_id, round),
    CHECK (
        (scores_json IS NULL AND reason IS NOT NULL AND reason = 'unscored')
        OR (scores_json IS NOT NULL AND reason IS NULL)
    )
) STRICT;

CREATE INDEX round_scores_case_idx
    ON round_scores(case_id, round);

CREATE TABLE contest_steps (
    step_id TEXT PRIMARY KEY,
    contest_id TEXT NOT NULL REFERENCES contests(contest_id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 1),
    script_version TEXT NOT NULL,
    move TEXT NOT NULL CHECK (move IN (
        'collect', 'pick', 'fanout', 'turns',
        'judge', 'tally', 'barrier', 'report'
    )),
    pass INTEGER CHECK (pass BETWEEN 1 AND 5),
    state_json TEXT NOT NULL CHECK (
        json_valid(state_json) AND json_type(state_json) = 'object'
    ),
    inputs_json TEXT NOT NULL CHECK (
        json_valid(inputs_json) AND json_type(inputs_json) = 'array'
    ),
    committed INTEGER NOT NULL DEFAULT 0 CHECK (committed IN (0, 1)),
    updated_seq INTEGER REFERENCES events(seq),
    UNIQUE (contest_id, ordinal)
) STRICT;

CREATE TABLE contest_assignments (
    step_id TEXT NOT NULL REFERENCES contest_steps(step_id),
    target TEXT NOT NULL,
    context_id TEXT NOT NULL,
    payload_refs_json TEXT NOT NULL CHECK (
        json_valid(payload_refs_json)
        AND json_type(payload_refs_json) = 'array'
    ),
    assignment_json TEXT NOT NULL CHECK (
        json_valid(assignment_json) AND json_type(assignment_json) = 'object'
    ),
    work_id TEXT UNIQUE REFERENCES work(work_id),
    outcome TEXT NOT NULL CHECK (outcome IN (
        'pending', 'completed', 'skipped', 'cancelled'
    )),
    error_json TEXT CHECK (
        error_json IS NULL OR json_valid(error_json)
    ),
    PRIMARY KEY (step_id, target)
) STRICT;

CREATE TABLE mode_artifacts (
    event_seq INTEGER PRIMARY KEY REFERENCES events(seq),
    contest_id TEXT NOT NULL REFERENCES contests(contest_id),
    step_id TEXT NOT NULL REFERENCES contest_steps(step_id),
    artifact_kind TEXT NOT NULL,
    work_id TEXT UNIQUE REFERENCES work(work_id),
    source_refs_json TEXT NOT NULL CHECK (
        json_valid(source_refs_json)
        AND json_type(source_refs_json) = 'array'
    )
) STRICT;

CREATE INDEX mode_artifacts_contest_step_idx
    ON mode_artifacts(contest_id, step_id, event_seq);

CREATE TABLE contest_private_maps (
    contest_id TEXT NOT NULL REFERENCES contests(contest_id),
    scope_key TEXT NOT NULL,
    map_json TEXT NOT NULL CHECK (
        json_valid(map_json) AND json_type(map_json) = 'object'
    ),
    PRIMARY KEY (contest_id, scope_key)
) STRICT;

CREATE TABLE receipts (
    request_id TEXT PRIMARY KEY,
    epoch INTEGER NOT NULL CHECK (epoch >= 1),
    producer_identity TEXT NOT NULL,
    normalized_digest TEXT NOT NULL,
    result_json TEXT NOT NULL CHECK (json_valid(result_json)),
    committed_cursor INTEGER REFERENCES events(seq)
) STRICT;

CREATE INDEX receipts_cursor_idx
    ON receipts(committed_cursor);

CREATE TABLE vars (
    name TEXT PRIMARY KEY CHECK (name IN (
        'ROUNDS', 'GRILL_ROUNDS', 'CENSUS_PASSES'
    )),
    value INTEGER NOT NULL,
    updated_seq INTEGER REFERENCES events(seq),
    CHECK (
        (name = 'ROUNDS' AND value BETWEEN 1 AND 20)
        OR (name = 'GRILL_ROUNDS' AND value BETWEEN 1 AND 10)
        OR (name = 'CENSUS_PASSES' AND value BETWEEN 1 AND 5)
    )
) STRICT;

CREATE TABLE spend_daily (
    day TEXT PRIMARY KEY,
    spent_micros INTEGER NOT NULL CHECK (spent_micros >= 0),
    reserved_micros INTEGER NOT NULL CHECK (reserved_micros >= 0),
    unknown_calls INTEGER NOT NULL CHECK (unknown_calls >= 0)
) STRICT;

CREATE TABLE attempts (
    work_id TEXT NOT NULL,
    attempt INTEGER NOT NULL CHECK (attempt >= 1),
    epoch INTEGER NOT NULL CHECK (epoch >= 1),
    target TEXT NOT NULL,
    context_id TEXT NOT NULL,
    provider_request_id TEXT,
    model TEXT NOT NULL,
    response_model TEXT,
    day TEXT NOT NULL,
    started_at TEXT NOT NULL,
    ended_at TEXT,
    outcome TEXT NOT NULL CHECK (outcome IN (
        'completed', 'failed', 'cancelled', 'unknown'
    )),
    usage_json TEXT CHECK (
        usage_json IS NULL
        OR (
            json_valid(usage_json)
            AND json_type(usage_json) = 'object'
        )
    ),
    reported_cost_micros INTEGER CHECK (reported_cost_micros >= 0),
    estimated_cost_usd REAL CHECK (estimated_cost_usd >= 0),
    unresolved INTEGER NOT NULL CHECK (unresolved IN (0, 1)),
    debate_id TEXT,
    case_id TEXT,
    contest_id TEXT,
    PRIMARY KEY (work_id, attempt)
) STRICT;

CREATE INDEX attempts_unresolved_idx
    ON attempts(unresolved, epoch);

CREATE INDEX attempts_day_idx
    ON attempts(day);

CREATE TABLE reservations (
    reservation_id TEXT PRIMARY KEY,
    epoch INTEGER NOT NULL CHECK (epoch >= 1),
    work_id TEXT,
    attempt INTEGER CHECK (attempt >= 1),
    debate_id TEXT,
    plan_id TEXT,
    amount_usd REAL NOT NULL CHECK (amount_usd >= 0),
    unresolved INTEGER NOT NULL DEFAULT 0
        CHECK (unresolved IN (0, 1)),
    meta TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(meta)),
    contest_id TEXT,
    day TEXT NOT NULL,
    amount_micros INTEGER NOT NULL CHECK (amount_micros >= 0)
) STRICT;

CREATE INDEX reservations_work_attempt_idx
    ON reservations(work_id, attempt);

CREATE INDEX reservations_plan_idx
    ON reservations(plan_id);

CREATE INDEX reservations_unresolved_idx
    ON reservations(unresolved, epoch);

CREATE INDEX reservations_day_idx
    ON reservations(day);

CREATE TABLE scores (
    nick TEXT PRIMARY KEY,
    wins INTEGER NOT NULL CHECK (wins >= 0),
    losses INTEGER NOT NULL CHECK (losses >= 0),
    trophies INTEGER NOT NULL DEFAULT 0 CHECK (trophies >= 0),
    updated_seq INTEGER REFERENCES events(seq)
) STRICT;

CREATE TABLE catalog (
    slug TEXT PRIMARY KEY,
    context_length INTEGER NOT NULL CHECK (context_length >= 1),
    max_output INTEGER CHECK (max_output >= 1),
    pricing_json TEXT NOT NULL CHECK (json_valid(pricing_json)),
    supported_params_json TEXT NOT NULL CHECK (
        json_valid(supported_params_json)
    ),
    fetched_at TEXT NOT NULL
) STRICT;

CREATE TABLE models (
    nick TEXT PRIMARY KEY,
    role TEXT NOT NULL CHECK (role IN ('member', 'judge')),
    roster_order INTEGER UNIQUE CHECK (
        roster_order IS NULL OR roster_order >= 0
    ),
    requested_slug TEXT NOT NULL,
    resolved_slug TEXT NOT NULL,
    context_length INTEGER NOT NULL CHECK (context_length >= 1),
    pricing_json TEXT NOT NULL CHECK (json_valid(pricing_json)),
    resolved_at TEXT NOT NULL,
    CHECK (
        (role = 'member' AND roster_order IS NOT NULL)
        OR (role = 'judge' AND roster_order IS NULL)
    )
) STRICT;

CREATE TABLE superseded (
    superseded_event_id TEXT PRIMARY KEY REFERENCES events(id),
    superseded_message_id TEXT NOT NULL UNIQUE,
    member TEXT NOT NULL,
    in_reply_to_message_id TEXT NOT NULL,
    replacement_work_id TEXT NOT NULL UNIQUE REFERENCES work(work_id),
    replacement_message_id TEXT NOT NULL UNIQUE,
    marker_event_seq INTEGER NOT NULL UNIQUE REFERENCES events(seq)
) STRICT;

CREATE INDEX superseded_member_idx
    ON superseded(member, marker_event_seq);
```

#### M5 schema extensions

Each mode’s M5 phase adds its table group below and rebuilds the `events.kind` and `work.kind` CHECKs to include only that mode’s registered kinds. Contests owns payload schemas; this section owns their SQL representation. M1 does not validate future mode rows or run their migrations. Tournament work requires a replay value matching the accepted `(contest_id, round, replay)` execution; add a CHECK requiring nonnull `replay` for `judge.tournament` and reply work with a tournament round, enforced against contest kind transactionally. Grill work requires the matching contest and round. The four later modes require matching contest, step, assignment, prompt and ceiling; their member contexts are `contest:{contestId}:{stepId}:{nick}` and their judge contexts are `judge:{caseId}`. Their judge ceiling CHECK is 4096. Existing core constraints remain in force when the work table is rebuilt.

##### Tournament (M5 only)

```sql
CREATE TABLE tournament_labels (
    contest_id TEXT NOT NULL REFERENCES contests(contest_id),
    round INTEGER NOT NULL CHECK (round >= 1),
    replay INTEGER NOT NULL CHECK (replay IN (0, 1)),
    label TEXT NOT NULL,
    nick TEXT NOT NULL,
    PRIMARY KEY (contest_id, round, replay, label),
    UNIQUE (contest_id, round, replay, nick)
) STRICT;

CREATE TABLE tournament_rounds (
    contest_id TEXT NOT NULL REFERENCES contests(contest_id),
    round INTEGER NOT NULL CHECK (round >= 1),
    replay INTEGER NOT NULL CHECK (replay IN (0, 1)),
    judge_work_id TEXT UNIQUE REFERENCES work(work_id),
    ranking_event_seq INTEGER UNIQUE REFERENCES events(seq),
    state_json TEXT NOT NULL CHECK (
        json_valid(state_json) AND json_type(state_json) = 'object'
    ),
    PRIMARY KEY (contest_id, round, replay)
) STRICT;

CREATE TABLE tournament_results (
    contest_id TEXT NOT NULL REFERENCES contests(contest_id),
    round INTEGER NOT NULL CHECK (round >= 1),
    nick TEXT NOT NULL,
    entry_event_seq INTEGER REFERENCES events(seq),
    result_event_seq INTEGER NOT NULL UNIQUE REFERENCES events(seq),
    rank INTEGER NOT NULL CHECK (rank >= 1),
    score INTEGER CHECK (score BETWEEN 1 AND 10),
    critique TEXT,
    eliminated INTEGER NOT NULL CHECK (eliminated IN (0, 1)),
    PRIMARY KEY (contest_id, round, nick),
    UNIQUE (contest_id, round, rank)
) STRICT;
```

##### Grill (M5 only)

```sql
CREATE TABLE grill_plans (
    contest_id TEXT NOT NULL REFERENCES contests(contest_id),
    version INTEGER NOT NULL CHECK (version >= 1),
    round INTEGER NOT NULL CHECK (round >= 0),
    event_seq INTEGER NOT NULL UNIQUE REFERENCES events(seq),
    content TEXT NOT NULL,
    PRIMARY KEY (contest_id, version)
) STRICT;

CREATE TABLE grill_turns (
    contest_id TEXT NOT NULL REFERENCES contests(contest_id),
    round INTEGER NOT NULL CHECK (round BETWEEN 1 AND 10),
    turn INTEGER NOT NULL CHECK (turn >= 1),
    griller TEXT NOT NULL,
    angle TEXT NOT NULL,
    plan_version INTEGER NOT NULL CHECK (plan_version >= 1),
    question_event_seq INTEGER UNIQUE REFERENCES events(seq),
    answer_event_seq INTEGER UNIQUE REFERENCES events(seq),
    pass_event_seq INTEGER UNIQUE REFERENCES events(seq),
    state_json TEXT NOT NULL CHECK (
        json_valid(state_json) AND json_type(state_json) = 'object'
    ),
    PRIMARY KEY (contest_id, round, turn),
    FOREIGN KEY (contest_id, plan_version)
        REFERENCES grill_plans(contest_id, version),
    CHECK (
        pass_event_seq IS NULL
        OR (question_event_seq IS NULL AND answer_event_seq IS NULL)
    )
) STRICT;
```

##### Census (M5 only)

```sql
CREATE TABLE census_passes (
    contest_id TEXT NOT NULL REFERENCES contests(contest_id),
    pass INTEGER NOT NULL CHECK (pass BETWEEN 1 AND 5),
    plan_event_seq INTEGER NOT NULL REFERENCES events(seq),
    merged_ledger_event_seq INTEGER REFERENCES events(seq),
    final_ledger_event_seq INTEGER REFERENCES events(seq),
    report_event_seq INTEGER UNIQUE REFERENCES events(seq),
    tally_json TEXT CHECK (
        tally_json IS NULL
        OR (
            json_valid(tally_json)
            AND json_type(tally_json) = 'object'
        )
    ),
    PRIMARY KEY (contest_id, pass)
) STRICT;

CREATE TABLE census_finders (
    contest_id TEXT NOT NULL,
    pass INTEGER NOT NULL,
    gap_id TEXT NOT NULL,
    nick TEXT NOT NULL,
    hunt_event_seq INTEGER NOT NULL REFERENCES events(seq),
    PRIMARY KEY (contest_id, pass, gap_id, nick),
    FOREIGN KEY (contest_id, pass)
        REFERENCES census_passes(contest_id, pass)
) STRICT;
```

##### Crosscheck (M5 only)

```sql
CREATE TABLE crosscheck_cells (
    contest_id TEXT NOT NULL REFERENCES contests(contest_id),
    cell_id TEXT NOT NULL,
    grid_event_seq INTEGER REFERENCES events(seq),
    origin_event_seq INTEGER REFERENCES events(seq),
    items_json TEXT NOT NULL CHECK (
        json_valid(items_json) AND json_type(items_json) = 'array'
    ),
    originators_json TEXT NOT NULL CHECK (
        json_valid(originators_json)
        AND json_type(originators_json) = 'array'
    ),
    reviewer TEXT,
    review_event_seq INTEGER REFERENCES events(seq),
    ruling_event_seq INTEGER REFERENCES events(seq),
    status TEXT NOT NULL CHECK (status IN (
        'pending', 'covered', 'inapplicable', 'gap', 'accepted'
    )),
    evidence_json TEXT NOT NULL CHECK (
        json_valid(evidence_json) AND json_type(evidence_json) = 'object'
    ),
    acceptance_event_seq INTEGER REFERENCES events(seq),
    PRIMARY KEY (contest_id, cell_id),
    CHECK (status <> 'accepted' OR acceptance_event_seq IS NOT NULL),
    CHECK ((grid_event_seq IS NOT NULL AND origin_event_seq IS NULL)
        OR (grid_event_seq IS NULL AND origin_event_seq IS NOT NULL))
) STRICT;

CREATE TABLE crosscheck_audit_samples (
    step_id TEXT NOT NULL REFERENCES contest_steps(step_id),
    auditor TEXT NOT NULL,
    sample_json TEXT NOT NULL CHECK (
        json_valid(sample_json) AND json_type(sample_json) = 'array'
    ),
    audit_event_seq INTEGER REFERENCES events(seq),
    PRIMARY KEY (step_id, auditor)
) STRICT;
```

##### Survey (M5 only)

```sql
CREATE TABLE survey_claims (
    contest_id TEXT NOT NULL REFERENCES contests(contest_id),
    claim_id TEXT NOT NULL,
    extraction_event_seq INTEGER NOT NULL REFERENCES events(seq),
    claim_text TEXT NOT NULL,
    origins_json TEXT NOT NULL CHECK (
        json_valid(origins_json) AND json_type(origins_json) = 'array'
    ),
    PRIMARY KEY (contest_id, claim_id)
) STRICT;

CREATE TABLE survey_votes (
    contest_id TEXT NOT NULL,
    claim_id TEXT NOT NULL,
    ballot INTEGER NOT NULL CHECK (ballot IN (1, 2)),
    nick TEXT NOT NULL,
    vote TEXT NOT NULL CHECK (vote IN ('AGREE', 'DISAGREE', 'UNSURE')),
    confidence INTEGER NOT NULL CHECK (confidence BETWEEN 1 AND 5),
    reason TEXT NOT NULL,
    change_evidence TEXT CHECK (ballot <> 2 OR (change_evidence IS NOT NULL AND length(trim(change_evidence)) > 0)),
    event_seq INTEGER NOT NULL REFERENCES events(seq),
    PRIMARY KEY (contest_id, claim_id, ballot, nick),
    FOREIGN KEY (contest_id, claim_id)
        REFERENCES survey_claims(contest_id, claim_id)
) STRICT;
```

##### Probe (M5 only)

```sql
CREATE TABLE probe_roles (
    contest_id TEXT NOT NULL REFERENCES contests(contest_id),
    nick TEXT NOT NULL,
    role TEXT NOT NULL CHECK (role IN (
        'explainer', 'checker', 'sitting_out'
    )),
    initial_event_seq INTEGER REFERENCES events(seq),
    prediction_event_seq INTEGER REFERENCES events(seq),
    PRIMARY KEY (contest_id, nick),
    CHECK (role = 'explainer' OR prediction_event_seq IS NULL)
) STRICT;
```

Rust validation additionally enforces UUIDv7 identities and UUIDv4 anonymous item IDs, canonical dates, nick rules, unique recipients, finite money, payload schemas, matching event IDs and sequences, and all owner-defined lifecycle preconditions.

`events.payload` stores the canonical payload. Other columns reconstruct the envelope base and commit fields. `agent.reply`, `contest.transition`, `delta` and `presence` cannot enter `events`.

For attributed projections, `history.meta` preserves `originEventId`, `originSeq`, authorship and projection metadata. For each anonymous step projection, write one bundled row per destination context, with both `event_seq` and `origin_event_id` pointing to the step’s barrier event, `name = "squad"`, and `meta = {"anonymous":true}` only. Examples are `survey.selection`, `census.ledger` and the probe `contest.step` barrier. Never originate an anonymous row from an individual contributor’s event. `projection_key` deduplicates a step projection in that context; anonymous `HistoryEntry` omits source IDs. The router mints UUIDv4 item IDs or shuffled short labels at the barrier, randomizes item/reason order, and removes contributor metadata before rendering. Survey reasons are deduplicated and bundled across voters so event IDs, row boundaries and order cannot reconstruct voter groups or side counts.

`history.pos` describes delivery order. `originSeq` describes the source event’s acceptance order. A barrier allocates fresh history positions for earlier accepted turns; it does not reuse their event sequences as history positions.

`work.scheduling_meta`, `contests.state_json`, plan data and case execution state store their owners’ validated contracts without redefining them. Persisted work metadata retains the context-selection information required for ordinary queue scheduling. `contests.plan_json` stores frozen prompt settings and plan provenance; `cases.state_json` stores invocation provenance and execution state. The contest state retains its case-ID list and any prior terminal status needed for judge-only cancellation.

`round_scores` stores one immutable scored or unscored outcome per debate round, with the originating case, work and event. Rust validation verifies that the contest is a debate, the case and work belong to it, the work kind is `judge.round`, the round matches the work and does not exceed `totalRounds`, both score and tally maps have exactly the two debater keys, scores and reasons satisfy their bounds, and the tally matches the transaction’s accepted scored rounds. Round-score events are associated with both `contest_events` and `case_events`. `scores` remains the separate win-loss and trophy scoreboard.

`work.max_output_tokens` stores the visible ceiling. The `judge.round` CHECK remains 512 because the reasoning budget is added when constructing the provider request; a reasoning-capable `judge.round` request sends 1,536 total tokens with an explicit reasoning budget of 1,024.

`contest_answers` tracks only the collection work created by that question-first request. `contest_picks.eligible_json` freezes the eligible nicks and answer references. The pending pick is represented by contest state, not by a timer or an expiring plan. Its `expires_at` is null until an actual preflight plan is created.

Survey question-first collection also uses `contest_answers`, without creating `contest_picks`. Reused survey sources are frozen in the setup and step inputs rather than represented as newly executed collection work.

`tournament_labels` is router-only storage. No history reader, work reader, feed payload, export or judge prompt exposes the label-to-nick map. Human ranking events contain resolved nicks, but are never projected into the blind judge context. Entrant-identifying nick, attribution and model metadata are removed from the judge’s answer projection under the Contests anonymisation contract.

`tournament_rounds` distinguishes the initial round execution from its single permitted replay. The router validates that only one execution supplies the accepted round outcome. `tournament_results` stores each entrant’s accepted per-round result; its nullable score and critique represent an authorised forfeit without inventing model scores.

`grill_plans` retains every complete plan version. `grill_turns` links questions, answers, passes, angle assignments and plan versions for working-context selection and the exported appendix. Rust validation enforces participant membership, turn order, question bounds, matching answers and the Contests-owned context-compaction rules.

`contest_steps` and `contest_assignments` store the Contests-owned script execution contracts. Rust validation verifies that a step, its work, assignments and artifacts belong to the same contest and epoch. The assignment’s context, prompt, input projection and item allocation must match its work. A committed step cannot be rerun under a new identity to repeat its effects. Random role splits, auditor choices, audit samples and anonymisation maps are frozen in assignment or restricted map records before execution.

`mode_artifacts` indexes committed artifact events without duplicating their payload text. `contest_private_maps` retains identity-bearing provenance outside executable history. Scoped readers receive only the script-authorized projections of these records. The generic tables also support shared moves for the three existing scripts without changing their established payloads.

`census_passes` retains plan and ledger versions and the app-computed tally snapshot for each pass. `census_finders` counts each initial finder once per merged gap and pass; rating additions cannot become initial finds. Ratings, covered citations, finalized gaps, estimate bounds and discovery-curve points remain in their canonical artifact payloads. Rust validation checks their item references and recomputes Chao1 arithmetic from accepted records. `estimateLow` and `estimateHigh` are required nullable `Option<f64>` fields retained with the input gap IDs, `f1`, `f2`, formula branch and unrounded Chao1 value. Until the range rule is decided, only Chao1 is validated; bounds remain null and cannot justify a quiet-stop result. This typed census work lands in M5.P7, not M1.

`crosscheck_cells` retains every admitted grid cell and its current evidence, reviewer, ruling and explicit human acceptance. Rust validation checks inventory origins, the pre-grid cap, assignment coverage and the prohibition on self-origin review. `crosscheck_audit_samples` freezes sampled closure evidence and its plan and grid references through the assignment payload. A ruling cannot erase a cell. An acceptance is valid only for its recorded current plan and grid.

`survey_claims.origins_json` is restricted provenance. `survey_votes` stores each member’s ballot once per claim and ballot number. Rust validation requires exact assigned-claim coverage, one-line reasons, confidence bounds and second-ballot change evidence; skipped members have no fabricated vote rows. Selection, switches and report counts are recomputed from accepted ballots.

`probe_roles` retains the split and sit-out decision. Initial explanation or check events and prediction events link to immutable canonical payloads through `mode_artifacts`. Rust validation checks role eligibility, the frozen check set, complete prediction coverage and report references. Missing predictions remain explicit and cannot be inferred as support.

`attempts` and `reservations` deliberately have no foreign keys to conversational records. Required billing reconciliation can survive `/clear` without retaining conversation text. Their JSON metadata must contain accounting data only. `judge_repairs` contains conversational output and is deleted with its case and work during clear.

`spend_daily` stores integer millionths of a US dollar. Conversion of a reported decimal cost uses one deterministic rounding operation per call; reservations use a conservative upward conversion. The original reported usage remains in `attempts.usage_json`. `amount_micros` is authoritative for reservation arithmetic; `amount_usd` is its presentation value.

The attempt’s local day is fixed when its provider-call start record commits. Responses and later reconciliation update that day even if they arrive after midnight. Before a new provider call, the router rechecks the current local day and transfers any unstarted reservation to that day atomically. An in-flight call retains its original day. A new day selects a new daily row; it does not erase prior spending or unresolved calls.

Actual cost is added once for each `(work_id, attempt)`. A later correction applies only the difference from `reported_cost_micros`. A response without a cost increments `unknown_calls` once and retains the unresolved reservation under Cost guard; later reported cost replaces that unknown state atomically. Billed failures and cancellations follow the same path as successful calls.

`models::save_roster(tx, models)` atomically replaces member cache rows, preserving the judge and removing departed members. Delete/reinsert avoids roster-order UNIQUE conflicts and respects the non-null member-order constraint; invalid input or a write failure preserves the previous roster. `save_model` returns a typed domain error for invalid cache roles, roster order or context length.

`models` is a startup cache, not a configuration source. Persona text, prompts and credentials are not model-cache fields. Member rows use their nick and roster order; the judge row uses reserved key `judge`, role `judge` and no roster order. `catalog` stores normalised model metadata and fetch time for the Agents-owned 24-hour fallback policy.

Presence remains in memory. There is no durable notification queue: committed state is the recovery source.

### Ordering and commit groups

Each durable event receives a distinct increasing `seq`. A transaction inserting several events allocates a contiguous range.

All events in that transaction have the same `acceptedAt` and `commitSize`, and contiguous zero-based `commitIndex` values. Their `seq` values differ. `commit_start_seq` is storage metadata identifying the first event in the group; it is not an additional envelope field.

A barrier may update histories and work without adding a room event. Its private projections are still one atomic transaction. Attributed projections retain their source identity; an anonymous bundle uses its barrier identity under the visibility contract.

A replay checkpoint advances only after the complete event group has been applied. Within a group, consumers order by `commitIndex` and deduplicate by event ID. Repeated feed delivery cannot repeat an effect.

Page reads preserve complete groups. A requested page limit is a target when a whole group must be included. A live consumer receiving only part of a group repairs it from `events` before advancing its cursor.

### Commit transaction

This 12-step list is the single normative commit pipeline. The router serializes all mutations; the dedicated writer thread executes one transaction per commit. Other descriptions specify effects only and must follow this order.

1. Validate the submission’s structure, producer capability, room epoch and applicable execution authority.
2. Derive its stable submission identity and normalized digest without repeating mutable source resolution.
3. Begin `BEGIN IMMEDIATE`.
4. Read `receipts`. An identical accepted submission returns its stored result without mutation. Changed content under the same identity returns `IDEMPOTENCY_CONFLICT`.
5. Recheck transactional preconditions: work status, attempt, lease fence, deadlines, contest state version, case state, active-contest claim, queue admission and daily reservation requirements.
6. Resolve the operation’s canonical effects using authoritative state. Invalid operations leave conversation, variables and work unchanged.
7. Allocate event sequences from `room_meta.next_event_seq`. Insert all canonical events with complete commit metadata and advance the counter in the same transaction.
8. Allocate history positions from `room_meta.next_history_pos`. Insert only authorized content projections, preserving source identities and deduplicating existing projections. Defer ordinary inputs held by a contest until its hold is released.
9. Insert or update work, leases, scheduling flags, attempt records, reservations, variables, contest state, case state, round scores, collection and pick records, tournament outcomes, grill versions, supersession state and scores as applicable. Capture reported actual cost, release or retain reservations, update the applicable `spend_daily` row and emit `budget.state` in this transaction. A result and its terminal work transition commit together.
10. Store the submission receipt, normalized digest, result and committed cursor. Producer-result receipts use the producer envelope ID as submission identity.
11. Commit the transaction. Any failure before successful commit rolls back all its effects.
12. Update in-memory projections, complete the request’s oneshot reply, broadcast committed events in order and wake affected mailboxes using nonblocking `try_send`. No durable acceptance or notification precedes commit.

The relative scheduling of the reply and post-commit notifications does not change acceptance. A crash after commit but before either is repaired from receipts, events and unfinished work.

A verdict, its sealed case decision, both score increments and `scores.changed` are one transaction. An existing receipt, terminal work record or sealed decision prevents a second score update.

A round-score commit atomically inserts `debate.round_score`, its contest and case associations, the `round_scores` row and the judge-history projection, computes its running tally and settles the originating work. A scored outcome completes the work; an exhausted outcome records `scores: null`, `reason: "unscored"` and failed work. Neither outcome changes the win-loss scoreboard. A duplicate receipt, terminal work record or existing debate-round outcome prevents a second tally contribution.

The verdict transaction computes `roundTally` from retained scored outcomes, validates any required `tallyOverrideReason`, and delivers the verdict announcement with its round tally and per-round outcomes to both debaters. This is their first delivery of round scores. The final judge’s case input identifies unscored rounds.

Accepting `debate.turn` inserts its room event and `contest_events` association only. Private-history delivery waits for the debate-owned barrier.

For a judged debate, the numbered-round barrier also appends both turns to the judge case context and records the durable need for that round’s `judge.round` work. Allocation after the barrier is idempotent and recoverable. Scoring completion is not a precondition for starting the next round.

Collection completion, the eligible-answer snapshot and `contest.pick_requested` commit together. A valid pick, its frozen source selection and `contest.picked` commit together. Duplicate picks cannot start multiple preflights or allocate duplicate contest work.

A tournament ranking commit validates the complete label permutation, resolves its private map, commits individual result events and histories, eliminates exactly one entrant and advances the contest atomically. A champion event and trophy increment are one transaction. Grill content commits include all authorised shared-floor projections in the same transaction as their source event and work completion.

Following the [12-step pipeline](#commit-transaction), artifact acceptance in a built later mode atomically writes the canonical event, contest and applicable case associations, typed artifact index, derived rows, authorized history projections, assignment outcome, work completion and accounting. A step barrier commits its delivery decisions and recoverable next-work allocation together. Duplicate results cannot add a finder, ballot, review, prediction, risk acceptance or report twice.

Census report commits compute tally fields from the recorded initial finder sets and finalized ledger. Crosscheck ruling and acceptance commits preserve all retained cells and their evidence lineage. Survey selection and report commits validate their counts and switches against stored ballots. Probe report acceptance verifies the frozen prediction matrix and referenced checks. None of these transactions changes the scoreboard.

Provider-call start is recorded durably before issuing the call. Once that record commits, recovery treats the call as possibly started even if the process stopped before sending it. The start transaction rechecks the current local day and daily max. Actual spending at or above the max rejects new calls with `DAILY_MAX_REACHED`; already in-flight calls finish and reconcile. Contest reservations must fit the remaining daily amount before admission.

### Visibility and prompt authority

The human feed retains all accepted room activity. This is the sole projection table: fields listed here are the complete prompt-visible data, after template rendering; envelope, accounting and source metadata are excluded unless explicitly listed. All unaddressed members receive nothing. Core and debate rows are implemented in M1; the six other modes add their exact per-step field rows in their own M5 phases.

| Step or material | Authorized destination | Projected fields |
|---|---|---|
| Ordinary addressed user input | Each resolved member, after any hold release | `body`; quote `author` and frozen `text` in attachment order; attributed source identity in safe metadata |
| Ordinary completion | Author’s own member context | `content`, author nick; no other member’s reply |
| Explicit quote into standalone case | That case only | Selected quote `author`, `text`; user `question`; no other history |
| Case follow-up | Same sealed case only | Original material, accepted decision, prior addressed follow-ups, current question and explicit quote text/attribution |
| Debate round-0 input | Both debaters separately | `topicText`, `instruction`, `round=0`, admitted `rounds`; no opponent opening |
| Debate setup after selection/opening barrier | Both debaters | `topicText`, ordered `solutions[].nick` and `solutions[].text`, `instruction`, `rounds`; no round scores |
| Debate numbered-round work | Each debater | Admitted setup and all prior barrier-delivered turns; current `round` and `totalRounds`; no current opponent output |
| Debate turn barrier | Both debaters | Both outcomes in debater order: `speaker`, `round`, completed `content` or forfeit `reasonCode` and `text` |
| Debate case initialization | Its judge case | Setup topic, attributed solutions, retained instruction, debaters and rubric version; no ordinary histories |
| Debate numbered-round barrier to case | Its judge case | Both ordered turn outcomes with speaker and round; no unrelated room activity |
| Debate round scoring | Its judge case only | Target `round`, both outcomes, setup/rubric and prior authorized case material; accepted `scores`, `tally` or unscored `reason` append to this case only |
| Final debate judging | Its judge case only | `topic.{messageId,text}`, `solutions[].{nick,messageId,text}`, ordered `transcript` turn outcomes, `instruction`, frozen `rubric.{version,criteria,verdictSchema}`, `judge.{model,promptPackVersion,renderedTemplateHashes}`, `roundScoring.{roundScores,roundTally,unscoredRounds}`; no other case |
| Debate verdict announcement | Both debaters after accepted verdict | `winner`, `scores`, `reasoning`, `whatToStealFromLoser`, `tallyOverrideReason`, `weightedTotals`, `roundTally`, per-round scored/unscored outcomes |
| Question collection | Each addressed member | Question `body` and its explicit quotes; completion only to its author; selection control events add no prompt entry |
| Content notice or execution error | Explicit authorized addressee only | Sanitized `text` rendered as a user-role Squad notice; no operational metadata |
| Preflight, state, picks, steps, work, usage, budget, variables, scoreboard, supersession and clear controls | Human only | No automatic prompt fields |
| Delta and presence | Human live feed only | No history entry |
| Judge repair | Next authorized attempt, same case and work only | Previous `rejectedOutput`, `validationErrors`; never ordinary history |
| Tournament entry task | Each surviving entrant’s own member context | Frozen `prompt`, current `round`, `entrants_remaining`, ordinary word limit; own prior answers; only the same contest’s immediately preceding `rank`, `remainingBefore`, nullable `score`, nullable `critique`, `survived`; no other entrant’s answer or result |
| Tournament blind judging | Fresh case for this round and replay only | Redacted `prompt`, `round`, shuffled `entries[].label` and nullable `entries[].answer`, ranking schema; no nicks, models, personas, roster, maps, or prior-round case history |
| Tournament accepted entry | Author’s own member context only | Completed `content`; no operational generation/accounting fields; forfeits add no invented answer |
| Tournament own result barrier | Corresponding entrant only | `rank`, `remainingBefore`, nullable `score`, nullable `critique`, `survived`, logical `round`; full decoded rankings remain human-only |
| Tournament final report | Every admitted entrant | `placements[].nick`, `placements[].placement` only; no answers, per-round ranks, scores or critiques |
| Grill (M5.P6) | Declared shared floor only | Shared-floor rule; exact step field rows added in M5.P6 |
| Census hunt | One isolated member step | Redacted `plan`, `planVersion`, own `lens`; no ordinary history or peer findings |
| Census merge | Fresh census judge case | Redacted `plan`, `planVersion`, shuffled `finds[].{id,omission,consequence,lens}` and exact `sourceIds`; random handles carry no member identity |
| Census rating | One fresh isolated member step | Redacted `plan`, `planVersion`, `ledger[].{id,omission,consequence}` only; no lenses, source-find references, finder identities or counts |
| Census finalization | Fresh census judge case | Redacted `plan`, `planVersion`, canonical gap dispositions and accepted vote breakdown/citations, anonymous addition handles; finder identities and missing-member identities excluded |
| Census plan, pass report and human drilldown | Human only | Immutable plan versions, attributed hunts/ratings, finder table, omissions/consequences/lenses, citations, missing participation, Chao1 provenance, nullable range and discovery curve; no automatic ordinary-member history projection |
| Crosscheck goal extraction | Fresh contest-bound judge case only | Frozen `plan`; response `goal`, `constraints`, `uncertainties`; no ordinary history |
| Crosscheck inventory | Fresh assigned member step only | `goal`, `constraints`, `uncertainties`, privately assigned `lens`; no plan, inventory peers or ordinary history |
| Crosscheck normalization | Fresh contest-bound judge case only | Goal/constraints and inventory event references, lens and three local item IDs/text/rationale per accepted inventory; no plan |
| Crosscheck review | Assigned member step only | Frozen `plan` and assigned `cells[].{cellId,items}`; no originators, reviewer map, unassigned cells or prior history |
| Crosscheck audit | Selected auditor in fresh step only | Frozen `plan`, anonymous `grid[].{cellId,items}` and persisted sampled `cellId,status,citation,reason,proposedFix`; no reviewer/originator identities or previous contexts |
| Crosscheck ruling | Fresh contest-bound judge case only | Frozen `plan`, retained grid, accepted reviews and audits with case references; no unrelated histories |
| Crosscheck report and explicit acceptance | Human only | Full coverage ledger, cap omissions, residual risks, failures and version-bound acceptance; no automatic working-history entry |
| Tournament (M5.P5) | Own entrant context or blind case only | Own answers/results; anonymous judging; final placements shared; exact field rows added in M5.P5 |
| Survey optional collection | Each participant's isolated collection step | `question`; ordinary reply schema and room word limit; no peer answers or prior room history |
| Survey extraction | Its fresh judge case only | Frozen `question`, accepted `answers[].{nick,messageId,text}`; private source references remain case/router-only |
| Survey first ballot | Each participant's fresh assigned ballot context | Redacted `question`, `claims[].{claimId,claim}` only; no provenance, whole answers, other ballots, confidence aggregates or counts |
| Survey re-vote | Each participant's fresh assigned second-ballot context | Redacted `question`, selected `claims[].{claimId,claim,agreeReasons,disagreeReasons,unsureReasons}`; deduplicated anonymous reasons bundled under `survey.selection` origin; no voter grouping, provenance or counts |
| Survey report | Its fresh judge case only | Frozen `setup`, `claims`, accepted `first` and `second` ballots, application `map[].{claimId,first,finalTally,notRevoted,switches,bands}`; no automatic report delivery to member contexts |
| Probe (M5.P10) | Isolated role/step/case only | Blind proposals; own explanation and frozen anonymous checks for prediction; exact field rows added in M5.P10 |

Raters receive gap id, omission and consequence only; no lenses. The current plan is a separate task input. Anonymous projections use one bundled row per step and destination, barrier origin, `name="squad"`, metadata `{anonymous:true}` only and barrier-minted UUIDv4 IDs or shuffled labels. No `HistoryEntry` exposes contributor source IDs, event timestamps, row grouping or source order. Text anonymization follows the deterministic [Contests redaction rule](08-contests-debate-tournament-grill-census-crosscheck-survey-probe.md#blind-text-redaction). Every mode’s M5 visibility tests must prove these common restrictions with the real store.

An ordinary reply’s recipient is `me`; retaining it in its author’s history does not route it to another member. Debate source sharing happens through setup, not by changing the original reply’s recipients.

Tournament entrants never receive another entrant’s answers, per-round ranks, scores or critiques. The final placing list is the sole automatic cross-entrant result disclosure. The blind judge receives no entrant nick, model identity or label map. The grill shared floor is a deliberate exception to ordinary mutual invisibility, limited to its declared participants.

Blind steps in the four new modes are enforced by stored deliveries and scoped readers. A member receives no forbidden artifact through its ordinary history, another step, quotes, report projections, source attribution or operational metadata. Anonymised content carries no member nick, model name or finder count. Human-visible counts and identities remain separate from member prompt projections.

Only the base member prompt uses `role: "system"`. Notices, forfeits and verdict announcements use `role: "user", name: "squad"` with the prefix `[squad notice]`. Their quoted or generated text never becomes system authority. `debate.setup` and `debate.opening` use their Prompt pack templates with user role and name `squad`. Prompt rendering and role assembly follow Prompt pack and Agents and OpenRouter.

### Router algorithm

All durable operations follow the [12-step commit transaction](#commit-transaction), including idempotency before mutable resolution and notification after commit. Front ends call `parse` and map its AST to `Request` through the table below; the router validates that supplied AST and metadata against authoritative room state, variables and records. It never interprets mentions-only `UserMessage` text as a pick. It resolves quotes and canonical recipients, derives permitted effects, and commits staged variable updates atomically with the operation. Model output is never parsed. Ephemeral delta/presence handling rechecks authority and updates in-memory state only.

Census, crosscheck, survey and probe preparation resolves the script’s sources and participant rules before preflight. Admission freezes the script settings and per-step inputs; it does not broadcast the raw command or all source quotes into every member’s working history. Subsequent artifacts advance only the already admitted script. Census continuation and crosscheck risk acceptance use their versioned request contracts.

For multi-recipient ordinary messages, per-member queue or model failures are independent execution outcomes. Eligible recipients continue; an overloaded recipient receives an explicit error and no undisclosed work allocation. No accepted work is silently discarded.

A completed ordinary reply creates no further ordinary work. Model text is never parsed for routing, commands, variables, spending permission or case creation. Completion of collection work may advance its already accepted contest under the orchestrator’s contract.

### Durable delivery, interruption and startup recovery

A request waiting only in an in-process channel is not durable. After a crash, its caller may resubmit the same identity. If the transaction committed, the receipt returns the original result; otherwise the operation is processed once it is resubmitted.

Durable work delivery and notification repair may repeat. Receipts and terminal work fencing provide one committed effect per accepted submission or result.

Workers inspect their scoped unfinished work on startup, after wake-ups and on periodic recovery scans, subject to the room’s open mode. A mailbox carries no work payload and is never the sole record of pending work.

`OpenMode` is the Protocol-owned M1 enum used by recovery; `RoomHandle::open` in M3 consumes it. M1 recovery tests use an open-mode runtime harness, not the M3 API. Interactive open resumes eligible unfinished work. One-shot open never re-wakes or progresses work it did not create. `squad models` and `squad doctor` read the `models` cache; they do not open the room, probe aliases or start executors. Alias probes run only during room open under synthetic work IDs with context `probe:{nick}` and normal attempt accounting. The synthetic ID is used only by `attempts`, `reservations` and `UsageRecord`, not by a fabricated conversational `work` row; the private startup capability authorizes these accounting-only records, which survive clear. `--no-wait` exits after commit and prints the accepted cursor; its work runs at the next interactive open.

Startup proceeds in this order:

1. Acquire the exclusive room lock.
2. Open the database, apply migrations and enable WAL, full synchronous commits and foreign-key enforcement.
3. Restore the room epoch, event and history counters, replay floor, stored variables, daily spending, scores, round scores, contest state, pending picks, cases, receipts and work.
4. Apply the Agents and OpenRouter startup model-resolution contract through the router-owned writer.
5. Invalidate execution ownership left by the previous process. Advance lease fences before assigning recovered work.
6. Distinguish unstarted attempts from calls that may have started. Reuse an unstarted ordinal when permitted; consume uncertain calls and retain their unresolved accounting reservations.
7. Restore deadlines, held-work and deferred-history metadata, repair records and terminal tombstones. Apply contest and judge recovery according to Contests.
8. Construct scoped agent contexts and spawn case-bound judge executors for eligible unfinished work. Rebuild presence as a fresh in-memory snapshot.
9. In interactive mode, re-wake every context with eligible unfinished work and start periodic work scans. In one-shot mode, enable only work created by that invocation. Expose the recovered room state before new progression.

A crash after commit but before a mailbox wake cannot lose work. A crash after result commit but before the oneshot reply cannot duplicate the result.

Recovery reconciles committed numbered-round barriers with `judge.round` work and `round_scores`, allocating any missing required work once. It preserves the two-attempt limit, restores scored and unscored outcomes without recounting them, and keeps final `judge` work gated until all required round-scoring work has settled.

Recovery also restores question collection, non-expiring pending picks, tournament label maps and replay counts, individual results, grill turn order and plan versions. It does not repeat an elimination, trophy increment or shared-floor projection.

For census, crosscheck, survey and probe, recovery restores script versions, step identities, assignments, isolated histories, frozen sources, committed artifacts, skipped-member outcomes and pending barrier allocations. It restores census pass and finder records, crosscheck grids and sampled closures, human acceptances, survey ballots and selected claims, and probe roles and locked predictions. It does not reroll assignments, repeat a completed tally, reaccept a risk or import ordinary member history into a recovered step. The script determines which remaining work is eligible to resume.

The router serializes cancellation, supersession, clear and result acceptance. The first accepted terminal transition wins. A late result cannot revive cancelled work, reopen a terminal contest automatically or write into a cleared room. Late accounting remains eligible for reconciliation without accepting stale conversational content.

Global and targeted stop requests follow Agents and OpenRouter and Contests. A stopped debate keeps committed content and skips automatic judging. Stopping the debate cancels outstanding `judge.round` work and prevents further automatic round-scoring allocation. Explicit `/judge` uses the retained transcript and whatever round scores exist; it scores no rounds retroactively.

Stopping any of the four new modes retains its committed artifacts and fences further automatic progression under the script’s stop policy. A stopped or incomplete run cannot acquire a successful stop-rule result from missing work. Billed attempts remain accounted for.

Database failure prevents durable acceptance and invokes Hardening’s cancellation and recovery behavior. A final buffered only in process memory is not accepted work completion. A successful durable result follows a successful SQLite commit.

### Room lock

Before opening the room for mutation, Squad acquires an exclusive advisory lock on `<data_dir>/main.lock` and records its process ID while holding that lock.

A second process attempting to open the same room exits with:

`Room main is already open in another squad process (pid N).`

`N` is the owning process ID. A PID written in an unlocked file is not proof of ownership; the operating-system lock determines ownership. Process exit releases the lock. The runtime holds it until work has stopped and the writer has closed.

The data directory defaults to `~/.local/share/squad` on Linux and `~/Library/Application Support/squad` on macOS, with the Room API and CLI-defined overrides.

The store creates the database, lock, WAL, shared-memory and auxiliary files with mode 0600. The data directory is private to the user. File creation and reopening enforce the Hardening permission contract without exposing credentials.

Read-only markdown export may open a consistent database snapshot while another process holds the room lock. It cannot mutate room state or acquire agent capabilities.

### Replay, paging and storage lifecycle

The store exposes ordered bounded event reads, complete commit-group reads, current replay boundaries and durable high-water marks.

On start, the TUI pages `events` backward from the newest sequence. Older pages load on scroll. Room API and CLI owns the public paging behavior.

For live initialization, the room API subscribes to the feed before capturing a durable high-water mark, reads the required durable events through that mark, then reconciles buffered notifications by event ID and sequence. Broadcast lag triggers durable catch-up from the last applied cursor. Periodic reconciliation repairs missed notifications.

Replay includes every canonical durable kind, including errors, variable changes, preflight results, case events, round scores, collection and pick events, tournament and grill events, supersession, work state, daily spending and scoreboard updates. It excludes submission-only kinds and ephemeral traffic.

Replay also includes all census, crosscheck, survey and probe setup, state, artifact and report events. Stored state and step identities reconstruct banners and dividers without persisted visual-effect events. Replay and export do not rerun model calls or app tallies.

Round-score replay retains each event’s round identity and acceptance-time tally. Boundary banners are reconstructed from contest state transitions and pick requests for TUI rendering and markdown export; no banner records are persisted.

Presence comes from a fresh snapshot. Current durable work state restores pending displays. Incomplete token text is discarded after restart or a gap that cannot be reconstructed; missed chunks are never represented as recovered history. An accepted final remains authoritative.

A malformed or future cursor returns `CURSOR_INVALID`. A cursor preceding the replay floor after `/clear` returns `CURSOR_EXPIRED` with `earliestCursor` and requires an explicit reset of the displayed transcript. A successful read never silently substitutes a truncated recovery transcript.

An empty room has no cursor; omission or `null` in an owning API’s snapshot represents that condition. Zero is not a valid `Cursor`.

There is no automatic retention or trimming in v1. Events, histories, contests, cases, round scores, receipts and terminal work records survive until `/clear`. Daily spending records survive `/clear`.

The clear transaction:

1. Advances the epoch and fences or cancels existing work.
2. Preserves daily spending and the accounting records and reservations still required by Cost guard, without conversational content.
3. Deletes every event with seq below the `room.cleared` event in foreign-key-safe order; accounting survives only in `attempts`, `reservations` and `spend_daily`. Deletes room conversation rows, private histories, round scores, contest and case records, collection and pick records, tournament rankings and label maps, grill turns and plans, repair data, supersession state, scores, obsolete work and prior receipts in foreign-key-safe order.
4. Restores configured room-variable defaults and recomputes the daily spending projection under Cost guard without resetting actual spending or unknown-cost counts.
5. Preserves monotonic event and history allocation counters.
6. Inserts `room.cleared` with the new epoch and the previous durable high-water mark.
7. Sets the replay floor to the reset event and stores the clear request’s receipt.
8. Commits, signals cancellation, resets in-memory projections and broadcasts the reset.

Step 3 also deletes the four modes’ derived rows, private maps, artifacts, assignments and step records in foreign-key-safe order, including isolated working histories. Work references to steps and assignment references to work are removed in an order that preserves foreign-key enforcement.

A retry of the clear request returns its receipt rather than clearing again. Once the transaction commits, old-epoch executor results are rejected.

`VACUUM` runs after the clear transaction, outside any transaction. If it cannot complete immediately, cleanup is retried without reversing the committed reset.

The storage ceiling defined by Hardening rejects new generation admission with `STORAGE_LIMIT`. Cleanup, export, accounting reconciliation and completion of already accepted work remain possible. This section adds no retention configuration.

