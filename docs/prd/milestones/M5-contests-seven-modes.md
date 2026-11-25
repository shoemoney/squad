# M5 — Contests (seven modes)

**Goal:** Deliver the shared contest engine and its seven modes as scripts over shared moves, with versioned prompts for isolated chat, bounded contests, and case-scoped judging that use the canonical contracts.

**Exit criteria:**

- Admission, preflight call counts and estimates, remaining-daily-amount checks, participant holds, state transitions, restart recovery, stop behavior, and question-first collection and picks satisfy **Contests**.
- Debate barriers preserve frozen attempt boundaries, round scoring settles before final judging, accepted verdicts update the scoreboard once, and `/judge` follows the canonical judge-only transitions and cancellation behavior.
- Tournament rounds preserve blind judging and entrant isolation, eliminate exactly one entrant per round, and produce the canonical champion and trophy update.
- Grill turns follow the canonical rotation, shared-floor visibility, pass and failure handling, working-context projection, plan rebuilds, and final appendix.
- All prescribed templates are embedded at build time and load and render at startup with no unresolved or unknown placeholders.
- `SQUAD_PROMPTS_DIR` overrides load from local UTF-8 files, and invalid placeholders cause a startup error.
- Captured requests contain the complete configured roster, `user.name`, and the correct member persona or judge context, except that blind tournament judge requests contain no entrant identities.
- Round 0, numbered rounds, and closing compositions use the configured word limits without repeating source material.
- Scored verdicts validate against the contest-owned schema, require a winner, and use the fixed `fit` 0.30, `feasibility` 0.30, `originality` 0.15, and `survivedAttacks` 0.25 rubric.
- Standalone rulings and case follow-ups use their authorized case material and free-form output.
- Every judge attempt contains one system message and one user message, including repairs, with each evidence record represented once.
- Notices, forfeits, and verdict announcements use user role, name `squad`, and the required prefix.
- Validation and repair checks enforce the canonical judge attempt budget and explicit failure behavior.
- Contest prompt settings and invocation provenance are recorded through the canonical persistence contracts.
- Per-round prompts score only the targeted numbered round, allow equal scores, and enforce the canonical schema, low reasoning budget, 512-token visible output ceiling, and two-attempt budget.
- Final verdict prompts include available round scores, the application-computed tally, and unscored rounds; required tally override explanations are validated and repaired.
- Debaters receive per-round scores and `roundTally` for the first time in the verdict announcement.
- Tournament prompts expose only each entrant’s own preceding result, use anonymous current-round labels for judging and repairs, and deliver only nicks and final placements in the final entrant notice.
- Grill prompts enforce at most three questions within 200 words, exact `PASS`, bounded answers, complete plan rebuilds, and the supplied no-repeat question list.
- Question-first collection uses ordinary reply framing; picks make no provider call and selected answers are reused as contest source material.
- All 17 census, crosscheck, survey, and probe templates have complete task text, registered placeholders, canonical output schemas, and validated composition-specific input mappings.
- Model-bearing shared moves select registered compositions; each `fanout` actor receives only its own payload, and no prompt adds a model call to `pick`, `tally`, or `barrier`.
- Census hunts remain independent, ratings receive no finder identities or counts, and finalization preserves citations and separates rating additions from hunt captures. Estimate ranges, discovery curves, and quiet notices are rendered from application-computed data.
- Crosscheck inventories receive no plan, reviewers receive only their assigned cells, and two fresh-context auditors receive no reviewer identities or earlier personal work. Rulings preserve every retained cell and cannot manufacture human acceptance.
- Survey voting and revoting expose no whole answers, authorship, origin counts, or vote counts. The human report preserves all five bands, switch records, missing ballots, and settling questions without declaring a winner.
- Probe initial roles remain mutually blind, explainers commit predictions without seeing rival explanations, and the report preserves unknown predictions, inseparable explanations, and unexpected-result branches without declaring an explanation true.
- Four-mode judge requests and repairs use `judge.system.md`, medium reasoning, strict JSON schemas, and a 4,096-token visible output ceiling. Transport failures and repairs share the canonical attempt budget.
- Reports name skipped members, preserve incomplete evidence, and export accepted artifacts without invented findings, votes, closures, acceptances, or observations.
- Prompt work remains in M5.P3. The Contests section owns the engine phases and the D71 build order.

## Phase M5.P1 — Admission and state machine

- **M5.P1.L1 — Define shared debate contracts**
  - [x] M5.P1.L1.T2 — Add frozen source, model, prompt, limit, and plan records in `crates/squad-core/src/orchestrator/types.rs`.
  - [x] M5.P1.L1.T4 — Verify debate types reuse canonical IDs, `Work`, `ErrorCode`, and envelope fields in `crates/squad-protocol/src/debate.rs`.
  - [x] M5.P1.L1.T6 — Integrate the M1-defined transition, adjudication-bundle, committed-verdict, and optional case-opening work-reference contracts in `crates/squad-core/src/orchestrator/types.rs`.

- **M5.P1.L2 — Implement source selection and admission**
  - [x] M5.P1.L2.T1 — Implement completed-source lookup, supersession exclusion, explicit selection, and shared-topic validation in `crates/squad-core/src/orchestrator/sources.rs`.
  - [x] M5.P1.L2.T2 — Detect fresh topics and one-missing-source opening rounds in `crates/squad-core/src/orchestrator/sources.rs`.
  - [x] M5.P1.L2.T3 — Implement frozen estimates, context bounds, expiry, and unconditional preflight feed emission in `crates/squad-core/src/orchestrator/preflight.rs`.
  - [x] M5.P1.L2.T4 — Implement threshold auto-confirmation, confirmation and cancellation Requests, and pre-confirmed CLI admission in `crates/squad-core/src/orchestrator/preflight.rs`.
  - [x] M5.P1.L2.T5 — Use the `squad-store` transaction API for atomic active-contest claims, daily reservations, and confirmed direct-command commits without ordinary reply work in `crates/squad-core/src/router/debates.rs`.
  - [x] M5.P1.L2.T6 — Implement member holds and `WAITING_FOR_DEBATERS` using the existing queue contract in `crates/squad-core/src/orchestrator/admission.rs`.
  - [x] M5.P1.L2.T7 — Include one round-scoring invocation per numbered round, two attempts per invocation, low reasoning effort, a 1,024-token reasoning budget, a 512-token visible output ceiling, and accumulating judge context in automatic-judge estimates and reservations in `crates/squad-core/src/orchestrator/preflight.rs`.
  - [ ] M5.P1.L2.T8 — Defer held ordinary messages and quotes from participant histories until hold release in `crates/squad-core/src/router/debates.rs`.
  - [ ] M5.P1.L2.T9 — Consume the protocol-owned preflight payload, compare `estimatedUsd` for confirmation, and display planned calls separately from reserved attempts in `crates/squad-core/src/orchestrator/preflight.rs`.
  - [x] M5.P1.L2.T10 — Check and reserve the remaining daily amount atomically through the canonical accounting contract in `crates/squad-core/src/orchestrator/admission.rs`.

- **M5.P1.L3 — Persist and verify transitions**
  - [x] M5.P1.L3.T1 — Implement version-checked `debate.state` submissions and router persistence through the canonical contest schema in `crates/squad-core/src/orchestrator/machine.rs` and `crates/squad-core/src/router/debates.rs`.
  - [ ] M5.P1.L3.T2 — Test duplicate admission, stale confirmation, expiry, and competing debate starts against temporary SQLite databases in `crates/squad-core/tests/orchestrator/admission.rs`.
  - [ ] M5.P1.L3.T3 — Test that earlier ordinary work drains and later work remains held in `crates/squad-core/tests/orchestrator/queues.rs`.
  - [ ] M5.P1.L3.T4 — Test canonical parser error mapping, frozen room settings, and the 10-default, 1-to-20 round bounds in `crates/squad-core/tests/orchestrator/admission.rs`.
  - [x] M5.P1.L3.T5 — Test that automatic-judge preflight includes round-scoring attempts and context growth, while round 0 and debates without automatic judging add no scoring calls, in `crates/squad-core/tests/orchestrator/budget.rs`.
  - [ ] M5.P1.L3.T6 — Test held-message and held-quote exclusion from debate prompts before release in `crates/squad-core/tests/orchestrator/queues.rs`.
  - [ ] M5.P1.L3.T7 — Test daily-cap rejection, concurrent reservation checks, actual-cost replacement, unknown charges, and completion of in-flight calls at the cap in `crates/squad-core/tests/orchestrator/budget.rs`.

- **M5.P1.L4 — Implement the shared contest claim**
  - [x] M5.P1.L4.T1 — Generalise active ownership, admission, holds, terminal release, and state-version checks across contest kinds in `crates/squad-core/src/orchestrator/contests.rs`.
  - [x] M5.P1.L4.T2 — Integrate protocol-owned contest records and frozen plan provenance in `crates/squad-core/src/router/contests.rs`.
  - [x] M5.P1.L4.T3 — Test competing contest kinds and the exact active-contest error in `crates/squad-core/tests/orchestrator/contests.rs`.

- **M5.P1.L5 — Collect question-first answers and accept picks**
  - [x] M5.P1.L5.T1 — Implement `COLLECTING_ANSWERS` using ordinary reply work linked to the originating contest and question in `crates/squad-core/src/orchestrator/selection.rs`.
  - [x] M5.P1.L5.T2 — Freeze eligible answer snapshots, emit `contest.pick_requested`, and persist non-expiring `AWAITING_PICK` state in `crates/squad-core/src/router/contests.rs`.
  - [x] M5.P1.L5.T3 — Handle protocol-owned `ContestPick` requests, eligible-member validation, and idempotent `contest.picked` commits in `crates/squad-core/src/orchestrator/selection.rs`.
  - [ ] M5.P1.L5.T4 — Convert a debate pick into selected solutions with the original question, no round 0, and automatic judging enabled in `crates/squad-core/src/orchestrator/selection.rs`.
  - [ ] M5.P1.L5.T5 — Preserve ordinary room use while collecting and picking, and map insufficient answers and wrong picks to the parser-owned strings in `crates/squad-core/src/orchestrator/selection.rs`.
  - [ ] M5.P1.L5.T6 — Test collection completion, partial failures, source freezing, invalid picks, duplicate picks, automatic judging, and post-pick preflight in `crates/squad-core/tests/orchestrator/selection.rs`.
  - [ ] M5.P1.L5.T7 — Test that collection calls count as ordinary daily spend and are excluded from the execution preflight estimate in `crates/squad-core/tests/orchestrator/budget.rs`.

- **M5.P1.L6 — Execute shared moves and versioned scripts**
  - [ ] M5.P1.L6.T1 — Implement `collect`, `pick`, `fanout`, `turns`, `judge`, `tally`, `barrier` and `report` with persisted step identities, actors, visibility projections, limits and failure policies in `crates/squad-core/src/orchestrator/`.
  - [ ] M5.P1.L6.T2 — Derive normal call counts, reserved attempts, context bounds and remaining-daily checks from bounded script steps and loops in `crates/squad-core/src/orchestrator/preflight.rs`.
  - [ ] M5.P1.L6.T3 — Persist per-member payload references, authorized boundaries, random selections, accepted artifacts and idempotent delivery markers through `crates/squad-core/src/router/contests.rs`.
  - [ ] M5.P1.L6.T4 — Integrate M1 generic carriers and M2 executor capabilities; add each non-debate mode’s typed contracts only in its own phase without defining competing Work, storage or envelope types.
  - [ ] M5.P1.L6.T5 — Verify seven-kind exclusion, skipped-member reporting, stop fencing, barrier recovery and unchanged preflight counts for existing modes in `crates/squad-core/tests/orchestrator/contests.rs`.

