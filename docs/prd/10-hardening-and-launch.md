# Hardening and launch

### Scope and dependencies

M6 verifies isolation, recovery, cost controls, performance, and terminal safety across the completed product. M7 prepares documentation, release artifacts, release records, and the recorded demonstration.

Build order is M0, M1, M2, M3, then M4 and M5 in parallel. M4 exits after M5, followed by M6 and M7. Earlier milestones provide component tests; M6 provides the integrated release gate.

All seven modes are scripts over the same engine’s `collect`, `pick`, `fanout`, `turns`, `judge`, `tally`, `barrier`, and `report` moves. D71 sets their implementation order: debate, grill, census, crosscheck, tournament, survey, probe. M6 verifies shared-move behavior and each script’s declared visibility, prompts, schemas, limits, failure rules, and preflight counts. Restating debate, tournament, and grill over shared moves must preserve their existing behavior. The Contests section owns the M5 phases and scripts.

Squad ships as one self-contained Rust binary. The router, agent tasks, contest orchestrator, and TUI run in one process. Live delivery uses in-process tokio channels. Durable state uses one SQLite database for the persistent room `main`, with WAL mode and `rusqlite` using bundled SQLite. Daily spending is shared across all rooms on the machine under the storage contract owned by **Protocol, store and bus**.

The workspace contains six crates: `squad-protocol`, `squad-store`, `squad-agents`, `squad-core`, `squad-tui`, and `squad-cli`. Executors, the OpenRouter client, and the model registry live in `squad-agents`, which depends on `squad-protocol` and reader/submit traits, never on `squad-store` or `rusqlite`. `squad-core` constructs the context-bound readers and passes them to executors. Markdown export, FIGlet banner text, and embedded fonts live in `squad-core`.

Shared contracts retain their owners:

| Contract | Owning section |
|---|---|
| Envelopes, `Work`, `Cursor`, complete `ErrorCode` enum, transactional commits, typed readers, and durable replay | Protocol, store and bus |
| Grammar, parser AST, and exact validation strings | Command grammar and parser |
| `SquadConfig`, environment variables, and `./setup.sh` | Foundations |
| `RoomHandle`, process lifecycle, room locking, and CLI commands | Room API and CLI |
| Ordinary reply execution, queueing, context assembly, model resolution, and accounting | Agents and OpenRouter |
| Shared contest engine, all seven mode scripts, states, setup, results, attempts, deadlines, admission, and recovery | Contests: debate, tournament, grill |
| Prompt text, placeholders, limits, and overrides | Prompt pack |
| Interaction, readability, key handling, and terminal output | TUI (Ratatui) |
| Effects, motion, and frame-budget governor | TUI effects and motion |

M6 tests these contracts without introducing alternative schemas, error names, retry policies, or limits.

### Canonical numbers and configuration

| Thing | Value |
|---|---|
| Members | 1 to 12; nick pattern `^[a-z][a-z0-9_]{0,31}$`; reserved `me`, `all`, `judge`, `system`, `squad` |
| `ROUNDS` | Default 10, min 1, max 20 |
| Prompt-enforced word limits | `roomWords` 500, `turnWords` 400, `finalTurnWords` 500 |
| Visible output token ceilings | 2,048 for room replies, tournament entries and debate turns, 4,096 for final judge calls and rebuilt grill plans, 512 for `judge.round` calls |
| Reasoning effort | Medium by default for members, configurable per member; low for `judge.round`, high for final judge calls; reasoning text never forwarded |
| Ordinary reply | Timeouts 15 s connect / 90 s first token / 30 s idle / 300 s total; 2 attempts, then an error event with a retry action |
| Debate turn | 180 s deadline; 2 attempts, the second issued by the orchestrator; then forfeit |
| Judge call | 3 attempts including schema-repair attempts for final judging |
| Per-agent queue | 20 items, then `AGENT_QUEUE_FULL` |
| User message size | 1 to 32,768 UTF-8 bytes |
| Daily max and confirmation | `SQUAD_DAILY_MAX_USD` default 20; debate confirmation threshold `budget.debateConfirmUsd` = 2 |
| Quotes per message | Max 12 |
| Durable storage ceiling | Fixed 1 GiB, following the Foundations configuration contract |
| Round judge call | 2 attempts including schema-repair attempts; then record the round as unscored |
| Reasoning token budgets | Low 1,024; medium 4,096; high 8,192 |
| Final judge call deadline | 300 s |
| `judge.round` and `judge.tournament` deadline | 120 s |
| Preflight plan expiry | 10 min |
| Work lease | 60 s, renewed every 20 s |
| Ingress channel capacity | 256 |
| Mailbox capacity | 1; wake-up pending flag, synchronous `try_send` |
| Feed broadcast capacity | 4,096; lag recovers from the events table after the last seen cursor |
| Active contests | One per room, including answer collection and a pending pick |
| Tournament entrants | At least 3; with k entrants, k−1 elimination rounds |
| Tournament judging | 3 attempts including schema repair; on exhaustion replay the round once, then stop with an error if judging still fails |
| `GRILL_ROUNDS` | Default 2, min 1, max 10 |
| Grill question turn | Up to 3 questions, 200 words |
| Grill answer turn | `SQUAD_TURN_WORDS`, default 400 words |
| Rebuilt grill plan | `SQUAD_PLAN_WORDS`, default 1,200 words; visible output ceiling 4,096 tokens |
| `CENSUS_PASSES` | Default 2, min 1, max 5 |
| Census participants and hunt | At least 3 members; one assigned lens and up to 8 gaps per member per pass |
| Census calls | `2k + 2` per pass for k members; 14 for six, 28 for two six-member passes |
| Crosscheck inventory and grid | 3 items per member; single-item cells and every cross-lens pair; cap 200 cells |
| Crosscheck audit | 2 randomly chosen members, each in fresh context, each checking 5 sampled closures and proposing one missing case |
| Crosscheck calls | `2k + 4`; 16 for six members, plus one call if goal and constraints must be extracted |
| Survey claims and revote | Up to 25 claims; up to 5 most evenly split claims selected for revoting; confidence 1 to 5 |
| Survey calls | `3k + 2` with answer collection, `2k + 2` with existing answers; 20 or 14 for six members |
| Probe participants | An even active count of at least 4; with an odd addressed count, the last member sits out and is told in the transcript |
| Probe calls | `k + k/2 + 1` for k active members; 10 for six |
| Census, crosscheck, survey, and probe judge calls | Judge system prompt plus mode-specific task prompt and strict JSON schema; medium reasoning effort; 4,096 visible output tokens |

