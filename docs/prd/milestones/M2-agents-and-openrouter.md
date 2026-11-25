# M2 — Agents and OpenRouter

**Goal:** Execute isolated member, judge, tournament, and grill work with deterministic startup resolution, bounded context, recoverable streaming, attributable daily usage, and the shared prompt renderer. Execute census, crosscheck, survey, and probe Work through the same adapters, with per-member payload delivery and fresh crosscheck auditor contexts.

**Exit criteria:**

- The six default members and configurable rosters through 12 members validate concrete configured slugs and resolved alias targets; unknown concrete slugs produce startup errors naming their variables.
- Alias probes record concrete identities in the `models` table, completion requests use those identities until restart, and response-model mismatches reach diagnostics.
- The wiremock fixture demonstrates concurrent independent replies without foreign-history content in captured requests.
- Queue limits, ordered follow-ups, contest holds, cancellation, ordinary retries, regeneration, and restart fencing pass adapter integration tests.
- Judge fixtures prove fresh initial cases, isolated case follow-ups, and absence of room-member or unrelated-case history.
- Captured requests enforce resolved concrete slugs, personas, user name, per-member reasoning settings, separate visible and reasoning budgets, and context limits.
- Attempt accounting feeds daily totals and reconciles missing terminal usage or cost without duplicate charges or fabricated zero costs.
- No credential or reasoning text appears in captured feed events, persisted conversational content, or logs.
- The M2 suite passes under `cargo nextest`, and the affected crates pass `rustfmt` and `clippy -D warnings`.
- Round-score fixtures prove case-bound execution with low effort, a 512-token visible ceiling plus supported reasoning reservation, one provider call per authorized attempt, cancellation, and attributable usage.
- Captured debater requests contain no round scores before the verdict announcement and include the authorized score material when the verdict is delivered.
- `squad-agents` has no store or core dependency, and its compile-fail tests reject foreign-reader construction and direct database access.
- Judge requests contain one system prompt and one rendered user message, with evidence included once and repair data supplied through case-bound attempt records.
- The embedded prompt loader, manifest validation, overrides, and strict single-pass renderer work before M5.
- Tournament fixtures preserve entrant isolation and blind judge requests; grill fixtures use exactly the authorized shared-floor projection and owning result paths.
- Daily-max fixtures reject new calls while allowing already dispatched calls to finish and account for their actual cost.
- Every new-mode member and judge Work kind executes through the shared adapters with the owning template, schema, authorized payload, and result path.
- Census fixtures preserve blind hunts and anonymous rating ledgers; finder identities and counts appear only in authorized judge and human material.
- Crosscheck inventory requests contain no plan, cell reviewers receive only their assigned cells, and fresh auditor requests contain no prior member history or reviewer identity.
- Survey voting requests contain no other whole answers, authorship, or vote counts; revotes receive only the authorized anonymous reasons and claims.
- Probe fixtures preserve blind role-specific proposals and give each explainer its own explanation plus all checks without rival explanations or predictions.
- New-mode judge requests use medium effort, a visible ceiling no greater than 4,096 tokens, supported reasoning reservations, and strict schema validation through the owning path.
- Frozen payloads, auditor samples, context bindings, attempt allowances, and billing attribution survive cancellation and restart without extra executor-owned calls.

## Phase M2.P1 — Registry and resolution

- **M2.P1.L1 — Consume validated model configuration**
  - [x] M2.P1.L1.T1 — Consume the Foundations-owned `SquadConfig` in crate `squad-agents`, file `crates/squad-agents/src/registry.rs`, without defining another configuration schema.
  - [x] M2.P1.L1.T2 — Register the six default configured slugs in crate `squad-agents`, file `crates/squad-agents/src/registry.rs`.
  - [x] M2.P1.L1.T3 — Test arbitrary slugs, repeated models under distinct nicks, personas, and roster bounds in crate `squad-agents`, file `crates/squad-agents/tests/agents_registry.rs`.
  - [x] M2.P1.L1.T4 — Test unsupported reasoning parameters and effective visible output ceilings with separate reasoning reservations in crate `squad-agents`, file `crates/squad-agents/tests/agents_registry.rs`.
  - [x] M2.P1.L1.T5 — Implement per-member reasoning precedence, omission of implicit defaults for unsupported models, explicit unsupported-value errors, and low round-score effort in crate `squad-agents`, file `crates/squad-agents/src/registry.rs`.
  - [x] M2.P1.L1.T6 — Test low, medium, and high reasoning budgets, complete request caps, published output limits, and rejection of requests that cannot fit their reservation in crate `squad-agents`, file `crates/squad-agents/tests/agents_registry.rs`.
  - [x] M2.P1.L1.T7 — Apply and test medium effort and a 4,096-token visible ceiling for census, crosscheck, survey, and probe judge tasks independently of the final-judge effort setting in crate `squad-agents`, file `crates/squad-agents/tests/agents_registry.rs`.