## Phase M5.P2 — Round execution and recovery

- **M5.P2.L1 — Dispatch parallel turns**
  - [x] M5.P2.L1.T1 — Create canonical reference-based `Work` records and prompt references in `crates/squad-core/src/orchestrator/rounds.rs`.
  - [x] M5.P2.L1.T2 — Persist both round work records and fixed history boundaries before either mailbox wake-up in `crates/squad-core/src/orchestrator/rounds.rs`.
  - [x] M5.P2.L1.T3 — Integrate `debate.round0.md` and commit opening outcomes as setup solutions in `crates/squad-core/src/orchestrator/rounds.rs`.
  - [x] M5.P2.L1.T4 — Verify opening independence and final-round composition for one-round and multi-round debates in `crates/squad-core/tests/orchestrator/rounds.rs`.
  - [x] M5.P2.L1.T5 — Enqueue one canonical `judge.round` work item after each automatically judged numbered-round barrier without waiting before dispatching later debate turns in `crates/squad-core/src/orchestrator/rounds.rs`.
  - [x] M5.P2.L1.T6 — Reuse each round’s frozen history boundary for both slot attempts in `crates/squad-core/src/orchestrator/rounds.rs`.
  - [ ] M5.P2.L1.T7 — Test that a retry cannot see later ordinary messages, quotes, or an opponent’s current-round result in `crates/squad-core/tests/orchestrator/isolation.rs`.

- **M5.P2.L2 — Commit visible history**
  - [x] M5.P2.L2.T1 — Add atomic barrier history copies with distinct `history.pos` values and original turn `event_seq` references in `crates/squad-core/src/router/debates.rs`.
  - [ ] M5.P2.L2.T2 — Test fixed round boundaries, fresh judge contexts, and uninvolved-history exclusion in `crates/squad-core/tests/orchestrator/isolation.rs`.
  - [ ] M5.P2.L2.T3 — Test reverse completion order, duplicate barriers, and intervening history writes in `crates/squad-core/tests/orchestrator/barrier.rs`.
  - [ ] M5.P2.L2.T4 — Verify user-role notices, preserved finish reasons, and absence of duplicated source text in `crates/squad-core/tests/orchestrator/history.rs`.
  - [x] M5.P2.L2.T5 — Append numbered-round outcomes to the automatic judge case at each barrier and commit round scores to `me` and that case only in `crates/squad-core/src/router/debates.rs`.
  - [ ] M5.P2.L2.T6 — Assert that neither debater context ever contains a round score, tally, or score reason before the verdict announcement, including late scores and retries, in `crates/squad-core/tests/orchestrator/isolation.rs`.
  - [x] M5.P2.L2.T7 — Integrate the Prompt pack’s setup and opening projections without duplicating anchors in `crates/squad-core/src/orchestrator/rounds.rs`.

- **M5.P2.L3 — Handle interruption and recovery**
  - [x] M5.P2.L3.T1 — Implement persisted deadlines, orchestrator-issued second attempts, and durable forfeits in `crates/squad-core/src/orchestrator/rounds.rs`.
  - [x] M5.P2.L3.T2 — Implement debate stopping, participant-stop integration, and late-result fencing in `crates/squad-core/src/orchestrator/stop.rs`.
  - [x] M5.P2.L3.T3 — Recover pending work, member holds, confirmation plans, and incomplete barriers from SQLite in `crates/squad-core/src/orchestrator/recovery.rs`.
  - [x] M5.P2.L3.T4 — Test restart after dispatch, one result, barrier commit, and stop in `crates/squad-core/tests/orchestrator/recovery.rs`.
  - [ ] M5.P2.L3.T5 — Test single and double forfeits, interrupted attempt accounting, and terminal queue release in `crates/squad-core/tests/orchestrator/failures.rs`.
  - [x] M5.P2.L3.T6 — Cancel and fence outstanding round-scoring work on stop in `crates/squad-core/src/orchestrator/stop.rs`, and recover scoring attempts and final-judge readiness without duplicate commits in `crates/squad-core/src/orchestrator/recovery.rs`.
  - [x] M5.P2.L3.T7 — Test stop during round scoring, rejected late scores, interrupted scoring attempts, and restart before final judge enqueue in `crates/squad-core/tests/orchestrator/recovery.rs`.
  - [x] M5.P2.L3.T8 — Respect interactive versus one-shot wake-up rules and test that unrelated contest work remains dormant on one-shot open in `crates/squad-core/tests/orchestrator/recovery.rs`.
  - [ ] M5.P2.L3.T9 — Recover collecting slots and pending picks without expiry, duplicate answer calls, or lost candidates in `crates/squad-core/src/orchestrator/recovery.rs`.
  - [ ] M5.P2.L3.T10 — Implement `pick_cancelled` stop behavior while preserving ordinary answers in `crates/squad-core/src/orchestrator/stop.rs`.
  - [ ] M5.P2.L3.T11 — Test pending-pick restart, cancellation, continued ordinary chat, and active-contest exclusion in `crates/squad-core/tests/orchestrator/selection.rs`.

- **M5.P2.L4 — Run debate as the first engine script**
  - [x] M5.P2.L4.T1 — Express collection, two-member picking, optional opening, parallel numbered rounds, barriers, nonblocking round scoring and completion over the shared moves.
  - [ ] M5.P2.L4.T2 — Verify that script execution preserves existing debate state transitions, prompt limits, forfeit behavior, visibility, judge-only handling and call-count formulas.

## Phase M5.P3 — Prompt pack

- **M5.P3.L1 — Add prompt templates**
  - [x] M5.P3.L1.T1 — Add root `prompts/room.system.md` and `prompts/room.user.md` with roster, `user.name`, persona, word-limit, and quote framing for the `squad-core` prompt pack.
  - [x] M5.P3.L1.T2 — Add root `prompts/debate.round0.md`, `prompts/debate.round1.md`, `prompts/debate.rebuttal.md`, and `prompts/debate.closing.md` for the `squad-core` prompt pack.
  - [x] M5.P3.L1.T3 — Add root `prompts/judge.system.md`, `prompts/judge.user.md`, `prompts/judge.repair.md`, `prompts/judge.standalone.md`, and `prompts/judge.followup.md` for the `squad-core` prompt pack.
  - [x] M5.P3.L1.T4 — Add root `prompts/squad.notice.md`, `prompts/debate.forfeit.md`, and `prompts/debate.verdict.md` for the `squad-core` prompt pack.
  - [x] M5.P3.L1.T5 — Declare template IDs, required placeholders, composition rules, and pack version in root `prompts/manifest.json` for the `squad-core` prompt pack.
  - [x] M5.P3.L1.T6 — Add root `prompts/judge.round.md` with target-round scoring, equal-score support, bounded reasons, and strict JSON for the `squad-core` prompt pack; register its placeholders and composition in root `prompts/manifest.json`.
  - [x] M5.P3.L1.T7 — Add round-scoring mode to root `prompts/judge.system.md`, round-score evidence and `tallyOverrideReason` guidance to root `prompts/judge.user.md` and `prompts/judge.repair.md`, and per-round records to root `prompts/debate.verdict.md`; update the `squad-core` prompt pack version in root `prompts/manifest.json`.
  - [x] M5.P3.L1.T8 — Add root `prompts/debate.setup.md` and `prompts/debate.opening.md` for the `squad-core` prompt pack, with JSON source framing, user-role delivery, and registered placeholder requirements in `prompts/manifest.json`.
  - [x] M5.P3.L1.T9 — Update root `prompts/room.system.md` and `prompts/manifest.json` for the `squad-core` prompt pack with tournament privacy, the grill shared-floor exception, and the single-system-message tournament suffix composition.
  - [x] M5.P3.L1.T10 — Extend root `prompts/room.system.md` and `prompts/judge.system.md` for census, crosscheck, survey, and probe step boundaries and structured task modes without changing existing debate, tournament, or grill behavior.
  - [x] M5.P3.L1.T11 — Register all four modes' template IDs, composition-specific placeholders, strict schema sources, shared repair suffix, and the two `crosscheck.normalize.md` compositions in root `prompts/manifest.json`; increment the pack version.

- **M5.P3.L2 — Render configured prompts**
  - [x] M5.P3.L2.T3 — Map canonical environment-derived configuration, work identity, and authorized history to the placeholder table in `squad-core` at `crates/squad-core/src/prompts/inputs.rs`.
  - [x] M5.P3.L2.T4 — Apply `turnWords` and `finalTurnWords` consistently across complete prompt compositions in `squad-core` at `crates/squad-core/src/prompts/render.rs`.
  - [ ] M5.P3.L2.T5 — Record frozen contest prompt settings and invocation hashes through canonical plan and case provenance records in `squad-core` at `crates/squad-core/src/orchestrator/provenance.rs`.
  - [x] M5.P3.L2.T6 — Map `judge.round` work identity and case-bound target-round material to the canonical schema and placeholders in `squad-core` at `crates/squad-core/src/prompts/inputs.rs`; apply low reasoning with a 1,024-token budget for supported models and the 512-token visible output ceiling in `squad-agents` at `crates/squad-agents/src/judge.rs`.
  - [x] M5.P3.L2.T7 — Include available round-score records, the application-computed tally, and unscored rounds in final judge inputs, and expose per-round records to debaters only through verdict announcements in `squad-core` at `crates/squad-core/src/prompts/inputs.rs` and `crates/squad-core/src/prompts/render.rs`.
  - [ ] M5.P3.L2.T8 — Map opening and setup records to their templates and suppress duplicate source projections in `squad-core` at `crates/squad-core/src/prompts/inputs.rs`; apply the canonical provider-role rows in `squad-agents` at `crates/squad-agents/src/context.rs`.
  - [x] M5.P3.L2.T9 — Assemble each judge attempt as one system message and one user message, embedding authorized evidence once and repair content inside the same user message, in `squad-core` at `crates/squad-core/src/prompts/render.rs` and `squad-agents` at `crates/squad-agents/src/context.rs`.
  - [ ] M5.P3.L2.T10 — Map each model-bearing shared move to its registered composition in `squad-core` at `crates/squad-core/src/prompts/inputs.rs`; require actor-specific `fanout` inputs and reject fields forbidden by the step projection.
  - [ ] M5.P3.L2.T11 — Map `census_pass`, `step_word_limit`, `mode_schema_json`, `mode_data_json`, and `mode_step` from authorized work and frozen script settings in `squad-core` at `crates/squad-core/src/prompts/inputs.rs`.
  - [ ] M5.P3.L2.T12 — Record new-mode step, pass, plan version, schema identity, input projection, and composition hashes through canonical provenance in `squad-core` at `crates/squad-core/src/orchestrator/provenance.rs`; preserve those inputs on restart recovery.

