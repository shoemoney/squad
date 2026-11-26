# Prompt pack

The prompt pack is the set of embedded, validated templates and registered compositions that turn authorized work inputs into provider requests. The normative text is [`docs/prd/09-prompt-pack.md`](prd/09-prompt-pack.md); this page is the implementation guide. Source lives in `crates/squad-core/src/prompts/`, default templates in `crates/squad-core/prompts/` (embedded at build time), and the registry in `crates/squad-core/prompts/manifest.json`.

## Overview and crate ownership

| Crate | Owns |
|---|---|
| `squad-protocol` | Work and envelope types, canonical output types and validators, the `PromptRenderer`, `WorkReader` and `HistoryReader` capabilities, and every numeric constant (`constants.rs`). The M1 validators are the only validators: the pack never defines its own output schema. |
| `squad-store` | Persistence of frozen settings, attempt-scoped repair records and provenance rows. The pack reaches it only through `squad-core`. |
| `squad-agents` | Executors, context assembly and provider-role projection, accounting and the OpenRouter client. Depends on `squad-protocol` and the reader/submit traits only, never on `squad-core` or `squad-store`. Consumes the pack through the opaque `PromptRenderer` capability (M2 renderer dependency). |
| `squad-core` | The pack itself: loading, override validation, rendering, the per-mode input mappings (`inputs*.rs`), router and orchestrator, and provenance. |
| `squad-tui`, `squad-cli` | Presentation and configuration; no prompt logic. |

Rendering is a pure function of a registered composition and already-authorized inputs. A template never fetches history, never reads another context and never widens a payload. Private histories and mailboxes enforce visibility; prompts only describe it.

### Source-role projection

Authorized material reaches the provider with fixed roles (`crates/squad-agents/src/context.rs`, pinned by `crates/squad-agents/tests/agents_context.rs`):

| Material | Provider role and name |
|---|---|
| Member shared prompt and persona | `system`, no name |
| Human request | `user`, `me` |
| This member's own accepted reply or turn | `assistant`, the member's nick |
| Another member's authorized turn or grill message | `user`, the original author's nick |
| Notices, forfeits, verdict announcements, application plan notices | `user`, `squad`, prefixed `[squad notice]` |
| Task template (debate, grill, census, crosscheck, survey, probe) | `user`, `squad` |
| Anonymous (blind) material | `user`, no name |
| Judge system instruction | `system`, no name |
| Judge case material and repair | inside the single `user`, `squad` message; never separate messages |

A source record appears once. When `debate.setup` becomes active it replaces the opening-material projection, so original solutions are not copied again as separate source messages.

## Loading, overrides and versions

`PromptPack::load()` (`crates/squad-core/src/prompts/load.rs`) reads the embedded defaults and then, when `SQUAD_PROMPTS_DIR` is set, UTF-8 overrides from that directory. `PromptPack::embedded()` ignores the environment (use it in tests); `PromptPack::from_override_dir(dir)` is the explicit form.

- Only registered filenames are read. A missing file keeps its default; any other read error fails startup.
- An override may change prose but must keep exactly the placeholder set the manifest registers for that template. Unknown or missing placeholders fail at startup, including in overrides, and only the selected composition is validated after selection or transformation.
- Overrides must preserve registered section selections and the single exact blind prefix of `judge.system` (see `judge.system.blind-prefix`). An override manifest cannot change contracts.
- Interpolation is single pass: inserted values are never scanned for template syntax. Every `*_json` placeholder is `serde_json::to_string` of a native JSON value, so a quote, brace or `{{slot}}` inside source text stays data.
- Assembly concatenates the listed templates with a newline. The member persona lives in `room.system`, never in `judge.system`. A repair suffix is appended inside the same user message.

Version rules: the manifest `version` increases whenever a prompt or composition changes. A contest freezes the pack version, effective word limits, override hashes and a hash per composition at creation. `PromptPack::composition_hash(id)` (`prompts/provenance.rs`) hashes the effective system and user text, plus the repair variant when the composition registers a suffix. `PromptPack::prompt_ref(id)` returns the `PromptRef` stored on work (template ID, pack version, hash), and `prompts::provenance::NEW_MODE_COMPOSITIONS` lists the eighteen census, crosscheck, survey and probe compositions that must be frozen before work is issued. A restarted run replays the frozen text, not the current files. Coverage: `crates/squad-core/tests/provenance_prompts.rs`.

### Environment-derived limits