- **M2.P1.L2 — Resolve and cache concrete identities**
  - [x] M2.P1.L2.T1 — Implement shared catalog acquisition and concrete-slug validation with variable-naming startup errors in crate `squad-agents`, file `crates/squad-agents/src/resolver.rs`.
  - [x] M2.P1.L2.T2 — Implement 16-token alias probes with the lowest accepted reasoning setting, one HTTP 400 retry at 2,048 tokens, and response-model validation in crate `squad-agents`, file `crates/squad-agents/src/resolver.rs`.
  - [x] M2.P1.L2.T3 — Test exact concrete-slug preservation and alias identity capture without requiring aliases in the catalog in crate `squad-agents`, file `crates/squad-agents/tests/agents_resolver.rs`.
  - [x] M2.P1.L2.T4 — Submit catalog metadata and member and judge resolution snapshots through router-owned atomic persistence interfaces in crate `squad-agents`, file `crates/squad-agents/src/resolver.rs`.
  - [x] M2.P1.L2.T5 — Test unknown concrete slugs, failed probes, the three-failure cache policy, the 24-hour cache limit, fixed startup identities, and diagnostic logs in crate `squad-agents`, file `crates/squad-agents/tests/agents_resolver.rs`.
  - [x] M2.P1.L2.T6 — Test that every completion uses the pinned concrete slug and every attempt records its response model, with mismatch warnings exposed through diagnostics, in crate `squad-agents`, file `crates/squad-agents/tests/agents_resolver.rs`.

- **M2.P1.L3 — Provide a fake OpenRouter service**
  - [x] M2.P1.L3.T1 — Implement deterministic catalog and streaming fixtures with wiremock in crate `squad-agents`, file `crates/squad-agents/tests/support/fake_openrouter.rs`.
  - [x] M2.P1.L3.T2 — Add alias-probe responses, generation lookups, and captured request bodies in crate `squad-agents`, file `crates/squad-agents/tests/support/fake_openrouter.rs`.
  - [x] M2.P1.L3.T3 — Add malformed, delayed, disconnected, structured-judge, and free-form-case fixtures in crate `squad-agents`, file `crates/squad-agents/tests/support/fake_openrouter.rs`.
  - [x] M2.P1.L3.T4 — Add valid, tied, malformed, delayed, and failed round-score responses in crate `squad-agents`, file `crates/squad-agents/tests/support/fake_openrouter.rs`.
  - [x] M2.P1.L3.T5 — Add blind tournament rankings, repair outputs, grill questions, exact passes, answers, and rebuilds in crate `squad-agents`, file `crates/squad-agents/tests/support/fake_openrouter.rs`.
  - [x] M2.P1.L3.T6 — Add missing-cost, billed-failure, billed-cancellation, delayed-cost, and response-model-mismatch fixtures in crate `squad-agents`, file `crates/squad-agents/tests/support/fake_openrouter.rs`.
  - [x] M2.P1.L3.T7 — Add census, crosscheck, survey, and probe member and judge outputs, optional goal extraction, malformed schemas, delayed calls, failed members, and repair exhaustion in crate `squad-agents`, file `crates/squad-agents/tests/support/fake_openrouter.rs`.

## Phase M2.P2 — Private workers and context

- **M2.P2.L1 — Execute durable addressed work**
  - [x] M2.P2.L1.T1 — Spawn one tokio task per member using Protocol’s canonical `AgentContext` bound to `ExecutorKey::Member`, enumerating that member’s ordinary and assigned step work across contexts in crate `squad-agents`, file `crates/squad-agents/src/worker.rs`.
  - [ ] M2.P2.L1.T2 — Implement FIFO queues, queue limits, and work deduplication in crate `squad-agents`, file `crates/squad-agents/src/worker.rs`.
  - [x] M2.P2.L1.T3 — Test ordered follow-ups and trigger-bounded context in crate `squad-agents`, file `crates/squad-agents/tests/agents_worker.rs`.
  - [x] M2.P2.L1.T4 — Test contest drain and hold signals with controlled scheduler fixtures in crate `squad-agents`, file `crates/squad-agents/tests/agents_worker.rs`.
  - [x] M2.P2.L1.T5 — Test pending-work recovery and missed mailbox wake-ups using temporary SQLite databases in crate `squad-core`, file `crates/squad-core/tests/agents_worker.rs`.
  - [ ] M2.P2.L1.T6 — Test deferred held-message history writes and reuse of the round’s frozen boundary across debate attempts in crate `squad-core`, file `crates/squad-core/tests/agents_worker.rs`.
  - [x] M2.P2.L1.T7 — Test capacity-1 wake-up coalescing and progress while ingress is full, without the router awaiting an agent channel, in crate `squad-core`, file `crates/squad-core/tests/agents_worker.rs`.
  - [x] M2.P2.L1.T8 — Test that one-shot opens do not resume unrelated work and that interactive opens recover pending accepted work in crate `squad-core`, file `crates/squad-core/tests/agents_worker.rs`.
  - [ ] M2.P2.L1.T9 — Execute canonical per-member `fanout` payloads without reading another assignment or broadcasting a shared payload superset in crate `squad-agents`, file `crates/squad-agents/src/worker.rs`.