- **M5.P3.L3 — Integrate verdict validation and repair**
  - [x] M5.P3.L3.T2 — Implement repair requests in `squad-agents` at `crates/squad-agents/src/judge.rs` using attempt-scoped data from the case-bound `WorkReader` within the canonical judge attempt budget, ending exhausted invalid output with `JUDGE_INVALID_VERDICT`.
  - [x] M5.P3.L3.T3 — Render announcements only from accepted verdicts and apply user role, name `squad`, and `[squad notice]` in `squad-core` at `crates/squad-core/src/prompts/render.rs` and `squad-agents` at `crates/squad-agents/src/context.rs`.
  - [x] M5.P3.L3.T6 — Integrate task-specific round-score repair using the canonical M1 validator and attempt-scoped repair record in `squad-agents` at `crates/squad-agents/src/judge.rs`, sharing two total attempts and handing exhaustion to the canonical unscored-round path.
  - [x] M5.P3.L3.T7 — Route missing required tally override explanations from the canonical M1 verdict validator through final-verdict repair in `squad-agents` at `crates/squad-agents/src/judge.rs`; keep application-computed `roundTally` outside model output.
  - [ ] M5.P3.L3.T8 — Integrate census, crosscheck, survey, and probe canonical schema and semantic validation with the shared task-specific repair suffix in `squad-agents` at `crates/squad-agents/src/judge.rs`; preserve authorized IDs, visibility, two-message judge composition, and the script's shared attempt budget.
  - [ ] M5.P3.L3.T9 — Preserve medium reasoning, the 4,096-token visible output ceiling, and explicit missing-work records throughout new-mode judge requests and repairs in `squad-agents` at `crates/squad-agents/src/judge.rs`; route exhaustion to canonical mode failure handling without fabricating artifacts.

- **M5.P3.L4 — Verify prompt behavior**
  - [x] M5.P3.L4.T1 — Test complete roster rendering, `SQUAD_USER_NAME`, empty and nonempty `SQUAD_PERSONA_<NICK>` values, and judge persona exclusion in `squad-core` at `crates/squad-core/src/prompts/tests.rs`.
  - [x] M5.P3.L4.T2 — Test round 0 composition for both debaters and the absence of opponent opening content in captured requests in `squad-core` at `crates/squad-core/tests/prompt_requests.rs`.
  - [x] M5.P3.L4.T3 — Test final-round composition for one-round and multi-round debates, including effective word limits, in `squad-core` at `crates/squad-core/src/prompts/tests.rs`.
  - [x] M5.P3.L4.T4 — Verify setup and barrier source text appears once and `round_data_json` contains only the prescribed metadata in `squad-core` at `crates/squad-core/tests/prompt_requests.rs`.
  - [x] M5.P3.L4.T5 — Validate scored-output fixtures with actual configured nicks, all four rubric criteria, equal totals, and no model-generated totals in `squad-protocol` at `crates/squad-protocol/tests/verdict.rs`.
  - [x] M5.P3.L4.T6 — Test malformed output, repair data framing, shared attempt exhaustion, and rejection of tie or null winners in `squad-core` at `crates/squad-core/tests/judge_repair.rs`.
  - [ ] M5.P3.L4.T7 — Capture standalone and follow-up requests to verify quote attribution, free-form output, case isolation, and fresh context for new cases in `squad-core` at `crates/squad-core/tests/judge_cases.rs`.
  - [ ] M5.P3.L4.T8 — Add fixtures for embedded instructions, fabricated evidence, premature surrender, honest concessions, forfeits, and concrete-model judge conflicts in `squad-core` at `crates/squad-core/tests/prompt_requests.rs`.
  - [ ] M5.P3.L4.T9 — Verify notices never acquire system role, direct debates without `@judge` produce no automatic judge prompt, and question-first debates request judging by default in `squad-core` at `crates/squad-core/tests/prompt_requests.rs`.
  - [ ] M5.P3.L4.T10 — Document the `squad-core` prompt pack, including `crates/squad-core/src/prompts/load.rs`, override examples, placeholder mappings, environment-derived limits, case modes, and version rules in `docs/prompt-pack.md`.
  - [x] M5.P3.L4.T11 — Test embedded defaults, partial overrides, unknown and missing placeholders, and single-pass insertion in `squad-core` at `crates/squad-core/src/prompts/tests.rs`.
  - [ ] M5.P3.L4.T12 — Capture `judge.round` requests to verify target-round identity, case isolation, judge system prompt reuse, persona exclusion, low reasoning with a 1,024-token budget for supported models, and the 512-token visible output ceiling in `squad-core` at `crates/squad-core/tests/prompt_requests.rs`.
  - [ ] M5.P3.L4.T13 — Test round-score fixtures with equal scores, invalid nick coverage, noninteger or out-of-range scores, overlong reasons, extra keys, and task-specific repair within two total attempts in `squad-protocol` at `crates/squad-protocol/tests/verdict.rs` and `squad-core` at `crates/squad-core/tests/judge_repair.rs`.
  - [ ] M5.P3.L4.T14 — Assert neither debater context nor captured debate-turn request contains round scores, reasons, tallies, or unscored notices before the verdict; verify the verdict announcement delivers the accepted verdict, `roundTally`, and all available round-score records to both debaters in `squad-core` at `crates/squad-core/tests/prompt_requests.rs`.
  - [ ] M5.P3.L4.T15 — Test late and failed round-score inputs, explicit unscored rounds, final prompt assembly after scoring settles, and stopped-debate final judging without retroactive round prompts in `squad-core` at `crates/squad-core/tests/judge_cases.rs`.
  - [x] M5.P3.L4.T16 — Test application-computed tally arithmetic, rejection of model-produced `roundTally`, tied round tallies, and required override explanations when the winner differs from the sole tally leader in `squad-protocol` at `crates/squad-protocol/tests/verdict.rs`; verify missing explanations trigger repair in `squad-core` at `crates/squad-core/tests/judge_repair.rs`.
  - [ ] M5.P3.L4.T17 — Document `judge.round`, its schema and repair budget, round-score visibility, final tally overrides, and verdict-announcement delivery for the `squad-core` prompt pack in `docs/prompt-pack.md`.
  - [ ] M5.P3.L4.T18 — Capture opening, setup, standalone, follow-up, round-score, final-verdict, and repair requests to verify provider roles, exactly two messages per judge attempt, and evidence appearing once in `squad-core` at `crates/squad-core/tests/prompt_requests.rs`.
  - [ ] M5.P3.L4.T19 — Verify held human messages and quotes are absent from captured contest turns and both debate-turn attempts reuse the frozen round boundary in `squad-core` at `crates/squad-core/tests/prompt_requests.rs`.
  - [ ] M5.P3.L4.T20 — Test repair recovery from the authorized attempt-scoped record without adding rejected output to history or exposing another case’s record in `squad-core` at `crates/squad-core/tests/judge_repair.rs`.
  - [ ] M5.P3.L4.T21 — Test required nullable fields, extra-key rejection, and schema validation before deserialization using canonical validators in `squad-protocol` at `crates/squad-protocol/tests/verdict.rs`.
  - [ ] M5.P3.L4.T22 — Verify question-first collection uses ordinary request framing, cancellation preserves those replies, picks cause no provider call, and selected answers appear once in later contest inputs in `squad-core` at `crates/squad-core/tests/prompt_requests.rs`.
  - [ ] M5.P3.L4.T23 — Verify visible ceilings and low, medium, and high reasoning budgets reach the request adapter correctly, unsupported models omit reasoning parameters, and captured requests use concrete model slugs in `squad-core` at `crates/squad-core/tests/prompt_requests.rs`.
  - [ ] M5.P3.L4.T24 — Document the six-crate ownership, M1 validators, M2 renderer dependency, source-role projection, single-message judge evidence, attempt-scoped repairs, daily-max references, and contest prompt provenance for `squad-core` in `docs/prompt-pack.md`.
  - [ ] M5.P3.L4.T25 — Capture all new-mode judge and repair compositions to verify system-prompt reuse, persona exclusion, exactly two provider messages, evidence appearing once, medium reasoning, canonical schema insertion, and the 4,096-token visible output ceiling in `squad-core` at `crates/squad-core/tests/prompt_requests.rs`.
  - [ ] M5.P3.L4.T26 — Test all 17 new template IDs, both crosscheck normalization compositions, missing and unknown placeholders, forbidden input fields, single-pass insertion, overrides, and frozen recovery provenance in `squad-core` at `crates/squad-core/src/prompts/tests.rs`.
  - [ ] M5.P3.L4.T27 — Verify model-call traces match script selection without hidden summary calls: six-member census uses 14 calls per pass, crosscheck uses 16 plus optional goal extraction, survey uses 20 with collection or 14 with existing answers, and probe uses 10; exclude retry calls from nominal counts and account for them under canonical preflight rules in `squad-core` at `crates/squad-core/tests/prompt_requests.rs`.
  - [ ] M5.P3.L4.T28 — Document all four modes' template texts, output-schema sources, actor-specific input mappings, repair composition, report ownership, step limits, and D71 shared-move integration in `docs/prompt-pack.md`.

- **M5.P3.L5 — Add tournament prompts**
  - [x] M5.P3.L5.T1 — Add root `prompts/tournament.entrant.md` for the `squad-core` prompt pack with elimination rules, remaining entrant count, ordinary word limits, and own-result improvement guidance; register its system-suffix composition in `prompts/manifest.json`.
  - [x] M5.P3.L5.T2 — Add root `prompts/tournament.judge.md` and the blind `judge.system.md` composition for the `squad-core` prompt pack, including anonymous strict ranking, forfeit guidance, and task-specific repair text; register composition-specific placeholders in `prompts/manifest.json`.
  - [x] M5.P3.L5.T3 — Add root `prompts/tournament.result.md` and `prompts/tournament.final.md` for the `squad-core` prompt pack, exposing only the addressed entrant’s own result and the final nick-and-placement list respectively.
  - [x] M5.P3.L5.T4 — Map canonical tournament state, own results, anonymized judge material, ranking schema, and final placements in `squad-core` at `crates/squad-core/src/prompts/inputs.rs`; record template and composition provenance in `crates/squad-core/src/orchestrator/provenance.rs`.
  - [x] M5.P3.L5.T5 — Integrate canonical tournament-ranking validation and blind attempt-scoped repair within three total attempts in `squad-agents` at `crates/squad-agents/src/judge.rs`, leaving replay and elimination decisions to the contest engine.
  - [x] M5.P3.L5.T6 — Capture first and later tournament-entry requests to verify one member system message, ordinary word limits, exactly one own preceding result, and absence of other entrants’ answers or results in `squad-core` at `crates/squad-core/tests/tournament_prompts.rs`.
  - [x] M5.P3.L5.T7 — Capture blind tournament judge and repair requests to verify no entrant nick, entrant model name, roster, persona, label map, or prior-round identity mapping enters any provider message, including when the judge’s model competes, in `squad-core` at `crates/squad-core/tests/tournament_prompts.rs`.
  - [x] M5.P3.L5.T8 — Test anonymous-label permutation fixtures, forfeit ranking, strict ordering with equal scores, repair exhaustion, shuffled-label rendering, and final placement notices without answer text, scores, or critiques in `squad-core` at `crates/squad-core/tests/tournament_prompts.rs` using canonical `squad-protocol` validators.
  - [x] M5.P3.L5.T9 — Document tournament template IDs, blind system composition and override requirements, own-result delivery, ranking repair, and final-placement visibility for the `squad-core` prompt pack in `docs/prompt-pack.md`.