Every output token ceiling limits visible output. For models whose catalog entry lists reasoning support, send `max_tokens = visible ceiling + reasoning budget` and an explicit `reasoning.max_tokens` equal to that budget. For unsupported models, omit reasoning parameters; only an explicitly configured unsupported reasoning setting raises `MODEL_PARAMETER_UNSUPPORTED`. Verify request bodies and reservations against **Agents and OpenRouter**.

`SquadConfig` is the typed configuration struct in `crates/squad-protocol/src/config.rs`. Configuration comes from environment variables, loaded with `dotenvy`. File lookup follows `--env-file <path>`, `./.env`, then `<OS config dir>/squad/.env`. Process environment values override the file.

`SQUAD_ROUNDS` seeds `ROUNDS` only when the room has no stored value. `$ROUNDS=n` and `/set ROUNDS n` update the persistent room value, which wins on later starts.

`SQUAD_GRILL_ROUNDS` seeds `GRILL_ROUNDS` under the same persistence rule. `$GRILL_ROUNDS=n` and `/set GRILL_ROUNDS n` update it independently of `ROUNDS`.

`SQUAD_CENSUS_PASSES` optionally seeds `CENSUS_PASSES` under the same persistence rule. Verify default 2, bounds 1 to 5, `$CENSUS_PASSES=n`, `/set CENSUS_PASSES n`, bare-variable reads, and independence from `ROUNDS` and `GRILL_ROUNDS`. The optional environment variable is not included in `.env.example`.

Roster variables use `SQUAD_MODEL_<NICK>`. Verify file order, lowercase nick validation, duplicate rejection, reserved names, supported roster bounds, and the six defaults when no roster variable exists. Personas and the user name use `SQUAD_PERSONA_<NICK>` and `SQUAD_USER_NAME`. Roster changes take effect on restart.

Concrete slugs are validated against the startup catalog. A `~...-latest` alias is probed at startup and its concrete response `model` is fixed until restart. The probe sends `max_tokens: 16` with the lowest reasoning setting the model accepts; on a 400 it retries once with `max_tokens: 2048`. Every completion uses the resolved concrete slug. Verify stored concrete identities, the version shown next to each nick, per-attempt response `model` recording, mismatch warnings, and mismatch reporting by `squad doctor`. Do not infer a model from a naming pattern.

Timeouts, attempt counts, queue depth, output token ceilings, reasoning budgets, message size limits, preflight expiry, work leases, and channel capacities are compile-time constants in `squad-protocol`.

Verify the optional environment variables owned by **Foundations**: `SQUAD_BANNERS` accepts `on` by default, `compact`, or `off`; `SQUAD_FX` accepts `full` by default, `lite`, or `off`. Automatic lite mode applies only when `SQUAD_FX` is unset. Also verify `SQUAD_REASONING_EFFORT_<NICK>`, `SQUAD_PLAN_WORDS`, and `SQUAD_FX_DEBUG`, where `1` shows the effects-grade tag.

### Non-functional requirements

Performance targets apply on a four-core development machine with 8 GB RAM, the persistent room `main`, local SQLite storage, six default members streaming, and one TUI. CI uses the fake OpenRouter fixture supplied by M2 to separate application overhead from provider latency.

| Measure | v1 acceptance target |
|---|---|
| Fan-out | p95 ≤100 ms from router acceptance to delivery of all six mailbox wake-ups |
| Application streaming overhead | p95 ≤100 ms from provider chunk receipt to TUI consumption |
| First visible token | Live smoke-test objective: p95 ≤5 seconds from acceptance; report provider wait separately |
| Deterministic first token | p95 ≤500 ms when the fake provider emits its first chunk after 100 ms |
| Replay | First page of 100 final messages delivered within 500 ms |
| Streaming memory | Whole-process RSS ≤512 MiB during a 30-minute six-agent streaming test |
| Transcript memory | Whole-process RSS ≤256 MiB with 500 displayed messages and no active generation |

Run latency checks across at least 100 requests after warm-up. Record provider latency separately. A live-objective miss remains visible even when application overhead passes. Run deterministic CI without live credentials.

The performance fixture uses the default roster. Functional and isolation checks also cover roster bounds, configurable nicks, and duplicate model slugs with different personas.

Add a 12-stream performance gate with a 12-member roster and `SQUAD_FX=full`, using the same machine and fake provider. Apply the application streaming overhead and deterministic first-token targets, and the streaming memory ceiling over 30 minutes. Verify that the frame-budget governor sheds effects under load according to **TUI effects and motion**. Verify separately that `SQUAD_FX=off` and `NO_COLOR` each produce no animation frames when idle.

The M4 spike uses a throwaway renderer without effects. The full-effects gate runs after M4.P5 as an M4 exit criterion and again as the integrated M6 gate.

Enforce canonical limits through their owning sections. TUI validation provides immediate feedback; router validation remains authoritative.

- Queueing and context selection follow **Agents and OpenRouter**. This section adds no fixed input-token cap.
- Contest admission, busy participants, context-fit checks, and held ordinary work follow **Contests: debate, tournament, grill**.
- Bounded ingress, mailbox wake-ups, feed delivery, and durable paging follow **Protocol, store and bus** and **Room API and CLI**.
- The TUI loads recent events first and pages older events on scroll through `RoomHandle.page_events`.
- At the storage ceiling, reject new work with `STORAGE_LIMIT`, preserve history, and provide markdown export and `/clear` guidance.
- There is no automatic retention deletion in v1. `/clear` is the only room-deletion operation. It deletes the room’s rows, including its scoreboard, in one transaction and then runs `VACUUM`, following **Protocol, store and bus**. It does not reset the machine-wide daily spending total.

TUI readability, resizing, transcript navigation, and keyboard behavior follow **TUI (Ratatui)**.

### Test strategy and isolation guarantees

Use Rust stable, edition 2024, with `cargo nextest` for unit and integration tests, `insta` snapshots, Ratatui `TestBackend`, `proptest`, temporary-directory SQLite databases, and three PTY-driven tests across M3, M4, and M6. Run compile-fail documentation tests separately with `cargo test --doc`. CI also runs `cargo clippy -- -D warnings` and `cargo fmt --check`.