- **M2.P2.L2 — Assemble bounded prompts**
  - [x] M2.P2.L2.T1 — Implement speaker roles and trusted metadata labels in crate `squad-agents`, file `crates/squad-agents/src/context.rs`.
  - [x] M2.P2.L2.T2 — Integrate Prompt pack outputs, roster identity, `SQUAD_USER_NAME`, and member personas through the injected renderer in crate `squad-agents`, file `crates/squad-agents/src/context.rs`.
  - [x] M2.P2.L2.T3 — Implement token estimation, visible and reasoning output reservation, and whole-exchange removal in crate `squad-agents`, file `crates/squad-agents/src/context.rs`.
  - [x] M2.P2.L2.T4 — Test mandatory anchors, oversized case material, and notice roles in crate `squad-agents`, file `crates/squad-agents/tests/agents_context.rs`.
  - [x] M2.P2.L2.T5 — Test author-labelled quotes and verify that context assembly uses only context-bound readers in crate `squad-agents`, file `crates/squad-agents/tests/agents_context.rs`.
  - [ ] M2.P2.L2.T6 — Render accumulated judge case turns and own round scores once in a single user message through canonical Work boundaries, with visible and reasoning reservations, in crate `squad-agents`, file `crates/squad-agents/src/context.rs`.
  - [ ] M2.P2.L2.T7 — Test that neither debater context contains round scores or tallies before the verdict, and that the verdict announcement supplies the authorized score material, in crate `squad-agents`, file `crates/squad-agents/tests/agents_context.rs`.
  - [ ] M2.P2.L2.T8 — Test mandatory accumulated round scores, unscored-round notices, and verdict announcement material against context budgets in crate `squad-agents`, file `crates/squad-agents/tests/agents_context.rs`.
  - [ ] M2.P2.L2.T9 — Add compile-fail tests rejecting foreign-reader construction, event access, and direct store access in crate `squad-agents`, file `crates/squad-agents/tests/isolation_compile_fail.rs`.
  - [ ] M2.P2.L2.T10 — Test `debate.setup.md` and `debate.opening.md` role mappings and judge requests containing exactly one system message and one rendered user message with evidence once in crate `squad-agents`, file `crates/squad-agents/tests/agents_context.rs`.
  - [ ] M2.P2.L2.T11 — Test repair-context budgeting with rejected output and validation errors supplied through attempt-scoped records in crate `squad-agents`, file `crates/squad-agents/tests/agents_context.rs`.
  - [ ] M2.P2.L2.T12 — Consume step-bound readers and mandatory per-member projections without falling back to ordinary running history in crate `squad-agents`, file `crates/squad-agents/src/context.rs`.
  - [ ] M2.P2.L2.T13 — Test anonymised payload bodies, provider names, serialized placeholders, and mandatory-material overflow without dropping entries or creating extra calls in crate `squad-agents`, file `crates/squad-agents/tests/agents_context.rs`.