- **M5.P3.L6 — Add grill prompts**
  - [ ] M5.P3.L6.T1 — Add root `prompts/grill.question.md` for the `squad-core` prompt pack with the assigned angle, at most three questions within 200 words, no-repeat guidance, and exact `PASS`; register its placeholders in `prompts/manifest.json`.
  - [ ] M5.P3.L6.T2 — Add root `prompts/grill.answer.md` for the `squad-core` prompt pack with direct answers, the configured turn word limit, honest concessions, and the required `Plan changes` section.
  - [x] M5.P3.L6.T3 — Add root `prompts/grill.rebuild.md` for the `squad-core` prompt pack with complete standalone plan output, the configured plan word limit, version progression, and application-owned appendix guidance.
  - [ ] M5.P3.L6.T4 — Map the canonical leader, griller, assigned angle, frozen rounds, plan versions, focus, current shared floor, and previously asked question list in `squad-core` at `crates/squad-core/src/prompts/inputs.rs`; register composition and provenance in `prompts/manifest.json` and `crates/squad-core/src/orchestrator/provenance.rs`.
  - [ ] M5.P3.L6.T5 — Capture grill participant requests to verify the shared floor reaches exactly the participants, the latest plan and current-round material appear once, and earlier rounds retain only the compact question list in the working projection in `squad-core` at `crates/squad-core/tests/grill_prompts.rs`.
  - [ ] M5.P3.L6.T6 — Test question, answer, and rebuild compositions for the angle rotation inputs, no-repeat list, exact `PASS`, word limits, plan versions, skipped all-pass rebuild, and absence of judge prompts in `squad-core` at `crates/squad-core/tests/grill_prompts.rs`.
  - [ ] M5.P3.L6.T7 — Verify leader-failure rendering preserves the last accepted plan and final output uses the accepted plan plus recorded question, author, answer, and plan-change appendix in `squad-core` at `crates/squad-core/tests/grill_prompts.rs`.
  - [ ] M5.P3.L6.T8 — Document grill template IDs, shared-floor boundaries, working-context projection, limits, pass handling, plan rebuilds, and recorded appendix ownership for the `squad-core` prompt pack in `docs/prompt-pack.md`.

- **M5.P3.L7 — Add census prompts**
  - [x] M5.P3.L7.T1 — Add root `prompts/census.hunt.md`, `prompts/census.merge.md`, `prompts/census.rate.md`, and `prompts/census.final.md` with the complete task text, canonical schemas, assigned-lens inputs, citations, source references, and majority-decision framing.
  - [x] M5.P3.L7.T2 — Map census pass, current plan version, assigned lens, private hunt references, anonymous rating ledger, accepted ratings, additions, and computed majority decisions in `squad-core` at `crates/squad-core/src/prompts/inputs.rs`.
  - [x] M5.P3.L7.T3 — Capture census requests to verify independent hunts, no finder identities or counts in rating payloads, no rating additions treated as hunt captures, preserved covered citations, and no model-computed estimate in `squad-core` at `crates/squad-core/tests/census_prompts.rs`.
  - [x] M5.P3.L7.T4 — Verify accepted-ledger report rendering uses the application's range, finder counts, singleton flags, discovery curve, exact headline, shared-blind-spots caveat, quiet notice, skipped-member records, and singleton-to-grill hint without another provider call in `squad-core` at `crates/squad-core/tests/census_prompts.rs`.
  - [x] M5.P3.L7.T5 — Test patched-plan version changes, lens rotation inputs, canonical gap coverage and reference validation, malformed ratings, repair isolation, and restart-frozen census compositions in `squad-core` at `crates/squad-core/tests/census_prompts.rs`.

- **M5.P3.L8 — Add crosscheck prompts**
  - [x] M5.P3.L8.T1 — Add root `prompts/crosscheck.inventory.md`, `prompts/crosscheck.normalize.md`, `prompts/crosscheck.cell.md`, `prompts/crosscheck.audit.md`, and `prompts/crosscheck.rule.md`, including complete goal-extraction and inventory-normalization compositions.
  - [x] M5.P3.L8.T2 — Map plan-free inventory inputs, private normalized source mappings, priority ordering, actor-specific cells, fresh anonymous audit samples, complete ruling evidence, and recorded human acceptance references in `squad-core` at `crates/squad-core/src/prompts/inputs.rs`.
  - [ ] M5.P3.L8.T3 — Capture inventory and normalization requests to verify no plan leaks through prior history, quotes, notices, schemas, or repairs; verify the optional extraction call delivers only goal, constraints, and uncertainties to inventory writers in `squad-core` at `crates/squad-core/tests/crosscheck_prompts.rs`.
  - [ ] M5.P3.L8.T4 — Capture reviewer and fresh-auditor requests to verify own-cell delivery, omitted authorship and reviewer identities, absence of earlier auditor history, and exact sampled-evidence projection in `squad-core` at `crates/squad-core/tests/crosscheck_prompts.rs`.
  - [ ] M5.P3.L8.T5 — Test coverage citations, justified exclusions, unsupported-coverage gaps, challenged closures, proposed additions, retention of every grid cell, and rejection of model-created human acceptances using canonical validators in `squad-core` at `crates/squad-core/tests/crosscheck_prompts.rs`.
  - [ ] M5.P3.L8.T6 — Verify report rendering preserves open gaps and proposed fixes, explicit residual-risk acceptance requirements, missing audit work, and the bounded stop statement without an extra provider call in `squad-core` at `crates/squad-core/tests/crosscheck_prompts.rs`.

- **M5.P3.L9 — Add survey prompts**
  - [x] M5.P3.L9.T1 — Add root `prompts/survey.extract.md`, `prompts/survey.vote.md`, `prompts/survey.revote.md`, and `prompts/survey.report.md` with neutral atomic claims, private origins, structured ballots, change-evidence fields, and all five report bands.
  - [x] M5.P3.L9.T2 — Map ordinary question-first collection or reused parallel answers, private origin references, anonymous claim and reason projections, selected revote claims, computed counts, band assignments, and switch records in `squad-core` at `crates/squad-core/src/prompts/inputs.rs`.
  - [x] M5.P3.L9.T3 — Capture vote and revote requests to verify absence of whole answers, authorship, origin counts, ballot counts, private mappings, and forbidden earlier history in `squad-core` at `crates/squad-core/tests/survey_prompts.rs`.
  - [x] M5.P3.L9.T4 — Test canonical claim coverage, AGREE/DISAGREE/UNSURE values, confidence bounds, selected-claim revoting, evidence-to-change-mind fields, skipped ballots, and task-specific repair in `squad-core` at `crates/squad-core/tests/survey_prompts.rs`.
  - [x] M5.P3.L9.T5 — Verify the human report preserves consensus, contested claims with both sides and settling questions, lone insights, lone dissents, rejected outliers, recorded switches, and uncertainty without declaring a winner or delivering private tallies to members in `squad-core` at `crates/squad-core/tests/survey_prompts.rs`.

- **M5.P3.L10 — Add probe prompts**
  - [x] M5.P3.L10.T1 — Add root `prompts/probe.explain.md`, `prompts/probe.check.md`, `prompts/probe.predict.md`, and `prompts/probe.report.md` with independent roles, measurable non-destructive checks, committed predictions, unknowns, and a structured diagnostic tree.
  - [x] M5.P3.L10.T2 — Map the diagnostic brief, application-assigned roles, own explanation, accepted checks, committed matrix, and missing-work records in `squad-core` at `crates/squad-core/src/prompts/inputs.rs`.
  - [ ] M5.P3.L10.T3 — Capture initial and prediction requests to verify checkers see no explanations, explainers see no rival explanations or predictions, and predictions receive only their own accepted explanation plus all checks in `squad-core` at `crates/squad-core/tests/probe_prompts.rs`.
  - [x] M5.P3.L10.T4 — Test canonical explanation and check cardinalities, measurement and outcome fields, exact check coverage, unknown predictions, immutable committed predictions, and schema repair in `squad-core` at `crates/squad-core/tests/probe_prompts.rs`.
  - [x] M5.P3.L10.T5 — Verify report compositions preserve the prediction matrix, discarded-check reasons, inseparable groups, valid tree references, unexpected-result branches, skipped work, and the absence of truth declarations or invented observations in `squad-core` at `crates/squad-core/tests/probe_prompts.rs`.

## Phase M5.P4 — Judge integration and verdict delivery

- **M5.P4.L1 — Summon isolated judges**
  - [x] M5.P4.L1.T1 — Implement concrete-model conflict detection, fallback skipping, and the exact model-naming swap warning in `crates/squad-core/src/orchestrator/judge.rs`.
  - [ ] M5.P4.L1.T2 — Commit referenced adjudication material to case histories and enqueue canonical judge work in `crates/squad-core/src/router/debates.rs`.
  - [x] M5.P4.L1.T3 — Integrate the case-bound judge executor using the restricted `AgentContext` in `crates/squad-agents/src/agents/judge.rs`.
  - [x] M5.P4.L1.T4 — Verify complete case evidence and exclusion of member histories and personas in `crates/squad-core/tests/orchestrator/judge_context.rs`.
  - [ ] M5.P4.L1.T5 — Create the automatic judge case before numbered-round scoring, retain its accumulating evidence, and gate final judge enqueue on all scoring work being completed or given up in `crates/squad-core/src/orchestrator/judge.rs`.
  - [x] M5.P4.L1.T6 — Integrate `judge.round.md`, low reasoning effort, the 1,024-token reasoning budget, the 512-token visible ceiling, the 120 s deadline, and the two-attempt allowance including repair in `crates/squad-agents/src/agents/judge.rs`; commit an unscored outcome after exhaustion through `crates/squad-core/src/router/debates.rs`.
  - [x] M5.P4.L1.T7 — Test late and failed round scores, equal round scores, nonblocking later turns, accumulated case evidence, and deferred final judging in `crates/squad-core/tests/orchestrator/round_scores.rs`.
  - [x] M5.P4.L1.T8 — Spawn and retire case executors for unfinished work, including after recovery, in `crates/squad-core/src/router/cases.rs`.
  - [x] M5.P4.L1.T9 — Persist rejected output and validation errors in attempt-scoped repair records exposed only through case-bound work readers in `crates/squad-core/src/router/cases.rs`.
  - [ ] M5.P4.L1.T10 — Test one rendered user message per judge request, evidence deduplication, repair-record access, and repaired-attempt recovery in `crates/squad-core/tests/orchestrator/judge_context.rs`.
  - [ ] M5.P4.L1.T11 — Test renamed Fable models, duplicate-model roster members, absent default fallback nicks, standalone conflicts, and response-model mismatch diagnostics in `crates/squad-core/tests/orchestrator/judge_context.rs`.