Reuse the M2 `wiremock` fixture in `crates/squad-agents/tests/support/fake_openrouter.rs`. It controls catalogs, streaming chunks, delays, usage, rate limits, malformed events, interrupted responses, and failures. Capture provider request bodies for assertions. M6 consumes this fixture rather than implementing another provider simulator.

Parser and submission tests use the grammar and exact strings from **Command grammar and parser**. Cover recipient requirements, configurable nicks, exclusions after `@all`, protected text, embedded variable assignments, supported slash commands, reserved commands, quotes, debate solution selection, and all legal judge summons. Add tournament and grill forms, `GRILL_ROUNDS`, question-first disambiguation, and pending-pick messages. Verify UTF-8 byte offsets and plain Rust `Result` contracts.

Add census, crosscheck, survey, and probe forms and `CENSUS_PASSES` using parser-owned rules and exact strings. Cover quoted and pasted plans, selected grill plan versions, the latest-reply census shortcut, crosscheck goal and constraints, survey answer-set reuse, probe brief validation, exclusions, participant bounds, and odd-count sit-outs. These tests consume the parser contract without defining alternative command forms.

Router tests cover producer authorization, explicit recipient resolution, persistence before notification, idempotency conflicts, work fencing, integer `Cursor` ordering, and exact private-history destinations. Exercise malformed worker and orchestrator submissions as well as human submissions. A direct debate command must not create ordinary reply work; a question-first debate deliberately creates ordinary `reply` work during answer collection.

Isolation is the centrepiece of the release gate. Exercise actual router commits, agent mailboxes, typed readers, recovery, and captured provider requests:

1. Generate distinct canaries for private human messages and model replies. Exercise every recipient subset for the default roster and generated subsets for other supported rosters.
2. Compare mailbox wake-ups, durable work, private histories, and serialized provider requests against independently maintained recipient expectations.
3. Assert that unauthorized canaries remain absent after retry, restart, context trimming, regeneration, and durable replay.
4. Verify that ordinary replies remain private to their author and the human. Human-triggered quotes deliver only the selected text, labelled with its original author, to the addressed recipients.
5. Verify debate setup, opening positions, turns, forfeits, and verdict visibility using **Contests: debate, tournament, grill**. Uninvolved members receive no debate content.
6. Complete parallel turns in both arrival orders. Verify that the round barrier preserves same-round isolation and commits ordered events and history entries without duplicate identifiers.
7. Verify that each agent receives exactly the capabilities in `AgentContext { mailbox, history: HistoryReader, work: WorkReader, submit: IngressSender, cancel }`. Its readers remain bound to one context and expose no operation for choosing another context, reading the room event log, or subscribing to the feed.
8. Use compile-fail tests in `squad-agents` against the public API to reject attempts to select another context through either reader, access their backing database, or obtain the room feed through `AgentContext`. Verify that `squad-agents` has no dependency on `squad-store` or `rusqlite`. Runtime canary tests verify the construction-time binding.
9. Apply the same restrictions to the judge executor, bound to `judge:{caseId}`. Seed member `fable` history and unrelated judge cases with canaries. New cases receive only prescribed material; follow-ups reopen only the selected case and its prior follow-ups.
10. Inject fabricated recipients, control instructions, debate identifiers, and role labels into model output. These must not change routing, variables, spending authorization, or history visibility.
11. Verify that notices, forfeits, verdicts, quoted text, and opponent content receive no system authority, following **Agents and OpenRouter** and **Prompt pack**.
12. Verify that the orchestrator submits work and coordinates barriers without assembling prompt text.
13. Verify that each `debate.round_score` is addressed only to `me` and stored in the event log and its judge case context. Neither debater context nor any captured debater request may contain a round score before the verdict. At the verdict, both debaters receive the per-round scores and `roundTally` through the verdict announcement; uninvolved members receive none.
14. Verify that ordinary messages accepted for held participants do not enter their private histories until the hold is released. Debate-turn attempts reuse the round’s frozen boundary, including when a held message quotes the opponent’s current-round turn.
15. Verify tournament isolation: an entrant receives only its own answers and per-round results until the final placing list. Other entrants’ answers, ranks, scores, and critiques remain absent from its history and provider requests. Non-entrants receive nothing.
16. Verify blind tournament judging: each round uses shuffled anonymous labels; the label-to-nick map remains outside the judge context. No entrant nick, model identity, or member history is supplied as judge identity metadata.
17. Verify the grill shared floor: the current plan, committed questions, answers, and plan versions reach exactly the leader and grillers. Non-participants receive no grill content. Only the router writes these histories.
18. Verify census hunt isolation: each member receives the selected plan and its own assigned lens, never another member’s hunt. Rating requests contain the plan and anonymised ledger without finder nicks, model names, counts, or the member-by-gap table. The human retains access to every hunt and the attribution table; human report visibility must not broaden member delivery.
19. Verify crosscheck inventory isolation: each writer receives only goal, constraints, and its own lens. The plan and other inventories must be absent even when the same plan already exists in that member’s ordinary history. Cell reviewers receive the plan and only their assigned cells, with no cell built from an inventory item they supplied. Both auditors use fresh contexts containing only the prescribed plan, grid, and sampled evidence, with no reviewer identity, ordinary history, or earlier review work.
20. Verify survey isolation: initial answers are mutually invisible; claim extraction delivers neutral, unattributed claims without whole peer answers or provenance. Initial votes are blind. Revote requests contain only the selected claims and anonymous reasons from both sides, without authorship, vote counts, confidence aggregates, or other members’ votes. Human-facing switch identities and report counts remain outside member payloads unless the script explicitly permits delivery.
21. Verify probe isolation: explainers and checkers independently receive the brief and their own role. Checkers receive no explanations. Each explainer’s prediction request contains its own explanation and all checks, without other explanations or predictions. Predictions remain committed before the judge receives the matrix. The sitting-out member and non-participants receive no working artifacts.
22. Exercise every shared move with independent per-actor payload expectations. `collect` must not become a shared transcript; `fanout` must not broadcast the union of payloads; `turns` must not imply shared-floor visibility; `barrier` must deliver only to script-declared destinations. `pick`, `tally`, and application-assembled reports create no provider calls.
23. Repeat the four modes’ canary checks across retries, schema repair, all relevant completion orders, pass changes, cancellation, restart, replay, and subsequent ordinary replies. Seed earlier room work and unrelated contests with forbidden material. Blindness must follow context-bound readers and router delivery, including wrapper metadata and repair payloads; a prompt asking the model to ignore leaked material does not pass.