- **M2.P2.L3 — Execute isolated judge cases**
  - [ ] M2.P2.L3.T1 — Consume canonical judge Work through a case-bound `AgentContext` in crate `squad-agents`, file `crates/squad-agents/src/judge.rs`.
  - [ ] M2.P2.L3.T2 — Implement debate-verdict, standalone-ruling, and case-follow-up execution modes in crate `squad-agents`, file `crates/squad-agents/src/judge.rs`.
  - [ ] M2.P2.L3.T3 — Test fresh cases, retained case follow-ups, foreign-case exclusion, and Fable-history canaries in crate `squad-agents`, file `crates/squad-agents/tests/agents_judge.rs`.
  - [ ] M2.P2.L3.T4 — Verify default high reasoning effort, absent member personas, and one provider call per authorized judge attempt in crate `squad-agents`, file `crates/squad-agents/tests/agents_judge.rs`.
  - [ ] M2.P2.L3.T5 — Execute canonical `judge.round` Work with Prompt pack’s `judge.round.md`, the judge system prompt, low effort, a 1,024-token supported reasoning budget, and a 512-token visible ceiling in crate `squad-agents`, file `crates/squad-agents/src/judge.rs`.
  - [ ] M2.P2.L3.T6 — Test round-score case isolation, round identity, low effort despite a different final-judge setting, absent personas, and one provider call per authorized attempt in crate `squad-agents`, file `crates/squad-agents/tests/agents_judge.rs`.
  - [ ] M2.P2.L3.T7 — Test executor consumption of orchestrator-authorized round-score repair attempts within the two-attempt allowance, and final prompt fixtures containing existing scores and unscored rounds, in crate `squad-agents`, file `crates/squad-agents/tests/agents_judge.rs`.
  - [ ] M2.P2.L3.T8 — Read rejected output and validation errors only from case-bound `(work_id, attempt)` repair records and render the owning repair template in crate `squad-agents`, file `crates/squad-agents/src/judge.rs`.
  - [ ] M2.P2.L3.T9 — Spawn, recover, reuse, and retire case-bound executors through router-owned lifecycle management in crate `squad-core`, file `crates/squad-core/src/router/cases.rs`.
  - [x] M2.P2.L3.T10 — Test final-judge and round-score deadlines, recovered repair records, foreign-case repair exclusion, and rejected-output absence from conversational history in crate `squad-core`, file `crates/squad-core/tests/agents_judge.rs`.
  - [ ] M2.P2.L3.T11 — Execute the four new modes’ structured judge Work through case-bound readers, medium effort, the 4,096-token visible ceiling, and owning validation and repair paths in crate `squad-agents`, file `crates/squad-agents/src/judge.rs`.
  - [ ] M2.P2.L3.T12 — Test new-mode judge evidence inclusion once, absent member personas and unrelated history, supported reasoning reservations, and one call per authorized attempt in crate `squad-agents`, file `crates/squad-agents/tests/agents_judge.rs`.

- **M2.P2.L4 — Execute retries and regenerated replies**
  - [x] M2.P2.L4.T1 — Handle router-enqueued replacement reply Work in crate `squad-agents`, file `crates/squad-agents/src/worker.rs`.
  - [ ] M2.P2.L4.T2 — Test superseded-reply exclusion and unchanged quote and case snapshots in crate `squad-agents`, file `crates/squad-agents/tests/agents_context.rs`.
  - [ ] M2.P2.L4.T3 — Test user-requested retry work without duplicating the human message in crate `squad-agents`, file `crates/squad-agents/tests/agents_worker.rs`.

