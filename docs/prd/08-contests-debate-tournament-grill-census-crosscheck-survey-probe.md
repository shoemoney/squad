# Contests: debate, tournament, grill, census, crosscheck, survey, probe

### Part 1: Specification

### Ownership and shared contracts

Squad is one self-contained Rust binary running one process, with one TUI front end per room. The orchestrator coordinates contests and judge cases. The router is the only writer of durable events, contest state, transcripts, private histories, work, budget reservations, and score records.

This section owns the shared contest engine, all seven state machines and mode scripts, admission, preflight estimates, contest event payloads, `DebateState`, the `debate.setup` payload, the verdict schema, contest deadlines, and contest attempt limits. This section owns each mode’s typed payloads. Once a mode is built, its Rust types in `squad-protocol` are the normative contract. M1 implements the core and debate typed contracts only; grill, census, crosscheck, tournament, survey and probe payloads, statuses, mode-specific tables and visibility tests are added in their respective M5 phases. Protocol (02) owns generic carriers and shared interfaces. Orchestration lives in `crates/squad-core/src/orchestrator/`.

Other contracts retain their owners:

- **Protocol, store and bus:** envelope kinds and common fields, `Work`, storage schema, `Cursor`, `ErrorCode`, `Request`, `RequestResult`, the canonical `DebatePreflight` and `DebateRoundScore` payloads, commit ordering, receipts, publication after commit, and work fencing.
- **Command grammar and parser:** AST, grammar, instruction extraction, solution-selection metadata, validation precedence, and exact user-facing errors.
- **Foundations:** `SquadConfig`, environment variables, and `RoomState`.
- **Room API and CLI:** `RoomHandle`, feed delivery, and command entry points.
- **Agents and OpenRouter:** model resolution, ordinary work queues, context assembly, provider transport, and accounting.
- **Prompt pack:** prompt text, template IDs, placeholders, and overrides. The prompt renderer and template loading are implemented in M2.

Lifecycle signals use Protocol’s `ContestTransitionSubmission { contestId, kind, expectedStateVersion, transition }` through `InternalRequest::Transition`, with the standard submission identity. Each mode adds its typed `ContestTransition` variant in its M5 phase. They are committed as kind-specific state events rather than introducing a separate envelope kind for each transition.

The router uses the transactional API in `squad-store`. A dedicated writer thread accesses SQLite in WAL mode using `rusqlite` with its `bundled` feature. Live communication uses the in-process `Bus` implementation and tokio channels. Events, history, work, and state commit before feed publication or mailbox wake-ups.

The workspace contains six crates: `squad-protocol`, `squad-store`, `squad-agents`, `squad-core`, `squad-tui`, and `squad-cli`. Executors, the OpenRouter client, and the model registry live in `squad-agents`. That crate depends on `squad-protocol` and reader/submit traits, never on `squad-store` or `rusqlite`. `squad-core` constructs context-bound capabilities and hands them to executors. Compile-fail tests enforcing this boundary live in `squad-agents`.