Supplement fixed examples with generated event sequences. These checks establish behavior on tested paths; they do not prove universal correctness.

The canonical integration fixture submits the exact question-first input and pick from **Product overview**, without changing their text. It verifies independent initial answers, `COLLECTING_ANSWERS`, `AWAITING_PICK`, correct solution selection, automatic judge selection, parallel rounds using the room default of 10, closing statements, and a valid verdict delivered to authorized contexts. Automatic confirmation preserves the two-input flow when the estimate does not exceed the threshold. Above the threshold, the fixture verifies the required confirmation. The direct debate form remains covered as a shortcut.

Additional integration scenarios cover:

- Fresh-topic debates and round 0 when either participant lacks a prior solution.
- `WAITING_FOR_DEBATERS`, existing ordinary work draining, later debater messages remaining queued, and unrelated members continuing.
- Debates ending in `COMPLETE_UNJUDGED`, later `/judge`, standalone rulings, and case follow-ups.
- Mandatory winner selection, core-computed totals, rubric coverage, judge substitution, and malformed-output handling.
- Global and targeted stopping, ordinary reply retry actions, regeneration, markdown export, and scoreboard updates committed once.
- Persistent variables, environment-driven participants, personas, user name, concrete slugs, aliases, and supported roster sizes.
- Rejection of a second process on the same room, with read-only export still available.
- Startup reconstruction of durable state and wake-ups for unfinished work.
- One `judge.round` work item after each scored round’s barrier commit, with both turns already in the judge case context; round 0 is not scored.
- Late round scores without blocking the next round, correct round attribution, and final judge work waiting until every round-scoring item has completed or been given up.
- Round-score validation, equal round scores, two-attempt exhaustion including schema repair, committed unscored records with `scores: null` and `reason: "unscored"`, and final prompts identifying unscored rounds.
- Core-computed running tallies and final `roundTally`, including late results, unscored rounds, replay, and restart without double counting.
- Validation and repair of a missing `tallyOverrideReason` when the final winner is not the round-tally leader.
- `/stop` cancelling outstanding `judge.round` work, later `/judge` performing no retroactive scoring, and `COMPLETE_UNJUDGED` debates creating no round-scoring work.
- Judge-only plans from `STOPPED`, `COMPLETE_UNJUDGED`, and `FAILED`, with cancellation or expiry restoring the prior status and cases retained under the owning contest contract.
- Judge conflict checks using resolved concrete models for debates and standalone rulings, skipped unavailable or conflicting fallbacks, and no tournament judge substitution when `fable` competes.
- Attempt-scoped repair data containing rejected output and validation errors, readable only through the case-bound `WorkReader`; one provider call per attempt; case executors spawned and retired by the router, including after recovery.
- JSON Schema validation with `additionalProperties: false` and required nullable fields before deserialization; judge evidence rendered once as the system prompt plus one user message.
- One active contest per room across all seven modes, answer collection, and pending picks, with `A {kind} is already running. Stop it before starting another.` on conflicting admission.
- Tournament minimum entrants, exclusions, parallel entries, strict ranking permutations, scores from 1 to 10, exactly one elimination per round, forfeits ranked last, k−1 rounds, one round replay after exhausted judging, final placements, and one trophy increment.
- Grill plan selection, explicit or default grillers, sequential roster order rotated by one each round, assigned angles, up to three questions, `PASS`, all-pass early completion, and rebuild skipping when no questions were asked at all.
- Grill context compaction to the latest plan and current round’s questions and answers plus the earlier-question list, prevention of repeated questions, all retained plan versions, final plan and complete question-and-answer appendix, skipped failed grillers, and leader failure preserving the last good plan.
- Question-first debate and grill selection from eligible answers only, normal room messages while awaiting a pick, invalid picks preserving the pending state, insufficient answers, cancellation, and restart restoration without pick expiry.
- `OpenMode::OneShot` never re-waking work it did not create, `squad models` and `squad doctor` not opening the room, and `--no-wait` committing and printing the accepted cursor before deferring execution to the next interactive open.
- Every new mode entering `PREFLIGHT`, `WAITING_FOR_PARTICIPANTS`, its script’s step states, and the appropriate `COMPLETE`, `STOPPED`, or `FAILED` state; one setup, ordered state transitions, committed-artifact events, and the report following the Protocol contract.
- Member-step retry exhaustion skipping that member and naming it in the report, without fabricating missing gaps, citations, votes, checks, or predictions; judge failures following the script’s bounded repair and terminal rules.
- Shared-engine regression coverage preserving debate parallel rounds and private scores, tournament blind elimination and own-result delivery, grill sequential shared-floor turns, existing picks, and existing accounting.

Census fixtures cover the six ordered lenses, repeated lenses above six members, and rotating assignments below six across passes. Verify at most eight hunt gaps per member, duplicate merging with distinct-member finder counts, anonymous ratings, required plan citations for `covered`, majority-based removal of covered or trivial gaps, retained citations, and merged additions. Compute `f1` and `f2` from step-1 finder data in application code: `f1² / (2·f2)` when `f2 > 0`, and `f1(f1−1)/2` when `f2 = 0`. Rating additions must not acquire invented independent hunt finders. Verify the range against the owning tally contract, the exact headline `Found {n}; estimated {lo} to {hi} still unfound.`, the shared-blind-spots caveat, singleton highlighting, ranked ledger, member-by-gap table for the human, and discovery curve. Bind every pass to its plan version, preserve human patches between passes, and verify `Census is quiet. Good place to stop.` only when a pass adds no new confirmed gap and its estimate is below 2. The singleton-to-grill hint does not start a grill or grant visibility automatically.

