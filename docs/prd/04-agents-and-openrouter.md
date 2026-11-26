# Agents and OpenRouter

### Scope and contract ownership

`crates/squad-agents/src/` owns model resolution, ordinary-reply execution, per-member queueing, context assembly, streaming transport, cancellation, and attempt accounting. It also provides the executor for judge calls, case follow-ups, tournament entries, and grill work.

Squad runs these components as tokio tasks within one self-contained Rust binary and one process. Live traffic uses the in-process bus. Durable work, history, resolution snapshots, and accounting use SQLite through router-owned interfaces.

Use `SquadConfig` from `crates/squad-protocol/src/config.rs`, owned by **Foundations**. It is built only from the environment variables permitted by the Foundations configuration contract. The selected `.env` file supplies values, and process environment values override them. This section defines no additional configuration schema or variables. Foundations owns `.env.example` and `setup.sh`.

Use the **Protocol, store and bus** section’s `Work` record, envelopes, `ErrorCode` enum, typed readers, leases, fencing, and commit rules. Durable work belongs in the `work` table. History contains conversational evidence, never work-control records. Agents request durable changes through ingress; the router is the sole logical database writer, using the store’s dedicated writer thread.

`squad-agents` depends on `squad-protocol` and the opaque reader, prompt-rendering, and submit traits supplied through the shared contracts. It never depends on `squad-store`, `rusqlite`, or `squad-core`, including in tests. `squad-core` constructs context-bound capabilities and passes them to executors. Prompt rendering remains in `squad-core`; executors consume it through the injected contract. Compile-fail tests in `squad-agents` verify that executors cannot construct foreign readers or access store APIs. Tests requiring temporary SQLite databases live in `squad-core`.

**Contests** owns contest admission, states, round scheduling, barriers, judge selection, verdict and ranking validation, schema repair, and contest and judge attempt limits. **Prompt pack** owns prompt text, template IDs, placeholders, rendering rules, and overrides. Executors consume those contracts without defining alternatives.

The same ownership extends to census, crosscheck, survey, and probe. All seven modes are scripts over one contest engine’s `collect`, `pick`, `fanout`, `turns`, `judge`, `tally`, `barrier`, and `report` moves, with one active contest per room. Agents execute the authorized model calls for those moves. The engine owns payload selection, participant selection, delivery barriers, tallies, reports, and state transitions. A move does not create another executor-owned scheduler. The build order is debate, grill, census, crosscheck, tournament, survey, probe.