- **M2.P2.L5 — Embed and register Prompt pack templates**
  - [x] M2.P2.L5.T1 — Embed Prompt pack’s root `prompts/room.system.md` and `prompts/room.user.md` in crate `squad-core`, file `crates/squad-core/src/prompts/mod.rs`.
  - [x] M2.P2.L5.T2 — Embed root `prompts/debate.round0.md`, `prompts/debate.round1.md`, `prompts/debate.rebuttal.md`, and `prompts/debate.closing.md` in crate `squad-core`, file `crates/squad-core/src/prompts/mod.rs`.
  - [x] M2.P2.L5.T3 — Embed root `prompts/judge.system.md`, `prompts/judge.user.md`, `prompts/judge.repair.md`, `prompts/judge.standalone.md`, and `prompts/judge.followup.md` in crate `squad-core`, file `crates/squad-core/src/prompts/mod.rs`.
  - [x] M2.P2.L5.T4 — Embed root `prompts/squad.notice.md`, `prompts/debate.forfeit.md`, and `prompts/debate.verdict.md` in crate `squad-core`, file `crates/squad-core/src/prompts/mod.rs`.
  - [x] M2.P2.L5.T5 — Load and embed template IDs, required placeholders, composition rules, and pack version from root `prompts/manifest.json` in crate `squad-core`, file `crates/squad-core/src/prompts/mod.rs`.
  - [x] M2.P2.L5.T6 — Embed root `prompts/judge.round.md` and register its target-round placeholders and composition in crate `squad-core`, file `crates/squad-core/src/prompts/mod.rs`.
  - [x] M2.P2.L5.T7 — Load the Prompt pack’s round-scoring system mode, final score evidence, `tallyOverrideReason` guidance, and verdict-announcement composition with its pack version in crate `squad-core`, file `crates/squad-core/src/prompts/mod.rs`.
  - [x] M2.P2.L5.T8 — Embed and register root `prompts/debate.setup.md` and `prompts/debate.opening.md` with their user-role composition in crate `squad-core`, file `crates/squad-core/src/prompts/mod.rs`.
  - [x] M2.P2.L5.T9 — Embed and register root `prompts/census.hunt.md`, `prompts/census.merge.md`, `prompts/census.rate.md`, and `prompts/census.final.md` with Prompt pack’s placeholders, schemas, and composition in crate `squad-core`, file `crates/squad-core/src/prompts/mod.rs`.
  - [x] M2.P2.L5.T10 — Embed and register root `prompts/crosscheck.inventory.md`, `prompts/crosscheck.normalize.md`, `prompts/crosscheck.cell.md`, `prompts/crosscheck.audit.md`, and `prompts/crosscheck.rule.md` with Prompt pack’s placeholders, schemas, and composition in crate `squad-core`, file `crates/squad-core/src/prompts/mod.rs`.
  - [x] M2.P2.L5.T11 — Embed and register root `prompts/survey.extract.md`, `prompts/survey.vote.md`, `prompts/survey.revote.md`, and `prompts/survey.report.md` with Prompt pack’s placeholders, schemas, and composition in crate `squad-core`, file `crates/squad-core/src/prompts/mod.rs`.
  - [x] M2.P2.L5.T12 — Embed and register root `prompts/probe.explain.md`, `prompts/probe.check.md`, `prompts/probe.predict.md`, and `prompts/probe.report.md` with Prompt pack’s placeholders, schemas, and composition in crate `squad-core`, file `crates/squad-core/src/prompts/mod.rs`.

- **M2.P2.L6 — Render and load configured prompts**
  - [x] M2.P2.L6.T1 — Implement strict single-pass rendering and `serde_json` serialization in crate `squad-core`, file `crates/squad-core/src/prompts/render.rs`.
  - [x] M2.P2.L6.T2 — Implement embedded-default loading, `SQUAD_PROMPTS_DIR` overrides, and startup placeholder validation in crate `squad-core`, file `crates/squad-core/src/prompts/load.rs`.
  - [x] M2.P2.L6.T3 — Provide the opaque prompt-rendering capability to `squad-agents` without a reverse crate dependency in crate `squad-core`, file `crates/squad-core/src/prompts/mod.rs`.
  - [x] M2.P2.L6.T4 — Test embedded defaults, partial overrides, unknown and missing placeholders, single-pass insertion, and one-message judge composition using contract fixtures in crate `squad-core`, file `crates/squad-core/src/prompts/tests.rs`.
  - [x] M2.P2.L6.T5 — Test all four modes’ template registration, partial overrides, strict placeholder validation, and schema-preserving payload serialization in crate `squad-core`, file `crates/squad-core/src/prompts/tests.rs`.

- **M2.P2.L7 — Execute tournament and grill work**
  - [ ] M2.P2.L7.T1 — Execute reply-like tournament entry Work with contest identity, ordinary output limits, and own-result inputs through the injected Prompt pack renderer in crate `squad-agents`, file `crates/squad-agents/src/worker.rs`.
  - [ ] M2.P2.L7.T2 — Execute `judge.tournament` with blind case material, the owning ranking and repair templates, a 120-second deadline, and one provider call per authorized attempt in crate `squad-agents`, file `crates/squad-agents/src/judge.rs`.
  - [ ] M2.P2.L7.T3 — Test that entrant requests contain only their own tournament answers and results and that judge requests contain no entrant nicks, model names, personas, roster, or label map in crate `squad-agents`, file `crates/squad-agents/tests/agents_tournament.rs`.
  - [ ] M2.P2.L7.T4 — Execute `grill.question`, `grill.answer`, and `grill.rebuild` through their owning templates, word limits, output reservations, and result paths in crate `squad-agents`, file `crates/squad-agents/src/worker.rs`.
  - [ ] M2.P2.L7.T5 — Assemble the latest plan, current-round shared floor, assigned angle, and earlier-question list without earlier full answers in crate `squad-agents`, file `crates/squad-agents/src/context.rs`.
  - [ ] M2.P2.L7.T6 — Test grill attribution, exact `PASS`, answer and rebuild limits, frozen retry inputs, and absence of executor-owned judging or scheduling in crate `squad-agents`, file `crates/squad-agents/tests/agents_grill.rs`.
  - [ ] M2.P2.L7.T7 — Test controlled router fixtures delivering grill content to exactly the participants and retaining full history while supplying the round working projection in crate `squad-core`, file `crates/squad-core/tests/agents_grill.rs`.
  - [ ] M2.P2.L7.T8 — Test question-first collection as ordinary mutually invisible replies, continued chat while a pick is pending, and retention of collected answers after cancellation in crate `squad-core`, file `crates/squad-core/tests/agents_worker.rs`.