Crosscheck fixtures independently enumerate every single item and every cross-lens pair. The six-member fixture has 18 items and 153 cells: 18 singles plus 135 pairs. Verify the 200-cell cap and judge-priority trimming before assignment, item provenance through normalisation, balanced assignment without self-review, and one recorded disposition for every retained cell. A closure needs a plan citation, an exclusion needs a reason, and a gap needs a proposed fix; “probably covered” remains a gap. Use controlled randomness to verify auditor selection and five-closure sampling, including the owning rule when fewer than five closures exist. Audit rulings retain cells, reopen false closures, and account for proposed missing cases. Residual risk acceptance requires an explicit human action; no model can accept it. The stop indication requires every cell to be closed or explicitly accepted and an audit finding no false closure or material missing case. State that this is a bounded stop, without claiming completeness.

Survey fixtures cover question-first collection and reuse of the most recent parallel answer set for one question, without mixing questions or collecting existing answers again. Verify up to 25 deduplicated neutral atomic claims, private provenance, AGREE / DISAGREE / UNSURE votes, one-line reasons, confidence 1 to 5, application-computed split selection, and revotes on up to five claims. Counts, split ordering, switches, and five-band classification must match stored votes and the owning tally rules, including non-six-member rosters, skipped members, and uncertainty. Verify consensus, contested claims with the best reason from each side and switch identities, lone insights, lone dissents, rejected outliers, and the question that would settle each contested claim. Neither the report nor the judge turns agreement into a truth ruling or selects a winner.

Probe fixtures cover at least four active members, odd-count last-member sit-out, and a persisted random split into equal explainer and checker groups shown to the human. Verify one explanation and key assumption per explainer, one cheap non-destructive check per checker with exact measurements and outcomes, and one prediction or explicit inability to predict for each explainer/check pair. The judge receives the locked matrix, removes checks that do not distinguish explanations, orders useful checks by effort and usefulness, and identifies explanations the available checks cannot separate. The report contains a runnable decision tree, the matrix, and an unexpected-result branch without declaring an explanation true. Checks remain proposed text; Squad does not execute them.

Use `TestBackend` and `insta` for deterministic TUI states. The M6 PTY test covers the canonical flow, streaming, commands, quote selection, recipient recall, stopping, resizing, search, regeneration, export, terminal closure, and terminal restoration. CLI subprocess checks cover text and JSON output, exit codes, pre-confirmation, and supported input behavior.

Add scorecard checks for pending, scored, late, and unscored rounds, expandable reasons, correct tallies, and an unchanged participants win-loss panel until the verdict. Add `insta` snapshots of `OPENING`, `ROUND n`, `FINAL ROUND`, and `VERDICT` banners at 120, 80, and 40 columns, including `NO_COLOR`. Verify default-font, compact-font, and one-line fallbacks, centring, accent colour, banner height in the virtualised row cache, `SQUAD_BANNERS` modes, and replay derived from `debate.state` without banner events. Verify markdown export uses fenced code blocks with the default font. Verify fonts and licences are embedded in `squad-core` and rendering invokes no external command. Effects tests follow **TUI effects and motion** and include governor shedding and idle-frame checks.

Add tournament snapshots and effects checks for `TOURNAMENT`, `ROUND r · N REMAIN`, `FINAL`, and `CHAMPION`, bottom-up ranking reveal, eliminated participant rows, and champion celebration. Add grill checks for `GRILL`, `GRILL ROUND r`, and `FINAL PLAN`, turn headings, question and pass counters, collapsible plan versions, turn handoff, plan version bumps, and survived completion.

Add census, crosscheck, survey, and probe snapshots for their banners, step dividers, pending and skipped-member states, and inline collapsible reports at 120, 80, and 40 columns. Verify the census ledger, estimate range and discovery curve; crosscheck coverage summary and residual risks; survey five-band map; and probe box-character decision tree, prediction matrix and unexpected-result branch. Verify banner entrances, census estimate count-down, crosscheck cells filling, survey bars splitting, and probe branches drawing against **TUI effects and motion**. Test full, lite, off, `NO_COLOR`, replay, width fallbacks, keyboard operation, and export without hiding final evidence behind an animation or replaying live effects.

For pending picks, verify `CHOOSE YOUR DEBATE BROS` and `CHOOSE WHO GETS GRILLED`, eligible candidates, the hint `Type two names, e.g. @grok @gemini · Esc to cancel` for debate, the corresponding one-name grill hint, the changed input placeholder, `pick pending` status, eligible-first autocomplete, banner entrance, and candidate-row highlights ending on selection or cancellation. Test the parser-owned strings `Pick exactly two of: @a, @b, @c.`, `Pick exactly one of: @a, @b, @c.`, `@{nick} did not answer, so it cannot be picked.`, and `Not enough answers to start. {n} of {m} members answered.`

The TUI owns the single event loop in `crates/squad-tui/src/event_loop.rs`. Verify that its timer is armed from `FxManager::next_wake()`, capped at 60 Hz, and disarmed when idle; input is handled first and the feed is drained in batches of at most 256 items per iteration. Effects tests use the TUI’s internal live/replay tag and palette contract. Verify inline versus-card settling, colour-only `/clear` wipes, roster effects from resolved state at first draw, the `VERDICT` banner on entering `JUDGING`, and verdict reveal on arrival.

### Observability

Emit JSON logs through `tracing` to a file, never to the terminal while the TUI is active. Include `timestamp`, `level`, `service`, `event`, `room_id`, `trace_id`, `message_id`, `debate_id`, `contest_id`, `nick`, `attempt`, `duration_ms`, and `error_code` where applicable.

Generate a trace ID at acceptance and propagate it through dispatch, provider calls, retries, finals, and judging. Use identities and provider-call attempt semantics from **Protocol, store and bus**.

Record fan-out latency, provider wait, first-token latency, stream duration, queue depth, retries, forfeits, token usage, cost, outstanding reservations, storage failures, recovery, and process memory. Log concrete model identities at startup. Default logs exclude message bodies, credentials, and reasoning text.

`squad --bus-log <file>` and the F12 TUI debug pane inspect requests, commits, and wake-ups. Message bodies are redacted unless `--bus-log-bodies` explicitly enables them. Explain the privacy implications before documenting content capture. Credentials and authorization headers remain excluded in every mode.

Operational metrics remain local. v1 has no telemetry, and no application data leaves the machine except through OpenRouter calls.

### Cost guard

Use the daily spending configuration owned by **Foundations**, including `SQUAD_DAILY_MAX_USD` with default 20 and `SQUAD_DEBATE_CONFIRM_USD` with default 2. Persist actual spending, outstanding reservations, and unknown-call counts across restarts. The TUI always shows today’s spend and daily max, with a lighter pending segment for reservations, from authoritative `RoomHandle` state. The participants panel footer owns the bar; narrow layouts place it in the status line. The bar changes colour at 75% and 90%.