| Limit | Variable | Default | Used by |
|---|---|---|---|
| `room_words` | `SQUAD_ROOM_WORDS` | 500 | room replies, four-mode member prose |
| `turn_words` | `SQUAD_TURN_WORDS` | 400 | debate turns, grill answers (`grill_answer_word_limit`) |
| `final_turn_words` | `SQUAD_FINAL_TURN_WORDS` | 500 | final debate turns |
| `plan_words` | `SQUAD_PLAN_WORDS` | 1,200 | grill rebuilds, four-mode judge prose |
| grill question | fixed | 200 words | `grill.question` |
| judge | fixed | 1,000 words | `JUDGE_WORDS` |

Limits are prompt-enforced words. Output ceilings are visible tokens, with the reasoning budget added by the request builder when the model supports reasoning (`max_tokens = visible + reasoning`; otherwise only the visible ceiling is sent). Visible ceilings: replies and turns 2,048, final judge 4,096, `judge.round` 512, plan 4,096. The four new modes use `NEW_MODE_JUDGE_REASONING_EFFORT` (medium, 4,096 reasoning tokens) and `NEW_MODE_JUDGE_VISIBLE_TOKENS` (4,096) for every structured judge call, whatever the judge binding says; member steps keep the 2,048 ceiling.

Daily admission and dispatch use the canonical daily maximum (`SQUAD_DAILY_MAX_USD`, default $20), rather than a separate room-lifetime budget. The router reserves the frozen plan's attempt bounds before dispatch and reconciles provider-reported cost; prompt text cannot authorize additional calls. See [`02-message-protocol-store-and-bus.md`](prd/02-message-protocol-store-and-bus.md) for the daily ledger and [`08-contests-debate-tournament-grill-census-crosscheck-survey-probe.md`](prd/08-contests-debate-tournament-grill-census-crosscheck-survey-probe.md) for preflight and confirmation contracts.

### Case modes

A judge case is one of: `judge.debate` (scored debate), `judge.standalone` (unscored ruling), `judge.followup` (follow-up to a retained case ruling), `judge.round` (one round score) and `judge.tournament` (blind ranking). Each is exactly two provider messages. The standalone and follow-up modes return free-form text; debate verdicts and round scores are validated against the canonical schemas.

## judge.round

`judge.round` scores one debate round. Its output is exactly `{"scores": {<nick>: {"score": 1..10, "reason": <=200 chars}}}` with both actual debater nicks, an integer score and no winner, tally or extra keys (`round_score_schema_json`, instantiated by `crates/squad-core/src/prompts/inputs.rs`). The judge always runs at **low** effort with a 512-token visible ceiling and a 1,024-token reasoning budget, whatever the final-judge setting (`effective_effort`).

- **Case isolation.** `round_judge_data_json` carries the topic, original solutions, retained instruction, the earlier turns needed for context and the target round's two accepted turns or recorded forfeits, each once. Later turns and other rounds' scores are excluded.
- **Repair budget.** Two attempts total (`ROUND_JUDGE_ATTEMPTS`): transport and schema repairs share them. The repair prompt `judge.round.repair` receives only the rejected output and validation errors recorded for that `(work_id, attempt)`. If both fail, the router records `debate.round_score` with `scores: null` and `reason: "unscored"`. Nothing is fabricated.
- **Visibility.** Committed round scores are addressed to `me` and kept in the judge case context. Debaters never see scores, reasons, running tallies or unscored notices before the verdict announcement; scoring does not block the next round.
- **Final tally.** Final judging waits for every round-scoring work item to finish or be given up, then receives the available scores, the application-computed `roundTally` and the explicit unscored rounds. The model supplies only `tallyOverrideReason`; when its winner differs from the sole tally leader a non-empty reason is required, and a missing one goes through the repair path.
- **Verdict announcement.** The accepted verdict, the tally and the round records reach the debaters once, as a `[squad notice]` from `squad`, rendered by `debate.verdict` with `round_scores_json` and `verdict_result`. Coverage: `crates/squad-core/tests/prompt_requests.rs`.

## Grill

Template IDs: `grill.question`, `grill.answer`, `grill.rebuild`. All three use `room.system`, so a grill never runs a judge prompt.