- **M5.P4.L2 — Verify verdict validation integration**
  - [ ] M5.P4.L2.T1 — Test invalid JSON, duplicate candidates, missing criteria, noninteger scores, ties, and inconsistent winners against M1 validation in `crates/squad-core/tests/orchestrator/verdict.rs`.
  - [x] M5.P4.L2.T2 — Test the shared transport-and-repair attempt ceiling and `JUDGE_INVALID_VERDICT` outcome in `crates/squad-core/tests/orchestrator/verdict.rs`.
  - [x] M5.P4.L2.T3 — Verify locally computed totals and instruction-based equal-total decisions in `crates/squad-core/tests/orchestrator/verdict.rs`.
  - [x] M5.P4.L2.T4 — Test required nullable `tallyOverrideReason`, repair when an override explanation is missing, equal and zero round tallies, and preservation of the weighted-total winner rule in `crates/squad-core/tests/orchestrator/verdict.rs`.
  - [x] M5.P4.L2.T5 — Test round-score nick validation, integer bounds, reason length, strict JSON, and the shared two-attempt transport-and-repair ceiling in `crates/squad-core/tests/orchestrator/round_scores.rs`.
  - [x] M5.P4.L2.T6 — Test schema-first validation of missing nullable fields and unknown properties before serde deserialization in `crates/squad-core/tests/orchestrator/verdict.rs`.
  - [x] M5.P4.L2.T7 — Verify visible-plus-reasoning token limits and final judge deadlines against the fake provider in `crates/squad-core/tests/orchestrator/verdict.rs`.

- **M5.P4.L3 — Commit verdicts and scores**
  - [x] M5.P4.L3.T1 — Atomically commit verdict delivery, case sealing, completion, and score updates through the `squad-store` transaction API in `crates/squad-core/src/router/debates.rs`.
  - [ ] M5.P4.L3.T2 — Test duplicate verdicts, replay, crash recovery, and independent records for nicks sharing a model in `crates/squad-core/tests/orchestrator/scores.rs`.
  - [x] M5.P4.L3.T3 — Verify verdict score records and case histories are removed by `/clear` in `crates/squad-core/tests/orchestrator/clear.rs`.
  - [x] M5.P4.L3.T4 — Verify usage reconciliation and reservation release for every terminal state in `crates/squad-core/tests/orchestrator/budget.rs`.
  - [x] M5.P4.L3.T5 — Compute round tallies from accepted scores, persist round-score outcomes idempotently through the canonical `round_scores` store contract, and attach `roundTally` to committed verdicts in `crates/squad-core/src/router/debates.rs`.
  - [x] M5.P4.L3.T6 — Deliver per-round scores and tally to both debaters for the first time in the verdict announcement in `crates/squad-core/src/router/debates.rs`, consuming the Prompt pack’s announcement template.
  - [ ] M5.P4.L3.T7 — Test tally arithmetic, late scores, unscored rounds, duplicate commits, unchanged win/loss records before verdict, and atomic verdict delivery in `crates/squad-core/tests/orchestrator/scores.rs`.
  - [x] M5.P4.L3.T8 — Verify round-score records are removed by `/clear` in `crates/squad-core/tests/orchestrator/clear.rs`.

- **M5.P4.L4 — Support manual judging and cases**
  - [x] M5.P4.L4.T1 — Implement `/judge` selection and judge-only preflight for retained stopped or unjudged debates in `crates/squad-core/src/orchestrator/judge.rs`.
  - [x] M5.P4.L4.T2 — Implement quote-based standalone case creation and free-form rulings in `crates/squad-core/src/orchestrator/cases.rs`.
  - [x] M5.P4.L4.T3 — Implement verdict-quote case reopening with prior case follow-ups in `crates/squad-core/src/orchestrator/cases.rs`.
  - [ ] M5.P4.L4.T4 — Test fresh-case isolation, follow-up isolation, sealed verdicts, and unchanged score records in `crates/squad-core/tests/orchestrator/cases.rs`.
  - [ ] M5.P4.L4.T5 — Test manual judging cancellation and repeated `/judge` on an already judged debate in `crates/squad-core/tests/orchestrator/manual_judge.rs`.
  - [x] M5.P4.L4.T6 — Supply existing round scores and unscored-round information to fresh manual-judge cases without retroactive scoring in `crates/squad-core/src/orchestrator/judge.rs`.
  - [ ] M5.P4.L4.T7 — Test manual judging of stopped debates with partial scores and unjudged debates with no scores, asserting no `judge.round` work is created, in `crates/squad-core/tests/orchestrator/manual_judge.rs`.
  - [x] M5.P4.L4.T8 — Persist judge-only prior terminal snapshots, explicit preparation transitions, and the case-ID list in `crates/squad-core/src/orchestrator/judge.rs`.
  - [x] M5.P4.L4.T9 — Test restoration after judge-only rejection, cancellation, expiry, and restart without losing earlier cases in `crates/squad-core/tests/orchestrator/manual_judge.rs`.

- **M5.P4.L5 — Verify complete integration**
  - [x] M5.P4.L5.T1 — Run the exact canonical commands through `RoomHandle` against the M2 fake OpenRouter service in `crates/squad-core/tests/orchestrator/integration.rs`.
  - [ ] M5.P4.L5.T2 — Test threshold confirmation Requests, preflight feed delivery, fresh-topic openings, and completion without `@judge` in `crates/squad-core/tests/orchestrator/integration.rs`.
  - [x] M5.P4.L5.T3 — Verify committed debate records supply complete markdown export material in `crates/squad-core/tests/orchestrator/export.rs`.
  - [ ] M5.P4.L5.T4 — Test oversized debate and case evidence fails before provider dispatch in `crates/squad-core/tests/orchestrator/context.rs`.
  - [x] M5.P4.L5.T5 — Verify automatically judged debates score numbered rounds only, completion without `@judge` creates no scoring work, and final verdict delivery includes scores and tally in `crates/squad-core/tests/orchestrator/integration.rs`.
  - [x] M5.P4.L5.T6 — Verify export material includes round scores, unscored outcomes, the final round tally, and any tally override explanation in `crates/squad-core/tests/orchestrator/export.rs`.
  - [x] M5.P4.L5.T7 — Test preflight ordering, threshold admission, preconfirmation, cancellation, expiry, and changed plans with M5 fixtures in `crates/squad-core/tests/room_preflight.rs`.
  - [x] M5.P4.L5.T8 — Verify preflight includes one round judge call per scored round, excludes round 0 and unjudged debates, and preserves the round count using M5 fixtures in `crates/squad-core/tests/room_preflight.rs`.
  - [ ] M5.P4.L5.T9 — Verify stop-all cancels outstanding round scoring and `/judge` on a stopped debate uses existing scores without retroactive scoring using M5 fixtures in `crates/squad-core/tests/room_actions.rs` and `crates/squad-core/tests/room_judge.rs`.
  - [ ] M5.P4.L5.T10 — Test waiting through outstanding round scoring before the final verdict, successful completion with an unscored round, and absence of scoring for unjudged debates using M5 fixtures in `crates/squad-cli/tests/send.rs`.
  - [ ] M5.P4.L5.T11 — Test the canonical question-first debate flow through `RoomHandle`, including the pick event, selected source IDs, confirmation, and verdict, in `crates/squad-core/tests/orchestrator/integration.rs`.

- **M5.P4.L6 — Integrate structured judge moves for new modes**
  - [ ] M5.P4.L6.T1 — Bind new-mode judge tasks to contest cases, medium reasoning, the 4,096-token visible ceiling and shared attempt-scoped repair handling.
  - [x] M5.P4.L6.T2 — Validate source references, exact requested-ID coverage and app-computed tallies before accepting new-mode judge artifacts.
  - [ ] M5.P4.L6.T3 — Verify judge exhaustion preserves partial artifacts, releases resources and never commits a verdict score or trophy for a new mode.

## Phase M5.P5 — Tournament

- **M5.P5.L1 — Admit and schedule elimination rounds**
  - [x] M5.P5.L1.T1 — Integrate this phase’s tournament payloads and the shared contest claim in `crates/squad-core/src/orchestrator/tournament.rs`.
  - [x] M5.P5.L1.T2 — Freeze entrants and prompt, calculate k−1 rounds, and implement normal-call and retry/replay reservation estimates in `crates/squad-core/src/orchestrator/preflight.rs`.
  - [x] M5.P5.L1.T3 — Implement entrant holds, `WAITING_FOR_ENTRANTS`, and parallel reply-like entry work with contest and replay identity in `crates/squad-core/src/orchestrator/tournament.rs`.
  - [x] M5.P5.L1.T4 — Integrate `tournament.entrant.md` and each survivor’s own previous result using the executor in `crates/squad-agents/src/agents/tournament.rs`.
  - [x] M5.P5.L1.T5 — Test minimum entrants, exclusions, queue draining, daily admission, planned call counts, and k−1 rounds in `crates/squad-core/tests/orchestrator/tournament.rs`.

- **M5.P5.L2 — Judge blind rankings**
  - [x] M5.P5.L2.T1 — Persist shuffled round label maps outside judge-readable capabilities and construct anonymised judging projections in `crates/squad-core/src/router/tournaments.rs`.
  - [x] M5.P5.L2.T2 — Integrate `judge.tournament`, the judge system prompt, high reasoning, visible-plus-reasoning limits, and the 120 s deadline in `crates/squad-agents/src/agents/judge.rs`.
  - [x] M5.P5.L2.T3 — Consume this phase’s permutation validation and the three-attempt repair path with attempt-scoped repair records in `crates/squad-core/src/orchestrator/tournament.rs`.
  - [x] M5.P5.L2.T4 — Implement forfeit placement below completed entries and single-elimination commits in `crates/squad-core/src/orchestrator/tournament.rs`.
  - [x] M5.P5.L2.T5 — Implement one durable replay after exhausted judging and fail after replay exhaustion in `crates/squad-core/src/orchestrator/tournament.rs`.
  - [x] M5.P5.L2.T6 — Test permutations, duplicate and missing labels, invalid scores, strict rank order, single-forfeit elimination, and replay fencing in `crates/squad-core/tests/orchestrator/tournament_ranking.rs`.
  - [x] M5.P5.L2.T7 — Capture judge requests and prove absence of entrant nicks, model identities, personas, and label maps, including self-identifying answer text, in `crates/squad-core/tests/orchestrator/tournament_isolation.rs`.
  - [x] M5.P5.L2.T8 — Verify that a Fable entrant does not trigger judge swapping in `crates/squad-core/tests/orchestrator/tournament_isolation.rs`.

- **M5.P5.L3 — Deliver results and commit the champion**
  - [x] M5.P5.L3.T1 — Commit full rankings to `me` and only each entrant’s own result to its history in `crates/squad-core/src/router/tournaments.rs`.
  - [x] M5.P5.L3.T2 — Integrate `tournament.result.md` and `tournament.final.md` without exposing other entrants’ answers or individual results in `crates/squad-core/src/orchestrator/tournament.rs`.
  - [x] M5.P5.L3.T3 — Atomically commit final placements, champion, terminal state, and one trophy increment in `crates/squad-core/src/router/tournaments.rs`.
  - [x] M5.P5.L3.T4 — Test entrant and non-entrant isolation, final-placement visibility, duplicate champion commits, and independent debate score records in `crates/squad-core/tests/orchestrator/tournament_isolation.rs`.
  - [x] M5.P5.L3.T5 — Supply tournament state and committed ranking records to the TUI and export consumers in `crates/squad-core/src/orchestrator/tournament.rs`.