“Today” is the local calendar day. Verify the `spend_daily(day TEXT PRIMARY KEY, spent_micros INTEGER, reserved_micros INTEGER, unknown_calls INTEGER)` contract owned by **Protocol, store and bus**, including machine-wide totals across rooms. Spending is the sum of actual per-call costs reported by OpenRouter, including billed failed and cancelled calls. The router updates accounting in the same transaction as the event completing the call. Pending estimates are replaced by actual cost when it arrives.

Before dispatch, require an atomic reservation covering estimated input, visible output and supported reasoning budgets, growing contest context where relevant, and every permitted provider attempt. Use resolved model prices and label estimates clearly. Block paid dispatch with `COST_UNKNOWN` when pricing is unavailable. There is no configured pricing override in v1.

Contest admission and reservation creation belong to **Contests: debate, tournament, grill**. Every preflight states the number of model calls and the estimate. `debate.preflight` is always emitted and states the debaters, round count, whether judging is requested, and whether an opening round is required, using the canonical payload contract from **Protocol, store and bus**. Estimates at or below the threshold auto-confirm; higher estimates wait for `debate.confirm` or `debate.cancel`. `squad send --yes` pre-confirms through the same contract. Changed or expired plans follow the owning preflight rules, including the 10-minute expiry. M6 verifies these behaviors without defining confirmation types or interactions.

For debates with automatic judging, preflight includes one `judge.round` call per round, excluding round 0, in addition to the final verdict call. Reserve for both permitted round-scoring attempts, accumulated judge context, low reasoning effort with its 1,024-token budget, and a 512-token visible output ceiling per attempt. Final judging retains high reasoning effort with its 8,192-token budget and its existing visible ceiling and attempt policy. Verify estimates at `ROUNDS` default 10, min 1, and max 20. Debates without automatic judging reserve no round-scoring calls, and later `/judge` adds no retroactive round scoring.

Tournament preflight shows k + (k−1) + ... + 2 answer calls plus k−1 judge calls. Grill preflight shows `rounds × (grillers × 2 + 1)` calls. Verify that reservations also cover the retries and tournament round replay permitted by the owning contest contract. Grill estimates include answer limits, reasoning budgets where supported, and the 4,096-token visible ceiling for rebuilt plans; they include no judge calls.

For the four new modes, derive the preflight from the script’s paid moves and compare it with the remaining daily amount. With k participating members, census plans `passes × (2k + 2)` calls; crosscheck plans `2k + 4`, plus one when goal and constraints require extraction; survey plans `3k + 2` when collecting answers or `2k + 2` when reusing them; probe plans `k + k/2 + 1` after removing any sit-out. Verify the default six-member counts of 14 per census pass and 28 for two passes, 16 for crosscheck or 17 with extraction, 20 or 14 for survey, and 10 for probe. Advertised script counts describe planned calls; reservations also cover permitted retries and schema repair. Early completion and skipped work release unused reservations under the owning accounting contract.

The new modes’ judge calls use medium reasoning effort with the supported 4,096-token reasoning budget and a 4,096-token visible ceiling, including final reports. Do not apply debate final-judge high reasoning to them. Price each member’s own payload and each judge’s prescribed evidence, including grid cells, rating ledgers, claim reasons, or prediction matrices. `pick`, `tally`, `barrier`, and application-assembled reports cost no model call; a report using `judge` is counted once. Plan patches and resumed census passes must use the current plan and remaining budget under the owning preflight rules. A hint to start another mode does not authorize its spending.

Question-first answer collection uses ordinary `reply` work and ordinary daily spending checks. Those calls are charged when made and excluded from the later contest preflight estimate. Preflight and confirmation occur after the pick, using the remaining daily amount at that time.

Survey preflight must account for whether its answer collection remains to be performed. Reused answers incur no new answer calls, while answers already collected remain charged once. Recovery must not recollect them merely to recreate the report.

Reconcile reservations against the attempt accounting owned by **Agents and OpenRouter**. That section uses OpenRouter’s `GET /api/v1/generation?id=` lookup when terminal usage is missing. Missing cost remains unavailable rather than zero. Record the call as `cost unknown`, retain uncertain reservations, show a `~` prefix on the spend bar, and report the count through `squad doctor`. Replayed accounting must not increment spending twice.

At the daily max, reject new generation requests, including queued calls that have not been dispatched, with `DAILY_MAX_REACHED`. In-flight calls finish. With the default max, the message is `Daily max of $20.00 reached. Raise SQUAD_DAILY_MAX_USD or wait until tomorrow.` Format the amount from the configured max. Refuse a contest before it starts when its preflight estimate would exceed the remaining daily amount, using the same code. New requests may proceed on the next local day or after the max is raised, subject to reservations and the owning admission contract.

The daily total counts only Squad’s own calls. It does not include other applications using the same key. In-flight calls and uncertain billing mean the local daily max is not a guaranteed provider invoice cap.

### Security

Load `OPENROUTER_API_KEY` through the Foundations configuration contract. Pass it only to code that requires it for provider requests. Exclude it from SQLite records, errors, logs, debug output, artifacts, and exported transcripts. Redact authorization headers before logging. Test with a sentinel credential.

Verify that `./setup.sh` creates `.env` with mode 0600, preserves an existing file, and never echoes the key. Keep `.env` excluded from version control. Ship the exact Foundations `.env.example`, including the six default members and an empty key value.

Create the room database with mode 0600. Verify that SQLite auxiliary files do not expose private content through broader permissions. Existing database permissions must satisfy the same requirement before private work proceeds.

Treat model output as untrusted data. Sanitise it before terminal display or CLI text output so it cannot inject terminal escape sequences, including OSC 52 clipboard writes. Test sequences split across streaming chunks, alternate control encodings, transcript replay, quoted text, errors, debug content, and exports displayed through Squad. Only trusted application code emits terminal controls.