- **M2.P2.L8 — Execute census work**
  - [x] M2.P2.L8.T1 — Execute census hunt and rating Work with the assigned lens, frozen plan version, anonymous ledger, and owning result schemas in crate `squad-agents`, file `crates/squad-agents/src/worker.rs`.
  - [x] M2.P2.L8.T2 — Execute census merge and finalization Work through the case-bound judge adapter and owning templates in crate `squad-agents`, file `crates/squad-agents/src/judge.rs`.
  - [x] M2.P2.L8.T3 — Capture requests to verify blind hunts, the eight-gap bound, anonymous rating inputs without finder counts, covered citations, and no executor-computed authoritative estimate in crate `squad-agents`, file `crates/squad-agents/tests/agents_census.rs`.
  - [x] M2.P2.L8.T4 — Test committed per-member delivery, patched plan versions between passes, frozen retry inputs, and exclusion of the human’s provenance table from member contexts in crate `squad-core`, file `crates/squad-core/tests/agents_census.rs`.

- **M2.P2.L9 — Execute crosscheck work and fresh audits**
  - [ ] M2.P2.L9.T1 — Execute crosscheck inventory and cell-review Work with only the authorized goal, constraints, lens, plan, and assigned-cell projection in crate `squad-agents`, file `crates/squad-agents/src/worker.rs`.
  - [ ] M2.P2.L9.T2 — Execute crosscheck normalization, optional goal-and-constraints extraction, and audit-ruling Work through the case-bound judge adapter in crate `squad-agents`, file `crates/squad-agents/src/judge.rs`.
  - [ ] M2.P2.L9.T3 — Construct and recover fresh audit-step `AgentContext` capabilities with no ordinary, inventory, review, or prior audit history while retaining member execution and accounting identity in crate `squad-core`, file `crates/squad-core/src/router/cases.rs`.
  - [ ] M2.P2.L9.T4 — Execute audit Work from the committed plan, grid, and frozen sample without reviewer identity, resampling, or executor-selected replacements in crate `squad-agents`, file `crates/squad-agents/src/worker.rs`.
  - [ ] M2.P2.L9.T5 — Capture inventory, review, audit, and ruling requests to verify plan blindness, assigned-cell limits, anonymous evidence, and one call per authorized batch in crate `squad-agents`, file `crates/squad-agents/tests/agents_crosscheck.rs`.
  - [ ] M2.P2.L9.T6 — Test plan-bearing quote and ordinary-history canaries, exclusion of self-authored inventory cells, fresh auditor readers, frozen samples after recovery, and the one-active-generation rule in crate `squad-core`, file `crates/squad-core/tests/agents_crosscheck.rs`.

- **M2.P2.L10 — Execute survey work**
  - [ ] M2.P2.L10.T1 — Execute optional ordinary collection and survey vote and revote Work through the owning templates and result paths in crate `squad-agents`, file `crates/squad-agents/src/worker.rs`.
  - [ ] M2.P2.L10.T2 — Execute survey extraction and report Work with private case provenance and application-supplied tallies in crate `squad-agents`, file `crates/squad-agents/src/judge.rs`.
  - [ ] M2.P2.L10.T3 — Capture requests to verify the claim bound, vote enums, confidence range, anonymous reasons, and absence of other whole answers, authorship, and vote counts from voting contexts in crate `squad-agents`, file `crates/squad-agents/tests/agents_survey.rs`.
  - [ ] M2.P2.L10.T4 — Test reuse of an existing parallel answer set without replacement calls, step-bound voting contexts, frozen revote inputs, and no broadcast of private report provenance in crate `squad-core`, file `crates/squad-core/tests/agents_survey.rs`.