- **M5.P5.L4 — Recover and verify tournament integration**
  - [x] M5.P5.L4.T1 — Recover survivors, replay identity, label maps, judging work, elimination commits, and trophy state in `crates/squad-core/src/orchestrator/recovery.rs`.
  - [x] M5.P5.L4.T2 — Fence tournament entries and judging on stop and release participant holds and unused reservations in `crates/squad-core/src/orchestrator/stop.rs`.
  - [x] M5.P5.L4.T3 — Test restart at answering, judging, replay, elimination, and champion commit in `crates/squad-core/tests/orchestrator/tournament_recovery.rs`.
  - [x] M5.P5.L4.T4 — Test canonical tournament commands, daily-cap interruption, no pick step, and mutual exclusion with debate through `RoomHandle` in `crates/squad-core/tests/orchestrator/tournament_integration.rs`.
  - [x] M5.P5.L4.T5 — Verify tournament export records include failed runs, accepted rankings, eliminations, placements, and champion in `crates/squad-core/tests/orchestrator/export.rs`.
  - [x] M5.P5.L4.T6 — Verify `/clear` removes tournament records and trophies through the shared clear contract in `crates/squad-core/tests/orchestrator/clear.rs`.

- **M5.P5.L5 — Run tournament over shared moves**
  - [x] M5.P5.L5.T1 — Express initial collection, survivor-specific fanouts, blind judge calls, elimination tallies, own-result barriers and champion report as the tournament script.
  - [x] M5.P5.L5.T2 — Verify unchanged entry limits, shuffled-label isolation, single-elimination rounds, replay identity, call counts and trophy commits over the shared engine.

- **M5.P5.L6 — Complete mode protocol, storage and visibility contracts**
  - [x] M5.P5.L6.T1 — Declare tournament statuses, typed payloads, preflight settings, lifecycle transition variant, case material where applicable, work kinds and validators in `crates/squad-protocol/src/tournament.rs`, and register them in Protocol’s gated enums; these are not M1 types.
  - [x] M5.P5.L6.T2 — Add the tournament tables and event/work CHECK extensions from 02 in a new mode migration, retaining all earlier released records; enforce mode-specific identities and accepted-result fencing in `crates/squad-store/src/schema.rs`.
  - [x] M5.P5.L6.T3 — Implement router-only tournament label-map storage and restricted anonymised judge projections in crate `squad-store`, file `crates/squad-store/src/tournaments.rs`, and crate `squad-core`, file `crates/squad-core/src/router/tournaments.rs`. (Deferred from M1.P3.L5.T3.)
  - [x] M5.P5.L6.T4 — Commit accepted rankings, individual result histories, one elimination and final placements with an idempotent trophy increment in crate `squad-store`, file `crates/squad-store/src/tournaments.rs`. (Deferred from M1.P3.L5.T4.)
  - [x] M5.P5.L6.T5 — Prove tournament entrant histories contain no other entrant’s answers or individual results, and blind judge reads contain no entrant nick, model name or label map, in crate `squad-core`, file `crates/squad-core/tests/router_visibility.rs`, and crate `squad-core`, file `crates/squad-core/tests/isolation.rs`. (Deferred from M1.P4.L4.T2.)
  - [x] M5.P5.L6.T6 — Test duplicate ranking delivery, round replay records, final-placement projections and exactly one trophy increment in crate `squad-store`, file `crates/squad-store/tests/tournaments.rs`. (Deferred from M1.P4.L4.T3.)
  - [x] M5.P5.L6.T7 — Add the exact tournament per-step projected-field rows to 02’s sole visibility table and verify real-store reads and deterministic blind-text redaction in `crates/squad-core/tests/isolation.rs`; preserve human transcript originals.
  - [x] M5.P5.L6.T8 — Require tournament `Work.replay` and matching round-execution replay on entry/judge result acceptance; test nullable forfeit scores/critiques and idempotent champion `trophies`.

## Phase M5.P6 — Grill

- **M5.P6.L1 — Select plans and admit the shared floor**
  - [x] M5.P6.L1.T1 — Implement direct leader-plan selection and explicit griller restrictions using parser-owned metadata in `crates/squad-core/src/orchestrator/grill.rs`.
  - [x] M5.P6.L1.T2 — Convert a question-first grill pick into plan v1 and the other eligible answerers into grillers in `crates/squad-core/src/orchestrator/selection.rs`.
  - [x] M5.P6.L1.T3 — Freeze `GRILL_ROUNDS`, focus, participants, source plan, and limits and implement the call-count estimate and retry reservation in `crates/squad-core/src/orchestrator/preflight.rs`.
  - [x] M5.P6.L1.T4 — Implement participant holds, `WAITING_FOR_PARTICIPANTS`, and setup delivery to exactly the shared floor in `crates/squad-core/src/router/grills.rs`.
  - [x] M5.P6.L1.T5 — Test selected and quoted plans, the missing-plan string, griller restrictions, the 2-default and 1-to-10 round bounds, and question-first picking in `crates/squad-core/tests/orchestrator/grill_admission.rs`.

- **M5.P6.L2 — Execute sequential questions and answers**
  - [x] M5.P6.L2.T1 — Persist rotated roster order, primary angles, round, and turn index in `crates/squad-core/src/orchestrator/grill.rs`.
  - [x] M5.P6.L2.T2 — Integrate `grill.question`, `grill.answer`, and `grill.rebuild` work with the shared deadlines and two-attempt allowance in `crates/squad-agents/src/agents/grill.rs`.
  - [ ] M5.P6.L2.T3 — Integrate question, answer, and rebuild prompt texts and their word and visible-token limits in `crates/squad-agents/src/agents/grill.rs`.
  - [x] M5.P6.L2.T4 — Commit questions and complete attributed answers with plan changes to every floor participant before advancing the turn in `crates/squad-core/src/router/grills.rs`.
  - [x] M5.P6.L2.T5 — Implement exact `PASS`, distinguish voluntary passes from failed griller skips, and apply leader-failure termination in `crates/squad-core/src/orchestrator/grill.rs`.
  - [x] M5.P6.L2.T6 — Test sequential execution, rotation, angle assignment, three-question limits, answer coverage, pass handling, skipped grillers, and leader failure in `crates/squad-core/tests/orchestrator/grill_turns.rs`.

- **M5.P6.L3 — Rebuild plans and preserve the appendix**
  - [x] M5.P6.L3.T1 — Build context-bound round projections from the latest plan, current floor, and compact earlier-question list in `crates/squad-core/src/orchestrator/grill_context.rs`.
  - [x] M5.P6.L3.T2 — Commit complete plan versions and retain all earlier versions and source references in `crates/squad-core/src/router/grills.rs`.
  - [x] M5.P6.L3.T3 — Implement all-pass early completion with reason `survived` and skip rebuild when no questions were asked in `crates/squad-core/src/orchestrator/grill.rs`.
  - [x] M5.P6.L3.T4 — Build the complete attributed appendix and final-plan reference from committed records in `crates/squad-core/src/orchestrator/grill.rs`.
  - [x] M5.P6.L3.T5 — Test exact shared-floor delivery, non-participant exclusion, compact prior-question context, no-repeat instructions, and retained plan versions in `crates/squad-core/tests/orchestrator/grill_isolation.rs`.
  - [x] M5.P6.L3.T6 — Verify final-plan and appendix export through the shared core export API in `crates/squad-core/tests/orchestrator/export.rs`.

- **M5.P6.L4 — Recover and verify grill integration**
  - [ ] M5.P6.L4.T1 — Recover floor delivery, turn order, angles, plan version, questions, answers, and incomplete rebuild work in `crates/squad-core/src/orchestrator/recovery.rs`.
  - [x] M5.P6.L4.T2 — Fence grill work on stop, preserve the last good plan, and release holds and unused reservations in `crates/squad-core/src/orchestrator/stop.rs`.
  - [ ] M5.P6.L4.T3 — Test restart after a question, answer, pass, plan commit, and terminal transition in `crates/squad-core/tests/orchestrator/grill_recovery.rs`.
  - [ ] M5.P6.L4.T4 — Test direct and question-first grill commands, normal room use during picking, daily-cap failure, and cross-kind active-contest exclusion through `RoomHandle` in `crates/squad-core/tests/orchestrator/grill_integration.rs`.
  - [x] M5.P6.L4.T5 — Verify grill completion creates no judge work, verdict, debate win/loss change, or tournament trophy in `crates/squad-core/tests/orchestrator/grill_integration.rs`.
  - [ ] M5.P6.L4.T6 — Verify `/clear` removes floor records, plan versions, selection state, and appendix references through the shared clear contract in `crates/squad-core/tests/orchestrator/clear.rs`.

- **M5.P6.L5 — Run grill over shared moves**
  - [x] M5.P6.L5.T1 — Express optional collection and picking, sequential shared-floor exchanges, rebuild barriers and final-plan assembly as the grill script without changing existing behavior.
  - [ ] M5.P6.L5.T2 — Verify the shared `turns` and `barrier` moves preserve rotated order, compact projections, attributed skips, all-pass completion and the existing call-count formula.

- **M5.P6.L6 — Complete mode protocol, storage and visibility contracts**
  - [x] M5.P6.L6.T1 — Declare grill statuses, typed payloads, preflight settings, lifecycle transition variant, case material where applicable, work kinds and validators in `crates/squad-protocol/src/grill.rs`, and register them in Protocol’s gated enums; these are not M1 types.
  - [x] M5.P6.L6.T2 — Add the grill tables and event/work CHECK extensions from 02 in a new mode migration, retaining all earlier released records; enforce mode-specific identities and accepted-result fencing in `crates/squad-store/src/schema.rs`.
  - [x] M5.P6.L6.T3 — Commit grill setup, questions, answers, passes and plan versions to exactly the participant histories and retain appendix references in crate `squad-store`, file `crates/squad-store/src/grills.rs`. (Deferred from M1.P3.L5.T5.)
  - [x] M5.P6.L6.T4 — Implement authorised grill working-context selection using the latest plan, current-round exchanges and prior-question list in crate `squad-store`, file `crates/squad-store/src/readers.rs`. (Deferred from M1.P3.L5.T6.)
  - [x] M5.P6.L6.T5 — Prove grill shared-floor delivery reaches exactly the participants and that working reads exclude full earlier-round answers while preserving the no-repeat question list in crate `squad-store`, file `crates/squad-store/tests/grills.rs`. (Deferred from M1.P4.L4.T4.)
  - [ ] M5.P6.L6.T6 — Add the exact grill per-step projected-field rows to 02’s sole visibility table and verify real-store reads and deterministic blind-text redaction in `crates/squad-core/tests/isolation.rs`; preserve human transcript originals.

## Phase M5.P7 — Census

- **M5.P7.L1 — Admit plans and schedule blind passes**
  - [x] M5.P7.L1.T1 — Implement quoted, pasted and latest-member plan selection, minimum participation and frozen `CENSUS_PASSES` in `crates/squad-core/src/orchestrator/census.rs`.
  - [x] M5.P7.L1.T2 — Persist plan versions, pass numbers and rotating lens assignments and derive the `p × (2m + 2)` normal-call and `p × (4m + 6)` reserved-attempt counts.
  - [x] M5.P7.L1.T3 — Execute hunt and anonymous rating fanouts with barriers using `census.hunt.md` and `census.rate.md`.