Key handling follows **TUI (Ratatui)**. Model text and pasted text cannot become synthetic key events. Verify that `Esc`, `/stop`, targeted stopping, recipient recall, and documented shortcuts retain their intended behavior during streaming and resizing. Escape closes the topmost transient element first. With nothing open, it requests a global stop; during a contest, that stop requires a confirming second Escape within 2 s. Animation skipping must not consume Escape or input keys. A confirmed global stop while a pick is pending cancels the contest with `STOPPED` and reason `pick_cancelled`, preserving the ordinary answers. Restore terminal state after normal exit, handled failure, and handled termination.

Model output cannot execute commands, change variables, route messages, summon participants, authorize spending, or grant visibility. Router permissions enforce these boundaries; prompts reinforce them. v1 provides plain text chat without model tools, code execution, file access, images, or file uploads.

### Failure handling

| Failure | Required behavior |
|---|---|
| SQLite busy | Follow the store’s contention policy. Do not acknowledge an uncommitted result or emit its notifications. If persistence cannot proceed, surface the store failure and prevent further paid dispatch requiring durable state. Preserve reservations and existing history. |
| SQLite corrupt | Fail room opening or stop further durable work when detected. Surface the store failure, preserve the database for recovery, and never silently replace it with an empty room. |
| Disk full | Reject writes that cannot commit, prevent further paid dispatch requiring persistence, preserve committed history, and retain uncertain reservations. Do not delete history automatically or report success for failed writes. |
| Crash mid-commit | SQLite recovery exposes either the complete transaction or none of it. Verify atomic events, history, work, receipts, contest state, daily accounting, and scores. Notifications occur only after commit. An interactive restart reconstructs state and re-wakes unfinished work, including work whose notification was lost after commit. One-shot opens never re-wake work they did not create. |
| Crash mid-debate | Resume using **Contests: debate, tournament, grill** recovery rules. An interrupted attempt consumes an attempt; uncertain charges remain reserved. Test dispatch, result acceptance, and round-barrier boundaries. |
| OpenRouter 429, 5xx, or timeout | Follow **Agents and OpenRouter** for ordinary work and **Contests: debate, tournament, grill** for contest and judge work. Honor `Retry-After` within the owning deadline. |
| Partial stream fails | Replace obsolete provisional output using the owning attempt rules. Persist only accepted completion or terminal failure records. |
| One model unavailable | Other addressed models continue. Report its availability or execution failure. An admitted contest follows the owning retry and forfeit rules. |
| Terminal closed mid-stream | The process lifecycle follows **Room API and CLI**. Handled termination cancels active requests and shuts down the room. Abrupt termination is recovered on the next start. Preserve committed content and uncertain reservations; missed token deltas are unavailable. |
| Second process opens the same room | The exclusive advisory lock rejects it with `Room main is already open in another squad process (pid N).` It must not dispatch provider calls or mutate the room. Read-only `squad export` remains allowed. |
| User stops generation | Apply global or targeted cancellation through the owning execution contracts. A stopped debate retains its transcript, reaches `STOPPED`, and skips automatic judging. Later `/judge` follows **Contests: debate, tournament, grill**. |
| TUI falls behind the feed | Follow the feed and paging contracts owned by **Protocol, store and bus** and **Room API and CLI**. Durable finals remain available through event paging; ephemeral deltas are not durable history. |
| Storage ceiling reached | Reject new work with `STORAGE_LIMIT`; retain history and offer markdown export and `/clear` guidance. |
| Round scoring is late or fails | Continue subsequent rounds without waiting. After two failed attempts, commit an unscored round under the owning contract. Final judge work waits until all round-scoring work has completed or been given up. `/stop` cancels outstanding round-scoring work; later `/judge` uses existing scores without scoring retroactively. |
| Daily max reached | Reject new generation with `DAILY_MAX_REACHED`; let in-flight calls finish, preserve their accounting, and allow new requests when the next local day or a raised max provides capacity. |
| Tournament judging fails | After three attempts including schema repair, replay the round once under the owning contest contract. If judging still fails, stop with an error and preserve the transcript and billed costs. |
| Grill participant fails | Skip a griller’s turn after retry exhaustion. A leader failure after retry ends the grill as `FAILED`, preserving the last good plan version and committed appendix material. |
| Not enough question-first answers | End as `FAILED` with `Not enough answers to start. {n} of {m} members answered.` Preserve ordinary replies and their daily charges. |
| Restart with a pending pick | Restore `AWAITING_PICK`, eligible answer references, and the room’s active-contest slot without expiry. Do not regenerate completed answers. |
| Census, crosscheck, survey, or probe member step fails | Retry under the script’s attempt policy, then skip the member for that step and name the failure in the report. Preserve accepted artifacts; never count a missing response as a vote, closure, discovery, or prediction. Apply the script’s rule when the remaining evidence cannot support completion. |
| New-mode judge output fails validation | Use the case-bound schema-repair policy and terminal failure rule from the owning script. Rejected output does not become accepted evidence or a report. Preserve committed artifacts and billed costs. |
| Crash during a new-mode step or barrier | Restore the committed script state, plan version, assignments, anonymous mappings, audit samples, votes, and predictions. Preserve attempt counts and uncertain charges. Re-wake only unfinished work under the existing lifecycle rules; do not resample or redeliver committed artifacts. |
| User stops a new mode | Cancel its outstanding work, reject late results, preserve committed artifacts and accounting, reach `STOPPED`, and release holds and the active-contest slot under the owning script. Do not synthesize a successful stop indication or completion report. |

Deduplicate internal delivery and committed effects using **Protocol, store and bus**. Provider calls do not have exactly-once execution.

### Risk register

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Cross-agent disclosure | Medium | Critical | Context-bound capabilities, private histories, independent canary release gate |
| Provider latency or outage | High | Medium | Separate latency metrics, bounded retries, independent member execution |
| Unexpected charges | Medium | High | Reservations for permitted attempts, output ceilings, threshold confirmation, uncertain-charge retention |
| Restart repeats paid work | Medium | High | Durable attempt records, fencing, bounded recovery, retained uncertain reservations |
| Growing history exhausts resources | Medium | High | Context budgeting, paginated replay, storage ceiling, export and `/clear` |
| SQLite failure loses availability | Medium | High | Transactional commits, failure injection, preserved database, verified recovery |
| Model output controls the terminal | Medium | High | Streaming sanitisation, OSC 52 checks, trusted key events, PTY coverage |
| Alias changes model behavior | High | Medium | Startup-fixed concrete identities, concrete configured slugs, recorded model identity |
| Judge favors related models | Medium | Medium | Concrete-model conflict checks and disclosed fallback selection for debates and standalone rulings; blind shuffled labels for tournaments; isolated cases and recorded judge identity |
| Shared model blind spots make census look complete | High | High | Range and floor caveat, preserved finder evidence, explicit quiet condition, no completeness claim |
| Crosscheck inventory inherits the plan or audit inherits the review | Medium | High | Plan-blind inventory contexts, provenance-aware assignment, fresh auditor contexts, captured-request canaries |
| Survey agreement is mistaken for truth | Medium | High | Private votes, visible uncertainty and missing responses, five-band report, settling questions, no truth ruling |
| Probe checks cannot separate explanations | Medium | Medium | Locked predictions, indistinguishable-group disclosure, unexpected-result branch, no selected true explanation |