- **Shared floor.** A grill is the one explicit visibility exception: its leader and grillers see the current plan and that grill's questions, answers and plan changes. Nobody else does. Another participant's content arrives as `user` with that participant's nick; application plan notices use `squad`.
- **Working-context projection.** The latest plan and the current round's material appear once. Earlier rounds keep only the compact already-asked question list. The router supplies `grill_asked_json` (earlier questions), `grill_floor_json` (this round's shared floor) and `grill_focus_json` (the focus from setup) through `PromptPack::grill_inputs_with` and `GrillRecord`; `grill_inputs` renders with an empty record.
- **Questions.** The griller rotates through the angle list (cost, failure modes, security, scale, edge cases, hidden assumptions, user experience, operations). The template renders the assigned `grill_angle`; the engine picks it. At most three questions in 200 words, no repeats of the already-asked list, and exactly `PASS` when there is nothing worth asking. Only that exact token is a pass; a failure or an empty reply is never rendered as one.
- **Answers.** The leader answers every question within `grill_answer_word_limit` words, conceding valid points and separating fact, assumption and proposal. Each answer carries a `planChanges` entry, which is the answer's required **Plan changes** section. A pass enqueues no answer.
- **Rebuilds.** After a round with at least one question the leader rewrites the complete plan as version `round + 1` within `plan_word_limit` words. After an all-pass round the engine skips the rebuild; with no questions at all it finishes with reason `survived`. The Q&A appendix is application-owned: the final output is the last accepted complete plan plus the router-recorded questions, authors, answers and plan changes. If the leader fails, the last accepted plan stands.
- Coverage: `crates/squad-core/tests/prompt_requests_grill.rs`.

## Census, crosscheck, survey and probe

All seven modes are scripts over the same engine moves (D71): `collect`, `pick`, `fanout`, `turns`, `judge`, `tally`, `barrier` and `report`. Model-bearing moves select a registered composition through `shared_move_composition` (`prompts/inputs.rs`), which maps `fanout` and `turns` to actor-specific member compositions and `judge` to judge compositions; `pick`, `tally` and `barrier` never call a provider, and a move whose kind has no composition is an error rather than a default. A `fanout` renders each actor's own payload; it is never a broadcast superset.