- **M5.P7.L2 — Finalize ledgers and estimates**
  - [x] M5.P7.L2.T1 — Integrate `census.merge.md` and `census.final.md`, preserving distinct blind finder sets, rating-stage origins, majority exclusions and covered citations.
  - [x] M5.P7.L2.T2 — Implement app-computed finder counts, `f1`, `f2`, both Chao1 branches, singleton markers and cross-pass discovery identities.
  - [x] M5.P7.L2.T3 — Resolve the estimate-range rule, implement its bounds and shared-blind-spot caveat, and gate the exact quiet message on complete qualifying evidence.
  - [x] M5.P7.L2.T4 — Support between-pass continuation, quoted replacement plans, revised preflight when needed and claim-releasing handoff to grill.

- **M5.P7.L3 — Report and recover census**
  - [x] M5.P7.L3.T1 — Assemble the ranked ledger, human finder table, covered citations, estimate line, singleton hint and discovery curve in `crates/squad-core/src/orchestrator/census.rs`.
  - [x] M5.P7.L3.T2 — Recover each step and between-pass wait without duplicate hunts, ratings, estimates or deliveries; fence all work on stop.
  - [x] M5.P7.L3.T3 — Verify 14-call passes, 28-call defaults, failed-member reporting, majority arithmetic, duplicate finder handling, zero-doubleton estimates, plan revisions and markdown export in `crates/squad-core/tests/orchestrator/census.rs`.

- **M5.P7.L4 — Complete mode protocol, storage and visibility contracts**
  - [x] M5.P7.L4.T1 — Declare census statuses, typed payloads, preflight settings, lifecycle transition variant, case material where applicable, work kinds and validators in `crates/squad-protocol/src/census.rs`, and register them in Protocol’s gated enums; these are not M1 types.
  - [x] M5.P7.L4.T2 — Add the census tables and event/work CHECK extensions from 02 in a new mode migration, retaining all earlier released records; enforce mode-specific identities and accepted-result fencing in `crates/squad-store/src/schema.rs`.
  - [x] M5.P7.L4.T3 — Implement the census setup, state, plan, hunt, ledger, rating, report and case-material types with exhaustive envelope registration in crate `squad-protocol`, file `crates/squad-protocol/src/census.rs` and file `crates/squad-protocol/src/envelopes.rs`. (Deferred from M1.P1.L5.T2.)
  - [x] M5.P7.L4.T4 — Add valid and invalid fixtures for artifact references, assigned-item coverage, anonymity, missing work, required nullable fields and model attempts to supply authoritative tallies or risk acceptance in crate `squad-protocol`, file `crates/squad-protocol/tests/modes.rs`. (Deferred from M1.P1.L5.T9.)
  - [x] M5.P7.L4.T5 — Commit census plan versions, hunt artifacts, finder sets, ledgers, ratings and reports with distinct initial-find and addition provenance in crate `squad-store`, file `crates/squad-store/src/census.rs`. (Deferred from M1.P3.L7.T4.)
  - [x] M5.P7.L4.T6 — Verify all seven kinds compete for one active claim and that new artifact commits preserve complete event groups and one committed effect in crate `squad-core`, file `crates/squad-core/tests/contest_storage.rs`. (Deferred from M1.P4.L5.T1.)
  - [x] M5.P7.L4.T7 — Prove census hunters cannot read peer finds, raters cannot read finder identities or counts, and later additions cannot alter initial finder sets in crate `squad-store`, file `crates/squad-store/tests/census.rs`. (Deferred from M1.P4.L5.T2.)
  - [x] M5.P7.L4.T8 — Test guessed work IDs, alternate-context access and metadata leaks across every new step-bound reader in crate `squad-store`, file `crates/squad-store/tests/readers.rs`, and crate `squad-core`, file `crates/squad-core/tests/isolation.rs`. (Deferred from M1.P4.L5.T6.)
  - [x] M5.P7.L4.T9 — Crash and recover at assignment, artifact, barrier and next-work allocation boundaries without rerolling samples, repeating tallies or losing skipped-member outcomes in crate `squad-core`, file `crates/squad-core/tests/mode_recovery.rs`. (Deferred from M1.P4.L5.T7.)
  - [ ] M5.P7.L4.T10 — Verify four-mode report and artifact replay supports consistent export and banner reconstruction without model calls, raw private maps or provisional chunks in crate `squad-store`, file `crates/squad-store/tests/event_pages.rs`. (Deferred from M1.P4.L5.T9.)
  - [x] M5.P7.L4.T11 — Test stop and clear fencing for every new work kind, foreign-key-safe deletion of all new conversational records, `CENSUS_PASSES` reset and retained daily accounting in crate `squad-store`, file `crates/squad-store/tests/clear.rs`, and crate `squad-core`, file `crates/squad-core/tests/router_fencing.rs`. (Deferred from M1.P4.L5.T10.)
  - [x] M5.P7.L4.T12 — Add the exact census per-step projected-field rows to 02’s sole visibility table and verify real-store reads and deterministic blind-text redaction in `crates/squad-core/tests/isolation.rs`; preserve human transcript originals.
  - [x] M5.P7.L4.T13 — Persist private random lens permutation/tie-break seed; exclude lenses and finder metadata from ratings; store nullable estimateLow/estimateHigh with Chao1 inputs and validate only Chao1 until the range rule is settled. Implement `/continue`, hold release at WAITING_FOR_PLAN, and reacquisition/draining on continue.

## Phase M5.P8 — Crosscheck

- **M5.P8.L1 — Build and assign the independent grid**
  - [x] M5.P8.L1.T1 — Freeze plan, goal and constraints and implement optional judge extraction with its extra preflight call in `crates/squad-core/src/orchestrator/crosscheck.rs`.
  - [x] M5.P8.L1.T2 — Deliver plan-hidden inventory payloads through fresh contexts and integrate `crosscheck.inventory.md` and `crosscheck.normalize.md`.
  - [x] M5.P8.L1.T3 — Build singles and all cross-lens pairs, validate the 153-cell default, apply the 200-cell priority cap and preserve omitted-cell records.
  - [x] M5.P8.L1.T4 — Balance eligible review assignments while excluding every contributor to a cell’s constituent items; retain unassignable cells as open.

- **M5.P8.L2 — Review and audit closures**
  - [x] M5.P8.L2.T1 — Integrate `crosscheck.cell.md`, validate exact assigned-cell coverage and retain citation, exclusion or proposed-fix evidence.
  - [x] M5.P8.L2.T2 — Persist random auditor selections and closure samples and invoke the M2 fresh-context auditor executor with `crosscheck.audit.md`.
  - [x] M5.P8.L2.T3 — Integrate `crosscheck.rule.md`; reopen false closures, preserve every retained cell and record accepted additions without automatic human risk acceptance.
  - [x] M5.P8.L2.T4 — Bind explicit human acceptances to cell or residual-risk identity and plan version and compute the bounded-stop result.

- **M5.P8.L3 — Report and recover crosscheck**
  - [x] M5.P8.L3.T1 — Assemble the coverage ledger, grid summary, audit findings, cap omissions, residual risks and export records.
  - [x] M5.P8.L3.T2 — Recover inventories, assignments, samples, reviews, rulings and acceptances without redrawing randomness or repeating accepted work.
  - [x] M5.P8.L3.T3 — Verify 16-call and 17-call preflights, no self-review, failed-review open cells, short audit samples, no silent cell deletion, stop fencing and markdown export in `crates/squad-core/tests/orchestrator/crosscheck.rs`.

- **M5.P8.L4 — Complete mode protocol, storage and visibility contracts**
  - [x] M5.P8.L4.T1 — Declare crosscheck statuses, typed payloads, preflight settings, lifecycle transition variant, case material where applicable, work kinds and validators in `crates/squad-protocol/src/crosscheck.rs`, and register them in Protocol’s gated enums; these are not M1 types.
  - [x] M5.P8.L4.T2 — Add the crosscheck tables and event/work CHECK extensions from 02 in a new mode migration, retaining all earlier released records; enforce mode-specific identities and accepted-result fencing in `crates/squad-store/src/schema.rs`.
  - [x] M5.P8.L4.T3 — Implement the crosscheck setup, state, goal, inventory, grid, review, audit, ruling, acceptance, report and case-material types in crate `squad-protocol`, file `crates/squad-protocol/src/crosscheck.rs` and file `crates/squad-protocol/src/envelopes.rs`. (Deferred from M1.P1.L5.T3.)
  - [x] M5.P8.L4.T4 — Add valid and invalid fixtures for artifact references, assigned-item coverage, anonymity, missing work, required nullable fields and model attempts to supply authoritative tallies or risk acceptance in crate `squad-protocol`, file `crates/squad-protocol/tests/modes.rs`. (Deferred from M1.P1.L5.T9.)
  - [x] M5.P8.L4.T5 — Commit crosscheck inventories, grids, origin-aware cell assignments, reviews, fixed audit samples, rulings and version-bound human risk acceptances in crate `squad-store`, file `crates/squad-store/src/crosscheck.rs`. (Deferred from M1.P3.L7.T5.)
  - [x] M5.P8.L4.T6 — Verify all seven kinds compete for one active claim and that new artifact commits preserve complete event groups and one committed effect in crate `squad-core`, file `crates/squad-core/tests/contest_storage.rs`. (Deferred from M1.P4.L5.T1.)
  - [x] M5.P8.L4.T7 — Prove crosscheck inventory contexts cannot read a plan already present in ordinary history, reviewers cannot read unassigned cells or review self-origin items, and fresh auditors cannot read reviewer histories or identities in crate `squad-store`, file `crates/squad-store/tests/crosscheck.rs`. (Deferred from M1.P4.L5.T3.)
  - [x] M5.P8.L4.T8 — Test guessed work IDs, alternate-context access and metadata leaks across every new step-bound reader in crate `squad-store`, file `crates/squad-store/tests/readers.rs`, and crate `squad-core`, file `crates/squad-core/tests/isolation.rs`. (Deferred from M1.P4.L5.T6.)
  - [x] M5.P8.L4.T9 — Crash and recover at assignment, artifact, barrier and next-work allocation boundaries without rerolling samples, repeating tallies or losing skipped-member outcomes in crate `squad-core`, file `crates/squad-core/tests/mode_recovery.rs`. (Deferred from M1.P4.L5.T7.)
  - [x] M5.P8.L4.T10 — Test human-only version-bound risk acceptance, stale acceptance rejection and rulings preserving every retained crosscheck cell in crate `squad-store`, file `crates/squad-store/tests/crosscheck.rs`. (Deferred from M1.P4.L5.T8.)
  - [ ] M5.P8.L4.T11 — Verify four-mode report and artifact replay supports consistent export and banner reconstruction without model calls, raw private maps or provisional chunks in crate `squad-store`, file `crates/squad-store/tests/event_pages.rs`. (Deferred from M1.P4.L5.T9.)
  - [x] M5.P8.L4.T12 — Test stop and clear fencing for every new work kind, foreign-key-safe deletion of all new conversational records, `CENSUS_PASSES` reset and retained daily accounting in crate `squad-store`, file `crates/squad-store/tests/clear.rs`, and crate `squad-core`, file `crates/squad-core/tests/router_fencing.rs`. (Deferred from M1.P4.L5.T10.)
  - [x] M5.P8.L4.T13 — Add the exact crosscheck per-step projected-field rows to 02’s sole visibility table and verify real-store reads and deterministic blind-text redaction in `crates/squad-core/tests/isolation.rs`; preserve human transcript originals.
  - [x] M5.P8.L4.T14 — Persist private lens permutation/tie-break seed; support nullable reviewer/grid origin with exclusive origin_event_seq; implement `/accept` on COMPLETE with plan/grid/state fencing.