### Launch plan

Squad ships as one Rust binary with a TUI built on Ratatui 0.30.x and crossterm. Prepare `cargo-dist` binaries for macOS arm64 and Linux x64, a Homebrew formula, and `cargo install` distribution.

The documented quick start is `./setup.sh` from a clone. Foundations owns its implementation. M7 verifies it on the supported platforms, including reruns, preservation of `.env`, missing-key guidance, `squad doctor`, and the printed next command. SQLite is bundled into the binary.

The README covers purpose, privacy boundaries, prerequisites, quick start, binary installation, Homebrew, `cargo install`, API-key setup, environment configuration, participants, personas, aliases and concrete versions, addressing, quotes, commands, the canonical example, debates, tournaments, grills, question-first picks, judge cases, daily spending, export, regeneration, storage, recovery, troubleshooting, development, tests, security limitations, license, and contribution guidance.

Add census, crosscheck, survey, and probe documentation, examples, report layouts, visibility rules, retry and skip behavior, call counts, stop indications, and export scopes. Explain the census estimate caveat, crosscheck’s bounded coverage and explicit human risk acceptance, survey uncertainty and rejected outliers, and probe’s proposed checks and unresolved explanations. Document the recommended `$grill` → `$census` → patch → `$crosscheck` → `$census` until quiet pipeline as human-directed v1 use.

Document all optional environment variables from **Foundations** and verify the documentation against `squad doctor --env`. Explain configuration lookup, process environment precedence, persistent `ROUNDS` and `GRILL_ROUNDS`, startup-only roster changes, and the default 10-round estimate.

Document persistent `CENSUS_PASSES`, its default and bounds, and optional `SQUAD_CENSUS_PASSES` without adding it to `.env.example`.

Verify the shipped commands from a clean installation: `squad`, `squad send`, `squad export`, `squad models`, and `squad doctor`, including `--bus-log`, `--bus-log-bodies`, supported text input, JSON output, exit codes, pre-confirmation, `--no-wait`, and markdown export for room, debate, tournament, and grill scopes. Verify that the same-room lock applies to commands that open the room, while read-only export remains available. `squad models` and `squad doctor` do not open the room. Document `OpenMode::OneShot` behavior and the accepted cursor printed by `--no-wait`.

Verify census, crosscheck, survey, and probe exports through `RoomHandle`, TUI commands, and offline `squad export` using the Foundations-owned scopes. Include the reports and their supporting committed evidence, skipped-member notices, plan versions where relevant, and honest stop caveats. Read-only export remains available while the room is locked.

Use the Apache License 2.0, consistent SPDX metadata, and required dependency notices. Ship `LICENSE` and `NOTICE`; every crate sets `license = "Apache-2.0"`. Embedded fonts and other vendored assets must be Apache-2.0 compatible and credited in `NOTICE`. Start at `0.1.0`; use SemVer and a changelog.

Repository public (D61); release publication requires the owner's M7 decision. M7 prepares binaries, the formula, source distribution metadata, checksums, release notes, and documentation for owner review. Release publication is never automatic.

D48, as refined by D61 and D63, governs implementation after the PRD is final: start M0 and M1 in the existing public `github.com/shoemoney/squad` repository, dispatch most coding units asynchronously to the GPT Sol 6.1 worker on its fast tier with the widest fan-out each milestone allows, use one git worktree per worker and orchestrator merges, send overflow to the other coding workers, perform one review pass per milestone, and include an owner check-in at each milestone boundary.

The README includes a recorded terminal demonstration of the canonical question-first flow:

| Time | Action |
|---|---|
| 0–8 s | Show the default participants and daily spend bar, then enter the exact canonical `@all $debate` question from **Product overview**. |
| 8–22 s | Show independent streaming answers and explain their visibility. |
| 22–30 s | Show `CHOOSE YOUR DEBATE BROS`, enter the exact canonical pick, show the round count, model-call count and estimate, and confirm only when required. |
| 30–48 s | Show parallel rounds, opponent turns, and closing statements. |
| 48–60 s | Show the judge’s scores, winner, explanation, and what to steal from the loser. |

Label edited waiting periods and retain the complete recording. Do not imply that a 10-round debate or provider calls reliably finish within the recording window.

Add separate recorded walkthroughs of the four new modes without changing the canonical demonstration. Show their blind steps, reports, cost preflights, stop behavior, and exports. The census and crosscheck walkthroughs disclose their limits; the survey walkthrough shows all five bands; the probe walkthrough shows an unexpected result and explanations that remain unresolved.

### Post-v1 roadmap

The following remain outside v1 acceptance:

- `$reveal` with explicit visibility grants, `$vote`, `$summarize`, and `$review`.
- Debates with 3+ debaters.
- Multi-room storage and navigation.
- A networked bus backend enabling a second TUI or remote attach through the existing `Bus` trait.
- Multiple human users with identity and authorization.
- Model tools with explicit capability and spending controls.
- Images and file uploads.
- Optional Intel Mac and Linux arm64 distribution targets.
- macOS signing and notarisation after the owner’s publication decision and provision of an Apple Developer ID.
- OpenRouter key-wide usage alongside Squad’s own daily spending.
- `$lattice`.
- `$delphi`.
- `$assemble`.
- A meta-mode that loops `$grill` and `$census` until the census is quiet, subject to explicit spending controls and the census stop rule.

v1 includes human-triggered quotes, standalone judge cases and follow-ups, regeneration, markdown export, the room scoreboard, the daily spend bar, tournaments, grills, census, crosscheck, survey, probe, and question-first contests with a pick step. These are release requirements.