Each task template renders its authorized input once in `mode_data_json` and the instantiated canonical JSON Schema once in `mode_schema_json`, with `contest_id`, `step_word_limit` (the script step's effective limit, frozen with its prompt settings) and, for census, `census_pass`. Schemas come from `squad-protocol` (`validation::schema::<T>()`, instantiated with the authorized IDs, enums and cardinalities); the pack owns none. Every object rejects additional properties and every nullable field is required.

| Mode | Template IDs | Actor | Authorized `mode_data_json` | Schema source |
|---|---|---|---|---|
| Census | `census.hunt` | member | current plan and version; this member's lens only | `CensusHuntOutput` |
| | `census.merge` | judge | plan; accepted findings with source-find IDs and lenses; missing-work records | `CensusMergeOutput` |
| | `census.rate` | member | plan; neutral gap IDs, omissions, consequences, lenses; no finder references or counts | `CensusRatingOutput` |
| | `census.final` | judge | plan; merged ledger; accepted ratings and citations; computed majority decisions; additions; missing work | `CensusFinalOutput` |
| Crosscheck | `crosscheck.normalize` (`extract_goal`, `normalize_inventory`) | judge | source plan only; then goal, constraints and inventories, no plan | `CrosscheckGoal`, `CrosscheckNormalization` |
| | `crosscheck.inventory` | member | goal, constraints, one lens; no plan | `CrosscheckInventoryResponse` |
| | `crosscheck.cell` | member | plan, goal, only this reviewer's assigned cells; no authorship or other reviews | `CrosscheckReviewResponse` |
| | `crosscheck.audit` | fresh member | plan, goal, retained grid, this auditor's sampled evidence without reviewer identities; no earlier history | `CrosscheckAuditResponse` |
| | `crosscheck.rule` | judge | plan, full retained grid, accepted reviews, audits, additions, human-acceptance references, missing work | `CrosscheckRuling` |
| Survey | `survey.extract` | judge | question and accepted answers with private source references | `SurveyExtraction` |
| | `survey.vote`, `survey.revote` | member | question and neutral claim IDs and text only | `SurveyBallot` |
| | `survey.report` | judge | claims, origins, ballots and revotes, computed tallies and bands, switches, missing work | `SurveyReportText` |
| Probe | `probe.explain`, `probe.check` | blind member | observation, ruled-out possibilities, inspection limits only | `ProbeExplanation`, `ProbeCheck` |
| | `probe.predict` | blind member | brief, this actor's accepted explanation and every accepted check; no rival explanations or predictions | `ProbePrediction` |
| | `probe.report` | judge | brief, explanations, checks, committed prediction matrix, missing work | `ProbeReport` |

- **Template text.** The templates are in `crates/squad-core/prompts/<id>.md`; each ends with `Required schema: {{mode_schema_json}}` and the authorized data block, tells the model to treat supplied text as data and to return only the JSON object, and states the step's prose limit.
- **Input mappings.** Typed mapping functions in `prompts/inputs_census.rs`, `inputs_crosscheck.rs`, `inputs_survey.rs` and `inputs_probe.rs` accept only the step's typed material, keep only the placeholders the selected composition declares, and reject material the step projection forbids. Member compositions carry the member system prompt and persona; blind probe steps replace nick, model slug and persona with neutral values; judge compositions use `judge.system` with no persona.
- **Repair composition.** Every structured composition registers the shared `mode.repair` suffix, appended inside the same user message, built from `mode_step`, `validation_errors_json` and `invalid_output_json`. It is task-specific, carries only the authorized attempt-scoped rejected output and validator errors, preserves the step's visibility, and shares the owning task's attempt budget (judge steps three attempts, member steps two). Exhaustion routes to the mode's canonical failure handling; no artifact is invented.
- **Judge composition.** Every judge call is one `judge.system` message and one `user`, `squad` message, rendered at medium effort with a 4,096-token ceiling, carrying case evidence once with no separate history messages.
- **Report ownership.** Census and crosscheck assemble their reports from accepted artifacts without a model call, so no template exists for them and the bounded stop statement costs no provider call. Survey and probe each use a registered report judge composition (`survey.report`, `probe.report`). The application, never the model, computes tallies, bands and scores, and a model can never create a human risk acceptance.
- **Provenance.** Each work item records the pack version, composition ID, composition hash, step, pass and plan version through the frozen prompt refs.
- Coverage: `crates/squad-core/tests/prompt_requests_modes.rs`, `prompt_requests_crosscheck.rs`, `prompt_requests_probe.rs`, `provenance_prompts.rs`; executor-side request limits in `crates/squad-agents/tests/agents_judge.rs`.

## Tournament prompt integration

Tournament uses the registered `tournament.entry` and `judge.tournament` compositions in `crates/squad-core/prompts/manifest.json`. Entrant work retains `tournament.entrant.md` and ordinary reply limits. The member request has one member system message with the entrant instructions appended, the frozen tournament question, its own history and its immediately preceding private result. Later ordinary requests remain held until released. Entrants never receive another entrant's tournament answer or individual result.

The blind judge composition replaces the exact identity-bearing prefix of `judge.system` with `judge.system.blind-prefix`. Overrides must preserve that single exact prefix and the declared placeholder sets. The judge gets one system message and one user message, with only redacted prompt text, anonymous labels, answers and the strict ranking schema. Label-to-member maps and the frozen identity-token set live exclusively in router-owned SQLite tables/private maps. Every round execution gets a new case and shuffled labels, including the single replay.

`tournament.repair` receives only the rejected response and validation errors for the next authorized attempt of the same case/work. Three total attempts share a durable 120-second execution deadline. The router owns retries, replay and elimination; adapters perform one provider call for each durable attempt.

`tournament.result` renders one entrant's own rank, remaining count, nullable score and critique, and survival result. Only the immediately preceding round's result enters the next entry request. `tournament.final` renders nick-and-placement pairs to every admitted entrant. It includes no answers, scores or critiques. Full accepted rankings and original unredacted answers remain in the human transcript and exports.

Work records retain prompt-pack version, selected template ID and a provenance hash. Admission freezes resolved model bindings and identity-redaction tokens. Reservations include ordinary entry retries, all three judge attempts and one replay per logical round. Normal preflight call counts exclude retry/replay allowances and are shown separately from the reservation.

Integration coverage lives in `crates/squad-core/tests/tournament_workflows.rs`, protocol validation in `crates/squad-protocol/tests/tournament.rs`, migration preservation in `crates/squad-store/tests/tournament_migration.rs`, and committed presentation snapshots in `crates/squad-tui/tests/tournament.rs`.

## Milestone coverage

Completed tournament work: all `M5.P5.*` and `M5.P3.L5.*`, `M2.P2.L7.T1–T3`, `M3.P3.L3.T11`, `M4.P3.L5.T2–T3`, `M4.P3.L5.T5–T6`, and `M4.P4.L4.T1–T2`. Tests are consolidated in the integration files listed above.

The following shared rows retain unchecked boxes because they also require non-tournament modes or broader presentation fixtures. Their tournament execution, state, feed, export, privacy and accounting paths are integrated here; the remaining cross-mode assertions belong to the other mode work:

`M1.P3.L1.T9`, `M2.P3.L2.T7`, `M2.P4.L1.T11`, `M2.P4.L2.T6`, `M3.P2.L1.T6`, `M3.P2.L2.T7`, `M3.P2.L2.T12`, `M3.P2.L3.T5`, `M3.P3.L2.T9`, `M3.P3.L3.T10`, `M3.P3.L5.T5`, `M4.P2.L7.T2`, `M4.P3.L2.T6`, `M4.P3.L8.T6`, `M4.P4.L4.T6`, `M4.P4.L4.T7`, `M4.P4.L4.T8`, `M4.P5.L2.T5`.