## Phase M5.P9 — Survey

- **M5.P9.L1 — Freeze answers and extract claims**
  - [x] M5.P9.L1.T1 — Implement question-first collection and reuse of the most recent parallel answer set with frozen source identities in `crates/squad-core/src/orchestrator/survey.rs`.
  - [x] M5.P9.L1.T2 — Integrate `survey.extract.md`, enforcing at most 25 neutral deduplicated claims and private origin references.
  - [x] M5.P9.L1.T3 — Derive 20-call question-first and 14-call reuse preflights and their reserved-attempt bounds.

- **M5.P9.L2 — Lock votes and anonymous revotes**
  - [x] M5.P9.L2.T1 — Integrate `survey.vote.md`, exact claim coverage, three-value ballots, one-line reasons and confidence bounds.
  - [x] M5.P9.L2.T2 — Compute first tallies and deterministic split ordering, select up to five claims and persist anonymous reason projections without counts.
  - [x] M5.P9.L2.T3 — Integrate `survey.revote.md`, preserve missing second ballots and compute switches only from two accepted ballots.

- **M5.P9.L3 — Report and recover survey**
  - [x] M5.P9.L3.T1 — Integrate `survey.report.md` and validate the five bands, cross-references and settling questions against app-computed origins and tallies.
  - [x] M5.P9.L3.T2 — Recover collection, extraction, locked ballots, selected claims and report delivery without duplicate calls or changed source sets.
  - [x] M5.P9.L3.T3 — Verify empty claims, unsure and missing votes, singleton origins, rejected outliers, identified switches, failed-member reporting, stop fencing and markdown export in `crates/squad-core/tests/orchestrator/survey.rs`.

- **M5.P9.L4 — Complete mode protocol, storage and visibility contracts**
  - [x] M5.P9.L4.T1 — Declare survey statuses, typed payloads, preflight settings, lifecycle transition variant, case material where applicable, work kinds and validators in `crates/squad-protocol/src/survey.rs`, and register them in Protocol’s gated enums; these are not M1 types.
  - [x] M5.P9.L4.T2 — Add the survey tables and event/work CHECK extensions from 02 in a new mode migration, retaining all earlier released records; enforce mode-specific identities and accepted-result fencing in `crates/squad-store/src/schema.rs`.
  - [x] M5.P9.L4.T3 — Implement the survey setup, state, claims, vote, selection, revote, report and case-material types in crate `squad-protocol`, file `crates/squad-protocol/src/survey.rs` and file `crates/squad-protocol/src/envelopes.rs`. (Deferred from M1.P1.L5.T4.)
  - [x] M5.P9.L4.T4 — Add valid and invalid fixtures for artifact references, assigned-item coverage, anonymity, missing work, required nullable fields and model attempts to supply authoritative tallies or risk acceptance in crate `squad-protocol`, file `crates/squad-protocol/tests/modes.rs`. (Deferred from M1.P1.L5.T9.)
  - [x] M5.P9.L4.T5 — Commit survey source snapshots, claim provenance, both ballots, app-selected claims and report projections without exposing authorship or counts to voters in crate `squad-store`, file `crates/squad-store/src/survey.rs`. (Deferred from M1.P3.L7.T6.)
  - [x] M5.P9.L4.T6 — Verify all seven kinds compete for one active claim and that new artifact commits preserve complete event groups and one committed effect in crate `squad-core`, file `crates/squad-core/tests/contest_storage.rs`. (Deferred from M1.P4.L5.T1.)
  - [x] M5.P9.L4.T7 — Prove survey voters cannot read whole peer answers, claim authorship, vote counts or attributed reasons, and duplicate ballots cannot change tallies in crate `squad-store`, file `crates/squad-store/tests/survey.rs`. (Deferred from M1.P4.L5.T4.)
  - [x] M5.P9.L4.T8 — Test guessed work IDs, alternate-context access and metadata leaks across every new step-bound reader in crate `squad-store`, file `crates/squad-store/tests/readers.rs`, and crate `squad-core`, file `crates/squad-core/tests/isolation.rs`. (Deferred from M1.P4.L5.T6.)
  - [x] M5.P9.L4.T9 — Crash and recover at assignment, artifact, barrier and next-work allocation boundaries without rerolling samples, repeating tallies or losing skipped-member outcomes in crate `squad-core`, file `crates/squad-core/tests/mode_recovery.rs`. (Deferred from M1.P4.L5.T7.)
  - [ ] M5.P9.L4.T10 — Verify four-mode report and artifact replay supports consistent export and banner reconstruction without model calls, raw private maps or provisional chunks in crate `squad-store`, file `crates/squad-store/tests/event_pages.rs`. (Deferred from M1.P4.L5.T9.)
  - [x] M5.P9.L4.T11 — Test stop and clear fencing for every new work kind, foreign-key-safe deletion of all new conversational records, `CENSUS_PASSES` reset and retained daily accounting in crate `squad-store`, file `crates/squad-store/tests/clear.rs`, and crate `squad-core`, file `crates/squad-core/tests/router_fencing.rs`. (Deferred from M1.P4.L5.T10.)
  - [x] M5.P9.L4.T12 — Add the exact survey per-step projected-field rows to 02’s sole visibility table and verify real-store reads and deterministic blind-text redaction in `crates/squad-core/tests/isolation.rs`; preserve human transcript originals.
  - [x] M5.P9.L4.T13 — Enforce second-ballot change_evidence in SQL and bundle/deduplicate all anonymous revote reasons under the survey.selection barrier origin with no voter grouping or counts.

## Phase M5.P10 — Probe

- **M5.P10.L1 — Admit the brief and lock roles**
  - [x] M5.P10.L1.T1 — Freeze observation, ruled-out material and inspectable resources, enforce an even participating count of at least four and announce an odd-roster sit-out in `crates/squad-core/src/orchestrator/probe.rs`.
  - [x] M5.P10.L1.T2 — Persist a random equal explainer/checker split and derive `m + m/2 + 1` calls and `3m + 3` reserved attempts.
  - [x] M5.P10.L1.T3 — Run blind proposal fanouts with `probe.explain.md` and `probe.check.md`, validating one explanation or cheap non-destructive check per accepted member.

- **M5.P10.L2 — Lock predictions and assemble the tree**
  - [x] M5.P10.L2.T1 — Deliver only each explainer’s own explanation and all accepted anonymous checks through `probe.predict.md`.
  - [x] M5.P10.L2.T2 — Validate one prediction or explicit inability per check and retain missing rows without treating them as evidence.
  - [x] M5.P10.L2.T3 — Integrate `probe.report.md`, validate tree references, record discarded checks and inseparable groups, and require an unexpected-result branch at every check node.

- **M5.P10.L3 — Report and recover probe**
  - [x] M5.P10.L3.T1 — Assemble the runnable decision tree, prediction matrix, role and sit-out notices and export evidence.
  - [x] M5.P10.L3.T2 — Recover the same random split, proposals, locked predictions and report without redrawing roles or repeating accepted steps.
  - [x] M5.P10.L3.T3 — Verify the six-member 10-call path, odd-roster behavior, failed members, insufficient usable evidence, no truth declaration, stop fencing and markdown export in `crates/squad-core/tests/orchestrator/probe.rs`.


- **M5.P10.L4 — Complete mode protocol, storage and visibility contracts**
  - [x] M5.P10.L4.T1 — Declare probe statuses, typed payloads, preflight settings, lifecycle transition variant, case material where applicable, work kinds and validators in `crates/squad-protocol/src/probe.rs`, and register them in Protocol’s gated enums; these are not M1 types.
  - [x] M5.P10.L4.T2 — Add the probe tables and event/work CHECK extensions from 02 in a new mode migration, retaining all earlier released records; enforce mode-specific identities and accepted-result fencing in `crates/squad-store/src/schema.rs`.
  - [x] M5.P10.L4.T3 — Implement the probe setup, state, explanation, check, prediction, report and case-material types in crate `squad-protocol`, file `crates/squad-protocol/src/probe.rs` and file `crates/squad-protocol/src/envelopes.rs`. (Deferred from M1.P1.L5.T5.)
  - [x] M5.P10.L4.T4 — Add valid and invalid fixtures for artifact references, assigned-item coverage, anonymity, missing work, required nullable fields and model attempts to supply authoritative tallies or risk acceptance in crate `squad-protocol`, file `crates/squad-protocol/tests/modes.rs`. (Deferred from M1.P1.L5.T9.)
  - [x] M5.P10.L4.T5 — Commit probe role splits, sitting-out notices, blind initial artifacts, locked predictions and report references in crate `squad-store`, file `crates/squad-store/src/probe.rs`. (Deferred from M1.P3.L7.T7.)
  - [x] M5.P10.L4.T6 — Verify all seven kinds compete for one active claim and that new artifact commits preserve complete event groups and one committed effect in crate `squad-core`, file `crates/squad-core/tests/contest_storage.rs`. (Deferred from M1.P4.L5.T1.)
  - [x] M5.P10.L4.T7 — Prove probe initial roles remain mutually blind, explainers receive only their own explanation and frozen checks for prediction, and committed predictions cannot change before reporting in crate `squad-store`, file `crates/squad-store/tests/probe.rs`. (Deferred from M1.P4.L5.T5.)
  - [x] M5.P10.L4.T8 — Test guessed work IDs, alternate-context access and metadata leaks across every new step-bound reader in crate `squad-store`, file `crates/squad-store/tests/readers.rs`, and crate `squad-core`, file `crates/squad-core/tests/isolation.rs`. (Deferred from M1.P4.L5.T6.)
  - [x] M5.P10.L4.T9 — Crash and recover at assignment, artifact, barrier and next-work allocation boundaries without rerolling samples, repeating tallies or losing skipped-member outcomes in crate `squad-core`, file `crates/squad-core/tests/mode_recovery.rs`. (Deferred from M1.P4.L5.T7.)
  - [x] M5.P10.L4.T10 — Verify four-mode report and artifact replay supports consistent export and banner reconstruction without model calls, raw private maps or provisional chunks in crate `squad-store`, file `crates/squad-store/tests/event_pages.rs`. (Deferred from M1.P4.L5.T9.)
  - [x] M5.P10.L4.T11 — Test stop and clear fencing for every new work kind, foreign-key-safe deletion of all new conversational records, `CENSUS_PASSES` reset and retained daily accounting in crate `squad-store`, file `crates/squad-store/tests/clear.rs`, and crate `squad-core`, file `crates/squad-core/tests/router_fencing.rs`. (Deferred from M1.P4.L5.T10.)
  - [x] M5.P10.L4.T12 — Add the exact probe per-step projected-field rows to 02’s sole visibility table and verify real-store reads and deterministic blind-text redaction in `crates/squad-core/tests/isolation.rs`; preserve human transcript originals.
  - [x] M5.P10.L4.T13 — Mint anonymous proposal/check IDs at the proposal barrier and deliver one anonymous check bundle per prediction context, with no contributor event IDs or time order.