- **M2.P2.L11 — Execute probe work**
  - [ ] M2.P2.L11.T1 — Execute probe explanation, check, and prediction Work using the committed role assignment and owning templates in crate `squad-agents`, file `crates/squad-agents/src/worker.rs`.
  - [ ] M2.P2.L11.T2 — Execute probe report Work from the locked explanations, checks, and prediction matrix through the case-bound judge adapter in crate `squad-agents`, file `crates/squad-agents/src/judge.rs`.
  - [ ] M2.P2.L11.T3 — Capture requests to verify mutually blind initial proposals, own-explanation-plus-all-checks prediction inputs, explicit inability to predict, and no rival explanation or prediction leakage in crate `squad-agents`, file `crates/squad-agents/tests/agents_probe.rs`.
  - [ ] M2.P2.L11.T4 — Test committed role delivery, no generation for the member sitting out, frozen prediction inputs, and absence of executor-run checks or role reshuffling in crate `squad-core`, file `crates/squad-core/tests/agents_probe.rs`.

## Phase M2.P3 — OpenRouter execution

- **M2.P3.L1 — Stream and submit responses**
  - [x] M2.P3.L1.T1 — Implement `reqwest` streaming requests and fragmented SSE parsing in crate `squad-agents`, file `crates/squad-agents/src/openrouter.rs`.
  - [x] M2.P3.L1.T2 — Submit canonical attempt-scoped deltas through ingress in crate `squad-agents`, file `crates/squad-agents/src/openrouter.rs`.
  - [x] M2.P3.L1.T3 — Submit ordinary `agent.reply` results and await router acknowledgement in crate `squad-agents`, file `crates/squad-agents/src/worker.rs`.
  - [ ] M2.P3.L1.T4 — Test finish reasons, terminal usage, duplicate result submissions, and reasoning-text exclusion in crate `squad-agents`, file `crates/squad-agents/tests/agents_openrouter.rs`.
  - [x] M2.P3.L1.T5 — Send resolved concrete slugs, capability-gated reasoning parameters, explicit reasoning budgets, and complete output reservations, and capture response model identity, in crate `squad-agents`, file `crates/squad-agents/src/openrouter.rs`.
  - [ ] M2.P3.L1.T6 — Submit each new-mode artifact through its owning typed result and validation path without treating partial deltas as committed output in crate `squad-agents`, file `crates/squad-agents/src/worker.rs`.

- **M2.P3.L2 — Bound failures and cancellation**
  - [x] M2.P3.L2.T1 — Implement constant ordinary deadlines, one automatic retry, jitter, and `Retry-After` in crate `squad-agents`, file `crates/squad-agents/src/openrouter.rs`.
  - [x] M2.P3.L2.T2 — Use `CancellationToken` to stop active HTTP requests, response streams, retry timers, and queued work in crate `squad-agents`, file `crates/squad-agents/src/worker.rs`.
  - [ ] M2.P3.L2.T3 — Test higher-attempt replacement after partial output and the absence of hidden debate retries in crate `squad-agents`, file `crates/squad-agents/tests/agents_openrouter.rs`.
  - [x] M2.P3.L2.T4 — Test cancellation versus result acceptance and rejection of stale attempts in crate `squad-core`, file `crates/squad-core/tests/agents_worker.rs`.
  - [x] M2.P3.L2.T5 — Test restart recovery with consumed interrupted attempts and no duplicate committed replies in crate `squad-core`, file `crates/squad-core/tests/agents_worker.rs`.
  - [ ] M2.P3.L2.T6 — Test cancellation of queued and active `judge.round` calls, rejection of late results, consumed interrupted attempts, and absence of adapter-owned retries in crate `squad-core`, file `crates/squad-core/tests/agents_judge.rs`.
  - [ ] M2.P3.L2.T7 — Test tournament and grill cancellation, attempt exhaustion, and preservation of billing information after result fencing in crate `squad-core`, file `crates/squad-core/tests/agents_integration.rs`.
  - [ ] M2.P3.L2.T8 — Test all four new modes’ cancellation, late-result rejection, interrupted-attempt consumption, frozen payload recovery, and absence of hidden retries or repairs in crate `squad-core`, file `crates/squad-core/tests/agents_integration.rs`.
  - [ ] M2.P3.L2.T9 — Test member failure returned for an engine-owned skip and judge exhaustion returned through the owning failure path without executor-owned state transitions in crate `squad-agents`, file `crates/squad-agents/tests/agents_worker.rs`.

## Phase M2.P4 — Accounting and integration