Canonical values are defined in [Protocol’s canonical numbers and configuration](02-message-protocol-store-and-bus.md#canonical-numbers-and-configuration).

Timeouts, attempt counts, queue depth, visible output token ceilings, reasoning budgets, plan expiry, lease timings, channel capacities, probe caps, and message size limits are compile-time constants in `squad-protocol`. They are not configurable in v1.

`SQUAD_ROUNDS` seeds the room’s `ROUNDS` only when no stored value exists. `$ROUNDS=n` and `/set ROUNDS n` update the stored room value, which wins thereafter.

`SQUAD_GRILL_ROUNDS` seeds the room’s `GRILL_ROUNDS` only when no stored value exists. `$GRILL_ROUNDS=n` and `/set GRILL_ROUNDS n` update the stored room value, which wins thereafter.

The optional `SQUAD_CENSUS_PASSES` seeds the room’s `CENSUS_PASSES` only when no stored value exists. `$CENSUS_PASSES=n` and `/set CENSUS_PASSES n` update the stored room value, which wins thereafter. Foundations owns this optional environment variable and leaves it out of `.env.example`. Executors consume the admitted pass and plan version from canonical Work; they do not reread a mutable room setting during execution.

### Model registry and configuration

`crates/squad-agents/src/registry.rs` consumes the validated roster from `SquadConfig` and defines the six defaults:

| Default nick | Configured slug |
|---|---|
| `claude` | `~anthropic/claude-opus-latest` |
| `fable` | `~anthropic/claude-fable-latest` |
| `openai` | `~openai/gpt-sol-latest` |
| `gemini` | `google/gemini-4-neon` |
| `grok` | `~x-ai/grok-latest` |
| `muse` | `meta/muse-wonder` |

The roster consists of the `SQUAD_MODEL_<NICK>` variables in file order, subject to the Foundations configuration contract. If none is set, use the six defaults. Any available OpenRouter slug may be assigned to a valid unique nick. Multiple members may use the same model with different personas. Each nick retains separate history, work, queue, presence, and accounting identity.

Roster additions, removals, renames, personas, and model selections take effect on restart. Concrete slugs select a specific model; aliases track releases at startup. There is no pattern-based model selection or substitution of a historical slug.

`SQUAD_JUDGE_MODEL` configures the judge separately and defaults to `~anthropic/claude-fable-latest`. Judge selection and fallback policy remain owned by **Contests**. Conflicts for debates and standalone judging compare resolved concrete models. Unconfigured default fallbacks, participants, and fallbacks sharing a conflicting concrete model are skipped; only explicitly configured fallback entries require configuration validation. Blind tournament judging never swaps the judge model.

Validate request parameters against the resolved model’s capabilities. A member’s optional `SQUAD_REASONING_EFFORT_<NICK>` overrides `SQUAD_REASONING_EFFORT`; the default is medium. `SQUAD_JUDGE_REASONING_EFFORT` defaults to high for `judge` and `judge.tournament`. `judge.round` always uses low reasoning effort, independently of `SQUAD_JUDGE_REASONING_EFFORT`. Send `reasoning.max_tokens` only when the catalog lists reasoning support, selecting its budget from the effective effort. Do not also send `reasoning.effort`: OpenRouter rejects the two fields together. An explicitly configured reasoning value for an unsupported model raises `MODEL_PARAMETER_UNSUPPORTED`; an implicit default is omitted. Reject explicit unsupported parameters rather than silently dropping them. Omit optional parameters when the shared configuration contract specifies omission.

Judge calls for census, crosscheck, survey, and probe use medium effort independently of `SQUAD_JUDGE_REASONING_EFFORT`. For a reasoning-capable model, reserve the medium reasoning budget of 4,096 tokens in addition to the effective visible ceiling, which is no greater than 4,096 tokens. These calls use the judge system prompt, no member persona, and the owning mode-specific task prompt and strict JSON schema.

Every output ceiling limits visible answer tokens. For a reasoning-capable model, select the compile-time reasoning budget for the effective effort and send `max_tokens = effective visible ceiling + reasoning budget`, with `reasoning.max_tokens` equal to that budget. For a model without reasoning support, send only the effective visible ceiling as `max_tokens`. The effective visible ceiling cannot exceed the canonical ceiling or the portion of the published output limit remaining after the reasoning reservation. Do not dispatch a request whose published limit cannot accommodate its required reservation. There is no output-cap environment override in v1. Startup probes use the separate probe contract below.

Word limits come from `SQUAD_ROOM_WORDS`, `SQUAD_TURN_WORDS`, `SQUAD_FINAL_TURN_WORDS`, and optional `SQUAD_PLAN_WORDS` through `SquadConfig` and the Prompt pack. Daily spend and confirmation values come from `SQUAD_DAILY_MAX_USD` and `SQUAD_DEBATE_CONFIRM_USD`.

### Startup resolution

`crates/squad-agents/src/resolver.rs` fetches `GET https://openrouter.ai/api/v1/models` once per startup and shares the normalized catalog across members and judge selections.

For each configured model:

1. For a concrete slug, validate that it exists and read its limits, capabilities, and pricing. An unknown concrete slug is a startup error naming its environment variable.
2. For a concrete slug, use that exact catalog entry.
3. For an alias, probe through the alias with `max_tokens: 16` and the lowest reasoning setting the model accepts. On HTTP 400, retry once with `max_tokens: 2048`. Take the response’s `model` field as its concrete identity and validate it against the catalog. The alias itself need not appear in the catalog.

A probe that fails or returns no valid concrete target does not justify guessing the target. Report the resolution failure through the shared error contract and mark the affected member `offline`. Other valid members remain usable. Apply normal credential protection, daily dispatch guards, and accounting to probe calls. Alias probes run only during room open, under a router-created synthetic `workId` with `contextId = "probe:{nick}"`; the same attempt ledger accounts for successful, failed and retried probes. `squad models` and `squad doctor` read the `models` cache and never probe.

Persist the normalized catalog, fetch timestamp, and resolution snapshot through the router and store interfaces, using an atomic transaction. Use the Protocol section’s `catalog` table and role-aware `models` records for members and judge selections.

Every startup attempts a fresh catalog fetch. After three transient fetch failures, a catalog cache at most 24 hours old may be used with a visible warning. An older or absent cache prevents startup. A cached catalog does not replace a required alias probe. A successful fresh catalog response is authoritative.

Resolution occurs only at startup. The concrete identity recorded for display remains fixed until restart. Every completion request uses the resolved concrete slug. Record the response `model` for each attempt; a mismatch logs a warning and is exposed to `squad doctor` without silently changing the pinned identity. Log each nick, selection method, concrete identity or availability error, catalog timestamp, and cache use. Expose resolved identities through `RoomHandle.state()` so the TUI can show them beside the nicks.

### Worker lifecycle and queueing

`crates/squad-agents/src/worker.rs` creates one tokio task per configured member.

Each task receives the Protocol-owned `AgentContext`, bound to `ExecutorKey::Member(nick)`. Its `WorkReader` enumerates that member’s work across ordinary and step contexts; `HistoryReader::for_work` verifies ownership and binds the read to the selected work’s own frozen context. `squad-core` constructs every context and injects the Protocol-owned `PromptRenderer` and `ExecutorSubmit` capabilities. Workers cannot enumerate foreign histories, read `events`, or subscribe to the feed.

The router commits canonical `Work` before sending a mailbox wake-up. Each mailbox is an in-process capacity-1 `mpsc` channel carrying wake-ups only. `wake_mailbox` uses synchronous `try_send`; `Full` means a wake-up is already pending. The router never awaits a channel owned by an agent. Workers read durable jobs through their executor-bound `WorkReader`.

On startup, establish the mailbox receiver before scanning pending work. Recover unfinished work authorized by the **Room API and CLI** open-mode contract, deduplicate wake-ups by `workId`, and obey lease and cancellation fences. `OpenMode::OneShot` never re-wakes work it did not create; `--no-wait` work remains pending until the next interactive open. Never scan conversational history for control records.

Each member has one active generation and a FIFO queue. Different members execute concurrently. A new ordinary message never interrupts existing work. Enforce the canonical queue limit independently for each recipient; one member’s overflow or failure does not stop other addressed members.

Queued execution must include completed earlier replies while excluding user messages after its triggering message. Use the router-supplied `Work.historyThrough` and `triggerEventId` boundaries for context assembly. Do not substitute the current event position or an unrestricted history read.

Contest scheduling follows **Contests**:

- Existing ordinary work drains while the contest is waiting for its participants.
- Ordinary work addressed to a held participant remains queued until the contest releases its hold (including census `WAITING_FOR_PLAN`). The router defers writing those messages and their quotes into that participant’s private history until the hold is released.
- Contest work does not wait behind newly queued ordinary work.
- Other members continue ordinary chat.
- A direct `$debate` command creates no ordinary `reply` work.
- Question-first `$debate` and `$grill` collection creates ordinary `reply` work for the addressed members. Those replies use normal queueing, isolation, attempts, and daily accounting. While a pick is pending, ordinary chat continues and participant holds have not begun.
- Tournament entries use the Protocol section’s reply-like work carrying a contest identity.
- Grill work executes only for the participant and turn authorized by the orchestrator.
- Debate-turn attempts reuse the round’s frozen boundary.
- Round-score work runs on the judge case context and does not block either debater’s next round.
- Census, crosscheck, survey, and probe work runs only for the member, step, pass, role, and payload authorized by the script.
- A `fanout` dispatch delivers each member’s own committed payload. It never delivers a common superset and asks the model to ignore material assigned to someone else.
- Survey’s optional `collect` step uses ordinary isolated reply execution. Reusing an existing parallel answer set creates no replacement reply calls.
- A crosscheck auditor call uses a fresh step context while retaining the selected member’s execution and accounting identity. It does not create a second concurrent generation for that member.

Workers submit presence through ingress using the bus section’s shared presence contract, including active work, queue depth, observation time, and sanitized error code. Presence remains in memory and is emitted through the feed.

The normal lifecycle is:

`offline → idle → thinking → streaming → idle`

A terminal request failure enters `error`; a later accepted task may return it to `thinking`. Heartbeats occur every five seconds. Missing heartbeats for fifteen seconds mark the worker `offline`.

### Context assembly and prompt authority

`crates/squad-agents/src/context.rs` builds provider messages exclusively from the addressed context’s private history through the canonical Work boundary, plus Squad-owned Prompt pack instructions and Work metadata.

Use the Prompt pack’s templates and placeholder mapping through the injected renderer. Do not duplicate prompt text in the agents crate. Round metadata does not carry another copy of the topic, solutions, instruction, or previous turns. Those inputs must already be committed to the recipient’s history.

Every member’s system prompt names the configured roster, supplies `SQUAD_USER_NAME`, and explains its visibility limits, including the explicit shared-floor exception for an active grill. The name defaults to `the user`; the human remains `me` in the TUI and protocol identity. The tournament judge’s system prompt contains no entrant nick, model name, roster, or label-to-nick map.

A member’s optional `SQUAD_PERSONA_<NICK>` value is empty by default. Append it after the shared room system prompt within the single top-level system message. Personas are environment configuration only and never apply to the judge.

Provider roles preserve attribution:

| History content | Provider representation |
|---|---|
| Human message, including its author-labelled quotes | `role: "user", name: "me"` |
| This member’s completed reply or completed debate turn | `role: "assistant", name: nick` |
| Another member’s authorized debate turn | `role: "user", name: senderNick` |
| Notices, forfeits, and verdicts delivered to a member | `role: "user", name: "squad"`, prefixed `[squad notice]` |
| `debate.setup` | `role: "user", name: "squad"`, rendered with `debate.setup.md` |
| `debate.opening` | `role: "user", name: "squad"`, rendered with `debate.opening.md` |
| Judge case material, including prior judge outputs and accepted round scores | Included once within the single rendered judge user message, `role: "user", name: "squad"` |
| This entrant’s completed tournament answer | `role: "assistant", name: nick` |
| This entrant’s tournament notice, own result, or final placing list | `role: "user", name: "squad"`, rendered through the owning tournament template |
| This participant’s completed grill question, answer, or rebuild | `role: "assistant", name: nick` |
| Another grill participant’s authorized question or answer | `role: "user", name: senderNick` |
| Grill setup, current plan version, prior-question list, pass, or turn notice | `role: "user", name: "squad"`, rendered through the owning grill or notice template |
| Census, crosscheck, survey, or probe step inputs and assigned payload | `role: "user", name: "squad"`, rendered through the owning mode template |
| An anonymised ledger, claim list, reasons, checks, grid, or sampled evidence authorized for that step | Included in the owning rendered user message without source nicks, model names, or finder counts |
| This actor’s accepted mode output, when the script retains it for a later step | `role: "assistant", name: nick`, only within that actor’s authorized context |

Only the Squad-owned top-level prompt uses `role: "system"`. Labels come from validated envelopes, never from claims inside model text.

For every judge invocation, send only the judge system prompt and one rendered user message. Build that message from the authorized case material and, for a repair, the attempt-scoped repair record. Do not also append the underlying case history as separate provider messages. Prompt pack owns the rendering of setup, opening, case material, and repair data.

The router resolves human-selected quotes and writes their author-labelled text into each recipient’s user message as specified by the bus section. The executor neither dereferences arbitrary room events nor copies foreign replies itself. Outside authorized contest delivery, human quoting is the only path for another member’s words to enter the recipient’s context.

Round scores and running tallies never enter either debater’s context before the verdict. The router delivers the per-round scores and final `roundTally` to both debaters with the verdict announcement; context assembly consumes that authorized announcement through their private histories.

Blind mode steps receive only the material their script authorizes. `squad-core` supplies step-bound readers and committed inputs where a member’s running history would expose forbidden material. A prompt instruction to ignore earlier history does not establish blindness. In particular, crosscheck inventory work cannot receive the plan through the triggering command, selected quotes, an earlier ordinary reply, an earlier contest, or a previous step. A fresh auditor context cannot contain its member’s inventory, cell reviews, prior audit conversation, or ordinary history.

Anonymisation applies to delivered content and its metadata, including provider message names, source labels, and serialized placeholders. Private provenance maps and the human’s full report do not become member context unless the script explicitly authorizes a suitable projection. Step-bound execution preserves durable history; it does not rewrite or delete the member’s ordinary history.

Model output is plain text. Mentions, commands, speaker labels, and quoted instructions inside it cannot route messages, summon the judge, change settings, execute tools, or grant access to other histories. Members have no web search, code execution, or file access in v1.

### Context budgeting

Let `C` be the catalog context limit, `O` the complete output reservation including visible output and any reasoning budget, and `S = max(1024, ceil(0.05 × C))` the safety reserve. Input must fit `C − O − S`. Apply the canonical visible ceiling, reasoning budget, and published model output limit when calculating `O`.

There is no separate fixed input-token ceiling. The allowed environment variables provide no context-limit override. Do not dispatch a request whose context budget cannot be established.

Use a matching tokenizer where available. Otherwise use a conservative UTF-8 byte estimate with message-structure overhead. Record the estimator used.

Retain the system prompt, current request, and mandatory contest or case material. Remove oldest complete optional exchanges until the input fits. Do not summarize automatically or truncate message text mid-message. Exclude superseded ordinary replies according to the bus contract. The grill’s explicit working-context projection follows its owning contract and does not rewrite or delete durable history.

If mandatory material cannot fit, return `CONTEXT_TOO_LARGE` with estimated and allowed tokens. Judge requests retain all material required by the case contract, including the complete debate transcript where applicable.

Expose this estimator and effective model limits to **Contests**. Its preparing step rejects a contest when the worst-case mandatory material would exceed a participant’s or the judge’s context budget. Include accumulated round scores, unscored-round notices, and the verdict announcement’s score material in the applicable budgets. For a reasoning-capable `judge.round` model, reserve the effective visible ceiling up to 512 tokens plus the low reasoning budget of 1,024 tokens. Include attempt-scoped rejected output and validation errors when budgeting a repair. Context assembly must not silently discard mandatory evidence after admission.

Apply the same estimator to each census, crosscheck, survey, and probe step’s authorized projection. Mandatory material includes the current plan version where permitted, the complete assigned ledger or cell batch, required claims and reasons, checks and predictions, and the judge evidence declared by the script. A cell batch is one member payload, not permission to create a provider call per cell. Executors cannot shrink a grid, omit ledger entries, drop claims, or split work into extra calls to make a request fit. Return the owning context error to the engine.

### Judge and case executor

`crates/squad-agents/src/judge.rs` executes canonical `judge`, `judge.round`, and `judge.tournament` Work using the same transport, context-budgeting, cancellation, and accounting adapter as member workers.

The router commits adjudication material into the dedicated `judge:{caseId}` history context and enqueues judge Work for that context. Each executor receives an `AgentContext` bound to that case. It reads only that case’s history and Work. The orchestrator selects and schedules evidence through router-owned interfaces; it never assembles provider prompt text.

The router spawns an executor for every case with authorized unfinished work, including during interactive recovery. It reuses that case-bound executor while work remains and retires it when the case has no runnable or unfinished work. A later follow-up may spawn a new executor bound to the retained case. One-shot recovery follows the Room API open-mode contract.

Initial summons start without prior judge or member history. The judge never reads room-member `fable` history, even when it uses the same concrete model.

The executor supports the three judge paths defined by **Command grammar and parser** and **Contests**:

- A debate judge receives its router-committed adjudication bundle and returns output governed by the owning verdict schema.
- A standalone summon with `@judge` as sole recipient and at least one quote creates a new case. Its evidence is exactly the selected quoted messages plus the user’s question. Its ruling is free-form text.
- A sole-recipient message quoting a verdict reopens that case. Its context contains that case’s material, its verdict, prior follow-ups, and the new question. It receives no unrelated case history.

For a debate with automatic judging, the orchestrator supplies one `judge.round` Work item after each round-barrier commit for rounds 1 through `totalRounds`. Round 0 opening positions are not scored. The router has already committed that round’s two turns to the same judge case context. That context accumulates topic, solutions, instruction, round turns, and the judge’s own accepted round scores. Assemble each request through its canonical Work boundary.

Consume Prompt pack’s `judge.round.md` with the judge system prompt. It asks for scores only for the identified round’s two turns, using the owning strict JSON schema with integer scores from 1 to 10 and one sentence of reason per debater, at most 200 characters. Equal round scores are allowed. Use low reasoning effort with a 1,024-token reasoning budget when supported and an effective visible output cap no greater than 512 tokens. No new configuration variable controls either value.

Submit round-score results through the owning validation path. The router commits `debate.round_score`, computes the running tally, and writes the result to the judge case context and the human event log only. The executor never copies scores to debater histories or computes authoritative tallies.

Round scoring does not delay the next round. Scores may arrive late and retain their supplied round identity. Final `judge` Work becomes runnable only after all round-score work has completed or been given up. After two failed attempts, including schema repair, the owning section records an unscored round with `scores: null` and `reason: "unscored"`. Final prompt assembly consumes the committed scores and unscored-round information.

A debate that ends `COMPLETE_UNJUDGED` has no round scoring. `/judge` on a stopped debate executes the final verdict from the transcript and existing round scores without scheduling retrospective round scoring.

A debate verdict is also a case. Retain its material and follow-ups for later case questions. There is no automatic history retention limit in v1; `/clear` follows the store section’s deletion rules.

Use the judge-specific Prompt pack instructions, `SQUAD_JUDGE_REASONING_EFFORT`, and the canonical judge visible output ceiling for `judge` Work. The default effort is high. Do not append a member persona. The final verdict’s `roundTally` is computed by the router, never requested from the model. Consume the owning verdict schema and repair instructions for `tallyOverrideReason`, which must explain a winner that is not the round-tally leader.

Repair attempts read the rejected answer and validation errors from the Protocol section’s attempt-scoped record keyed by `(work_id, attempt)`, accessible only through the case-bound `WorkReader`. Rejected output never becomes conversational history. Consume the owning two-pass validation result: strict JSON Schema validation with `additionalProperties: false` and required nullable fields, followed by deserialization. The executor does not define another verdict or ranking validator.

Judge model selection, conflict warnings, automatic judging eligibility, `/judge`, attempt scheduling, and schema repair belong to **Contests**. The executor makes one provider call per authorized attempt and returns its result through the authorized result path. It does not add transport retries outside that section’s attempt allowance or summon itself after an unjudged or stopped debate.

The same case-bound executor executes the Protocol section’s census merge and finalization, crosscheck normalization and audit ruling, survey extraction and report, and probe report Work kinds. Crosscheck’s optional goal-and-constraints extraction is also an authorized judge call. These are structured mode tasks, separate from standalone free-form rulings and debate verdicts. Each uses its owning schema, medium effort, the 4,096-token visible ceiling, and the existing case-bound repair path. Only the engine may schedule a repair or advance the script after validation.

### Tournament and grill execution

The executor consumes the new Work kinds and visibility rules from **Protocol, store and bus** and the scheduling contracts from **Contests**:

| Work | Execution contract |
|---|---|
| Tournament entry | Reply-like work with a contest identity; ordinary reply word and visible output limits; only the entrant’s authorized private material |
| `judge.tournament` | One blind ranking request per authorized attempt on the tournament case context; 120 s deadline; judge visible output ceiling and configured judge effort |
| `grill.question` | Current plan, current-round floor, prior-question list, assigned angle, and owning question template; at most 200 words and three questions, or exact `PASS` |
| `grill.answer` | Authorized question and shared floor through the Work boundary; `turnWords` limit; direct answers and “Plan changes” |
| `grill.rebuild` | Latest plan and current-round floor through the Work boundary; Prompt pack’s `plan_words` limit; visible output ceiling 4,096 |

Tournament entrant requests consume `tournament.entrant.md`. From round 2, the authorized material includes only that entrant’s own previous rank, score, critique, and survival result. They never receive another entrant’s answer, rank, score, or critique. Consume `tournament.result.md` for the entrant’s own result and `tournament.final.md` for the final placing list, which contains nicks and placements only.

`judge.tournament` consumes `tournament.judge.md` with the judge system prompt and one rendered user message. The router supplies the blind, shuffled labels and authorized anonymized answers. The label-to-nick map remains outside the judge context and is unavailable to the executor’s case-bound readers. The rendered request contains no entrant nick or model name. Submit the ordered ranking through the owning permutation validator and repair path; do not map labels, select an elimination, award trophies, or replay a round in the executor. The judge model remains unchanged when it is also an entrant.

Grill requests consume `grill.question.md`, `grill.answer.md`, and `grill.rebuild.md`. The router commits shared-floor material into exactly the participants’ private histories. Executors neither broadcast floor content nor fetch it from another participant’s context.

At the start of each grill round, assemble the working context from the latest plan version, that round’s questions and answers, and the compact list of earlier questions. Do not include earlier rounds’ full answers in the grill working context. Preserve the full durable transcript and every plan version for the owning export and history contracts. This explicit projection prevents repeated questions without automatic summarization.

Return a griller’s exact `PASS` through the owning result path. The orchestrator owns turn rotation, angle rotation, pass counting, rebuild scheduling, early completion, griller skips, and leader failure. A grill never schedules judge work. Each retry uses the original turn’s frozen inputs and the owning attempt allowance.

Question-first collection uses ordinary reply execution and does not require a separate contest prompt or judge call. The router and contest engine own eligible-answer tracking and the pick. Collected answers remain ordinary replies after cancellation and are excluded from the later contest preflight estimate.

### Census, crosscheck, survey, and probe execution

Consume the Protocol section’s canonical Work kinds for the following tasks. The task names below identify execution responsibilities and template mappings; Protocol owns their representation. Member tasks run through the member adapter and judge tasks through the case adapter. Prompt pack owns prompt text, placeholders and word limits; Contests owns each mode’s output schema and typed payloads, added to `squad-protocol` in its M5 phase.

Step payload visibility is defined only by [Protocol’s visibility and prompt authority contract](02-message-protocol-store-and-bus.md#visibility-and-prompt-authority); each mode adds its projection rows in its M5 phase.

Census hunters return up to eight omissions with consequences. Rating work returns `real`, `covered`, or `trivial` for each ledger gap, with a plan citation for `covered`, and any proposed additions. The engine assigns and rotates lenses, merges accepted artifacts through its validation paths, computes finder counts and the unseen-gap estimate, and applies the quiet rule. Executors do not calculate an authoritative estimate or expose the human’s member-by-gap table to members. A later pass uses the plan version committed for that pass, including a human-supplied patch; retries retain the original pass’s frozen plan and lens.

Crosscheck inventory work must start without plan-bearing history. The router supplies the goal and constraints directly, or first schedules the owning judge extraction task when they are absent. Executors do not obtain the plan themselves to infer missing inventory inputs. Cell assignments are committed by the engine and exclude cells built from that reviewer’s own inventory items. The reviewer receives only its assigned batch and returns the owning citation, justified-inapplicability, or gap-with-fix result for each cell. It cannot fetch the rest of the grid or another reviewer’s findings.

The engine randomly selects two crosscheck auditors and commits each auditor’s closure sample before dispatch. `squad-core` constructs a fresh audit history context for each step; the member executor receives its work through the same `ExecutorKey::Member` capabilities and `HistoryReader::for_work` binds the read to that audit context. The configured member supplies the resolved model and accounting identity, but its ordinary conversation, inventory, review work, and previous audit conversation are absent. Each auditor receives the plan, grid, and sampled evidence without reviewer identities. Audit retries reuse the same sample and frozen inputs. The executor neither resamples evidence nor selects replacement auditors. The judge’s ruling passes through the owning validator; executors cannot drop a cell or accept a residual risk for the human.

Survey collection, when required, produces ordinary mutually invisible replies. The engine selects the answer set and delivers it only to the extraction case. Voting contexts receive the extracted claims rather than the whole answers, including when a voter originally wrote one of those answers. Revoting contexts receive only the script-authorized claims and anonymous reasons, without another member’s votes, authorship, or counts. Return the owning vote enum, one-line reason, confidence, and revote evidence fields. The engine computes splits and switches and supplies the report case with private provenance. The full five-band report is not broadcast into voting contexts.

Probe executors consume the engine’s committed explainer/checker assignment. They do not split or shuffle members themselves. A member sitting out an odd roster receives no probe generation work. Initial explanation and check tasks remain mutually blind. Prediction work retains the explainer’s own committed explanation and receives every committed check, without rival explanations or predictions. Preserve explicit inability to predict in the owning schema. Report work consumes the locked matrix and returns the structured decision tree, inseparable explanations, and unexpected-result branch. No executor runs a proposed check or treats the report as proof that an explanation is true.

Each task makes one provider call per authorized attempt and submits its result through the owning validation and commit path. The engine owns member retries, skips after retry exhaustion, judge repair, barriers, and failure transitions. A skipped member’s identity and failed step remain available to the human report without entering anonymised member payloads. Executors do not advance a step from partial streaming output.

### Streaming and completion

`crates/squad-agents/src/openrouter.rs` uses `reqwest` to send `POST /api/v1/chat/completions` with `stream: true`, the resolved concrete slug, assembled messages, supported parameters, and the complete output reservation. Parse SSE incrementally, including events split across response chunks.

Read `OPENROUTER_API_KEY` through `SquadConfig`. Keep it inside the provider adapter. It never reaches the TUI, feed events, database, exports, or logs.

Submit shared `delta` envelopes through ingress. Use the bus contract’s integer `attempt`, `chunkIndex`, `workId`, `messageId`, `triggerEventId`, and applicable contest and round metadata. The router validates ownership before forwarding. Workers never publish directly to the human feed.

Buffer answer text locally. Ordinary completion submits `agent.reply` through ingress; the router durably accepts it as `message.final` under the bus contract. Contest and judge results use their owning submission and validation paths. Preserve the shared `finishReason`, including truncation or content filtering, rather than presenting every response as an unrestricted completion.

Deltas are ephemeral. A higher `attempt` replaces the older attempt’s provisional text. Never concatenate attempts or emit an undefined reset event. The accepted final replaces provisional text even when chunks were missed.

Discard reasoning text before any feed forwarding or conversational persistence. Process usage independently from answer content.

A worker considers completion accepted only after router acknowledgement. Result acceptance is idempotent and fenced by the canonical Work rules. Recovery may cause an uncertain provider request to be billed more than once, but only one authorized result can commit.

### Ordinary failures, cancellation, and recovery

Apply the canonical ordinary-reply timeouts from `squad-protocol`. The total deadline includes retry waiting and both permitted attempts.

Retry transient network failures, 429 responses, and 5xx responses once. Use exponential backoff with full jitter, a one-second base, and a thirty-second cap. Honor `Retry-After`; terminate when waiting would exceed the total deadline. Other 4xx responses are terminal.

`attempt` counts provider calls. Persist attempt state through the bus interfaces before dispatch. A retry after partial output starts a new attempt and replaces provisional text.

After the permitted ordinary attempts fail, submit a shared error event with the supported retry action. A user-requested retry schedules new reply work through the **Room API and CLI** and router contracts; it does not modify or duplicate the original human message. Failure of one recipient remains local to that recipient.

A debate worker makes exactly one provider call per authorized attempt. It reports timeout or provider error to the orchestrator, which alone issues the second attempt and records any forfeit. Judge attempts and repairs similarly remain under **Contests**, with three attempts for `judge` and `judge.tournament` and two for `judge.round`, including schema repairs. Tournament entries and grill work consume their owning attempt and failure contracts without adding retries or round replays in the adapter.

Census, crosscheck, survey, and probe tasks follow the same engine-owned attempt scheduling. Member failure after the permitted retry is returned for a skip and report entry. Judge exhaustion follows the script’s failure rule. A retry cannot change a lens, plan version, assigned cells, audit sample, claim list, role assignment, or locked prediction input. No transport retry or schema-repair call occurs outside an authorized attempt.

Use `tokio_util::sync::CancellationToken` through `AgentContext.cancel`. Cancellation must stop pending HTTP dispatch, response streaming, and retry timers. Canceled queued work makes no provider call.

- `/stop` and the TUI’s confirmed global stop action cancel all active and queued generation through the shared control path, including outstanding judge and contest work.
- Escape closes the topmost transient element first. With nothing open, it requests a global stop; during a contest, the TUI requires a confirming second Escape within two seconds.
- `/stop @nick` targets that member.
- Cancellation involving a contest follows **Contests**, preserving its committed transcript and applying the owning terminal-state and judging rules.

The router serializes cancellation against result acceptance. The first accepted terminal transition wins, and fencing rejects late results. Canceled partial text remains uncommitted. Billing information for a canceled or fenced attempt still follows the accounting path.

Restart recovery consumes durable work and attempt state. An interrupted provider attempt consumes an attempt; it does not reset the allowance. Preserve uncertain charge reservations. Store failures follow the bus and hardening failure rules, including aborting active calls and never acknowledging an unpersisted result.

Recover new-mode step contexts from their committed deliveries and Work boundaries, including fresh auditor contexts. Recovery cannot attach a reader to a member’s unrestricted history, rerandomize assignments or samples, or reuse a completed artifact as permission for another call.

### Regeneration

Regeneration uses the `regen` request through `RoomHandle`, including the parser’s `/regen [messageId]` path. The router commits `message.superseded` and enqueues new `reply` Work as specified by the bus section.

The replacement targets the original member and originating user request. It follows the same FIFO queue, context boundaries, daily spend checks, output limits, cancellation rules, and ordinary attempt allowance as other replies.

Exclude the superseded reply from that member’s context from then on. Keep the durable supersession record for replay. Do not rewrite previously delivered quotes, frozen contest evidence, or sealed cases. Sent human messages cannot be edited.

### Usage, cost, and dispatch guards

`crates/squad-agents/src/accounting.rs` records each provider call’s request ID, requested concrete model, response `model`, work and attempt identity, timing, token usage, reported cost, and outcome through router-owned persistence. Include startup probes, retries, judge calls, round-score calls, tournament entries and judging, grill work, question-first collection replies, and canceled or failed requests whenever billing information is available.

Request streaming usage and process terminal usage separately from content. Preserve provider accounting without double-counting reasoning tokens already included in completion totals.

When terminal usage or reported cost is absent and the provider request ID is known, reconcile through `GET /api/v1/generation?id=...`. Merge the lookup result into the existing attempt record rather than creating another charge. If the ID is unavailable or the lookup cannot establish usage or cost, retain unknown values and the uncertain reservation. Never substitute zero or an estimate for reported cost.

Use the shared `Usage` contract for envelopes. Keep reported and estimated costs distinct. Pricing estimates use the resolution snapshot and are labelled estimated. Replayed or repeated accounting updates cannot increment totals twice. Judge charges remain separate from member `fable` charges while contributing to the shared daily total.

Feed actual per-call cost to the Protocol section’s `spend_daily` accounting. The router commits the attempt’s accounting and daily-total update in the same transaction as the event completing the call. Failed and canceled calls that were billed count. Later reconciliation updates the existing charge and daily total atomically without counting the call again. The Protocol section owns local-day attribution, shared totals across rooms on the machine, reservations, and rollover.

In-flight estimates remain pending reservations and are replaced by actual cost when it arrives. A response without reported cost is recorded as `cost unknown`, contributes to the unknown-call count, and keeps the total visibly uncertain through the owning RoomState contract. `squad doctor` reports that count. Only Squad’s own calls count.

Every dispatch uses the router’s daily reservation and admission checks. Reservations cover every permitted attempt. The adapter cannot bypass those checks for a probe, retry, regeneration, contest turn, or judge call.

At the daily max, reject new generation requests with `DAILY_MAX_REACHED` and the message `Daily max of $150.00 reached. Raise SQUAD_DAILY_MAX_USD or wait until tomorrow.`, formatting the amount from the configured max. Do not send queued or subsequent calls until the next local day or a raised max permits them. In-flight calls finish and their actual cost is recorded; reaching the max does not cancel them.

Contest preflight and threshold confirmation belong to **Contests** and use the Protocol section’s canonical preflight payloads. Every preflight states the number of model calls and the estimated cost against the remaining daily amount. For automatically judged debates, the estimate includes one judge call per scored round in addition to the final verdict; round-score reservations cover both permitted attempts. Tournament estimates include every surviving-entry round and its judging; grill estimates include each question, answer, and rebuild. Question-first collection replies have already been charged as ordinary work and are excluded from the later estimate. Executors begin only after authorized Work becomes runnable. Uncertain provider charges remain reserved until reconciled.

Include every census, crosscheck, survey, and probe member and judge call in the same accounting path, including optional crosscheck goal extraction, fresh audits, retries, repairs, and survey collection. Preserve contest, step, pass, member or judge identity, work, and attempt attribution supplied by the owning contracts. A fresh context does not create a new untracked billing identity. Application-only tallies, barriers, picks, and report assembly create no provider charges; an authorized judge report call does. The engine computes preflight call counts from its script, and the adapter supplies the same model limits, pricing, and output reservations used at dispatch.

### Deterministic provider fixture

`crates/squad-agents/tests/support/fake_openrouter.rs` provides a wiremock-based fake OpenRouter service with compatible models, streaming completions, and generation lookup routes for credential-free tests.

Fixtures control catalog contents, alias targets, response model identity, chunks, delays, terminal usage, rate limits, malformed events, disconnects, provider errors, structured judge output, round-score output and failures, tournament rankings and repairs, grill questions, passes, answers and rebuilds, and free-form case replies. Capture request bodies for assertions.

Add fixture responses for census hunts, merges, ratings, and finalization; crosscheck goal extraction, inventories, normalization, cell reviews, fresh audits, and rulings; survey extraction, votes, revotes, and reports; and probe explanations, checks, predictions, and reports. Include malformed structured outputs, failed members, exhausted repairs, delayed calls, and billed cancellations. Captured requests must expose payload and context mistakes without real OpenRouter credentials.

M2 uses this fixture to verify execution before M5 exists. Supply canonical Work, opaque reader and submit test doubles, and Prompt pack fixture outputs. Tests requiring router-committed history backed by temporary SQLite databases remain in `squad-core`. Full contest admission, scheduling, round barriers, verdict and ranking semantics, picks, and command-to-completion integration remain M5 responsibilities.

The M2 prompt work embeds and loads the templates and manifest through `squad-core` and implements the renderer. Prompt pack retains ownership of prompt text and its M5.P3 text todos. Executor tests use contract fixtures without depending on the M5 orchestrator.