Canonical values are defined in [Protocol’s canonical numbers and configuration](02-message-protocol-store-and-bus.md#canonical-numbers-and-configuration).

### Execution limits

Mode deadlines and attempt policies are owned here; visible ceilings, reasoning budgets and general limits come from [Protocol’s canonical table](02-message-protocol-store-and-bus.md#canonical-numbers-and-configuration). Prompt word limits are owned by Prompt pack.

| Call | Deadline and attempts |
|---|---|
| Debate turn | 180 s; 2 attempts, the second issued by the orchestrator, then forfeit |
| Final judge | 300 s; 3 attempts including transport retries and at most one schema repair |
| `judge.round` | 120 s; 2 attempts including repair, then unscored |
| `judge.tournament` | 120 s; 3 attempts including repair; one round replay after exhaustion |
| Grill | 180 s; 2 attempts, the second issued by the orchestrator |
| Census, crosscheck, survey and probe member calls | 180 s; 2 attempts; the second is a plain retry without repair data |
| Census, crosscheck, survey and probe judge calls | 300 s; 3 attempts including transport retries and at most one schema repair |

### Blind text redaction

Projections replace, case-insensitively and on word boundaries, every roster nick, configured and resolved slug, slug provider and model-family token, and persona text with `[redacted]`. The human transcript is unchanged. Build the token set from the startup roster and model snapshots; match longer phrases before shorter tokens with Unicode word boundaries. Apply this deterministic replacement before rendering every blind answer, claim, reason, check and projected citation, as well as its metadata allowlist. Persist private lens permutations and tie-break seeds outside prompt-readable storage. Mode M5 fixtures cover case variants, provider/family tokens, multiword personas, punctuation boundaries, overlapping tokens and unchanged human originals.

Every output ceiling limits visible output. For a model whose catalog entry lists reasoning support, the request sends `max_tokens` equal to the visible ceiling plus the selected reasoning budget and sends that budget as `reasoning.max_tokens`. Reasoning parameters are omitted for models without reasoning support; an explicitly configured unsupported reasoning setting follows the Agents section’s `MODEL_PARAMETER_UNSUPPORTED` contract. Estimates and context checks include both visible and reasoning token allowances.

`SquadConfig` is the shared typed configuration struct in `crates/squad-protocol/src/config.rs`. Foundations owns its environment-variable loading and validation. Configuration comes from one `.env` file, with process environment variables taking precedence.

`SQUAD_ROUNDS` seeds `ROUNDS` only when the room has no stored value. `$ROUNDS=n` and `/set ROUNDS n` update the room variable at runtime; the stored value wins on restart. An admitted debate retains its frozen round count.

`SQUAD_GRILL_ROUNDS` seeds `GRILL_ROUNDS` only when the room has no stored value. `$GRILL_ROUNDS=n` and `/set GRILL_ROUNDS n` update it at runtime. An admitted grill retains its frozen round count.

`SQUAD_CENSUS_PASSES` seeds `CENSUS_PASSES` only when the room has no stored value. `$CENSUS_PASSES=n` and `/set CENSUS_PASSES n` update it at runtime. An admitted census retains its frozen pass count. The environment variable is optional and is not included in `.env.example`.

The relevant optional environment variables are `SQUAD_DAILY_MAX_USD`, `SQUAD_DEBATE_CONFIRM_USD`, `SQUAD_ROOM_WORDS`, `SQUAD_TURN_WORDS`, `SQUAD_FINAL_TURN_WORDS`, `SQUAD_PLAN_WORDS`, `SQUAD_GRILL_ROUNDS`, `SQUAD_CENSUS_PASSES`, `SQUAD_REASONING_EFFORT`, `SQUAD_REASONING_EFFORT_<NICK>`, `SQUAD_JUDGE_REASONING_EFFORT`, `SQUAD_JUDGE_MODEL`, `SQUAD_JUDGE_FALLBACKS`, and `SQUAD_PROMPTS_DIR`. Their definitions remain owned by Foundations. Timeouts, attempt counts, queue depth, visible output ceilings, reasoning budgets, channel capacities, lease intervals, plan expiry, and message size limits are compile-time constants in `squad-protocol`.

Foundations owns `setup.sh`, including creation of `.env` from `.env.example` without overwriting an existing file.

### Shared contest engine

Debate, tournament, grill, census, crosscheck, survey and probe use one orchestrator and one active-contest claim per room. The claim covers answer collection, a pending pick, preparation, confirmation, queue draining, execution, judging, replay, between-pass waiting, and stopping. A competing start is refused with the parser-owned string:

`A {kind} is already running. Stop it before starting another.`

The shared engine provides admission, context checks, cost preflight against the remaining daily amount, participant holds, durable work scheduling, attempt fencing, stop handling, restart recovery, banners, case execution, and export records. All seven modes are scripts over the same moves. Build order is debate, grill, census, crosscheck, tournament, survey, probe.

The storage layout for contests and their event associations is owned by **Protocol, store and bus**. All seven modes use that canonical schema and its transaction API. This section does not create a competing set of tables. Frozen plan provenance lives in the canonical contest plan JSON; judge provenance and case state live in `cases.state_json`.

### Engine moves and script contract

| Move | Execution | Model calls |
|---|---|---|
| `collect` | Addressed members answer one prompt in parallel, mutually invisible. | One per member. |
| `pick` | The human chooses k members from those who answered, using the inline picking flow. | None. |
| `fanout` | Each addressed member receives its own authorized payload and acts in parallel. Payloads may contain assigned lenses, assigned cells, or that member’s own results. | One per member. |
| `turns` | Members act sequentially in a persisted order, seeing only the material the script delivers. | One per turn. |
| `judge` | One structured call on the contest’s judge case context, with strict schema validation and repair. | One. |
| `tally` | The application computes counts, scores, estimates, rankings, or assignments from stored data. | None. |
| `barrier` | Commit a resolved batch and deliver the script’s authorized projections atomically. | None. |
| `report` | Assemble the final artifact and markdown export from committed records. A script may supply one final judge call. | None, or one judge call explicitly counted by the script. |

A script declares its states and transitions and, for each step, the move, actors, input references, visibility projection, prompt template, output schema, word limit, deadline, attempt allowance, failure rule, and delivery recipients. Non-model moves declare no prompt and no generated output. Their outputs are application-computed records.

The engine persists the script version, step identity, round or pass, assignments, authorized history boundaries, work identities, attempt counters, accepted artifacts, delivery markers, and state version. State transitions reference accepted artifacts rather than carrying another copy of their text. Step completion requires every scheduled slot to have an accepted result or a durable terminal failure.

A `barrier` never means broadcast to the whole room. Its delivery set and projection belong to the script. A human-facing result may commit before participant delivery, as debate turns already do. Participant delivery remains atomic and idempotent.

Blindness is structural. The router writes only the script’s authorized material into an actor’s context. It does not copy hidden material and ask the prompt to ignore it. A step requiring ignorance of earlier material uses a fresh, restricted context; retaining a member’s ordinary history must not expose the hidden plan, another answer, a finder table, or prior reviewer work. The **Agents and OpenRouter** section owns execution and context assembly.

Anonymous projections contain no nick, model name, persona attribution, finder count, vote count, or routing metadata unless that step explicitly permits it. Identity-bearing source records remain available to the human and to authorized router computations. Content that identifies an author is removed from anonymous projections without changing the original human transcript.

Preflight walks the script, expands bounded loops and fanouts, and counts each `judge` once, including a judge-backed `report`. It shows normal planned calls separately from reserved provider attempts. Existing debate, tournament and grill counts below remain unchanged. Retries and schema repairs consume the same per-work attempt allowance; only tournament has the specified round replay.

New modes consume the protocol owner’s envelope kinds, Work kinds, storage contracts, visibility rows, Requests, and ErrorCodes. The step and artifact contracts here define their behavior and payload requirements without introducing a second bus or work schema.

### Existing modes as scripts

These scripts preserve the detailed behavior specified below.

| Debate step | Move, actors and visibility | Template, output and limit | Failure rule |
|---|---|---|---|
| Question-first answers, when needed | `collect` addressed members; each sees the question and its own ordinary history, never another candidate answer. | Ordinary reply template and reply schema; `roomWords`. | Ordinary retry; insufficient eligible answers fails collection. |
| Select debaters, when needed | `pick(2)` by the human; selected source snapshots are frozen. | No prompt; `ContestPicked`. | Invalid picks preserve the pending pick. |
| Opening positions, when needed | `fanout` to both debaters; topic and instruction, no opponent opening. | `debate.round0.md`; completed turn or forfeit; `turnWords`. | Two attempts per slot, then forfeit. |
| Opening delivery | `barrier` to both debaters; freeze opening outcomes as solutions and deliver setup. | No prompt; `DebateSetup`. | Recover the same barrier without duplicate delivery. |
| Each numbered round | `fanout` to both debaters at frozen boundaries; opponent solution in round 1, committed previous turns thereafter. | `debate.round1.md` or `debate.rebuttal.md`, plus `debate.closing.md` in the final round; turn outcome; `turnWords` or `finalTurnWords`. | Two attempts per slot, then forfeit. |
| Round delivery | `barrier` to both debaters and, when requested, the judge case. | No prompt; both attributed outcomes. | Both slots must resolve before advancing. |
| Round scoring, when requested | `judge` on the case, followed by `tally`; scores go only to the human and case before verdict. Later turns do not wait. | `judge.round.md`; `RoundJudgeResponse`; one reason of at most 200 characters per debater. | Two attempts including repair, then unscored. |
| Final verdict, when requested | After scoring resolves, `judge`, `tally`, then `report` and atomic delivery to both debaters, human and case. | Existing final judge template and verdict schema; existing final-judge limits. | Three attempts with at most one repair; exhaustion fails judging. |
| Unjudged completion | `report` from committed turns. | No prompt; retained transcript and terminal status. | No judge work or score update. |

| Grill step | Move, actors and visibility | Template, output and limit | Failure rule |
|---|---|---|---|
| Question-first answers and selection, when needed | `collect`, then `pick(1)`; candidate answers remain mutually invisible. | Ordinary reply template, then `ContestPicked`; `roomWords` for replies. | Ordinary retry; invalid picks remain pending; insufficient answers fail collection. |
| Shared-floor setup | `barrier` to exactly the leader and grillers. | `GrillSetup`; no model call. | Frozen plan and participant set are required. |
| Each griller and leader exchange | `turns` in the persisted rotated order; everyone on the floor receives each committed question and answer before the next turn. | `grill.question.md`: one to three questions or exact `PASS`, 200 words. `grill.answer.md`: every question answered with plan changes, `turnWords`. | Griller exhaustion produces an attributed skip; leader exhaustion fails the grill. |
| Round rebuild | `turns` with the leader, then `barrier` to the floor. | `grill.rebuild.md`; complete `GrillPlan`; `planWords`. | Two attempts, then failure retaining the last good plan. |
| Completion | `report` from the last good plan and attributed appendix. | `GrillComplete`; no model call. | All voluntary passes allow early completion; failed skips do not count as passes. |

| Tournament step | Move, actors and visibility | Template, output and limit | Failure rule |
|---|---|---|---|
| Each round’s entries | `collect` from survivors in round 1; `fanout` thereafter with only each survivor’s own previous result. Entries remain mutually invisible. | `tournament.entrant.md`; `TournamentEntry`; `roomWords`. | Ordinary retry, then forfeit. |
| Blind judging input | `barrier` to the judge case with newly shuffled anonymous labels; identity map remains router-only. | No model call; blind current-round projection. | Identity-bearing text cannot enter the judge projection. |
| Ranking | `judge` on current anonymised entries. | `tournament.judge.md`; `TournamentJudgeResponse`; existing tournament-judge limits. | Three attempts; one replay of the logical round after exhaustion; replay exhaustion fails the tournament. |
| Elimination and own-result delivery | `tally`, then `barrier`; human sees full ranking, each entrant sees only its own result using `tournament.result.md`. | Validated ranks, elimination and `TournamentResult`; no additional model call. | Exactly one elimination per accepted round; failed judging commits none. |
| Champion | `report` and `barrier` using `tournament.final.md`; original entrants receive final placements only. | `TournamentChampion`; no model call. | Champion, placements and trophy commit once. |

The common identity, selection and generic carrier types (`ContestKind`, `ContestSelectionStatus`, `ContestAnswer`, `ContestCollecting`, `ContestPickRequested`, `ContestPicked`, `ContestSelectionState`) are declared once in [Protocol’s shared contest contract](02-message-protocol-store-and-bus.md#shared-contest-protocol). `requiredPicks` is two for debate and one for grill. A tournament never has selection state.

### Question-first contests and picking

The canonical question-first debate flow is:

`@all $debate <question>` → ordinary answers → `CHOOSE YOUR DEBATE BROS` → `@grok @gemini` → preflight → rounds → verdict.

The canonical question-first grill flow is:

`@all $grill <question>` → ordinary answers → `CHOOSE WHO GETS GRILLED` → one selected leader → preflight → grill rounds → final plan.

The parser owns the full direct-versus-question-first decision table. Exactly two explicit member mentions without `@all` select direct debate. `@all`, including exclusions, or three or more explicit member mentions select question-first debate and require free text. A single leader selects direct grill. Multiple mentions with free text select question-first grill; the existing leader-first, explicitly named-grillers form without free text remains direct.

For question-first debate or grill, the router atomically claims the active contest and commits the question with ordinary `reply` work for the resolved recipients. These requests use ordinary queues, limits, retries, visibility, and daily accounting. They do not acquire execution holds. The room remains usable during collection and while awaiting a pick.

Only work created for that collection contributes candidate answers. Later follow-ups are ordinary messages and do not replace those candidates. When all addressed slots have answered, errored, or been stopped, the router freezes the accepted answer snapshots and eligible nicks.

| State | Event or condition | Required action |
|---|---|---|
| No active contest | Accepted question-first command | Claim the active contest, commit `contest.collecting`, and enter `COLLECTING_ANSWERS`. |
| `COLLECTING_ANSWERS` | One addressed slot resolves | Retain its accepted answer or terminal failure; continue collecting the remaining slots. |
| `COLLECTING_ANSWERS` | All slots resolve with enough answers | Enter `AWAITING_PICK` and commit `contest.pick_requested`. |
| `COLLECTING_ANSWERS` | All slots resolve with fewer than two eligible answers | Enter `FAILED`, report the insufficient-answers string, and release the claim. |
| `AWAITING_PICK` | Valid pick | Commit `contest.picked`, freeze the selected sources, and enter the selected mode’s preparation state. |
| `AWAITING_PICK` | Invalid pick | Return the parser-owned error and preserve the pending pick. |
| `AWAITING_PICK` | Ordinary message or command unrelated to picking | Handle it normally and preserve the pending pick. |
| `AWAITING_PICK` | Accepted global stop | Enter `STOPPED` with reason `pick_cancelled`, preserving all ordinary answers. |
| `AWAITING_PICK` | Restart | Restore the same candidates and pending pick without expiry or new answer calls. |

The insufficient-answers string is:

`Not enough answers to start. {n} of {m} members answered.`

A pick message consists only of mentions. Debate requires two distinct eligible members. Grill requires one eligible leader; every other member that answered becomes a griller. Both modes therefore require at least two eligible answers overall.

Wrong picks leave the contest pending and use the parser’s exact strings:

- `Pick exactly two of: @a, @b, @c.`
- `Pick exactly one of: @a, @b, @c.`
- `@{nick} did not answer, so it cannot be picked.`

A picked debate uses the collection question as its topic and the selected answers as its two solutions, skips round 0, and requests the judge by default. A picked grill uses the selected answer as plan v1 and the collection question as its focus. Sources remain frozen even if later ordinary replies or regeneration change a member’s latest reply.

Preflight and confirmation occur after the pick. Collection calls have already been accounted for as ordinary replies and are excluded from that later estimate. A cancelled or rejected execution plan preserves the question and answers.

The TUI owns the inline pick banners, hints, placeholder, `pick pending` status, and eligible-first autocomplete. Effects owns the candidate highlights. Escape follows the TUI’s transient-dismissal and confirmed-global-stop contract; accepting that stop while a pick is pending produces `pick_cancelled`.

### Durable debate record

Each debate retains its ID, ordered debaters, originating command, topic, selected source IDs, source text snapshots, instruction, round count, automatic-judge choice, resolved member and judge models, judge-swap notice, rubric version, prompt-pack version, rendered-template hashes, effective limits, budget reservation, state version, round results, attempts, deadlines, case IDs, and terminal reason.

For automatically judged debates, retain each numbered round’s scoring work, attempt count, accepted score or unscored outcome, and router-computed tally.

The source snapshots and effective settings are frozen for the accepted plan. Later room-variable assignments, regenerated replies, or configuration changes do not rewrite an admitted debate.

Debate state and transcript associations use the canonical contest storage contract. Use the canonical event, history, work, reservation, daily-spend, score, case, receipt, and round-score records through the **Protocol, store and bus** contracts. The orchestrator never writes an alternative transcript or storage namespace.

One contest may be active in room `main`, including during preparation, confirmation, waiting for participants, and judging. Other members remain available for ordinary chat.

### DebateState contract

The Rust representation is:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DebateStatus {
    Preparing,
    AwaitingConfirmation,
    WaitingForDebaters,
    RoundRunning,
    RoundCommitting,
    Judging,
    Stopping,
    Completed,
    CompleteUnjudged,
    Stopped,
    Failed,
    Rejected,
    Cancelled,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DebateState {
    pub debate_id: DebateId,
    pub state: DebateStatus,
    pub debaters: [MemberNick; 2],
    pub round: u32,
    pub total_rounds: u32,
    pub judge_model: String,
    pub judge_requested: bool,
    pub state_version: u64,
    pub terminal_reason: Option<String>, // nullable
    pub case_id: Option<CaseId>, // nullable
    pub case_ids: Vec<CaseId>,
}
```

`EventId` and `MemberNick` use the protocol owner’s types and validation. Debaters must be distinct configured members. `round` is an integer from zero through `totalRounds`; zero also represents preparation before the first numbered round. `stateVersion` increases on every accepted state transition. `terminalReason` is null during normal progression and otherwise records a sanitized reason. An absent `caseId` is null.

`caseIds` retains every case created for the debate. `caseId` identifies the current invocation’s case or the accepted verdict’s case. Judge-only planning retains a snapshot of the prior terminal status, terminal reason, and case association so cancellation or expiry can restore them without discarding the case list.

Question-first collection and picking use `ContestSelectionState` before two debaters exist. After a pick, the same contest identity becomes `debateId` and the engine creates `DebateState`.

`judgeRequested` records the parser’s automatic-judge choice: explicit `@judge` for a direct debate and true by default for a picked debate. It is frozen at admission. `judgeModel` identifies the resolved effective judge, including when automatic judging is disabled.

The router commits `debate.state` to the human log only. It is included in event paging and `RoomHandle::state()` through the shared `DebateState` type. State events do not enter member histories.

### State transitions

| State | Event or condition | Required action |
|---|---|---|
| `PREPARING` | Valid direct `$debate` or accepted debate pick | Claim or retain the active-contest pointer, freeze the proposed inputs, select the judge, and calculate cost and context bounds without provider calls. |
| `PREPARING` | Valid plan | Always emit `debate.preflight`; auto-confirm when eligible, otherwise enter `AWAITING_CONFIRMATION`. |
| `PREPARING` | Invalid sources, unavailable required model, insufficient daily amount, or oversized required context | Enter `REJECTED`, report the canonical error, and release the pointer. |
| `AWAITING_CONFIRMATION` | Matching `debate.confirm` | Revalidate the plan and atomically admit it. |
| `AWAITING_CONFIRMATION` | `debate.cancel` | Enter `CANCELLED` and release the pointer. |
| `AWAITING_CONFIRMATION` | Plan expires | Enter `EXPIRED` and release the pointer. |
| Confirmed admission | Either debater has earlier ordinary work | Enter `WAITING_FOR_DEBATERS`. |
| Confirmed admission or `WAITING_FOR_DEBATERS` | Both debaters have drained earlier ordinary work | Start round 0 if required; otherwise start round 1. |
| `ROUND_RUNNING` | Round entered | Persist both work items and their history boundaries before either dispatch. |
| `ROUND_RUNNING` | One result or exhausted retry | Record that slot’s completed turn or forfeit; wait for the other slot. |
| `ROUND_RUNNING` | Both slots resolved | Enter `ROUND_COMMITTING`. |
| `ROUND_COMMITTING` | Round 0 barrier committed | Freeze the opening positions as solutions, commit `debate.setup`, then start round 1. |
| `ROUND_COMMITTING` | Numbered round barrier committed, more rounds remain | If `judgeRequested` is true, append both outcomes to the judge case and enqueue `judge.round`; start the next round without waiting for scoring. |
| `ROUND_COMMITTING` | Final numbered round committed, `judgeRequested` is true | Append both outcomes to the judge case, enqueue `judge.round`, and enter `JUDGING`; defer final `judge` work until every round-scoring work item has completed or been given up. |
| `ROUND_COMMITTING` | Final numbered round committed, `judgeRequested` is false | Enter `COMPLETE_UNJUDGED` and release the debate reservation and member holds. |
| `JUDGING` | Valid verdict accepted | Atomically commit the verdict, seal its case, update the scoreboard, and enter `COMPLETED`. |
| `JUDGING` | Final judge attempts exhausted or required evidence cannot be supplied | Enter `FAILED`; preserve the transcript and report the canonical error. |
| Any active state | Applicable stop request | Enter `STOPPING`, fence outstanding work including `judge.round`, and request cancellation. |
| `STOPPING` | Cancellation committed | Enter `STOPPED`; retain completed records and release unused reservations and holds. |
| Any active state | Unrecoverable integrity or admission failure | Enter `FAILED`, preserve committed evidence, and release unused resources. |
| `ROUND_RUNNING`, `ROUND_COMMITTING`, or `JUDGING` | Valid round score accepted | Commit `debate.round_score` and its updated tally without delaying debate turns. |
| `ROUND_RUNNING`, `ROUND_COMMITTING`, or `JUDGING` | Both round-scoring attempts fail | Commit an unscored `debate.round_score`; continue the debate. |
| `JUDGING` | Every round-scoring work item has completed or been given up | Enqueue final `judge` work with accepted scores and explicit unscored-round information. |
| `STOPPED`, `COMPLETE_UNJUDGED`, or eligible `FAILED` | Accepted `/judge` | Claim the active contest, save the prior terminal snapshot, and enter `PREPARING` for a judge-only plan. |
| Judge-only `PREPARING` | Valid plan | Emit preflight; auto-confirm when eligible, otherwise enter `AWAITING_CONFIRMATION`. |
| Judge-only `PREPARING` or `AWAITING_CONFIRMATION` | Plan rejected | Restore the prior terminal snapshot, report the canonical error, and release the claim. |
| Judge-only `AWAITING_CONFIRMATION` | Matching confirmation | Admit the judge-only plan, create a fresh case, append its ID, and enter `JUDGING` directly. |
| Judge-only `AWAITING_CONFIRMATION` | Cancellation | Restore the prior terminal snapshot and release the claim. |
| Judge-only `AWAITING_CONFIRMATION` | Plan expires | Restore the prior terminal snapshot and release the claim. |

The corresponding transition discriminants are defined in `squad-protocol`; each variant corresponds to one row above:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DebateTransition {
    Prepare,
    PlanReady,
    PlanRejected,
    Confirm,
    Cancel,
    Expire,
    WaitForDebaters,
    DebatersReady,
    RoundEntered,
    SlotResolved {
        outcome_event_id: EventId,
    },
    RoundResolved,
    OpeningBarrierCommitted,
    NumberedBarrierCommitted,
    JudgedFinalBarrierCommitted,
    UnjudgedFinalBarrierCommitted,
    VerdictAccepted {
        verdict_event_id: EventId,
    },
    FinalJudgingFailed,
    StopRequested,
    StopCommitted,
    UnrecoverableFailure,
    RoundScoreAccepted {
        score_event_id: EventId,
    },
    RoundScoreExhausted {
        work_id: WorkId,
    },
    RoundScoringFinished,
    JudgeOnlyRequested,
    JudgeOnlyPlanReady,
    JudgeOnlyPlanRejected,
    JudgeOnlyConfirmed,
    JudgeOnlyCancelled,
    JudgeOnlyExpired,
}
```

Transitions use Protocol’s `ContestTransitionSubmission`, with `contestId` equal to the debate’s `DebateId`, `kind = Debate`, and `ContestTransition::Debate` carrying the discriminant. Referenced result records must belong to the current fenced work, attempt, debate, and round. The stored plan supplies frozen inputs; transition messages do not carry a second copy of conversational evidence.

Terminal states are `COMPLETED`, `COMPLETE_UNJUDGED`, `STOPPED`, `FAILED`, `REJECTED`, `CANCELLED`, and `EXPIRED`. An eligible terminal debate may re-enter judge preparation through `/judge`.

Duplicate transition submissions return their existing receipt. A stale expected state version cannot create work, append a result, release another reservation, or change the scoreboard.

### Source selection and round 0

A direct `$debate` requires exactly two distinct explicit member mentions. The parser supplies the instruction, round snapshot, automatic-judge choice, and optional solution selections. Question-first forms follow the collection and pick contract above.

For existing solutions:

1. Explicit `replyTo` selections take precedence and follow the parser’s limit and validation rules.
2. Otherwise, use each debater’s most recent completed, unsuperseded ordinary reply to `me` before the command’s source-selection cutoff.
3. Each reply must identify the user message it answered.
4. Two selected solutions must answer the same user message. That user message supplies the topic.

Streaming fragments, debate turns, judge output, superseded replies, and replies created after the cutoff are ineligible. Invalid explicit selections and mismatched topics use the parser table’s exact errors.

A fresh-topic debate is valid. If either debater lacks an eligible reply on the topic, run round 0 for both. When one usable source establishes the topic, retain that topic but replace both starting solutions with opening positions. When neither source exists, the retained user instruction supplies the fresh topic. Invalid or mismatched explicit selections are not silently converted into a fresh-topic debate.

Round 0 uses `debate.round0.md`. Both participants state their positions independently and simultaneously. Each chooses its own side unless the user assigned sides. Round 0 does not count toward `ROUNDS` and is not scored.

The router places the topic and instruction into each debater’s authorized history before opening work starts. An opening request contains no opponent opening position. The round 0 barrier records both outcomes, and those outcomes become the solutions in `debate.setup`. An exhausted opening attempt remains an attributed forfeit with no substantive position; the application never invents a replacement solution.

### Preflight and admission

The canonical `DebatePreflight` and `DebateEstimate` contracts are owned by **Protocol, store and bus**. This section defines no duplicate preflight type. The payload includes `debaters`, `rounds`, `judgeRequested`, `openingRoundRequired`, `confirmationRequired`, optional `judgeModel`, and an estimate with `estimatedUsd`, `reservedUsd`, and `remainingUsd`.

Amounts are finite nonnegative USD values. `expiresAt` is a UTC ISO 8601 timestamp. The plan binds the source snapshots, topic, instruction, round count, opening-round requirement, automatic-judge choice, models, prompt snapshot, output limits, pricing snapshot, and reservation ceiling.

`debate.preflight` travels as a `FeedItem` through `RoomHandle::subscribe()`. `debate.confirm` and `debate.cancel` travel as `Request` values through `RoomHandle::submit()`. Their shared API definitions remain owned by **Protocol, store and bus**.

The TUI and `squad send` show the topic, solutions or opening-round requirement, rounds, effective judge, applicable warning, planned model-call count, estimated cost, reservation ceiling, and remaining daily amount. Every mode’s preflight states its model-call count and estimate.

For automatic judging, preflight includes one `judge.round` call per numbered round plus the final judge call, with their respective attempt allowances, reasoning effort, visible output ceilings, reasoning budgets, and accumulating case context. Round 0 adds no scoring call. Without automatic judging, preflight includes no round-scoring or final judge calls.

For a debate with r numbered rounds and an opening indicator o of zero or one, the normal call count is `2 × (r + o)`, plus `r + 1` when automatic judging is requested. The reservation covers `4 × (r + o)` member attempts, plus `2 × r + 3` judge attempts when requested. Judge-only preflight shows one planned final judge call with up to three attempts.

`debate.preflight` is always emitted for a valid plan. The threshold comparison uses `estimate.estimatedUsd`. If it is at or below `SQUAD_DEBATE_CONFIRM_USD`, the orchestrator auto-confirms. Above the threshold, it waits for a matching confirmation or cancellation. `squad send --yes` pre-confirms through the same admission path.

The same confirmation threshold applies to tournament, grill, census, crosscheck, survey and probe preflights. Their estimates include the mode-specific counts below; reservations include retries, repairs, and the tournament replay allowance. Confirmation rechecks the remaining daily amount atomically with admission.

Confirmation does not authorize a changed plan. Expired plans or changed inputs require a new preflight. Duplicate confirmations are idempotent.

Only confirmed admission commits a direct `$debate` user message. It creates no ordinary `reply` work. For debates with existing solutions, admission commits the command and `debate.setup` together. For opening-round debates, admission commits the command and opening inputs; `debate.setup` follows the round 0 barrier because its solutions do not yet exist. A question-first command and its ordinary answers were already committed during collection; admission records the picked execution without duplicating that command or creating more ordinary answer work.

Admission atomically reserves the required spending and establishes holds for the participants. Existing in-flight and queued ordinary work drains first. Ordinary messages accepted after the hold are queued until the contest reaches a terminal state, using the queue contract owned by **Agents and OpenRouter**. Waiting consumes no contest provider-call deadline.

Held messages and their quotes are committed to the human log but are not written into a held participant’s private history until its hold is released. Release appends them in accepted order before their ordinary work becomes runnable. A message addressed to both held and unheld members is delivered immediately only to the unheld members. Debate-turn attempts reuse the round’s frozen boundary.

### debate.setup contract

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DebateSolution {
    pub nick: MemberNick,
    pub message_id: EventId,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DebateSetup {
    pub debate_id: DebateId,
    pub topic_message_id: EventId,
    pub topic_text: String,
    pub solutions: [DebateSolution; 2],
    pub instruction: String,
    pub rounds: u32,
    pub judge_model: String,
    pub judge_swapped: bool,
}
```

The solution order matches `DebateState.debaters`. Each nick appears exactly once. `rounds` uses the canonical `ROUNDS` bounds. The instruction may be empty; topic and solution text must contain the supplied material or an explicit forfeit record.

For existing solutions, `messageId` identifies the selected reply. For round 0, it identifies the accepted opening outcome. The fresh-topic command supplies `topicMessageId` when there is no earlier topic message.

The router delivers `debate.setup` to `me` and both debaters. Other members receive nothing. The judge receives this material only through its case bundle.

Round 0 history entries and the resulting setup must not duplicate the same source text in a provider request. Context assembly preserves each supplied piece of evidence once.

The **Prompt pack** owns `debate.setup.md` and `debate.opening.md`. Their projections use `role: "user", name: "squad"`; they are not additional system prompts.

### Parallel round execution

Use the canonical `Work` record from **Protocol, store and bus**, stored in the `work` table under its `context_id`. Do not define another job schema, use `jobId`, or put work-control records in conversational history. Agent mailboxes carry wake-ups only; workers read the durable work record.

Work carries references and execution metadata. Topic text, instructions, solutions, and previous turns come from router-written authorized history, never from a second text-bearing work payload.

For every round:

1. The router commits the round task and captures both history boundaries before dispatch.
2. Both work items become runnable without waiting for either participant’s result.
3. Workers build prompts only from their own private histories through those boundaries.
4. Accepted results wait behind the round barrier before reaching either debater’s history.

Both attempts for a slot use the same frozen history boundary. Newly accepted ordinary messages, quotes, and the opponent’s current-round outcome cannot enter a retry’s context.

Round 1 uses `debate.round1.md` and the opponent’s solution. Later rounds use `debate.rebuttal.md` and the opponent’s accepted previous-round outcome. The final numbered round also uses `debate.closing.md`, including when `ROUNDS` is one. A participant produces one turn containing both the round response and closing statement.

The **Prompt pack** owns placeholder mapping and the compact round metadata object. It must not repeat the topic, solutions, or previous turns already present in history.

Debaters concede individual points when justified while continuing to defend their overall proposals. The judge rewards honest concessions and supported revisions.

### Visibility, ordering, and round barriers

| Material | Human | Debaters | Other members | Judge |
|---|---|---|---|---|
| Debate setup | Always | Both | No | In case bundle |
| Debate state | Always | No history delivery | No | No |
| Turn deltas | Live | No history entry | No | No |
| Accepted turn | Always | Both, only at the round barrier | No | In case bundle |
| Verdict | Always | Both, including round tally and per-round scores | No | In its case history |
| Debate round score | `me` only | No history delivery before verdict | No | Judge case context |

The canonical visibility matrix, including tournament entries, blind labels, and the grill shared floor, remains owned by **Protocol, store and bus**. The mode-specific rules below are its required execution behavior.

Accepted `debate.turn` events enter the canonical event and contest-event records immediately in one router transaction. They do not enter member histories until both round slots resolve.

The barrier is a separate atomic router transaction. It appends both outcomes to both histories in stable debater order, records each turn’s original `events.seq`, and marks the round committed. Each history row receives its own `history.pos` and references the original event through `event_seq`. Event sequences and history positions follow the protocol owner’s ordering contract.

For automatically judged debates, the router also appends each numbered round’s two outcomes to the debate’s judge case at the barrier. After that commit, the orchestrator enqueues one canonical `judge.round` work item for the round. This does not block dispatch of the next debate round.

A worker’s own completed turn uses the assistant role. The opponent’s completed turn uses an attributed user role. Notices, forfeits, and verdict announcements use `role: "user", name: "squad"` with the prefix `[squad notice]`. Only the base room prompt uses the system role.

Completed turns retain the canonical `finishReason`, including `length` and `content_filter`, so truncation or filtering remains visible to the judge. Forfeit `reasonCode` is `timeout` or `error`.

No next-round work may dispatch before the barrier commits. Replaying or recovering a barrier cannot duplicate history entries.

Each agent receives the canonical `AgentContext` with its mailbox, context-bound `HistoryReader` and `WorkReader`, submission sender, and cancellation handle. Those readers expose only that context. Agents receive no access to `events`, the feed, or another context.

The router never awaits a channel owned by an agent. Mailbox wake-ups use synchronous `try_send`; a full capacity-one mailbox already has a pending wake-up.

Model mentions remain plain output text. Neither debaters nor judges can route messages, summon models, change variables, or authorize spending.

### Per-round judge scoring

Only debates admitted with `judgeRequested` true receive round scoring. A debate that ends `COMPLETE_UNJUDGED` has no round scoring at all.

After each numbered round’s barrier commits, the orchestrator enqueues one `judge.round` work item on the debate’s case context `judge:{caseId}`. The work uses the canonical `Work` record, identifies its debate and round, and reads authorized evidence through the case-bound history reader. Round 0 opening positions are never scored.

The judge uses `judge.round.md` with the judge system prompt. It scores only the identified round’s two turns for argument quality, honesty of concessions, and how well each answered the other’s last attack. Each debater receives an integer score from 1 through 10 and a one-sentence reason of at most 200 characters. Equal round scores are allowed.

The provider output is strict JSON with exactly the two actual debater nicks:

```json
{
  "scores": {
    "<nickA>": {"score": 7, "reason": "One sentence explaining this round's score."},
    "<nickB>": {"score": 8, "reason": "One sentence explaining this round's score."}
  }
}
```

The provider response reuses the protocol owner’s canonical `RoundScore`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RoundJudgeResponse {
    pub scores: BTreeMap<MemberNick, RoundScore>,
}
```

`RoundScore` and the committed `DebateRoundScore` payload are defined only by **Protocol, store and bus**. `RoundScore` contains integer `score` and string `reason`. `DebateRoundScore` supplies `debateId`, `caseId`, `round`, nullable `scores`, router-computed `tally`, and the optional unscored `reason`.

Validation rejects additional properties, missing or extra debater nicks, noninteger or out-of-range scores, and empty or overlong reasons. `round` must identify a committed numbered round of the debate. The provider supplies neither identifiers nor tally.

A `judge.round` call uses low reasoning effort, a 1,024-token reasoning budget, a 512-token visible output ceiling, and a 120 s deadline. For a reasoning-capable model, `max_tokens` is 1,536. It permits two provider-call attempts in total, including transport retries and schema repair. If both fail, the router commits `debate.round_score` with `scores: null` and `reason: "unscored"`. A successful score omits `reason`. Round-scoring failure does not fail the debate.

The router commits `debate.round_score` addressed to `me` only, writes its durable record through `round_scores`, and appends it to the judge case history. The accepted judge score is the judge’s own output in that case context. It never enters either debater’s private history before the verdict.

`tally` contains exactly the two debater nicks and is the router-computed running sum of accepted round scores at that commit. Unscored rounds add nothing. Duplicate submissions, retries, replay, and recovery cannot add a round twice. Each event retains its `round`, so a late score remains associated with the correct round.

Scoring does not block debate turns. Round scores may arrive after later turns. Final `judge` work is enqueued only after every `judge.round` work item for the debate has completed or been given up. Its case evidence includes all accepted round scores and identifies unscored rounds.

`/stop` cancels and fences outstanding `judge.round` work. `/judge` on a stopped debate produces a final verdict from the retained transcript and whatever round scores exist; it does not score rounds retroactively.

### Judge selection

The default judge uses a fresh instance of the startup-resolved Fable model, separate from the member `fable`.

A debate judge conflicts when its resolved concrete model equals either debater’s resolved concrete model, regardless of nick. Select the first nonconflicting configured fallback, defaulting to Claude’s model and then OpenAI’s model. Skip unconfigured default fallback nicks, participating fallback members, and models that match either debater. Validate explicitly configured fallback entries at startup.

The preflight warning is exactly:

`Judge model changed to {judge_model_slug} because @{nick} is debating.`

`{nick}` identifies the conflicting debater; use stable debater order when both match. The same concrete-model conflict check applies to standalone judging against the models that authored the selected material, using that invocation’s applicable canonical notice.

Judge sources and fallback configuration use `SquadConfig`, populated from `SQUAD_JUDGE_MODEL` and `SQUAD_JUDGE_FALLBACKS`. Resolution and availability checks use **Agents and OpenRouter**. An unavailable or conflicting required judge is reported through the canonical error contract. A member persona is never applied to the judge.

Tournaments are judged blind and never swap the judge because an entrant uses the same model. Grill has no judge.

The selected model and prompt snapshot remain fixed for that invocation. Every completion uses the probed concrete slug. Each attempt records the response `model`; mismatches produce the Agents section’s warning and doctor diagnostic. Final judge requests use the judge’s reasoning setting; `judge.round` uses low reasoning effort. Neither forwards reasoning text.

Census, crosscheck, survey and probe use separate contest-bound judge cases with the judge system prompt and their mode-specific task templates. These calls use medium reasoning effort and a 4,096-token visible output ceiling. They do not select a winning member or update competitive scores.

### Cases and isolated judging

Every initial judge summon creates a new `caseId`. A debate verdict is a case. Case state lives in `cases`; its history rows use `context_id` equal to `judge:{caseId}`.

The router commits the authorized material to that case history and enqueues canonical `judge` or `judge.round` work as applicable. A judge executor in `crates/squad-agents/src/agents/` builds the request only from the case-bound readers. It receives the same restricted `AgentContext` shape as a member worker, bound to that case. The orchestrator selects record references and coordinates execution; it never assembles prompt text by filtering the room log.

The router spawns an executor for each case with unfinished work, including on recovery, and retires it when no runnable or unfinished work remains. Reopening a case creates a new executor with capabilities for that same case. Executors make one provider call per attempt.

An automatically judged debate creates its case before the first numbered round is scored. The case starts with topic, solutions, and instruction, then accumulates each committed round’s turns and the judge’s own round scores. The same case supplies the final verdict.

The debate adjudication bundle is:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdjudicationTopic {
    pub message_id: EventId,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RubricCriterion {
    pub criterion_id: CriterionId,
    pub weight_percent: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrozenDebateRubric {
    pub version: String,
    pub criteria: Vec<RubricCriterion>,
    pub verdict_schema: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JudgeProvenance {
    pub model: String,
    pub prompt_pack_version: String,
    pub rendered_template_hashes: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DebateScoringEvidence {
    pub round_scores: Vec<DebateRoundScore>,
    pub round_tally: BTreeMap<MemberNick, u32>,
    pub unscored_rounds: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DebateAdjudicationBundle {
    pub topic: AdjudicationTopic,
    pub solutions: [DebateSolution; 2],
    pub transcript: Vec<DebateTurn>,
    pub instruction: String,
    pub rubric: FrozenDebateRubric,
    pub judge: JudgeProvenance,
    pub round_scoring: DebateScoringEvidence,
}
```

`DebateTurn` and `DebateRoundScore` reuse the protocol-owned payloads. `transcript` preserves round 0 when present, completed turns, forfeits, and finish reasons. Round scoring identifies existing accepted scores and committed unscored outcomes; unfinished scoring remains unfinished work until accepted or given up.

A debate’s case evidence contains exactly:

1. The topic and source ID.
2. Both solutions with authors and source IDs.
3. The committed debate transcript, including round 0 when present, forfeits, and recorded finish reasons.
4. The retained user instruction.
5. The frozen rubric and required verdict schema.
6. Effective judge identity and prompt metadata.
7. Existing round scores, their router-computed tally, and explicit identification of unscored rounds for the final verdict.

The bundle deduplicates opening positions that also serve as solutions. It excludes unrelated room messages, other members’ answers, member histories, personas, and credentials.

`CaseOpened.workId` is optional under the protocol contract. An automatic debate case may open before any scoring work exists. Its initial bundle holds the topic, solutions, instruction, rubric, and judge identity, with empty transcript and scoring collections. Barriers and accepted scores append evidence later.

For judge work, context assembly sends only the judge system prompt plus one rendered user message containing the authorized case bundle. It does not also send those case-history records as separate provider messages.

A repair request uses the same case evidence plus the invalid response and validation errors, treated as data. Invalid responses never become accepted verdicts or round scores.

Repair data is persisted in an attempt-scoped record keyed by `(work_id, attempt)` and exposed only through that case’s `WorkReader`. A retry or recovered executor reads the rejected output and validation errors there. Work remains reference-based; rejected output does not enter authoritative case history.

Case histories survive completion so follow-ups can use them. `/clear` removes them with the rest of the room.

### Standalone judge and case follow-ups

The parser permits `@judge` in three contexts:

- Inside `$debate`, requesting per-round scoring and automatic judging after the final numbered round.
- As the sole recipient with at least one quoted message, opening a new case.
- As the sole recipient quoting a verdict, reopening that verdict’s case.

Otherwise, use the exact parser error:

`Quote at least one message for @judge to rule on.`

For a new standalone case, the router supplies exactly the quoted messages with their original authors and the user’s question. Quote resolution follows **Protocol, store and bus**. The judge starts with empty prior context and returns a free-form ruling, not the scored debate verdict schema. A standalone ruling has a `caseId` and does not update debate records or the scoreboard.

Replying to a verdict card submits a quote of that verdict. It does not use ordinary-message `replyTo`, which remains reserved for debate solution selection.

A case follow-up receives that case’s original material, its accepted verdict or ruling, prior follow-ups and judge replies from that case, and the current question. It receives no material from another case or the member `fable`. Follow-ups append to the case conversation without changing the sealed verdict, scores, or win/loss result. A new summon without a case verdict reference starts blank.

Standalone rulings and follow-up answers go to `me` and their case history. They do not enter member histories unless the human later quotes them to those members.

### Verdict schema and rubric

The scored debate verdict has one strict schema. Provider output contains:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum CriterionId {
    Fit,
    Feasibility,
    Originality,
    SurvivedAttacks,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CriterionScore {
    pub criterion_id: CriterionId,
    pub score: u8,
    pub reasoning: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct JudgeScore {
    pub nick: MemberNick,
    pub criteria: Vec<CriterionScore>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JudgeVerdict {
    pub winner: MemberNick,
    pub scores: Vec<JudgeScore>,
    pub reasoning: String,
    pub what_to_steal_from_loser: Vec<String>,
    pub tally_override_reason: Option<String>, // nullable
}
```

The committed verdict adds locally computed totals:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeightedTotal {
    pub nick: MemberNick,
    pub total: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Verdict {
    #[serde(flatten)]
    pub verdict: JudgeVerdict,
    pub weighted_totals: Vec<WeightedTotal>,
    pub round_tally: BTreeMap<MemberNick, u32>,
}
```

Use `schemars` and `jsonschema` for the verdict JSON Schema. Validate in two passes: first validate the JSON value against a schema with `additionalProperties: false` and every required nullable field explicitly listed in `required`, then deserialize into Rust types. Composed flattened payloads use an explicitly assembled schema; do not combine serde `flatten` with `deny_unknown_fields` or rely on `Option` deserialization to enforce required presence.

Runtime validation requires:

- No additional properties.
- `winner` is one of the two actual debater nicks, never null or `"tie"`.
- `scores` contains exactly two entries, one per debater.
- Each entry contains all four criteria exactly once.
- Every score is an integer from 1 through 10.
- Criterion reasoning is nonempty and at most 4,000 characters.
- Overall reasoning is nonempty and at most 12,000 characters.
- `whatToStealFromLoser` contains at most ten nonempty strings, each at most 2,000 characters. It may be empty when no useful borrowing is supported.
- `tallyOverrideReason` is a required field containing a string or null. When `winner` differs from the unique round-tally leader, it must contain a nonempty explanation. Missing, null, or empty explanations in that case trigger the repair path.
- The committed `roundTally` contains exactly the two debater nicks and their router-computed integer sums of accepted round scores. The model does not produce it.

The fixed rubric is:

| Criterion | Weight | Assessment |
|---|---|---|
| `fit` | 0.30 | Fit to the original ask and its constraints. |
| `feasibility` | 0.30 | Whether the proposal can be implemented and operated as described. |
| `originality` | 0.15 | The proposal’s distinctive and useful contribution. |
| `survivedAttacks` | 0.25 | How well the final proposal withstands substantive criticism, including honest concessions and revisions. |

Squad calculates weighted totals from validated integer scores. It computes the weighted numerator with integer arithmetic and divides by 100. The provider does not supply totals.

The higher total must win. Equal totals still require a winner, using the retained user instruction as the tiebreaker criterion. The reasoning must explain the choice. An empty instruction does not permit a tie.

The round tally is separate from the final weighted rubric totals. The final prompt includes the round scores and identifies unscored rounds. A winner may differ from the round-tally leader only with the validated `tallyOverrideReason`; this does not waive the weighted-total winner rule. Equal round tallies have no unique leader and do not permit a tied final verdict. Unscored rounds contribute nothing, and a debate with no accepted round scores has a zero tally for both debaters.

The committed `verdict` payload adds `debateId`, `caseId`, `judgeInvocationId`, canonical work and attempt identity, actual model, rubric version, `weightedTotals`, `roundTally`, and canonical `Usage`. `weightedTotals` contains one `{nick, total}` entry per debater. `roundTally` is a map from each debater nick to its integer sum, computed by the router from accepted round scores. The envelope owner supplies common envelope fields and consumes this payload schema without defining a competing verdict format.

### Validation, attempts, and verdict commit

Use provider structured output when supported, then validate locally. A single surrounding Markdown code fence may be removed. Arbitrary prose extraction and silent coercion are prohibited.

A final judge invocation permits three provider-call attempts in total, including transport retries and at most one schema-repair request. Transport and repair attempts share the same counter. A malformed repair result or exhausted allowance ends with `JUDGE_INVALID_VERDICT`. Retain rejected output for human inspection without delivering it as a verdict or authoritative history.

Round-scoring invocations use their separate two-attempt allowance. Exhaustion records an unscored round instead of failing final adjudication.

**Protocol, store and bus** owns schema validation, implemented in M1. The judge repair executor lives in `squad-agents`; the **Prompt pack** owns repair prompt text. Judge integration consumes those implementations and enforces the shared attempt allowance.

The router applies the single normative [12-step commit pipeline](02-message-protocol-store-and-bus.md#commit-transaction). The accepted verdict transaction validates the app-computed tally, seals the case, updates wins/losses, delivers the first round-score announcement, enters `COMPLETED`, and releases holds and unused reservations.

The scoreboard update is idempotent per committed debate verdict. Replayed events, duplicate submissions, case follow-ups, and standalone rulings cannot increment it again. Records belong to member nicks, including when two members use the same model. `/clear` clears them. **Room API and CLI** and the TUI expose the result through their owned state and participant contracts.

Per-round scores never update wins or losses.

### /judge on an existing debate

`/judge [debateId]` judges the existing transcript of a stopped or finished unjudged debate. With no ID, select the most recent eligible debate in the room. Explicit IDs resolve only within room `main`.

Eligible debates include `STOPPED`, `COMPLETE_UNJUDGED`, and debates whose judging failed after usable transcript material was committed. `/judge` does not run missing rounds, regenerate turns, or resume debate generation.

`/judge` does not create retroactive `judge.round` work. It uses whatever committed round scores exist, identifies unscored or unscored-by-cancellation rounds in the final prompt, and computes `roundTally` from accepted scores only. A previously unjudged debate remains without per-round scores.

The orchestrator asks the router to claim the active-contest pointer, prepares the existing evidence, checks context and daily spending, emits preflight, and applies the same confirmation policy. Cancellation, rejection, or expiry of this judge-only plan restores the saved prior terminal snapshot and preserves the debate’s earlier transcript.

Confirmed judge-only work enters `JUDGING` directly and creates a fresh case. Its ID is appended to the debate’s case list. Its authorized evidence includes existing round scores and unscored-round information. Its eventual verdict uses the same schema, delivery, and atomic scoreboard commit as automatic judging.

A debate with an accepted verdict retains that sealed result. Repeating `/judge` returns the existing result and case rather than creating another scored verdict. Questions about that result use case follow-ups.

### Tournament execution

The following typed contracts land in this mode’s M5 phase; Protocol’s generic carriers are available in M1.
- `TournamentSetup` — Frozen prompt, entrants and resolved judge.
- `TournamentState` — Round, replay, survivors and terminal state.
- `TournamentPreflight` — Shared ContestPreflight base plus round and replay settings.
- `TournamentTransition` — Typed lifecycle discriminants for `ContestTransitionSubmission`, added when the mode is built.
- `TournamentEntry` — One entrant’s completed answer or forfeit.
- `TournamentRanking` — Validated ranking decoded for the human.
- `TournamentResult` — One entrant’s rank and nullable forfeit score/critique.
- `TournamentChampion` — Final placements, champion and resulting trophy count.
- `TournamentCaseMaterial` — Blind labeled entries and ranking instructions.


`@all $tournament <prompt>` or a named subset starts a tournament. `@all -nick` uses the parser’s resolved recipient list. There must be at least three distinct entrants; otherwise return:

`$tournament needs at least three entrants.`

The free text is the tournament prompt. Judging is implied. There is no pick step.

Admission freezes entrant order, the prompt, models, judge identity, prompt snapshot, limits, and reservation. Entrants drain earlier ordinary work before entering `ANSWERING`; later ordinary messages are held outside their histories until their holds are released. Ordinary output and retry limits apply to entry generation.

In round r, every surviving entrant answers independently and in parallel using `tournament.entrant.md`. The instruction explains elimination, the remaining entrant count, and the requirement to improve its answer. From round 2, it includes only that entrant’s previous rank, score, and critique. An entrant never receives another entrant’s answer or individual result.

After all slots resolve, the router constructs a blind judging projection. Answers are labelled `Entrant A`, `Entrant B`, and so on, with a newly shuffled label assignment each round. The persisted label-to-nick map is router-only data; it never enters a judge capability, prompt, or case-history projection.

The tournament judge receives the question and current round’s anonymised entries through one rendered user message using `tournament.judge.md` and the judge system prompt. Entrant nicks, model names, personas, routing metadata, and the label map are excluded. The blind projection must also prevent identifying nick or model strings in supplied answer text from reaching the judge; originals remain intact in the human transcript. Captured-request tests cover this boundary.

The response is strict JSON ordered best first:

```json
{
  "ranking": [
    {"label": "A", "score": 9, "critique": "One-line critique."},
    {"label": "C", "score": 7, "critique": "One-line critique."},
    {"label": "B", "score": 4, "critique": "One-line critique."}
  ]
}
```

Validation requires a permutation of the round’s labels, no duplicates or omissions, integer scores from 1 through 10, and a nonempty one-line critique for every entry. Array position determines strict rank; equal numerical scores do not create tied ranks. Additional properties are rejected through the shared two-pass validation path.

The judge has three attempts, including transport and repair attempts, within the canonical deadline and token allowances. Repair data uses the same attempt-scoped, case-bound mechanism as debate judging.

An entrant that times out or errors after its retry forfeits. A forfeiting entrant ranks below every completed entry. Exactly one entrant is eliminated per accepted round. A single forfeit therefore occupies last place and is eliminated.

If all three judging attempts fail, replay that round once with the same survivors and a new durable replay identity. Replay regenerates entries, reshuffles blind labels, and uses a new judging invocation. No elimination, individual result, or trophy is committed from the failed run. If judging the replay also exhausts its allowance, enter `FAILED` and report the canonical judging error.

The logical round remains r during a replay. Work, entries, label maps, judging attempts, and accepted ranking identify both round and replay index so late results from the abandoned run cannot be accepted.

For k entrants, successful execution has k−1 rounds. Each round eliminates the last-ranked entrant until one champion remains. The final round has two entrants.

| State | Event or condition | Required action |
|---|---|---|
| `PREFLIGHT` | Valid plan | Emit the call count and estimate; confirm under the shared policy. |
| `PREFLIGHT` | Confirmed admission with busy entrants | Establish holds and enter `WAITING_FOR_ENTRANTS`. |
| `PREFLIGHT` or `WAITING_FOR_ENTRANTS` | All entrants drained | Persist entry work and boundaries, then enter `ANSWERING`. |
| `ANSWERING` | All entry slots resolve | Persist the shuffled label map and blind case input; enter `JUDGING`. |
| `JUDGING` | Valid ranking | Commit the accepted ranking and enter `ELIMINATING`. |
| `JUDGING` | Attempts exhausted, replay unused | Persist replay index one and return to `ANSWERING` with the same survivors. |
| `JUDGING` | Replay judging exhausted | Enter `FAILED`, preserve evidence, and release unused resources. |
| `ELIMINATING` | More than one survivor remains after elimination | Commit individual results and start the next round with survivors. |
| `ELIMINATING` | One survivor remains | Commit final placements, champion, and one trophy increment; enter `COMPLETE`. |
| Any active state | Accepted stop | Fence outstanding work, enter `STOPPED`, preserve evidence, and release unused resources. |

A preflight plan’s confirmation, cancellation, expiry, and rejection are retained under the shared preflight result contract. They do not dispatch tournament work.

Tournament events carry these payloads:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TournamentStatus {
    Preflight,
    WaitingForEntrants,
    Answering,
    Judging,
    Eliminating,
    Complete,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TournamentSetup {
    pub contest_id: ContestId,
    pub prompt_message_id: MessageId,
    pub prompt: String,
    pub entrants: Vec<MemberNick>,
    pub judge_model: String,
    pub total_rounds: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TournamentState {
    pub contest_id: ContestId,
    pub state: TournamentStatus,
    pub state_version: u64,
    pub round: u32,
    pub replay: u8,
    pub survivors: Vec<MemberNick>,
    pub eliminated: Vec<MemberNick>,
    pub terminal_reason: Option<String>, // nullable
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TournamentEntry {
    pub contest_id: ContestId,
    pub round: u32,
    pub replay: u8,
    pub entrant: MemberNick,
    pub outcome: DebateTurnOutcome,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TournamentJudgeEntry {
    pub label: String,
    pub score: u8,
    pub critique: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TournamentJudgeResponse {
    pub ranking: Vec<TournamentJudgeEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TournamentRank {
    pub nick: MemberNick,
    pub rank: u32,
    pub score: Option<u8>, // nullable
    pub critique: Option<String>, // nullable
    pub forfeited: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TournamentRanking {
    pub contest_id: ContestId,
    pub case_id: CaseId,
    pub round: u32,
    pub replay: u8,
    pub ranking: Vec<TournamentRank>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TournamentResult {
    pub contest_id: ContestId,
    pub round: u32,
    pub entrant: MemberNick,
    pub rank: u32,
    pub remaining_before: u32,
    pub score: Option<u8>, // nullable
    pub critique: Option<String>, // nullable
    pub survived: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TournamentPlacement {
    pub nick: MemberNick,
    pub placement: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TournamentChampion {
    pub trophies: u64,
    pub contest_id: ContestId,
    pub champion: MemberNick,
    pub placements: Vec<TournamentPlacement>,
}
```

`TournamentEntry.outcome` reuses the protocol-owned completed-or-forfeit shape; generation identity, usage, and envelope fields follow the canonical work/result contract. Tournament entry work is reply-like work carrying a contest identity. It is not debate-turn work.

Tournament delivery uses [Protocol’s sole visibility table](02-message-protocol-store-and-bus.md#visibility-and-prompt-authority); exact tournament projected-field rows are added in M5.P5.

The final placement is fixed when an entrant is eliminated: elimination with N remaining assigns placement N. The champion receives placement one. The router commits champion, final placement delivery, terminal state, and the champion’s trophy increment atomically and idempotently. Tournament trophies are separate from debate wins and losses. Grill updates neither.

The normal preflight counts `k + (k−1) + ... + 2` entry calls plus `k−1` judge calls. The reservation includes two attempts per entry and three per judge invocation, with one possible replay of each logical round. Context bounds include the largest permitted entry and judging requests. The estimate presents the normal count and identifies the additional retry and replay allowance.

The TUI owns `TOURNAMENT`, `ROUND r · N REMAIN`, `FINAL`, and `CHAMPION` banners, ranking presentation, and eliminated participant markers. Effects owns the bottom-up ranking reveal, eliminated stamp, and champion celebration. Exports retain all entry attempts, accepted rankings, eliminations, final placements, and champion, with failed replay runs identified as such.

### Grill execution

The following typed contracts land in this mode’s M5 phase; Protocol’s generic carriers are available in M1.
- `GrillSetup` — Selected plan, focus and declared shared floor.
- `GrillState` — Round, turn, plan version and terminal state.
- `GrillPreflight` — Shared ContestPreflight base plus rounds and floor settings.
- `GrillTransition` — Typed lifecycle discriminants for `ContestTransitionSubmission`, added when the mode is built.
- `GrillQuestion` — One scheduled griller’s question turn.
- `GrillAnswer` — Leader responses and plan changes.
- `GrillPass` — Voluntary pass or exhausted griller slot.
- `GrillPlan` — Immutable rebuilt plan version.
- `GrillComplete` — Last good plan and attributed appendix.


A direct `@leader $grill [free text]` selects the leader’s most recent completed, unsuperseded ordinary reply to `me`, or the parser’s selected or quoted message authored by that leader. Grillers default to all other room members. The direct leader-first form with explicitly named grillers and no free text restricts the floor to those grillers.

At least one distinct griller is required. A leader without an eligible plan is refused with:

`@{nick} has no plan to grill yet. Ask it something first.`

The free text is the grilling focus passed to everyone. A question-first grill instead uses the selected collection answer as its starting plan and every other answering member as a griller.

Admission freezes the leader, griller roster order, source plan and ID, focus, `GRILL_ROUNDS`, limits, prompt snapshot, and reservation. Plan v1 is the selected source snapshot. Participants drain earlier ordinary work before execution; later ordinary messages are held outside their histories.

Grill is a shared floor. Every participant receives the current plan, every question, every answer, and every rebuilt plan as committed. This is an explicit exception to mutual invisibility, limited to the leader and grillers. Non-participants receive no grill material.

One round visits each griller sequentially. Begin with roster order and rotate that order left by one additional position each round. Each turn receives a primary angle from this rotating list:

`cost`, `failure modes`, `security`, `scale`, `edge cases`, `hidden assumptions`, `user experience`, `operations`.

Advance the angle through the list for successive griller turns, wrapping at the end. Persist the order, angle, round, and turn index so restart does not change them.

A griller uses `grill.question.md`, sees the current plan version and the round’s floor, and asks up to three questions about the weakest unexamined part. It must not repeat a question already asked. It may output the exact token `PASS` if it has no worthwhile question.

For a question turn, the leader uses `grill.answer.md`, answers each question directly, and states the concrete “Plan changes” implied by those answers. That answer and its changes reach the entire floor before the next griller starts.

After the last griller, the leader uses `grill.rebuild.md` to rewrite the complete plan as a clean standalone document incorporating the round’s answers. The result becomes plan v(r+1); the next round grills that version. Every accepted plan version remains durable and viewable.

At the start of each round, the grill working context consists of the latest complete plan, the round’s questions and answers as they arrive, and a compact list of earlier questions to prevent repeats. Earlier full answers remain in durable history and export but are excluded from the next round’s working projection. The context-bound reader supplies this authorized projection without reading the room log or deleting history.

If every griller passes, complete early with reason `survived`. Complete after the round’s rebuild when one is required; skip rebuilding a round in which no questions were asked. A timed-out or errored griller is skipped after its retry and is recorded as a failure, not a voluntary `PASS`. A leader that fails after its retry causes `FAILED` while retaining the last good complete plan.

Grill has no judge and no winner. Its output is the latest accepted complete plan and an appendix containing every question, its griller, the corresponding answer, and the plan changes. A stopped or failed grill retains its last good plan and incomplete floor records.

| State | Event or condition | Required action |
|---|---|---|
| `PREFLIGHT` | Valid plan | Emit call count and estimate; confirm under the shared policy. |
| `PREFLIGHT` | Confirmed admission with busy participants | Establish holds and enter `WAITING_FOR_PARTICIPANTS`. |
| `PREFLIGHT` or `WAITING_FOR_PARTICIPANTS` | All participants drained | Deliver setup and begin `QUESTIONING`. |
| `QUESTIONING` | Valid questions | Commit questions to the entire floor and enter `ANSWERING`. |
| `QUESTIONING` | `PASS` or exhausted griller attempts | Commit the pass or attributed skip; advance to the next griller. |
| `ANSWERING` | Accepted leader answer and plan changes | Commit them to the entire floor; advance to the next griller. |
| Round’s last griller resolved | Questions were asked | Enter `REBUILDING`. |
| Round’s last griller resolved | Every griller passed and no questions were asked | Enter `COMPLETE` with reason `survived`, retaining the latest plan. |
| `REBUILDING` | Accepted complete plan, more rounds remain | Commit the new version, rotate griller order, and begin the next round. |
| `REBUILDING` | Accepted complete plan, final round | Commit the new version and enter `COMPLETE`. |
| `ANSWERING` or `REBUILDING` | Leader attempts exhausted | Enter `FAILED`, retaining the last good complete plan. |
| Any active state | Accepted stop | Fence outstanding work, enter `STOPPED`, preserve evidence, and release unused resources. |

A preflight plan’s confirmation, cancellation, expiry, and rejection follow the shared preflight result contract.

Grill events carry these payloads:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GrillStatus {
    Preflight,
    WaitingForParticipants,
    Questioning,
    Answering,
    Rebuilding,
    Complete,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrillSetup {
    pub contest_id: ContestId,
    pub leader: MemberNick,
    pub grillers: Vec<MemberNick>,
    pub source_message_id: EventId,
    pub initial_plan: String,
    pub focus: String,
    pub total_rounds: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrillState {
    pub contest_id: ContestId,
    pub state: GrillStatus,
    pub state_version: u64,
    pub round: u32,
    pub total_rounds: u32,
    pub leader: MemberNick,
    pub griller_order: Vec<MemberNick>,
    pub turn: u32,
    pub plan_version: u32,
    pub questions_asked: u32,
    pub passed: u32,
    pub terminal_reason: Option<String>, // nullable
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrillQuestion {
    pub contest_id: ContestId,
    pub round: u32,
    pub turn: u32,
    pub griller: MemberNick,
    pub leader: MemberNick,
    pub plan_version: u32,
    pub angle: String,
    pub questions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrillAnswerItem {
    pub question_index: u8,
    pub answer: String,
    pub plan_changes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrillAnswer {
    pub contest_id: ContestId,
    pub round: u32,
    pub turn: u32,
    pub leader: MemberNick,
    pub question_event_id: EventId,
    pub answers: Vec<GrillAnswerItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrillPassReason {
    Pass,
    Timeout,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrillPass {
    pub contest_id: ContestId,
    pub round: u32,
    pub turn: u32,
    pub griller: MemberNick,
    pub reason: GrillPassReason,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrillPlan {
    pub contest_id: ContestId,
    pub round: u32,
    pub version: u32,
    pub leader: MemberNick,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrillAppendixEntry {
    pub question_event_id: EventId,
    pub question_index: u8,
    pub griller: MemberNick,
    pub question: String,
    pub answer: Option<String>,
    pub plan_changes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrillComplete {
    pub contest_id: ContestId,
    pub final_plan_event_id: EventId,
    pub final_plan_version: u32,
    pub reason: Option<String>,
    pub appendix: Vec<GrillAppendixEntry>,
}
```

Question indexes are zero-based within the referenced question event. `questions` contains one to three nonempty questions; `PASS` produces `GrillPass`, not an empty question event. An accepted leader answer covers every referenced question exactly once. Generation identity, model, finish reason, usage, and attempt identity use the canonical protocol result fields rather than a second generation schema.

`grill.setup`, `grill.question`, `grill.answer`, `grill.pass`, `grill.plan`, and `grill.complete` go to the human and exactly the grill participants. `grill.state` is a human-facing control record. The initial source is plan v1; each accepted rebuild increments the version once. Complete references the last accepted plan event and supplies the appendix without generating another model call.

Question, answer and rebuilt-plan word limits are owned by [Prompt pack](09-prompt-pack.md#word-limit-defaults); visible token ceilings come from Protocol’s canonical table. Work kinds are `grill.question`, `grill.answer`, and `grill.rebuild`, using canonical work records and context-bound readers.

For r rounds and g grillers, normal preflight counts `r × (g × 2 + 1)` calls. This is the no-pass path: one question and one answer per griller, plus one rebuild per round. Reservations cover both permitted attempts for every call and conservative shared-floor context growth. Passes and early completion release unused reservations.

The TUI owns `GRILL`, `GRILL ROUND r`, and `FINAL PLAN` banners, griller-to-leader turn headings, `questions asked 14 · passed 2` counters, and collapsible plan versions with the newest expanded. Effects owns turn handoff, plan version changes, and the survived moment.

`/export grill [id]` and `squad export grill` use the shared export implementation in `squad-core/src/export/`. Exports include all plan versions and the complete attributed question-and-answer appendix. Tournament export uses the same API with tournament scope.

### Common execution contracts for census, crosscheck, survey and probe

Each new mode starts in `PREFLIGHT`, applies the shared confirmation and daily-admission policy, then enters `WAITING_FOR_PARTICIPANTS`. Once earlier ordinary work has drained, it enters its first script step. Waiting consumes no provider deadline. Cancellation, expiry and rejection before admission remain preflight outcomes and dispatch no mode work.

Each mode has `<mode>.setup`, `<mode>.state`, one event per committed artifact, and `<mode>.report`. Protocol owns generic envelope registration; each mode’s M5 phase adds its typed kinds and validators. Non-debate preflights and outcomes use the durable `contest.preflight` and `contest.preflight.result` carriers, not `<mode>.state`. Setup retains the source IDs and text snapshots, participant order, resolved models, prompt hashes, limits, assignments and mode settings. State retains `contestId`, `stateVersion`, current step, nullable terminal reason, and references to committed artifacts and unfinished work. Census also retains pass and plan version; crosscheck retains grid and assignment versions; survey retains ballot stage; probe retains its role assignment.

All generated objects use Protocol’s nullable/omitted `Option` rule. All generated objects use strict JSON schemas with unknown properties rejected, required nullable fields explicitly required, bounded arrays, unique local IDs, and validated references to the supplied inputs. Contests owns the full mode payload schemas and validators, implemented in `squad-protocol` when that mode is built. The **Prompt pack** owns prompt text and rendering only; **Protocol, store and bus** owns common validation and carriers. Prose string fields together use the step’s word budget. Identifiers, enums and numeric fields do not count as prose.

Member steps use `roomWords` and the new-mode member-call limits. Judge steps use `planWords` and the new-mode judge-call limits. Repairs consume the same attempt counter as transport retries. A failed member is skipped after its retry and named, with its failed step, in the human report. Missing output is never converted into an empty successful answer, a vote, a closure, or a prediction. Exhausted judge attempts enter `FAILED`; accepted partial artifacts remain exportable.

A mode may finish with incomplete evidence, but its report must identify missing participation and cannot claim a stop condition that depends on that evidence. If no usable input remains for a required step, enter `FAILED`. Crosscheck keeps cells assigned to a failed reviewer open. Probe requires at least one accepted explanation, one accepted check and one usable prediction row to produce a decision tree.

Every parallel step ends with a `barrier` before any later step receives its outputs. It commits successful artifacts and attributed failures and delivers only the next step’s authorized projection. Reports, attribution tables, ballots and final tally details go to the human and authorized judge case, not automatically into members’ ordinary histories. Later human quotes remain an explicit way to deliver them.

`/stop` fences every outstanding member, auditor and judge work item. Restart restores the same sources, assignments, random selections, accepted artifacts, attempts and delivery markers. Recovery does not rerun an accepted step or redraw auditors, samples or roles.

The inline report blocks use `CENSUS`, `CROSSCHECK`, `SURVEY` and `PROBE` banners and step dividers. The TUI owns rendering; Effects owns animation. All four export through the shared markdown export implementation with their respective mode scope. They do not update debate wins, losses or tournament trophies.

### Census execution

The following typed contracts land in this mode’s M5 phase; Protocol’s generic carriers are available in M1.
```rust
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CensusStatus {
    Preflight,
    WaitingForParticipants,
    Hunting,
    Merging,
    Rating,
    Finalizing,
    WaitingForPlan,
    Reporting,
    Complete,
    Stopped,
    Failed,
}
```

- `CensusSetup` — Frozen plan, participants and pass settings.
- `CensusState` — Pass, plan version, current step and terminal state.
- `CensusPreflight` — Shared ContestPreflight base plus pass and source settings.
- `CensusTransition` — Typed lifecycle discriminants for `ContestTransitionSubmission`, added when the mode is built.
- `CensusPlan` — Accepted replacement plan snapshot.
- `CensusHunt` — Blind lens omissions and consequences.
- `CensusLedger` — Merged or finalized gaps with private finder provenance.
- `CensusRating` — Per-gap ratings, covered citations and additions.
- `CensusReport` — Pass outcomes, Chao1 inputs/value and nullable estimate bounds.
- `CensusCaseMaterial` — Step-scoped anonymous merge/finalization material.


`@all $census` takes a selected or quoted plan, including a grill’s latest complete plan version. `@all $census <pasted plan>` supplies the plan directly. `@nick $census` takes that member’s latest eligible reply as the plan and runs the census with the room’s members; the single mention identifies the source, not a one-member hunt. At least three members are required.

Admission freezes `CENSUS_PASSES`, default 2 and bounded from 1 through 5. Each pass freezes its current plan version. A replacement plan between passes becomes a new version and never rewrites earlier hunt evidence or citations.

The lens list is ordered: stakeholders; failure modes; dependencies and sequencing; hidden assumptions; external and legal factors; the day after success. At admission, draw a random participant-to-lens permutation and a random tie-break seed and persist both in `contest_private_maps`. Rotate the persisted lens order between passes; repeat lenses when participants exceed lens count. Never derive assignments or tie breaks from publicly known roster order. Recovery reuses the persisted mapping and seed. Lenses remain private provenance and are excluded from the rater ledger.

| State | Move and actors | Visibility, template and output | Completion and failure |
|---|---|---|---|
| `HUNTING` | `fanout` to every participant. | Plan plus one assigned lens only; `census.hunt.md`; up to 8 `{localId, omission, consequence}` items. | Barrier after all slots resolve; retain member-to-find provenance privately. Failed members are skipped and named. |
| `MERGING` | `judge`. | Plan, assigned lenses and accepted blind hunts; `census.merge.md`; deduplicated gaps with source-find references, lens and consequence. | Validate that finder references exist and duplicates do not inflate finder counts. Judge exhaustion fails the mode. |
| `RATING` | `fanout` to every participant. | Plan and the anonymous ledger, with no finder names or counts; `census.rate.md`; exactly one `{gapId, rating, citation}` per supplied gap plus optional additions. `rating` is `real`, `covered` or `trivial`; `covered` requires a plan citation. | Barrier locks ratings before finalization. Failed ballots remain missing. |
| `FINALIZING` | `judge`, then `tally`. | `census.final.md`; retained and rejected gap references, merged additions, citations and ranking. The app computes vote counts, finder counts and the unseen estimate. | Commit the pass ledger, estimate inputs and discovery point together. Judge cannot invent finders or count rating-stage agreement as independent discovery. |
| `WAITING_FOR_PLAN` | Human continuation between passes; release participant holds and deliver queued ordinary inputs, retaining the active-contest claim. | `/continue` keeps the current plan; `/continue "<plan>"` supplies replacement text; a single quoted plan attachment may instead replace it. No model call while waiting. | On continue, reacquire holds, drain earlier ordinary work through `WAITING_FOR_PARTICIPANTS`, then enter `HUNTING` with the persisted next lens assignment. Stop on human request or a satisfied quiet condition. |
| `REPORTING` | `report`. | Assemble ranked confirmed gaps, covered items and citations, estimate line, singleton highlights and discovery curve. | Commit `census.report`, enter `COMPLETE`, and release resources. |

For each gap, finder count is the number of distinct members whose blind hunt contained that gap, after duplicate merging. Multiple finds from one member count once. Rating-stage additions retain their origin but do not acquire independent blind finder counts through later votes.

For each rated gap, let `v` be its number of accepted ratings and `d` the count rated `covered` or `trivial`. Drop it when `d > v / 2`; preserve the breakdown and every covered citation. A missing ballot contributes no vote. Gaps without enough evidence are marked unresolved rather than silently confirmed. Additions are merged and identified as rating-stage discoveries.

For the confirmed gaps traceable to the blind hunt, let `f1` be the number with exactly one finder and `f2` the number with exactly two. The application computes the Chao1 unseen-gap estimate:

`U = f1² / (2 × f2)` when `f2 > 0`.

`U = f1 × (f1 − 1) / 2` when `f2 = 0`.

The estimate is a `tally`, never a model calculation. Retain its input gap IDs, `f1`, `f2`, formula branch and unrounded result. Rating-stage additions are reported separately and do not fabricate capture-recapture observations.

The report must show a range, never `U` alone:

`Found {n}; estimated {lo} to {hi} still unfound.`

Always state that models share blind spots and the true number still unfound is at least this estimate. The rule deriving `{lo}` and `{hi}` remains an open question; a judge must not invent bounds or imply a calibrated confidence interval. `CensusReport` retains required nullable `estimateLow: Option<f64>` and `estimateHigh: Option<f64>` alongside the Chao1 inputs and value. While unresolved, store null bounds, validate only Chao1 and show “Estimate range unavailable; range rule unresolved.” rather than a fabricated numeric headline. The quiet message is unavailable until valid bounds exist.

A singleton is a confirmed gap found by exactly one blind hunter. Highlight it as something the other members missed. The human can inspect the full member-by-gap table, finder counts, lens, consequence and retained citations. Participants never receive finder identities or counts.

Across passes, match confirmed gap identities and retain each pass’s plan version, new confirmed count, cumulative confirmed count, singleton count and estimate range. A gap remaining open in a later pass is not a new discovery. Show the discovery curve without treating a plan edit as proof that an old gap was closed.

The displayed stop rule is a pass with no new confirmed gap and an unseen estimate below 2:

`Census is quiet. Good place to stop.`

Require a valid estimate range whose upper bound is below 2, no unresolved rating evidence affecting that conclusion, and complete required participation before displaying that line. Until the range rule is settled, the engine cannot assert quiet from the point estimate alone.

Between passes, the human may patch the plan or send the ledger, especially its singletons, to `$grill`. A grill cannot run while census holds the active-contest claim. Such a handoff first stops the census after preserving its pass report and releases the claim; a later census can reference that report and the revised plan. No automatic grill-and-census loop is part of this script.

For m members and p passes, normal preflight counts `p × (2m + 2)` calls: m hunts, one merge, m ratings and one finalization per pass. Six members cost 14 calls per pass and 28 for the default two passes. Reserved attempts are `p × (4m + 6)`. A changed plan between passes is context-checked and cost-checked before further dispatch; if it exceeds the frozen reservation bounds, require a revised preflight.

`/export census [id]` includes all plan snapshots, attributed hunts, finder table, anonymous ledgers, ratings, citations, pass outcomes, estimate provenance and discovery curve. The report includes a “send singletons to $grill” hint.

### Crosscheck execution

The following typed contracts land in this mode’s M5 phase; Protocol’s generic carriers are available in M1.
```rust
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CrosscheckStatus {
    Preflight,
    WaitingForParticipants,
    Extracting,
    Inventorying,
    BuildingGrid,
    Reviewing,
    Auditing,
    Ruling,
    Reporting,
    Complete,
    Stopped,
    Failed,
}
```

- `CrosscheckSetup` — Frozen plan, goal, constraints and participant settings.
- `CrosscheckState` — Grid version, current step and terminal state.
- `CrosscheckPreflight` — Shared ContestPreflight base plus source and optional extraction settings.
- `CrosscheckTransition` — Typed lifecycle discriminants for `ContestTransitionSubmission`, added when the mode is built.
- `CrosscheckGoal` — Explicit or extracted goal and constraints.
- `CrosscheckInventory` — Plan-hidden lens inventory.
- `CrosscheckGrid` — Retained cells, trim provenance and private assignments.
- `CrosscheckReview` — Assigned-cell evidence, exclusions or open gaps.
- `CrosscheckAudit` — Anonymous sampled-closure findings and proposed case.
- `CrosscheckRuling` — Challenged-closure rulings and retained additions.
- `CrosscheckAcceptance` — Human acceptance bound to exact cells, plan and grid.
- `CrosscheckReport` — Coverage ledger, audit outcome and residual risks.
- `CrosscheckCaseMaterial` — Step-scoped normalization or ruling inputs.


`@all $crosscheck` takes a selected, quoted or pasted plan as census does, plus goal and constraints in free text. If goal and constraints are absent, the judge extracts them from the plan in one extra call before inventory work. That extraction uses `crosscheck.normalize.md` with an extraction task and returns only `{goal, constraints}` for delivery to inventory writers.

Inventory writers receive fresh step contexts containing only the goal, constraints and assigned lens. They receive neither the plan nor ordinary history that could reveal it. The ordered lenses are stakeholders; phases; dependencies; failure states; operating conditions; decision rights. Assign them using a random lens permutation and tie-break seed drawn per contest, persisted in `contest_private_maps`, wrapping after six. Recovery reuses both; public roster order cannot reconstruct either assignment.

| State | Move and actors | Visibility, template and output | Completion and failure |
|---|---|---|---|
| `EXTRACTING`, only when needed | `judge`. | Plan; `crosscheck.normalize.md` extraction task; goal and constraints. | Extraction exhaustion fails the mode. No inventory call begins before extraction commits. |
| `INVENTORYING` | `fanout` to participants. | Goal, constraints and one lens, plan hidden; `crosscheck.inventory.md`; exactly 3 `{localId, item}` entries. | Barrier commits inventories and provenance without delivering other inventories to writers. |
| `BUILDING_GRID` | `judge`, then `tally`. | `crosscheck.normalize.md`; normalized items, lens and source references, plus priority order. The app builds singles and cross-lens pairs. | Persist the retained grid, any cap omissions and reviewer assignments before review dispatch. |
| `REVIEWING` | `fanout` to assigned reviewers. | Plan and only the reviewer’s cells; `crosscheck.cell.md`; one disposition per assigned cell. | Barrier commits all results. Failed reviewers leave their cells open. |
| `AUDITING` | `fanout` to two randomly selected auditors in separate fresh contexts. | Plan, anonymous grid and sampled closure evidence, no reviewer identities; `crosscheck.audit.md`; verdict on each sampled closure and one proposed missing case per auditor. | Persist selections and samples before dispatch. Failed auditors are named and prevent a clean-audit claim. |
| `RULING` | `judge`. | Plan, grid, cell evidence and audits; `crosscheck.rule.md`; challenged-cell rulings and accepted or rejected proposed additions with reasons. | The judge may reopen a closure or add a case; it may not drop a retained cell or accept a risk for the human. |
| `REPORTING` | `tally`, then `report`. | Assemble the full coverage ledger, audit outcome, cap omissions and residual risks. | Commit `crosscheck.report`, enter `COMPLETE`, and release resources. |

A normalized inventory item retains every originating member reference, even when duplicate items merge. A reviewer is ineligible for a cell if it contributed either constituent item. Assignment balances cell counts among eligible reviewers, with the persisted private random seed breaking ties. If no eligible reviewer exists, the cell stays open and the report names the assignment hole; it is never assigned back to its author.

For lens counts `n1 … nL`, let `N = Σ nl`. Before the cap, grid size is:

`G = N + Σ(l < k) nl × nk`.

Equivalently, `G = N + (N² − Σ nl²) / 2`. At six members with three distinct items in each of six lenses, this gives `18 + 135 = 153` cells. Pairs are unordered; self-pairs and same-lens pairs are excluded.

The cap is 200 retained cells. The judge supplies a priority order and the application trims deterministically to that bound. Preserve omitted candidate cells and the cap reason in the human report. The bounded coverage claim applies to the retained grid. Once retained, a cell cannot be removed by later judging.

Each review result contains `cellId` and exactly one disposition:

- `covered`, with a precise plan citation and explanation of how it handles the cell.
- `inapplicable`, with a concrete reason.
- `gap`, with the missing handling and a proposed fix.

“Probably covered,” an absent citation, or an unsupported closure remains a gap. Model text alone cannot create human risk acceptance.

Select two distinct auditors randomly from participating members and persist that selection. Each audit uses fresh context, including when an auditor was an inventory writer or reviewer. Sample closures without replacement within each auditor’s sample. Exclude that auditor’s own review results when alternatives exist. If fewer than five eligible closures exist, inspect all available and disclose the smaller sample. Never invent or duplicate closures to reach five.

Auditors identify false closures and each name one case the grid itself may have missed. The judge rules on challenges and proposed additions. An accepted addition becomes an open cell with its own proposed fix or evidence requirement; it is not silently closed by the ruling. Cap pressure cannot erase an accepted audit finding: retain it as an explicit residual gap if the grid already contains 200 cells.

The coverage ledger retains every cell’s constituents, disposition, citation or exclusion reason, proposed fix, audit challenges and resolution. `/accept <cellId…> -- <reason>` is valid on a `COMPLETE` crosscheck. It appends `crosscheck.acceptance`, binds to the current plan, grid and expected state version, updates the human coverage projection, and does not rerun completed script steps. An explicit human acceptance binds to a cell or residual risk and the current plan version. A later plan change does not silently carry that acceptance forward.

Display the stop rule: every retained cell is closed by evidence, excluded with justification, or explicitly accepted by the human, and the audit finds no false closure or material unresolved missing case. State this as a bounded stop, not proof of completeness. `COMPLETE` means script execution finished; it does not itself mean this stop rule passed. Failed review or audit work and cap omissions remain visible.

For m members and extraction indicator e of zero or one, normal preflight counts `2m + 4 + e` calls: m inventories, one normalization, m reviews, two audits and one ruling, plus optional extraction. Six members require 16 calls with supplied goal and constraints, or 17 with extraction. Reserved attempts are `4m + 10 + 3e`.

`/export crosscheck [id]` includes goal and constraints, inventory provenance, normalized items, candidate and retained grid, assignments, every review, audit samples and findings, rulings, human risk acceptances and the bounded-stop result.

### Survey execution

The following typed contracts land in this mode’s M5 phase; Protocol’s generic carriers are available in M1.
```rust
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SurveyStatus {
    Preflight,
    WaitingForParticipants,
    Collecting,
    Extracting,
    Voting,
    Selecting,
    Revoting,
    Reporting,
    Complete,
    Stopped,
    Failed,
}
```

- `SurveySetup` — Frozen question, answer set and participants.
- `SurveyState` — Ballot stage, current step and terminal state.
- `SurveyPreflight` — Shared ContestPreflight base plus source and optional collection settings.
- `SurveyTransition` — Typed lifecycle discriminants for `ContestTransitionSubmission`, added when the mode is built.
- `SurveyClaims` — Neutral claims and restricted answer provenance.
- `SurveyVote` — Immutable first ballot with reasons and confidence.
- `SurveySelection` — App-selected claims and bundled anonymous opposing reasons.
- `SurveyRevote` — Second ballot and required change evidence.
- `SurveyReport` — Five bands, switches and settling questions from stored tallies.
- `SurveyCaseMaterial` — Frozen extraction or report inputs with authorized provenance.


`@all $survey <question>` starts with parallel answers. `@all $survey` instead freezes the room’s most recent set of parallel answers to one question. Reuse only that set’s accepted answer snapshots, never a mixture of unrelated latest replies. A reused set creates no new answer calls. Grammar, eligibility validation and exact errors remain parser-owned.

| State | Move and actors | Visibility, template and output | Completion and failure |
|---|---|---|---|
| `COLLECTING`, when answers are needed | `collect` addressed members. | Question and each member’s own authorized input; ordinary reply template and reply schema; `roomWords`. | Barrier freezes accepted answers and failed slots. No member receives another’s full answer. |
| `EXTRACTING` | `judge`. | Question and accepted answer snapshots; `survey.extract.md`; up to 25 neutral atomic claims with source-answer references. | Preserve attribution only in the judge case and router records. Judge exhaustion fails the mode. |
| `VOTING` | `fanout` to participants. | Question and anonymous claims only; `survey.vote.md`; one ballot item per claim. | Barrier locks first ballots. Failed votes remain missing. |
| `SELECTING` | `tally`. | Application-only first-ballot counts. | Persist up to 5 most evenly split claim IDs and anonymous reason projections. |
| `REVOTING` | `fanout` to participants. | Selected claims and anonymous reasons from both sides only; `survey.revote.md`; second ballots with evidence that would change the member’s mind. | Barrier locks second ballots. No vote counts, authorship or whole answers are delivered. |
| `REPORTING` | `tally`, then one judge-backed `report`. | `survey.report.md`; five-band map referencing stored claims, reasons, vote changes and settling questions. | Validate against app-computed tallies, commit `survey.report`, enter `COMPLETE`, and release resources. |

An extracted claim has `claimId`, neutral `text`, and private source-answer references. Deduplication preserves all origins without treating repeated wording as separate claims. The judge extracts and explains; it does not rank members or decide truth.

Each first ballot item has `claimId`, `vote` from `AGREE`, `DISAGREE`, `UNSURE`, a one-line `reason`, and integer `confidence` from 1 through 5. A second ballot adds nonempty `changeEvidence`. It cannot introduce an unrequested claim or omit a requested claim.

For claim c, tally `A(c)`, `D(c)`, `U(c)` and missing ballots separately. Confidence is reported as supporting detail and does not turn one member into multiple votes. For selecting claims, use split distance `|A−D| / (A+D)` when `A+D > 0`; lower is more evenly split. Claims with no decisive votes sort after claims with decisive votes. Break ties by more decisive votes, then stable claim ID. Select at most five.

Anonymous reason delivery removes attribution, counts and identifying text. It does not label a side as the majority. Deduplicate repeated reasons so the payload does not present repetition as an announced vote tally. The participant sees its own task inputs and output, but receives no other member’s whole answer, author mapping or ballot.

For selected claims, the final vote is the accepted second ballot when present; otherwise retain the first ballot and mark it as not re-voted. Missing second ballots cannot be reported as unchanged votes. A switch requires two accepted ballots from the same member with different vote values.

The human report has five bands:

- Consensus: at the six-member default, 6–0 or 5–1 agreement. More generally, all members or all but one agree, with no missing or unsure ballot qualifying as agreement.
- Contested: opposing votes, the split, the best supported reason on each side, identified switches and the question that would settle the claim.
- Lone insights: raised in one original answer and endorsed by every other participating member.
- Lone dissents: a sole dissenter and its reason, including a cross-reference when the same claim appears under consensus.
- Rejected outliers: raised in one or two original answers and rejected by the remaining members; label these likely errors, not established falsehoods.

Claims may be cross-referenced between bands, but each retains one claim ID and one tally. Claims with missing or unsure votes that do not meet a band’s rule remain visibly unresolved within the map. The judge cannot manufacture consensus, infer a vote from an answer, or report an unobserved switch.

For m members and collection indicator q of zero or one, normal preflight counts `qm + 2m + 2` calls: optional answers, extraction, first votes, second votes and report. Six members require 20 calls question-first or 14 with existing answers. Reserved attempts are `2qm + 4m + 6`. An empty extracted claim set ends with an empty-map report and releases unused ballot reservations; no empty ballot calls are required.

`/export survey [id]` includes the question, frozen source answers, private claim provenance for human inspection, both ballot stages, selected claims, anonymous reason payloads, app-computed tallies, switches, missing participation and the five-band map.

### Probe execution

The following typed contracts land in this mode’s M5 phase; Protocol’s generic carriers are available in M1.
```rust
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProbeStatus {
    Preflight,
    WaitingForParticipants,
    Proposing,
    Predicting,
    Reporting,
    Complete,
    Stopped,
    Failed,
}
```

- `ProbeSetup` — Frozen brief, participants, role split and sit-out.
- `ProbeState` — Current step, artifact references and terminal state.
- `ProbePreflight` — Shared ContestPreflight base plus role and brief settings.
- `ProbeTransition` — Typed lifecycle discriminants for `ContestTransitionSubmission`, added when the mode is built.
- `ProbeExplanation` — One blind explanation and key assumption.
- `ProbeCheck` — One blind check with measurements and outcomes.
- `ProbePrediction` — Locked prediction or inability for every frozen check.
- `ProbeReport` — Prediction matrix, decision tree and inseparable groups.
- `ProbeCaseMaterial` — Locked proposals and predictions for reporting.


`@all $probe <observation, what is ruled out, what can be inspected>` takes a diagnostic brief. Models remain text-only and do not run checks. The output tells the human what to inspect.

Probe needs an even participating count of at least four. If the resolved count is odd, the last member in resolved roster order sits out. Name it in the transcript before execution and exclude it from holds, calls and role assignment. If fewer than four remain, refuse admission through the parser-owned error.

The application randomly splits the participating members into equally sized explainer and checker groups. Persist the split before dispatch and show it to the human. Do not redraw it after a failure or restart.

| State | Move and actors | Visibility, template and output | Completion and failure |
|---|---|---|---|
| `PROPOSING` | `fanout` to both groups in parallel. | Each sees only the diagnostic brief and its role. Explainers use `probe.explain.md` for one explanation and key assumption. Checkers use `probe.check.md` for one cheap, non-destructive check, exact measurement and recorded outcomes. | Barrier locks explanations and checks before delivery. Members see no peer proposals. Failed members are skipped and named. |
| `PREDICTING` | `fanout` to explainers with accepted explanations. | Own explanation and all accepted anonymous checks, no other explanations or predictions; `probe.predict.md`; one prediction or explicit inability to predict per check. | Barrier locks the prediction matrix. Failed rows remain missing. |
| `REPORTING` | One judge-backed `report`. | Brief, locked explanations, checks and prediction matrix; `probe.report.md`; ordered distinguishing checks, decision tree, inseparable explanation groups and unexpected-result branches. | Validate references and commit `probe.report`; enter `COMPLETE` and release resources. |

An explanation contains `explanation`, `keyAssumption`. A check contains `check`, `measure`, `outcomes`, `effort`, and a non-destructive procedure. The router assigns anonymous UUIDv4 explanation and check IDs, or shuffled opaque labels, at the proposal barrier after acceptance.

A prediction item contains `checkId`, nullable `expectedOutcome`, and nullable `cannotPredictReason`, with exactly one of those two fields nonnull. Every accepted check appears exactly once in an explainer’s row. Unpredictable and missing are different states. A missing row cannot support elimination of an explanation.

The judge compares locked predictions. It drops a check from the recommended sequence when it does not distinguish the available explanations, records why, and orders the remaining checks by effort and usefulness. It flags explanations the available checks cannot separate and does not declare one true.

The decision tree references only accepted checks and predictions. Each node says what to measure, which result leads to which next node or remaining explanation set, and what an unexpected result means. Every check node has an unexpected-result branch that reopens the diagnostic brief rather than forcing a result into a predicted category. Uncertain predictions remain uncertain; the report does not treat them as negative evidence.

For m participating members, normal preflight counts `m + m/2 + 1` calls: m proposals, m/2 prediction rows and one report. Six members require 10 calls. Reserved attempts are `3m + 3`. Sit-out members create no calls. Failed proposals or rows reduce actual calls where dependent work becomes unnecessary, and unused reservations are released.

`/export probe [id]` includes the diagnostic brief, sit-out notice, persisted random split, all accepted proposals, check procedures, prediction matrix, missing rows, dropped-check reasons, inseparable groups and runnable decision tree.

### Stop, failures, daily spending, and recovery

`/stop` invokes the cancellation behavior owned by **Agents and OpenRouter** and stops the active contest. `/stop @nick` stops that member’s generation; if it is participating in an executing contest, the containing contest stops so human cancellation is not retried as a provider failure. During question-first collection, stopping one addressed ordinary reply resolves that slot without cancelling the remaining collection.

Escape first closes the topmost transient element. With none open, it requests global stop; during a contest that requires a second Escape within 2 s under the TUI contract. Animation skipping does not consume Escape or input keys.

The router serializes stop against completion. The first accepted terminal transition wins. A committed stop prevents later rounds, retries, automatic judging, late round-score commits, late rankings, late plan updates, and late verdict commits. Outstanding judge work is canceled and fenced. Provider cancellation is best effort. Completed records remain; partial streamed text remains visibly incomplete and never becomes a completed turn.

Each debate-turn provider-call attempt has the canonical deadline. The worker makes exactly one provider call for each issued attempt. The orchestrator alone issues attempt two. A second timeout or provider failure records a forfeit. Both debaters forfeiting still resolves the barrier and allows subsequent rounds. Cancellation is not a forfeit.

Before admission, calculate worst-case input growth for all participants and any planned judge. Include opening positions, setup, every required retained turn, accumulated round scores, tournament replay allowances, grill plan and floor projections, prompt overhead, visible output and reasoning reservations, and the context safety reserve owned by **Agents and OpenRouter**. Reject with `CONTEXT_TOO_LARGE` if required evidence cannot fit. Never silently summarize or omit required judge evidence. `/judge` and case follow-ups repeat this check for their actual case material. Grill uses its explicitly specified compact prior-question projection.

New-mode context bounds also include the largest census hunt and ledger, crosscheck inventory and retained grid, survey claim and reason sets, probe checks and prediction matrix, and their strict structured outputs. Limits cannot silently remove required cells, ballots or predictions. A judge-backed report may return references and narrative while the application assembles the full stored artifact, avoiding repetition of the entire ledger in generated output.

The reservation covers every permitted attempt, including round 0 when needed and judging only when planned. Automatic debate judging includes one round-scoring invocation per numbered round, each with up to two attempts, and the final judge invocation with up to three attempts. Use resolved pricing and conservative input growth. Unavailable pricing blocks paid dispatch through the canonical cost error.

Daily spending follows the **Protocol, store and bus** and **Agents and OpenRouter** accounting contracts. It is the sum of actual per-call OpenRouter costs for Squad across rooms on the machine, grouped by local calendar day. Retries, repairs, replays, failed calls, and cancelled calls count whenever billed. The router commits usage and `spend_daily` changes atomically with the corresponding call event.

Pending reservations reduce the amount available for admission. Reported actual cost replaces its pending estimate without being counted twice. Unknown cost is never zero: retain the uncertain reservation, record `cost unknown`, and expose the unknown-call count through the shared state and doctor contracts. Reconcile interrupted requests through the accounting adapter’s OpenRouter generation lookup, `GET /api/v1/generation?id=`.

Reject a contest before execution if its reservation would exceed the remaining daily amount. At the daily max, prevent new generation requests with `DAILY_MAX_REACHED` and the exact message, formatted with the effective cap:

`Daily max of $20.00 reached. Raise SQUAD_DAILY_MAX_USD or wait until tomorrow.`

In-flight calls finish and their actual costs are recorded. Reaching the cap does not cancel them. No later turn, retry, repair, replay, or judge call may dispatch while the daily guard rejects it. Preserve accepted evidence and terminate an executing contest that cannot continue through the canonical daily-cap failure path. Generation becomes available on the next local day or when the effective max is raised.

On restart:

- Rebuild the active pointer, contest state, participant holds, work, deadlines, attempt counters, and reservations from SQLite before accepting new starts.
- Resume durable work and incomplete barriers without repeating accepted turns.
- Count an interrupted provider call as a consumed attempt.
- Preserve uncertain charges and reject stale results.
- Resume waiting and confirmation states; expire plans whose deadlines passed.
- Recover verdict and scoreboard commits together without double counting.
- Re-wake unfinished work only after durable state has been rebuilt.
- Recover round-scoring work, accepted scores, unscored outcomes, and tallies without duplicate scoring commits or tally increments; enqueue final judge work only after all round-scoring work has completed or been given up.
- Restore collection slots and pending picks from committed records; a pending pick does not expire.
- Restore tournament survivors, replay identity, shuffled label map, elimination results, and trophy commits without repeating accepted eliminations.
- Restore grill order, angle, turn, floor, plan version, and appendix references without repeating accepted questions, answers, or rebuilds.
- Restore judge-only prior terminal snapshots and the full case-ID list.
- Restore census plan versions, pass and lens assignments, blind finder provenance, ratings, ledger identities, estimate inputs and between-pass waiting.
- Restore crosscheck inventory provenance, retained and omitted cells, reviewer assignments, auditor selections, sampled closures, rulings and human acceptances.
- Restore survey source-answer snapshots, claim origins, locked ballots, selected claims, anonymous reason payloads and switches.
- Restore probe sit-out selection, random roles, accepted explanations and checks, locked predictions and report references.

Recovery respects `RoomHandle::open` mode. `OpenMode::OneShot` never re-wakes work it did not create. `--no-wait` exits after commit and prints the accepted cursor; that work runs at the next interactive open. Model and doctor commands do not open the room.

SQLite write failure prevents unpersisted acknowledgements and pauses progression. Active requests follow the shared failure-handling contract. Recovery uses durable records rather than missed feed items or mailbox wake-ups.

Debate and room markdown exports consume these committed records through `RoomHandle::export(scope, id) -> String` or offline `export_offline(data_dir, room, scope, id)`. Export implementation and embedded FIGlet banner text live in `squad-core/src/export/` and `squad-core/src/banner/`. Include original solutions, opening positions, rounds, forfeits, round scores and unscored outcomes, terminal status, judge identity, and any accepted verdict with its round tally and tally override explanation. Do not expose credentials or reasoning text.

### Later support

Three or more debaters, `$review`, `$reveal`, `$vote`, `$summarize`, and multiple rooms remain outside v1.

Keep participant and result collections internally extensible while enforcing exactly two debaters. A later multi-debater version must preserve simultaneous rounds, barriers, authorized visibility, and isolated judging. Any changed winner or borrowing-suggestion format requires a new schema version.

### Part 2: Milestones