- **M2.P4.L1 — Report operational state and costs**
  - [x] M2.P4.L1.T1 — Implement presence transitions, queue depth, and worker heartbeats in crate `squad-agents`, file `crates/squad-agents/src/worker.rs`.
  - [x] M2.P4.L1.T2 — Implement idempotent provider-call accounting through router-owned persistence in crate `squad-agents`, file `crates/squad-agents/src/accounting.rs`.
  - [x] M2.P4.L1.T3 — Implement generation-ID usage and missing-cost reconciliation in crate `squad-agents`, file `crates/squad-agents/src/accounting.rs`.
  - [ ] M2.P4.L1.T4 — Test retries, probes, judge charges, unknown costs, estimates, and repeated reconciliation in crate `squad-agents`, file `crates/squad-agents/tests/agents_accounting.rs`.
  - [x] M2.P4.L1.T5 — Test daily dispatch reservation checks and rejection of queued and subsequent calls while in-flight calls finish through bus fixtures in crate `squad-core`, file `crates/squad-core/tests/agents_worker.rs`.
  - [ ] M2.P4.L1.T6 — Test round-score charges by debate, round, work, and attempt, including repair, cancellation, unknown usage, and repeated reconciliation, in crate `squad-agents`, file `crates/squad-agents/tests/agents_accounting.rs`.
  - [ ] M2.P4.L1.T7 — Test round-score dispatch reservation checks for both permitted attempts and daily-max rejection without canceling already dispatched calls in crate `squad-core`, file `crates/squad-core/tests/agents_judge.rs`.
  - [ ] M2.P4.L1.T8 — Capture actual reported costs, response model identity, missing-cost status, and billed failure or cancellation outcomes for every new Work kind in crate `squad-agents`, file `crates/squad-agents/src/accounting.rs`.
  - [x] M2.P4.L1.T9 — Test atomic attempt-event and `spend_daily` updates, pending-to-actual replacement, unknown-call counts, and idempotent reconciliation with temporary SQLite databases in crate `squad-core`, file `crates/squad-core/tests/agents_accounting.rs`.
  - [x] M2.P4.L1.T10 — Test shared daily totals, local-day rollover, a raised daily max, and `DAILY_MAX_REACHED` with the configured-amount message in crate `squad-core`, file `crates/squad-core/tests/agents_accounting.rs`.
  - [ ] M2.P4.L1.T11 — Test tournament, grill, question-first collection, retry, and probe charges against daily reservations, including billed canceled calls and uncertain charges retained after restart, in crate `squad-core`, file `crates/squad-core/tests/agents_accounting.rs`.
  - [ ] M2.P4.L1.T12 — Attribute census, crosscheck, survey, and probe calls by contest, step, pass, execution identity, work, and attempt, including optional extraction and fresh audits, in crate `squad-agents`, file `crates/squad-agents/src/accounting.rs`.
  - [ ] M2.P4.L1.T13 — Test new-mode reservations, repairs, skipped-member billed failures, canceled audits, unknown costs, and reconciliation after restart without duplicate charges in crate `squad-core`, file `crates/squad-core/tests/agents_accounting.rs`.

- **M2.P4.L2 — Verify complete executor paths**
  - [x] M2.P4.L2.T1 — Add six-member and maximum-roster parallel-streaming scenarios in crate `squad-core`, file `crates/squad-core/tests/agents_integration.rs`.
  - [ ] M2.P4.L2.T2 — Capture provider requests for all judge execution modes in crate `squad-core`, file `crates/squad-core/tests/agents_integration.rs`.
  - [x] M2.P4.L2.T3 — Verify sentinel-key and reasoning-text absence from feed events, database records, and logs in crate `squad-core`, file `crates/squad-core/tests/agents_integration.rs`.
  - [ ] M2.P4.L2.T4 — Document environment configuration references, timeout constants, resolution diagnostics, and uncertain-charge recovery in crate `squad-agents`, file `crates/squad-agents/src/lib.rs`.
  - [ ] M2.P4.L2.T5 — Capture round-score and final-verdict requests with controlled scheduler fixtures, verifying continued debater execution during delayed scoring and score visibility only with the verdict, in crate `squad-core`, file `crates/squad-core/tests/agents_integration.rs`.
  - [ ] M2.P4.L2.T6 — Capture tournament and grill requests with controlled scheduler fixtures to verify owning Work kinds, prompt-rendering capabilities, visibility boundaries, attempt accounting, and absence of adapter-owned contest transitions in crate `squad-core`, file `crates/squad-core/tests/agents_integration.rs`.
  - [ ] M2.P4.L2.T7 — Capture every census, crosscheck, survey, and probe executor path with controlled script fixtures to verify template selection, per-member delivery, judge composition, frozen retries, and no executor-owned scheduling or tallies in crate `squad-core`, file `crates/squad-core/tests/agents_integration.rs`.
  - [ ] M2.P4.L2.T8 — Verify durable recovery of step-bound readers and fresh auditors with plan, provenance, vote-count, and rival-prediction canaries while preserving ordinary member histories in crate `squad-core`, file `crates/squad-core/tests/agents_integration.rs`.

