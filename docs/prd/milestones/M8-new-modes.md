# M8 — New modes (nine)

**Goal:** Add `$forge`, `$forecast`, `$decide`, `$negotiate`, `$blow_it_up`, `$readback`, `$debug`, `$trace` and `$rank` as scripts over the shared contest engine, exactly as specified in [New modes](../11-new-modes.md), with one registry scaffold landing first so nine mode workers can build in separate files.

**Exit criteria:**

- The scaffold lands every shared registry change in **New modes → Registry changes and scaffold**; all 166 existing parser fixtures and their snapshots are unchanged, and every existing suite passes with all nine `ENABLED` switches false.
- No mode phase edits a file outside its ownership list. Shared-file needs found during a mode phase are routed back to the orchestrator.
- Each mode's normal call count and reserved attempts match the **Call counts at a glance** table for six members and for its minimum member count.
- Every aggregate (Borda, pooled odds and spread, rubric weights, totals, leave-one-out leaders, flip thresholds, challenge tallies, severity ranking, elimination, claim status, Bradley-Terry strengths, cycles) is computed by application code from committed records and matches hand-computed fixtures. No judge schema has a field that could override one.
- Blind steps deliver only the authorized projection: captured requests contain no nick, slug, persona, vote count, finder count, hidden brief, withheld number or own-work item where the step forbids it.
- The human step enters `AWAITING_HUMAN`, releases holds, keeps the claim, never expires, survives restart, and accepts `/answer`, readback choice tokens and `/continue` with the exact parser strings.
- Every quote that a step requires to be verbatim is checked by the shared matcher; `$trace` never repairs a missing quote and `$debug` never treats an unrun test as evidence.
- Member steps get two attempts with one repair after invalid output; judge steps get three; skipped members are named with their failed step in every report.
- Each mode stops cleanly at every state, recovers at every state without rerunning an accepted step or redrawing private randomness, and exports through `/export <mode> [id]`.
- Typeahead, TIP rotation, `/help` and the README list each mode only once its switch is true; MCP tools are unchanged (D87).
- One review pass covers the milestone, as D63 requires.

Heavy builds and full test suites for this milestone run on dev2 or dev3, never on dev1.

## Phase M8.P1 — Registry scaffold

One worker. Nothing in P2 to P10 starts until this phase is merged.

- **M8.P1.L1 — Protocol registries**
  - [x] M8.P1.L1.T1 — Add the nine `ContestKind` and typed `ContestState` variants, with explicit serde renames for `blow_it_up` and `debug`, in crate `squad-protocol`, file `crates/squad-protocol/src/contest.rs`.
  - [x] M8.P1.L1.T2 — Create the nine protocol modules with `ENABLED = false`, the status enum from each mode's section, the `<Mode>State` struct with `validate()`, an empty transition enum and a stub `validate_artifact`, in crate `squad-protocol`, files `crates/squad-protocol/src/{forge,forecast,decide,negotiate,blow_it_up,readback,debugging,trace,rank}.rs`, and export them from `crates/squad-protocol/src/lib.rs`.
  - [x] M8.P1.L1.T3 — Add the `ContestArtifact` carrier with its `audience` rule, the `contest.artifact`, `contest.human_requested` and `contest.human_answered` envelope kinds, the per-mode `<mode>.setup`, `<mode>.state` and `<mode>.preflight` kinds, nine `ContestTransition` variants and nine `CaseMaterial` variants in crate `squad-protocol`, file `crates/squad-protocol/src/envelopes.rs`.
  - [x] M8.P1.L1.T4 — Add every work kind in the **Work kinds** table, with `attempt_limit`, composition lookup and judge or member classification, in crate `squad-protocol`, files `crates/squad-protocol/src/work.rs` and `crates/squad-protocol/src/constants.rs`, and their deadline classes in crate `squad-agents`, file `crates/squad-agents/src/agents/mod.rs`.
  - [x] M8.P1.L1.T5 — Add `NEGOTIATE_ROUNDS`, `READBACK_ROUNDS` and `DEBUG_ROUNDS` to `VariableName`, `SquadConfig`, the optional environment seeds and the env help in crate `squad-protocol`, files `crates/squad-protocol/src/parser/types.rs` and `crates/squad-protocol/src/config.rs`, crate `squad-core`, file `crates/squad-core/src/router/variables.rs`, and crate `squad-cli`, file `crates/squad-cli/src/env-help.md`.
  - [x] M8.P1.L1.T6 — Cover the new kinds, renames, carrier audience rule and variable bounds in crate `squad-protocol`, files `crates/squad-protocol/tests/contests.rs` and `crates/squad-protocol/tests/config.rs`.

- **M8.P1.L2 — Parser, human-step grammar and fixtures**
  - [x] M8.P1.L2.T1 — Register the nine verbs in the `verbs!` table and add nine `ParsedInput` variants with the fields in the **Start grammar** table, gated on each mode's `ENABLED`, in crate `squad-protocol`, file `crates/squad-protocol/src/parser/types.rs`.
  - [x] M8.P1.L2.T2 — Parse the nine start forms, including multi-segment `--` sources for `$trace`, line lists for `$decide` and `$rank`, and `PlanSource` forms for `$blow_it_up` and `$readback`, in crate `squad-protocol`, file `crates/squad-protocol/src/parser/parser.rs`.
  - [x] M8.P1.L2.T3 — Add `SlashAction::Answer`, `ParsedInput::HumanChoices`, `ParseContext.pending_human`, the `/continue` binding to a contest in `AWAITING_HUMAN`, and nine `ExportScope` variants with the `/export` usage strings in crate `squad-protocol`, files `crates/squad-protocol/src/parser/types.rs`, `parser.rs`, `context.rs` and `validate_context.rs`.
  - [x] M8.P1.L2.T4 — Add every rule in the **Proposed parser strings** table with its canonical `ErrorCode` in crate `squad-protocol`, files `crates/squad-protocol/src/parser/errors.rs`, `validation_strings.json` and `validation_codes.json`.
  - [x] M8.P1.L2.T5 — Add `examples.json` cases for every start form, one refusal per string, the busy error for each new kind, `/answer`, `/continue` on a human step, choice tokens with and without a pending step, and the nine exports, leaving the 166 existing cases and snapshots unchanged, in crate `squad-protocol`, file `crates/squad-protocol/src/parser/examples.json`; extend `parser_tests.rs` and `round_trip_tests.rs` with a test-only override that enables all nine switches.
  - [x] M8.P1.L2.T6 — Map the new parsed inputs and human answers to requests in crate `squad-protocol`, file `crates/squad-protocol/src/requests.rs`.

- **M8.P1.L3 — Store**
  - [x] M8.P1.L3.T1 — Add migration `0009_new_modes.sql` with kind and work-kind CHECK extensions for all nine modes, the artifact table, the human-step table and the three variable rows, and register it, in crate `squad-store`, files `crates/squad-store/migrations/0009_new_modes.sql` and `crates/squad-store/src/schema.rs`.
  - [x] M8.P1.L3.T2 — Create and register empty per-mode migrations `0010_forge.sql` to `0018_rank.sql` and stub store modules, in crate `squad-store`, files `crates/squad-store/migrations/`, `crates/squad-store/src/{forge,forecast,decide,negotiate,blow_it_up,readback,debugging,trace,rank}.rs` and `crates/squad-store/src/lib.rs`.
  - [x] M8.P1.L3.T3 — Add generic artifact commit and step-bound artifact readers that enforce `audience`, plus human-step commit and read, in crate `squad-store`, files `crates/squad-store/src/contest_steps.rs` and `crates/squad-store/src/readers.rs`.
  - [x] M8.P1.L3.T4 — Verify the migration preserves all earlier released records and that clear and stop fencing cover every new work kind, in crate `squad-store`, files `crates/squad-store/tests/mode_migrations.rs` and `crates/squad-store/tests/clear.rs`.

- **M8.P1.L4 — Engine dispatch, human step and shared helpers**
  - [x] M8.P1.L4.T1 — Fix the entry-point signatures (prepare, accepted work, human answer, stop, recover, load state, render export) and add dispatch arms to stub modules in crate `squad-core`, files `crates/squad-core/src/router/mod.rs`, `router/stop.rs`, `router/recovery.rs`, `router/preflight.rs`, `orchestrator/mod.rs`, `orchestrator/preflight.rs`, `runtime/mod.rs` and `export/mod.rs`, creating the nine stub files in each of `orchestrator/`, `router/`, `runtime/` and `export/`.
  - [x] M8.P1.L4.T2 — Implement the human step: `AWAITING_HUMAN`, hold release and reacquire, answer commit bound to state version, `/continue`, restart, in crate `squad-core`, files `crates/squad-core/src/router/human.rs` and `crates/squad-core/src/orchestrator/human.rs`.
  - [x] M8.P1.L4.T3 — Implement `verbatim(quote, source)` with its normalisation and span result in crate `squad-core`, file `crates/squad-core/src/orchestrator/verbatim.rs`.
  - [x] M8.P1.L4.T4 — Implement the D75 member repair attempt for the nine modes' work kinds in crate `squad-core`, files `crates/squad-core/src/router/retry.rs` and `crates/squad-core/src/router/executor.rs`.
  - [x] M8.P1.L4.T5 — Register every template named in **New modes** in the manifest, embedded table and provenance list with placeholder files, add the nine `inputs_<module>.rs` stubs and their composition dispatch, in crate `squad-core`, files `crates/squad-core/prompts/manifest.json`, `crates/squad-core/src/prompts/embedded.rs`, `provenance.rs`, `inputs.rs`, `render.rs` and `mod.rs`.
  - [x] M8.P1.L4.T6 — Test the human step (answer, invalid answer, unrelated message, `/continue`, stop, restart, no expiry), the matcher fixtures and the repair attempt in crate `squad-core`, files `crates/squad-core/tests/human_step.rs`, `crates/squad-core/tests/verbatim.rs` and `crates/squad-core/tests/router_validation.rs`.
  - [x] M8.P1.L4.T7 — Extend the one-active-claim test to all sixteen kinds in crate `squad-core`, file `crates/squad-core/tests/contest_storage.rs`.

- **M8.P1.L5 — TUI and CLI registration**
  - [x] M8.P1.L5.T1 — Add the nine `MODES` entries, the three TIP examples and the human-step key tip, each gated on `ENABLED`, in crate `squad-tui`, file `crates/squad-tui/src/typeahead.rs`.
  - [x] M8.P1.L5.T2 — Add per-mode view maps, widget and fx module registration and dispatch with stub files, and the banner words, in crate `squad-tui`, files `crates/squad-tui/src/session.rs`, `widgets/mod.rs`, `widgets/contest_status.rs`, `fx/mod.rs` and `fx/dispatch.rs`, and crate `squad-core`, file `crates/squad-core/src/banner/mod.rs`.
  - [x] M8.P1.L5.T3 — Build the shared human-step widget (banner, numbered items, placeholder, `answer pending` status) and the preflight call-count line for the nine kinds in crate `squad-tui`, files `crates/squad-tui/src/widgets/human_step.rs` and `crates/squad-tui/src/widgets/preflight.rs`.
  - [x] M8.P1.L5.T4 — Add the nine export scopes, generic report output and the `AWAITING_HUMAN` return for `squad send` in crate `squad-cli`, files `crates/squad-cli/src/args.rs`, `send.rs`, `output.rs` and `adapter.rs`.
  - [x] M8.P1.L5.T5 — Verify typeahead and `/help` hide disabled modes and that MCP tool discovery is unchanged in crate `squad-tui`, file `crates/squad-tui/tests/actions_shell.rs`, and crate `squad-cli`, file `crates/squad-cli/tests/mcp.rs`.

### M8.P1 evidence and handoff

- Registry/state/carrier/attempt tests: `squad-protocol/tests/contests.rs`; seeds and bounds: `tests/config.rs`.
- Parser: all original 166 fixtures and their snapshot retained byte-for-byte; 129 additional rows cover both scaffold phases under a resettable thread-local enable override in `parser/new_modes_tests.rs`. Validation strings and codes have matching-table checks.
- Store: `tests/mode_migrations.rs` upgrades populated released schemas and verifies indexes, foreign keys and all registries. `tests/new_mode_visibility.rs` checks frozen plan reservation shares, assignment and audience fences, and private attempt-bound repair diagnostics. Clear resets all new variables and removes the new durable tables.
- Shared runtime: `squad-core/tests/human_step.rs` checks literal answers, partial recovery, stale versions, unrelated ordinary work, held-work release, continue, stop and a restart a year later without expiry. The stop/clear fixture covers all 51 new work kinds. `tests/verbatim.rs` checks normalization and original byte spans; `tests/router_validation.rs` checks disabled admission and repair policy. `tests/contest_storage.rs` exercises every pair of the eighteen kinds against one active claim.
- TUI/CLI: disabled completion/help/TIP checks in `squad-tui/tests/actions_shell.rs`, parser-checked examples, unchanged MCP discovery/workflows, generic report and human-step output. The CLI state snapshot adds the room variables and the command popup snapshot includes the new controls.
- Corrected physical registry sites and exact owned paths are listed in [Scaffold ownership](M8-scaffold-ownership.md). `ContestTransition` is in contest.rs; store dispatch is leases.rs. The pre-existing artifact carrier was extended additively to preserve released wire records. Rendering is manifest-driven, with per-mode input adapters.
- M9.P1 is included for the owner's scope addition, following section 12 and D88–D100. Every switch remains false. No mode script or live API call is part of this handoff.
- Final gates, test totals and per-crate changed paths are recorded in [Scaffold report](M8-scaffold-report.md).

## Phase M8.P2 — Forge

Owns only the `forge` module files listed in **New modes → Files each mode worker owns**, migration `0010_forge.sql` and `docs/modes/forge.md`.

- **M8.P2.L1 — Contracts and storage**
  - [x] M8.P2.L1.T1 — Define `ForgeSetup`, `ForgeDraft`, `ForgeBallot`, `ForgeTally`, `ForgeFusion`, `ForgeVerdict`, `ForgeReport`, `ForgeCaseMaterial`, the transition enum and `validate_artifact` in crate `squad-protocol`, file `crates/squad-protocol/src/forge.rs`.
  - [x] M8.P2.L1.T2 — Add the tables in the mode migration and commit and read label maps per voter, ballots, the tally, fuser selection and candidate labels in crate `squad-store`, files `crates/squad-store/migrations/0010_forge.sql` and `crates/squad-store/src/forge.rs`.
  - [x] M8.P2.L1.T3 — Add valid and invalid fixtures, including a ballot containing the voter's own draft, a ranking that omits a draft, a fusion with unknown source labels and a verdict that edits a candidate, in crate `squad-protocol`, file `crates/squad-protocol/tests/forge.rs`, and step-visibility tests in crate `squad-store`, file `crates/squad-store/tests/forge.rs`.

- **M8.P2.L2 — Script and tally**
  - [x] M8.P2.L2.T1 — Declare the script steps `DRAFTING`, `RANKING`, tally, `FUSING`, `JUDGING` and `REPORTING` with the quorum reasons `insufficient_drafts` and `insufficient_ballots`, in crate `squad-core`, file `crates/squad-core/src/orchestrator/forge.rs`, with commits and transitions in `crates/squad-core/src/router/forge.rs`.
  - [x] M8.P2.L2.T2 — Implement normalised Borda `(k − p)/(k − 1)`, the three tie-break levels, `D*` and fuser selection as pure functions in crate `squad-core`, file `crates/squad-core/src/orchestrator/forge.rs`.
  - [x] M8.P2.L2.T3 — Enforce the `Lmax` length budget on fusions, the judge skip when no fusion is accepted, and the gate reasons `fusion_won`, `original_won` and `no_fusion` in crate `squad-core`, file `crates/squad-core/src/router/forge.rs`.
  - [x] M8.P2.L2.T4 — Supply the `router/bounds.rs` step list from the mode's reservation table and derive `2m + 3` calls and `4m + 7` reserved attempts, in crate `squad-core`, file `crates/squad-core/src/router/forge.rs`.

- **M8.P2.L3 — Prompts**
  - [x] M8.P2.L3.T1 — Write `forge.draft.md`, `forge.rank.md`, `forge.fuse.md` and `forge.judge.md` with the outputs and word budgets in **New modes**, replace the scaffold placeholders, and map typed inputs to placeholders, in crate `squad-core`, files `crates/squad-core/prompts/forge.*.md` and `crates/squad-core/src/prompts/inputs_forge.rs`; add the executor in crate `squad-agents`, file `crates/squad-agents/src/agents/forge.rs`.

- **M8.P2.L4 — Report, export and TUI**
  - [x] M8.P2.L4.T1 — Assemble the report and `/export forge [id]`: the artifact, provenance table resolved to nicks, Borda table, steal and flaw lists, judge reasons, and in export every draft, ballot and fusion, in crate `squad-core`, file `crates/squad-core/src/export/forge.rs`.
  - [x] M8.P2.L4.T2 — Render the `FORGE` banner, the four dividers, collapsed drafts and the verdict line, in crate `squad-tui`, files `crates/squad-tui/src/widgets/forge.rs` and `crates/squad-tui/src/fx/forge.rs`, with view tests in `crates/squad-tui/tests/forge_view.rs`.

- **M8.P2.L5 — Verification and enable**
  - [x] M8.P2.L5.T1 — Verify the forge acceptance tests in **New modes**, including blind captured requests, the byte-identical winner and recovery at every state, plus stop fencing and export, in crate `squad-core`, files `crates/squad-core/tests/forge_workflows.rs` and `crates/squad-core/tests/forge_export.rs`, and executor tests in crate `squad-agents`, file `crates/squad-agents/tests/agents_forge.rs`.
  - [x] M8.P2.L5.T2 — Verify the pure functions against hand-computed fixtures: Borda with complete ballots, a missing ballot and each tie-break level, in crate `squad-core`, file `crates/squad-core/tests/forge_tallies.rs`.
  - [x] M8.P2.L5.T3 — Write the user-facing mode section in `docs/modes/forge.md` in the README mode-section format, then set `ENABLED = true` in crate `squad-protocol`, file `crates/squad-protocol/src/forge.rs`.

**Forge worker evidence (2026-11-26, resumed branch `mode-forge-r2`):**

- Protocol and store fixtures reject invalid labels, incomplete rankings, unknown properties, over-length fusions, invented provenance and judge edits. Scoped readers enforce assignment/audience authorization and private attempt-bound repairs. The shared visibility fixture now uses valid anonymous metadata and its three checks pass.
- Pure fixtures cover complete and missing ballots, each Borda tie-break, fuser selection, length budgets and the six-seat 15-call/31-attempt plan. Exact integer fractions determine ties before the persisted private key.
- All eleven Forge workflows pass, including the actual shared `RoomHandle` path: fifteen blind requests, byte-identical winner/export, quorum failures, repair, second-overrun skip, judge exhaustion, no-fusion judge skip, exhausted-attempt recovery, and stop/restart in every provider phase. Captured requests also pass with the default six roster names. Restart preserves committed private maps and accepted work IDs and requires a successful report without duplicate assignments.
- The owner authorized the minimal shared fixes on resume. New-mode judges now enter the existing structured runner; Forge uses the established exact-prefix blind judge transform; shared parser, help and completion tests follow enablement. The two owned repair compatibility workarounds remain documented in `docs/modes/forge.md`.
- The enabled release smoke completed `original_won` in 75.90 seconds. All six drafts/ballots, two repaired fusions and the judge were accepted: 17 contest attempts plus four startup probes cost $0.373986 against a $24.723887 reserved ceiling; final reservations and unresolved calls are zero. CLI export preserves the winning bytes. The exact command, report excerpt and final workspace gates are in `docs/modes/forge.md`.

**Final workspace gates:** 1,352 unique tests passed, zero failures and four existing manual/PTY-entrypoint ignores; all 27 Forge checks are active and passing. Workspace Clippy with `-D warnings` and formatting passed. The gate rebuilt and exercised the release CLI. All Forge tasks are complete; the owner-approved shared fixes and enablement fixtures are included, with no remaining integrator blocker.

## Phase M8.P3 — Forecast

Owns only the `forecast` module files listed in **New modes → Files each mode worker owns**, migration `0011_forecast.sql` and `docs/modes/forecast.md`.

- **M8.P3.L1 — Contracts and storage**
  - [ ] M8.P3.L1.T1 — Define `ForecastSetup`, `ForecastFrame`, `ForecastEstimate`, `ForecastBundle`, `ForecastRevision`, `ForecastAggregate`, `ForecastCruxes`, `ForecastReport`, `ForecastCaseMaterial`, the transition enum and `validate_artifact`, with `probabilityPct` bounded 1 to 99 in crate `squad-protocol`, file `crates/squad-protocol/src/forecast.rs`.
  - [ ] M8.P3.L1.T2 — Add the tables in the mode migration and commit and read the frozen frame, locked first forecasts, each delivered bundle and revisions in crate `squad-store`, files `crates/squad-store/migrations/0011_forecast.sql` and `crates/squad-store/src/forecast.rs`.
  - [ ] M8.P3.L1.T3 — Add valid and invalid fixtures, including a probability of 0 or 100, two or four factors, a revision citing an unknown label and a crux output carrying a probability, in crate `squad-protocol`, file `crates/squad-protocol/tests/forecast.rs`, and step-visibility tests in crate `squad-store`, file `crates/squad-store/tests/forecast.rs`.

- **M8.P3.L2 — Script and tally**
  - [ ] M8.P3.L2.T1 — Declare the script steps `FRAMING`, `FORECASTING`, the sharing barrier, `REVISING`, `AGGREGATING`, `EXPLAINING` and `REPORTING`, failing with `not_forecastable` before any member call, in crate `squad-core`, file `crates/squad-core/src/orchestrator/forecast.rs`, with commits and transitions in `crates/squad-core/src/router/forecast.rs`.
  - [ ] M8.P3.L2.T2 — Implement number withholding for bundled prose (percent, decimal, `N in M`, odds) and exclusion of the recipient's own entry in crate `squad-core`, file `crates/squad-core/src/orchestrator/forecast.rs`.
  - [ ] M8.P3.L2.T3 — Implement the trimmed mean of log-odds with `k = ⌊n/5⌋`, the spread, the split flag and the uncited-move flag as pure functions in crate `squad-core`, file `crates/squad-core/src/orchestrator/forecast.rs`.
  - [ ] M8.P3.L2.T4 — Carry a missing revision forward as `not revised` and keep the aggregate visible when the crux call fails, in crate `squad-core`, file `crates/squad-core/src/router/forecast.rs`.
  - [ ] M8.P3.L2.T5 — Supply the `router/bounds.rs` step list from the mode's reservation table and derive `2m + 2` calls and `4m + 6` reserved attempts, in crate `squad-core`, file `crates/squad-core/src/router/forecast.rs`.

- **M8.P3.L3 — Prompts**
  - [ ] M8.P3.L3.T1 — Write `forecast.frame.md`, `forecast.estimate.md`, `forecast.revise.md` and `forecast.cruxes.md` with the outputs and word budgets in **New modes**, replace the scaffold placeholders, and map typed inputs to placeholders, in crate `squad-core`, files `crates/squad-core/prompts/forecast.*.md` and `crates/squad-core/src/prompts/inputs_forecast.rs`; add the executor in crate `squad-agents`, file `crates/squad-agents/src/agents/forecast.rs`.

- **M8.P3.L4 — Report, export and TUI**
  - [ ] M8.P3.L4.T1 — Assemble the report and `/export forecast [id]`: frame, pooled value with spread or split view, before and after table, uncited moves, cruxes, signposts, and in export the bundles and aggregation inputs, in crate `squad-core`, file `crates/squad-core/src/export/forecast.rs`.
  - [ ] M8.P3.L4.T2 — Render the `FORECAST` banner, the four dividers and the pooled bar with spread band and member ticks, in crate `squad-tui`, files `crates/squad-tui/src/widgets/forecast.rs` and `crates/squad-tui/src/fx/forecast.rs`, with view tests in `crates/squad-tui/tests/forecast_view.rs`.

- **M8.P3.L5 — Verification and enable**
  - [ ] M8.P3.L5.T1 — Verify the forecast acceptance tests in **New modes**, including withheld numbers in captured revise requests, plus stop fencing and export, in crate `squad-core`, files `crates/squad-core/tests/forecast_workflows.rs` and `crates/squad-core/tests/forecast_export.rs`, and executor tests in crate `squad-agents`, file `crates/squad-agents/tests/agents_forecast.rs`.
  - [ ] M8.P3.L5.T2 — Verify the pure functions against hand-computed fixtures: n of 3, 5 and 10 to 1e-9, a bimodal set and a unimodal set, in crate `squad-core`, file `crates/squad-core/tests/forecast_tallies.rs`.
  - [ ] M8.P3.L5.T3 — Write the user-facing mode section in `docs/modes/forecast.md` in the README mode-section format, then set `ENABLED = true` in crate `squad-protocol`, file `crates/squad-protocol/src/forecast.rs`.

## Phase M8.P4 — Decide

Owns only the `decide` module files listed in **New modes → Files each mode worker owns**, migration `0012_decide.sql` and `docs/modes/decide.md`.

- **M8.P4.L1 — Contracts and storage**
  - [x] M8.P4.L1.T1 — Define `DecideSetup`, `DecideOptionProposal`, `DecideShortlist`, `DecideCriteria`, `DecideRubric`, `DecideScores`, `DecideMatrix`, `DecideExplanation`, `DecideReport`, `DecideCaseMaterial`, the transition enum and `validate_artifact` in crate `squad-protocol`, file `crates/squad-protocol/src/decide.rs`.
  - [x] M8.P4.L1.T2 — Add the tables in the mode migration and commit and read proposals, the shortlist, criteria lists, the frozen rubric, per-member option orders and score sheets in crate `squad-store`, files `crates/squad-store/migrations/0012_decide.sql` and `crates/squad-store/src/decide.rs`.
  - [x] M8.P4.L1.T3 — Add valid and invalid fixtures, including weights that do not sum to 100, a missing or duplicated cell, a score outside 0 to 10, a judge criterion with no source and an explanation naming a winner, in crate `squad-protocol`, file `crates/squad-protocol/tests/decide.rs`, and step-visibility tests in crate `squad-store`, file `crates/squad-store/tests/decide.rs`.

- **M8.P4.L2 — Script and tally**
  - [x] M8.P4.L2.T1 — Declare the script steps optional `PROPOSING_OPTIONS` and `SHORTLISTING`, then `PROPOSING_CRITERIA`, `FREEZING`, `SCORING`, `COMPUTING`, `EXPLAINING` and `REPORTING`, with the four-option cap and the quorum reasons, in crate `squad-core`, file `crates/squad-core/src/orchestrator/decide.rs`, with commits and transitions in `crates/squad-core/src/router/decide.rs`.
  - [x] M8.P4.L2.T2 — Implement the rubric rule (support of at least two, cap of seven by mean weight, renormalisation) as pure functions in crate `squad-core`, file `crates/squad-core/src/orchestrator/decide.rs`.
  - [x] M8.P4.L2.T3 — Implement cell medians, totals, margin, the 0.25 too-close rule, contested cells, leave-one-criterion-out leaders, flip thresholds `w*` and the robustness label as pure functions in crate `squad-core`, file `crates/squad-core/src/orchestrator/decide.rs`.
  - [x] M8.P4.L2.T4 — Deliver criteria to scorers without weights, shuffle option order per member, and validate that every judge flip condition references an app-computed result, in crate `squad-core`, file `crates/squad-core/src/router/decide.rs`.
  - [x] M8.P4.L2.T5 — Supply the `router/bounds.rs` step list from the mode's reservation table and derive `2m + 2 + g(m + 1)` calls and `4m + 6 + g(2m + 3)` reserved attempts, in crate `squad-core`, file `crates/squad-core/src/router/decide.rs`.

- **M8.P4.L3 — Prompts**
  - [x] M8.P4.L3.T1 — Write `decide.options.md`, `decide.shortlist.md`, `decide.criteria.md`, `decide.freeze.md`, `decide.score.md` and `decide.explain.md` with the outputs and word budgets in **New modes**, replace the scaffold placeholders, and map typed inputs to placeholders, in crate `squad-core`, files `crates/squad-core/prompts/decide.*.md` and `crates/squad-core/src/prompts/inputs_decide.rs`; add the executor in crate `squad-agents`, file `crates/squad-agents/src/agents/decide.rs`.

- **M8.P4.L4 — Report, export and TUI**
  - [x] M8.P4.L4.T1 — Assemble the report and `/export decide [id]`: winner or too-close line, rubric, matrix, totals, leave-one-out table, flip thresholds, dissents, dropped criteria, and in export every proposal and score sheet, in crate `squad-core`, file `crates/squad-core/src/export/decide.rs`.
  - [x] M8.P4.L4.T2 — Render the `DECIDE` banner, the four dividers, the matrix table with contested marks and the leave-one-out toggle, in crate `squad-tui`, files `crates/squad-tui/src/widgets/decide.rs` and `crates/squad-tui/src/fx/decide.rs`, with view tests in `crates/squad-tui/tests/decide_view.rs`.

- **M8.P4.L5 — Verification and enable**
  - [x] M8.P4.L5.T1 — Verify the decide acceptance tests in **New modes**, both with options given and generated, plus stop fencing and export, in crate `squad-core`, files `crates/squad-core/tests/decide_workflows.rs` and `crates/squad-core/tests/decide_export.rs`, and executor tests in crate `squad-agents`, file `crates/squad-agents/tests/agents_decide.rs`.
  - [x] M8.P4.L5.T2 — Verify the pure functions against hand-computed fixtures: rubric fixtures, totals, leave-one-out and `w*` including the no-flip sign cases, in crate `squad-core`, file `crates/squad-core/tests/decide_tallies.rs`.
  - [x] M8.P4.L5.T3 — Write the user-facing mode section in `docs/modes/decide.md` in the README mode-section format, then set `ENABLED = true` in crate `squad-protocol`, file `crates/squad-protocol/src/decide.rs`.

Decide implementation evidence (`mode-decide-r2`, 2026-11-26):

- Contracts and persistence: `crates/squad-protocol/tests/decide.rs` checks bad weights/cells/sources/judge winner fields and blank shortlist labels; `crates/squad-store/tests/decide.rs` checks assignment/audience/lease fencing and private attempt-bound repairs. The migration adds `decide_records`; frozen inputs and private option permutations survive recovery.
- Script and tally: `given_and_generated_six_member_call_counts_blinding_and_recovery` completes both scripts through RoomHandle with mocked transport: 14/21 calls, 30/45 reserved attempts, distinct option orders, no score weights, persisted 4,096-token scoring allowances and restart without duplicate calls. `decide_tallies` covers singleton/eighth-criterion fixtures, normalization, even medians, outliers, contested cells, totals, both flip directions/no-flip cases, leave-one-out and the 0.2 close margin. Owned callback fixtures additionally verify complete exports and accounting.
- Failure paths: `member_invalid_output_repairs_once` now runs through RoomHandle, verifies six diagnostic repairs and 20 calls, and reconciles the ledger. Stop/startup recovery, skips, quorum failures, no shared rubric and judge exhaustion preserve committed evidence and exportable arithmetic. Both formerly ignored integration regressions are normal tests.
- Prompts, report and view: six owned prompt assets, frozen one-pass substitution, schema/reference validation and D75 diagnostics; `decide_export` verifies close-call and contested matrix text. `decide_view` tests projections, replay and the actual F7 app input handler, plus enabled completion/help/TIP discovery and public parsing.
- Owner-authorized shared integration: the outer structured judge dispatcher accepts new-mode judge work, invalid mode output enters repair classification, DecideScore has the canonical 4,096 ceiling in both protocol sites, and the F7 handler toggles Decide views. The composer example rotation includes Decide; activation-related scaffold fixtures are updated. No shared implementation requests remain.
- Activation and documentation: `ENABLED = true`; `docs/modes/decide.md` describes syntax, steps, cost, report/export and controls. The 25 focused workflow/TUI/typeahead checks pass without ignored tests.

Final Decide gates (dev2, `CARGO_BUILD_JOBS=4`): workspace tests **1,347 passed / 0 failed / 4 inherited ignores**, all **21 Decide tests** active and passing; all-target Clippy with `-D warnings`, format checking and the release build passed. The original 166 parser fixture rows and snapshot are unchanged through a test-only legacy activation override.

Live smoke: one default-six-roster release send reached **COMPLETE**, exit 0, **236.24 s**. All six criteria lists and score sheets were accepted; Grok succeeded on its scoring retry (15 contest attempts against 14 calls / 30 reserved). Initial reservation **$16.385052**, verified provider spend **$0.621906** including startup and one cancelled-attempt charge. The exit ledger records **$0.607680** and retains **$0.583480** for that cancelled attempt; a later free generation lookup verified its **$0.014226** cost, without manually editing the ledger. Full accounting caveat, real report excerpt and offline export evidence are in `docs/modes/decide.md`. No shared implementation requests remain.

## Phase M8.P5 — Negotiate

Owns only the `negotiate` module files listed in **New modes → Files each mode worker owns**, migration `0013_negotiate.sql` and `docs/modes/negotiate.md`.

- **M8.P5.L1 — Contracts and storage**
  - [ ] M8.P5.L1.T1 — Define `NegotiateSetup`, `NegotiateMap`, `NegotiateBrief`, `NegotiatePackage`, `NegotiateTurn`, `NegotiateTable`, `NegotiatePlaybook`, `NegotiateReport`, `NegotiateCaseMaterial`, the transition enum and `validate_artifact` in crate `squad-protocol`, file `crates/squad-protocol/src/negotiate.rs`.
  - [ ] M8.P5.L1.T2 — Add the tables in the mode migration and commit and read the role draw, hidden briefs with recipient-bound readers, packages and per-table transcripts with round in crate `squad-store`, files `crates/squad-store/migrations/0013_negotiate.sql` and `crates/squad-store/src/negotiate.rs`.
  - [ ] M8.P5.L1.T3 — Add valid and invalid fixtures, including a brief with the wrong number of counterparts, a counter with an unknown response, a turn for the wrong table and a playbook with an invalid `overlap`, in crate `squad-protocol`, file `crates/squad-protocol/tests/negotiate.rs`, and step-visibility tests in crate `squad-store`, file `crates/squad-store/tests/negotiate.rs`.

- **M8.P5.L2 — Script and tally**
  - [ ] M8.P5.L2.T1 — Declare the script steps `MAPPING`, `BRIEFING`, `PACKAGING`, `REHEARSING` and `WRITING_PLAYBOOK`, drawing `P = min(3, ⌊m/2⌋)` counterparts and negotiators once at admission, in crate `squad-core`, file `crates/squad-core/src/orchestrator/negotiate.rs`, with commits and transitions in `crates/squad-core/src/router/negotiate.rs`.
  - [ ] M8.P5.L2.T2 — Run P mutually invisible tables as parallel `turns` sequences of up to `NEGOTIATE_ROUNDS` rounds, ending each on `deal`, `no_deal`, `unfinished` or `abandoned`, in crate `squad-core`, file `crates/squad-core/src/router/negotiate.rs`.
  - [ ] M8.P5.L2.T3 — Build the negotiator, packager and counterpart projections so that hidden briefs reach only their counterpart and the case, and our fallback and limits never reach a counterpart, in crate `squad-core`, file `crates/squad-core/src/runtime/negotiate.rs`.
  - [ ] M8.P5.L2.T4 — Supply the `router/bounds.rs` step list from the mode's reservation table and derive `2m − P + 2PR + 2` calls and `4m − 2P + 4PR + 6` reserved attempts, releasing turns of tables that end early, in crate `squad-core`, file `crates/squad-core/src/router/negotiate.rs`.

- **M8.P5.L3 — Prompts**
  - [ ] M8.P5.L3.T1 — Write `negotiate.map.md`, `negotiate.brief.md`, `negotiate.package.md`, `negotiate.move.md`, `negotiate.counter.md` and `negotiate.playbook.md` with the outputs and word budgets in **New modes**, replace the scaffold placeholders, and map typed inputs to placeholders, in crate `squad-core`, files `crates/squad-core/prompts/negotiate.*.md` and `crates/squad-core/src/prompts/inputs_negotiate.rs`; add the executor in crate `squad-agents`, file `crates/squad-agents/src/agents/negotiate.rs`.

- **M8.P5.L4 — Report, export and TUI**
  - [ ] M8.P5.L4.T1 — Assemble the report and `/export negotiate [id]`: role draw, interest map, BATNA and ZOPA map, packages, per-table transcripts and outcomes, playbook and assumptions to confirm, in crate `squad-core`, file `crates/squad-core/src/export/negotiate.rs`.
  - [ ] M8.P5.L4.T2 — Render the `NEGOTIATE` banner, `PREP`, table and `PLAYBOOK` dividers, side-by-side tables when wide, and the `counterpart` tag, in crate `squad-tui`, files `crates/squad-tui/src/widgets/negotiate.rs` and `crates/squad-tui/src/fx/negotiate.rs`, with view tests in `crates/squad-tui/tests/negotiate_view.rs`.

- **M8.P5.L5 — Verification and enable**
  - [ ] M8.P5.L5.T1 — Verify the negotiate acceptance tests in **New modes**, including brief isolation in captured requests and all-tables-abandoned, plus stop fencing and export, in crate `squad-core`, files `crates/squad-core/tests/negotiate_workflows.rs` and `crates/squad-core/tests/negotiate_export.rs`, and executor tests in crate `squad-agents`, file `crates/squad-agents/tests/agents_negotiate.rs`.
  - [ ] M8.P5.L5.T2 — Verify the pure functions against hand-computed fixtures: table outcome counts and reservation release for early endings, in crate `squad-core`, file `crates/squad-core/tests/negotiate_tallies.rs`.
  - [ ] M8.P5.L5.T3 — Write the user-facing mode section in `docs/modes/negotiate.md` in the README mode-section format, then set `ENABLED = true` in crate `squad-protocol`, file `crates/squad-protocol/src/negotiate.rs`.

## Phase M8.P6 — Blow_it_up

Owns only the `blow_it_up` module files listed in **New modes → Files each mode worker owns**, migration `0014_blow_it_up.sql` and `docs/modes/blow_it_up.md`.

- **M8.P6.L1 — Contracts and storage**
  - [x] M8.P6.L1.T1 — Define `BlowItUpSetup`, `BlowItUpAttack`, `BlowItUpPool`, `BlowItUpChallenge`, `BlowItUpTally`, `BlowItUpRuling`, `BlowItUpReport`, `BlowItUpCaseMaterial`, the transition enum and `validate_artifact`, every enum with the explicit `blow_it_up` rename in crate `squad-protocol`, file `crates/squad-protocol/src/blow_it_up.rs`.
  - [x] M8.P6.L1.T2 — Add the tables in the mode migration and commit and read the lens draw, the pool with private attacker provenance, ballots and tallies in crate `squad-store`, files `crates/squad-store/migrations/0014_blow_it_up.sql` and `crates/squad-store/src/blow_it_up.rs`.
  - [x] M8.P6.L1.T3 — Add valid and invalid fixtures, including an attack set with zero or two diversity vectors, a `false_positive` without a defeating quote, a `duplicate` without `duplicateOf` and a finding citing only killed vectors, in crate `squad-protocol`, file `crates/squad-protocol/tests/blow_it_up.rs`, and step-visibility tests in crate `squad-store`, file `crates/squad-store/tests/blow_it_up.rs`.

- **M8.P6.L2 — Script and tally**
  - [x] M8.P6.L2.T1 — Declare the script steps `ATTACKING`, pooling, `CHALLENGING`, tally, `RULING` and `REPORTING`, with the persisted lens permutation and `insufficient_attacks`, in crate `squad-core`, file `crates/squad-core/src/orchestrator/blow_it_up.rs`, with commits and transitions in `crates/squad-core/src/router/blow_it_up.rs`.
  - [x] M8.P6.L2.T2 — Check every `targetQuote` and `defeatingQuote` with the shared matcher and pool vectors under opaque shuffled IDs without lens, diversity flag or attacker, in crate `squad-core`, file `crates/squad-core/src/router/blow_it_up.rs`.
  - [x] M8.P6.L2.T3 — Implement the challenge tally (`killed`, `contested`, `standing`, `unchallenged`), blind finder counts, and ranking by `sev × lik` with its tie-breaks as pure functions in crate `squad-core`, file `crates/squad-core/src/orchestrator/blow_it_up.rs`.
  - [x] M8.P6.L2.T4 — Reject rulings that reinstate a killed vector or cite no pooled vector, and require `weakened` or a stated reason for contested sources, in crate `squad-core`, file `crates/squad-core/src/router/blow_it_up.rs`.
  - [x] M8.P6.L2.T5 — Supply the `router/bounds.rs` step list from the mode's reservation table and derive `2m + 1` calls and `4m + 3` reserved attempts, in crate `squad-core`, file `crates/squad-core/src/router/blow_it_up.rs`.

- **M8.P6.L3 — Prompts**
  - [x] M8.P6.L3.T1 — Write `blow_it_up.attack.md`, `blow_it_up.challenge.md` and `blow_it_up.rule.md` with the outputs and word budgets in **New modes**, replace the scaffold placeholders, and map typed inputs to placeholders, in crate `squad-core`, files `crates/squad-core/prompts/blow_it_up.*.md` and `crates/squad-core/src/prompts/inputs_blow_it_up.rs`; add the executor in crate `squad-agents`, file `crates/squad-agents/src/agents/blow_it_up.rs`.

- **M8.P6.L4 — Report, export and TUI**
  - [x] M8.P6.L4.T1 — Assemble the report and `/export blow_it_up [id]`: ranked findings with mitigation and rebuttal status, lone findings, additions, the killed appendix and unchallenged vectors, in crate `squad-core`, file `crates/squad-core/src/export/blow_it_up.rs`.
  - [x] M8.P6.L4.T2 — Render the `BLOW IT UP` banner, the three dividers, severity colours and the collapsed killed appendix, in crate `squad-tui`, files `crates/squad-tui/src/widgets/blow_it_up.rs` and `crates/squad-tui/src/fx/blow_it_up.rs`, with view tests in `crates/squad-tui/tests/blow_it_up_view.rs`.

- **M8.P6.L5 — Verification and enable**
  - [x] M8.P6.L5.T1 — Verify the blow_it_up acceptance tests in **New modes**, including own-vector exclusion in captured challenge requests and the `@nick` source form, plus stop fencing and export, in crate `squad-core`, files `crates/squad-core/tests/blow_it_up_workflows.rs` and `crates/squad-core/tests/blow_it_up_export.rs`, and executor tests in crate `squad-agents`, file `crates/squad-agents/tests/agents_blow_it_up.rs`.
  - [x] M8.P6.L5.T2 — Verify the pure functions against hand-computed fixtures: all four statuses including the exact-half case, and ranking ties, in crate `squad-core`, file `crates/squad-core/tests/blow_it_up_tallies.rs`.
  - [x] M8.P6.L5.T3 — Write the user-facing mode section in `docs/modes/blow_it_up.md` in the README mode-section format, then set `ENABLED = true` in crate `squad-protocol`, file `crates/squad-protocol/src/blow_it_up.rs`.

**Completion evidence (2026-11-26, branch `mode-blow-it-up-r2`).** All fourteen mode tasks are implemented. The owned protocol, private migration/storage, frozen script, typed case, prompts, executor, app tallies/ranking, report/export and styled TUI are integrated. All 32 owned checks pass, with no ignored mode cases. Nine RoomHandle/direct workflows cover 13 calls / 27 reserved attempts, own-vector exclusion in captured prompts, one private repair, skips, insufficient attacks, judge exhaustion, stop fencing and persisted recovery. A real latest-member-reply workflow verifies the `@nick` source, frozen text/event ID and all six attack assignments. The completed-report replay fixture verifies severity styling, width wrapping and the F7 killed-appendix control through shared transcript composition. Parser, help, typeahead and TIP discovery pass with `ENABLED = true`.

**Final gates.** On dev2, with `CARGO_BUILD_JOBS=4` and builds run sequentially: `cargo test --workspace --no-fail-fast` passes (Cargo summaries: 1,358 passed, zero failed and four existing ignored entries; the passing total includes the repeated WAL child summary); `cargo clippy --workspace --all-targets -- -D warnings`, `cargo build --release -p squad-cli`, and `cargo fmt --all -- --check` pass. Required shared judge dispatch, challenge ceilings, invalid-output classification, typed case validation, repair placeholders, TUI details and discovery-test fixes are included with explicit authorization. The shared tournament audit now reads a fresh terminal event page before restoring its view. No shared change remains deferred to the integrator. The default-six release smoke completed in 170.052 seconds with a ranked report and offline export: $1.449895 recorded actual spend (including $0.001538 startup), versus $20.460108 reserved. All reservations and all twenty recorded starts settled; zero unknown-cost calls remain. Five attacks, six challenges and the first ruling were accepted; the skipped Grok attack is disclosed. Exact evidence and a real report excerpt are in `docs/modes/blow_it_up.md`.

## Phase M8.P7 — Readback

Owns only the `readback` module files listed in **New modes → Files each mode worker owns**, migration `0015_readback.sql` and `docs/modes/readback.md`.

- **M8.P7.L1 — Contracts and storage**
  - [ ] M8.P7.L1.T1 — Define `ReadbackSetup`, `ReadbackInterpretation`, `ReadbackLedger`, `ReadbackChoices`, `ReadbackRewrite`, `ReadbackReport`, `ReadbackCaseMaterial`, the transition enum and `validate_artifact` in crate `squad-protocol`, file `crates/squad-protocol/src/readback.rs`.
  - [ ] M8.P7.L1.T2 — Add the tables in the mode migration and commit and read spec versions, ledgers with private reader provenance, partial human answers and the round in crate `squad-store`, files `crates/squad-store/migrations/0015_readback.sql` and `crates/squad-store/src/readback.rs`.
  - [ ] M8.P7.L1.T3 — Add valid and invalid fixtures, including a single-reading entry, a reading with no source, a non-verbatim passage and a change log that misses or adds an entry, in crate `squad-protocol`, file `crates/squad-protocol/tests/readback.rs`, and step-visibility tests in crate `squad-store`, file `crates/squad-store/tests/readback.rs`.

- **M8.P7.L2 — Script and tally**
  - [ ] M8.P7.L2.T1 — Declare the script steps the `INTERPRETING`, `DIFFING`, `AWAITING_HUMAN`, `REWRITING` loop bounded by `READBACK_ROUNDS`, ending with `converged`, `rounds_complete` or `no_readings_chosen`, in crate `squad-core`, file `crates/squad-core/src/orchestrator/readback.rs`, with commits and transitions in `crates/squad-core/src/router/readback.rs`.
  - [ ] M8.P7.L2.T2 — Validate ledgers (verbatim passage, two to four readings with sources, cap of ten with the cut count) and build the human-step items with member support counts for the human only, in crate `squad-core`, file `crates/squad-core/src/router/readback.rs`.
  - [ ] M8.P7.L2.T3 — Accept choice tokens and `/answer n: text`, let a later answer replace an earlier one, and close on a full set or `/continue`, in crate `squad-core`, file `crates/squad-core/src/router/readback.rs`.
  - [ ] M8.P7.L2.T4 — Verify each rewrite: `before` spans verbatim in the old version, unresolved passages verbatim in the new one, changes covering exactly the resolved entries, and the two-times size bound, in crate `squad-core`, file `crates/squad-core/src/orchestrator/readback.rs`.
  - [ ] M8.P7.L2.T5 — Supply the `router/bounds.rs` step list from the mode's reservation table and derive at most `R(m + 2)` calls and `R(2m + 6)` reserved attempts, releasing unused rounds on convergence, in crate `squad-core`, file `crates/squad-core/src/router/readback.rs`.

- **M8.P7.L3 — Prompts**
  - [ ] M8.P7.L3.T1 — Write `readback.interpret.md`, `readback.diff.md` and `readback.rewrite.md` with the outputs and word budgets in **New modes**, replace the scaffold placeholders, and map typed inputs to placeholders, in crate `squad-core`, files `crates/squad-core/prompts/readback.*.md` and `crates/squad-core/src/prompts/inputs_readback.rs`; add the executor in crate `squad-agents`, file `crates/squad-agents/src/agents/readback.rs`.

- **M8.P7.L4 — Report, export and TUI**
  - [ ] M8.P7.L4.T1 — Assemble the report and `/export readback [id]`: final spec, ledgers per round with chosen meanings, open entries, the convergence table, and in export every version and the raw answers, in crate `squad-core`, file `crates/squad-core/src/export/readback.rs`.
  - [ ] M8.P7.L4.T2 — Render the `READBACK` banner, round dividers, the `READ IT BACK` human step through the shared widget and before and after rewrite pairs, in crate `squad-tui`, files `crates/squad-tui/src/widgets/readback.rs` and `crates/squad-tui/src/fx/readback.rs`, with view tests in `crates/squad-tui/tests/readback_view.rs`.

- **M8.P7.L5 — Verification and enable**
  - [ ] M8.P7.L5.T1 — Verify the readback acceptance tests in **New modes**, including fresh contexts in round 2 and human-step restart, plus stop fencing and export, in crate `squad-core`, files `crates/squad-core/tests/readback_workflows.rs` and `crates/squad-core/tests/readback_export.rs`, and executor tests in crate `squad-agents`, file `crates/squad-agents/tests/agents_readback.rs`.
  - [ ] M8.P7.L5.T2 — Verify the pure functions against hand-computed fixtures: convergence table counts and rewrite verification, in crate `squad-core`, file `crates/squad-core/tests/readback_tallies.rs`.
  - [ ] M8.P7.L5.T3 — Write the user-facing mode section in `docs/modes/readback.md` in the README mode-section format, then set `ENABLED = true` in crate `squad-protocol`, file `crates/squad-protocol/src/readback.rs`.

## Phase M8.P8 — Debug

Owns only the `debugging` module files listed in **New modes → Files each mode worker owns**, migration `0016_debug.sql` and `docs/modes/debug.md`.

- **M8.P8.L1 — Contracts and storage**
  - [ ] M8.P8.L1.T1 — Define `DebuggingSetup`, `DebuggingHypotheses`, `DebuggingDifferential`, `DebuggingTestProposal`, `DebuggingSelection`, `DebuggingResult`, `DebuggingElimination`, `DebuggingReport`, `DebuggingCaseMaterial`, the transition enum and `validate_artifact`, every enum with the explicit `debug` rename in crate `squad-protocol`, file `crates/squad-protocol/src/debugging.rs`.
  - [ ] M8.P8.L1.T2 — Add the tables in the mode migration and commit and read hypothesis tables, the differential with statuses, proposals with predictions, selections, recorded results and the evidence ledger in crate `squad-store`, files `crates/squad-store/migrations/0016_debug.sql` and `crates/squad-store/src/debugging.rs`.
  - [ ] M8.P8.L1.T3 — Add valid and invalid fixtures, including a prediction row with both or neither of `outcome` and `cannotPredictReason`, a selection of an unproposed or risky test, a non-splitting selection and a patch while `patchAllowed` is false, in crate `squad-protocol`, file `crates/squad-protocol/tests/debugging.rs`, and step-visibility tests in crate `squad-store`, file `crates/squad-store/tests/debugging.rs`.

- **M8.P8.L2 — Script and tally**
  - [ ] M8.P8.L2.T1 — Declare the script steps `HYPOTHESIZING`, `MERGING`, then the `PROPOSING_TESTS`, `SELECTING`, `AWAITING_HUMAN`, `ELIMINATING` loop bounded by `DEBUG_ROUNDS`, then the judge-backed `REPORTING`, in crate `squad-core`, file `crates/squad-core/src/orchestrator/debugging.rs`, with commits and transitions in `crates/squad-core/src/router/debugging.rs`.
  - [ ] M8.P8.L2.T2 — Mark non-verbatim evidence quotes `unanchored`, validate that a selection names a proposed non-risky test that splits two live hypotheses, and end the loop with `no_tests` or `no_discriminating_test`, in crate `squad-core`, file `crates/squad-core/src/router/debugging.rs`.
  - [ ] M8.P8.L2.T3 — Accept `/answer O<n> <text>`, `/answer other <text>`, `/answer unknown` and `/continue`, and append the pasted result to the evidence ledger with its 400-word prompt cap, in crate `squad-core`, file `crates/squad-core/src/router/debugging.rs`.
  - [ ] M8.P8.L2.T4 — Implement elimination, consistent-result counts, loop-end reasons (`isolated`, `all_eliminated`, `rounds_complete`) and `patchAllowed` as pure functions in crate `squad-core`, file `crates/squad-core/src/orchestrator/debugging.rs`.
  - [ ] M8.P8.L2.T5 — Supply the `router/bounds.rs` step list from the mode's reservation table and derive at most `(R + 1)(m + 1) + 1` calls and `2m(R + 1) + 3R + 6` reserved attempts, releasing unused rounds, in crate `squad-core`, file `crates/squad-core/src/router/debugging.rs`.

- **M8.P8.L3 — Prompts**
  - [ ] M8.P8.L3.T1 — Write `debug.hypothesize.md`, `debug.merge.md`, `debug.test.md`, `debug.select.md` and `debug.report.md` with the outputs and word budgets in **New modes**, replace the scaffold placeholders, and map typed inputs to placeholders, in crate `squad-core`, files `crates/squad-core/prompts/debug.*.md` and `crates/squad-core/src/prompts/inputs_debugging.rs`; add the executor in crate `squad-agents`, file `crates/squad-agents/src/agents/debugging.rs`.

- **M8.P8.L4 — Report, export and TUI**
  - [ ] M8.P8.L4.T1 — Assemble the report and `/export debug [id]`: differential with statuses, evidence ledger, eliminated hypotheses with the failed prediction, next best test, unselected risky tests, and the patch or the no-patch line, in crate `squad-core`, file `crates/squad-core/src/export/debugging.rs`.
  - [ ] M8.P8.L4.T2 — Render the `DEBUG` banner, the dividers, a differential table that greys eliminated rows and the `RUN THE TEST` human step through the shared widget, in crate `squad-tui`, files `crates/squad-tui/src/widgets/debugging.rs` and `crates/squad-tui/src/fx/debugging.rs`, with view tests in `crates/squad-tui/tests/debugging_view.rs`.

- **M8.P8.L5 — Verification and enable**
  - [ ] M8.P8.L5.T1 — Verify the debug acceptance tests in **New modes**, including that `unknown` changes no status and that no text gives a hypothesis a confirmed status, plus stop fencing and export, in crate `squad-core`, files `crates/squad-core/tests/debugging_workflows.rs` and `crates/squad-core/tests/debugging_export.rs`, and executor tests in crate `squad-agents`, file `crates/squad-agents/tests/agents_debugging.rs`.
  - [ ] M8.P8.L5.T2 — Verify the pure functions against hand-computed fixtures: elimination for matching, differing, `disputed`, `cannot_predict`, `other` and `unknown`, and the patch gate, in crate `squad-core`, file `crates/squad-core/tests/debugging_tallies.rs`.
  - [ ] M8.P8.L5.T3 — Write the user-facing mode section in `docs/modes/debug.md` in the README mode-section format, then set `ENABLED = true` in crate `squad-protocol`, file `crates/squad-protocol/src/debugging.rs`.

## Phase M8.P9 — Trace

Owns only the `trace` module files listed in **New modes → Files each mode worker owns**, migration `0017_trace.sql` and `docs/modes/trace.md`.

- **M8.P9.L1 — Contracts and storage**
  - [ ] M8.P9.L1.T1 — Define `TraceSetup`, `TraceExtraction`, `TraceQuoteCheck`, `TraceClaims`, `TraceVerification`, `TraceLedger`, `TraceBrief`, `TraceReport`, `TraceCaseMaterial`, the transition enum and `validate_artifact` in crate `squad-protocol`, file `crates/squad-protocol/src/trace.rs`.
  - [ ] M8.P9.L1.T2 — Add the tables in the mode migration and commit and read numbered sources, extractions, quote checks with spans, canonical claims with provenance, verifier assignments and verdicts in crate `squad-store`, files `crates/squad-store/migrations/0017_trace.sql` and `crates/squad-store/src/trace.rs`.
  - [ ] M8.P9.L1.T3 — Add valid and invalid fixtures, including a quote citing an unknown source, a canonical claim with no source reference, `missing_qualifier` without a qualifier quote and a brief paragraph with no claim ID, in crate `squad-protocol`, file `crates/squad-protocol/tests/trace.rs`, and step-visibility tests in crate `squad-store`, file `crates/squad-store/tests/trace.rs`.

- **M8.P9.L2 — Script and tally**
  - [ ] M8.P9.L2.T1 — Declare the script steps `EXTRACTING`, the quote check, `MERGING`, assignment, `VERIFYING`, the ledger tally, `ASSEMBLING` and `REPORTING`, with the empty-claim-set short path, in crate `squad-core`, file `crates/squad-core/src/orchestrator/trace.rs`, with commits and transitions in `crates/squad-core/src/router/trace.rs`.
  - [ ] M8.P9.L2.T2 — Run the shared matcher over every quote, record `quote_not_found` with no repair, and attach to each canonical claim only the union of verified quotes, in crate `squad-core`, file `crates/squad-core/src/router/trace.rs`.
  - [ ] M8.P9.L2.T3 — Implement verifier assignment (two per claim, contributors ineligible when alternatives exist, balanced, seeded), claim status and independent-source counting with `sharedOrigin` groups as pure functions in crate `squad-core`, file `crates/squad-core/src/orchestrator/trace.rs`.
  - [ ] M8.P9.L2.T4 — Validate the brief: every paragraph cites a claim, body paragraphs cite only supported claims, disputed entries only disputed claims, in crate `squad-core`, file `crates/squad-core/src/router/trace.rs`.
  - [ ] M8.P9.L2.T5 — Supply the `router/bounds.rs` step list from the mode's reservation table and derive `2m + 2` calls and `4m + 6` reserved attempts, failing admission with `CONTEXT_TOO_LARGE` for an oversized packet, in crate `squad-core`, file `crates/squad-core/src/router/trace.rs`.

- **M8.P9.L3 — Prompts**
  - [ ] M8.P9.L3.T1 — Write `trace.extract.md`, `trace.merge.md`, `trace.verify.md` and `trace.assemble.md` with the outputs and word budgets in **New modes**, replace the scaffold placeholders, and map typed inputs to placeholders, in crate `squad-core`, files `crates/squad-core/prompts/trace.*.md` and `crates/squad-core/src/prompts/inputs_trace.rs`; add the executor in crate `squad-agents`, file `crates/squad-agents/src/agents/trace.rs`.

- **M8.P9.L4 — Report, export and TUI**
  - [ ] M8.P9.L4.T1 — Assemble the report and `/export trace [id]`: the brief with claim markers, the claim-to-source ledger with spans, disputed, unsupported and unverified claims, quotes not found, and in export the numbered packet and every verdict, in crate `squad-core`, file `crates/squad-core/src/export/trace.rs`.
  - [ ] M8.P9.L4.T2 — Render the `TRACE` banner, the four dividers, claim-marker inspection and the separate flagged-claims list, in crate `squad-tui`, files `crates/squad-tui/src/widgets/trace.rs` and `crates/squad-tui/src/fx/trace.rs`, with view tests in `crates/squad-tui/tests/trace_view.rs`.

- **M8.P9.L5 — Verification and enable**
  - [ ] M8.P9.L5.T1 — Verify the trace acceptance tests in **New modes**, including that a fabricated quote reaches no later prompt, plus stop fencing and export, in crate `squad-core`, files `crates/squad-core/tests/trace_workflows.rs` and `crates/squad-core/tests/trace_export.rs`, and executor tests in crate `squad-agents`, file `crates/squad-agents/tests/agents_trace.rs`.
  - [ ] M8.P9.L5.T2 — Verify the pure functions against hand-computed fixtures: the four claim statuses, a missing verifier and shared-origin counting, in crate `squad-core`, file `crates/squad-core/tests/trace_tallies.rs`.
  - [ ] M8.P9.L5.T3 — Write the user-facing mode section in `docs/modes/trace.md` in the README mode-section format, then set `ENABLED = true` in crate `squad-protocol`, file `crates/squad-protocol/src/trace.rs`.

## Phase M8.P10 — Rank

Owns only the `rank` module files listed in **New modes → Files each mode worker owns**, migration `0018_rank.sql` and `docs/modes/rank.md`.

- **M8.P10.L1 — Contracts and storage**
  - [ ] M8.P10.L1.T1 — Define `RankSetup`, `RankBatch`, `RankTally`, `RankExplanation`, `RankReport`, `RankCaseMaterial`, the transition enum and `validate_artifact` in crate `squad-protocol`, file `crates/squad-protocol/src/rank.rs`.
  - [ ] M8.P10.L1.T2 — Add the tables in the mode migration and commit and read the pair design, the deal with per-judgment A and B order, and accepted batches in crate `squad-store`, files `crates/squad-store/migrations/0018_rank.sql` and `crates/squad-store/src/rank.rs`.
  - [ ] M8.P10.L1.T3 — Add valid and invalid fixtures, including a batch missing or duplicating a pair, a winner outside A and B, and an explanation carrying an order or omitting a reported cycle, in crate `squad-protocol`, file `crates/squad-protocol/tests/rank.rs`, and step-visibility tests in crate `squad-store`, file `crates/squad-store/tests/rank.rs`.

- **M8.P10.L2 — Script and tally**
  - [ ] M8.P10.L2.T1 — Declare the script steps `COMPARING`, `AGGREGATING`, `EXPLAINING` and `REPORTING`, with `insufficient_comparisons`, in crate `squad-core`, file `crates/squad-core/src/orchestrator/rank.rs`, with commits and transitions in `crates/squad-core/src/router/rank.rs`.
  - [ ] M8.P10.L2.T2 — Implement the pair design (`Q = min(n(n − 1)/2, 3n)`, shuffled rings, `v = max(1, min(3, ⌊60m/Q⌋))`) and the balanced deal with the 60-judgment cap as seeded pure functions in crate `squad-core`, file `crates/squad-core/src/orchestrator/rank.rs`.
  - [ ] M8.P10.L2.T3 — Implement Bradley-Terry by the MM update with the phantom item, the 1e-9 and 1,000-iteration stop, deterministic ordering, gaps and gap labels as pure functions in crate `squad-core`, file `crates/squad-core/src/orchestrator/rank.rs`.
  - [ ] M8.P10.L2.T4 — Implement majority edges, the three-cycle audit with its ten-cycle display rule, split pairs and connectivity components as pure functions in crate `squad-core`, file `crates/squad-core/src/orchestrator/rank.rs`.
  - [ ] M8.P10.L2.T5 — Supply the `router/bounds.rs` step list from the mode's reservation table and derive `m + 1` calls and `2m + 3` reserved attempts, refusing items over 60 words, in crate `squad-core`, file `crates/squad-core/src/router/rank.rs`.

- **M8.P10.L3 — Prompts**
  - [ ] M8.P10.L3.T1 — Write `rank.compare.md` and `rank.explain.md` with the outputs and word budgets in **New modes**, replace the scaffold placeholders, and map typed inputs to placeholders, in crate `squad-core`, files `crates/squad-core/prompts/rank.*.md` and `crates/squad-core/src/prompts/inputs_rank.rs`; add the executor in crate `squad-agents`, file `crates/squad-agents/src/agents/rank.rs`.

- **M8.P10.L4 — Report, export and TUI**
  - [ ] M8.P10.L4.T1 — Assemble the report and `/export rank [id]`: the ordered list with scores, records and gap labels, the judge's account, cycles, split pairs, connectivity warnings, the design summary, and in export every judgment and the win matrix, in crate `squad-core`, file `crates/squad-core/src/export/rank.rs`.
  - [ ] M8.P10.L4.T2 — Render the `RANK` banner, the three dividers, gap markers between neighbours and judgment progress, in crate `squad-tui`, files `crates/squad-tui/src/widgets/rank.rs` and `crates/squad-tui/src/fx/rank.rs`, with view tests in `crates/squad-tui/tests/rank_view.rs`.

- **M8.P10.L5 — Verification and enable**
  - [ ] M8.P10.L5.T1 — Verify the rank acceptance tests in **New modes**, including identical bits across two runs, plus stop fencing and export, in crate `squad-core`, files `crates/squad-core/tests/rank_workflows.rs` and `crates/squad-core/tests/rank_export.rs`, and executor tests in crate `squad-agents`, file `crates/squad-agents/tests/agents_rank.rs`.
  - [ ] M8.P10.L5.T2 — Verify the pure functions against hand-computed fixtures: design fixtures for n of 3, 7, 8 and 40, Bradley-Terry fixtures to 1e-9, a planted cycle and a disconnected graph, in crate `squad-core`, file `crates/squad-core/tests/rank_tallies.rs`.
  - [ ] M8.P10.L5.T3 — Write the user-facing mode section in `docs/modes/rank.md` in the README mode-section format, then set `ENABLED = true` in crate `squad-protocol`, file `crates/squad-protocol/src/rank.rs`.

## Phase M8.P11 — Integration, docs and review

One worker, after P2 to P10 are merged.

- **M8.P11.L1 — Cross-mode verification**
  - [ ] M8.P11.L1.T1 — Verify all sixteen kinds compete for one active claim, each new kind's busy error names the active kind, and every new work kind is fenced by `/stop` and removed by `/clear`, in crate `squad-core`, files `crates/squad-core/tests/contest_storage.rs` and `crates/squad-core/tests/router_fencing.rs`.
  - [ ] M8.P11.L1.T2 — Run the blind-text redaction fixtures (case variants, provider and family tokens, multiword personas, overlapping tokens) over one blind projection of each new mode, and test guessed work IDs and alternate-context access for every new step-bound reader, in crate `squad-core`, file `crates/squad-core/tests/isolation.rs`, and crate `squad-store`, file `crates/squad-store/tests/readers.rs`.
  - [ ] M8.P11.L1.T3 — Crash and recover each new mode at assignment, artifact, barrier, human-step and next-work boundaries without redrawing private randomness, in crate `squad-core`, file `crates/squad-core/tests/mode_recovery.rs`.
  - [ ] M8.P11.L1.T4 — Verify the six-member call counts and reserved attempts of all nine modes against the **Call counts at a glance** table in one table-driven test, in crate `squad-core`, file `crates/squad-core/tests/new_mode_counts.rs`.
  - [ ] M8.P11.L1.T5 — Verify `squad send` output, exit codes and export for each new mode, and the `AWAITING_HUMAN` return and `/answer` continuation, in crate `squad-cli`, files `crates/squad-cli/tests/send_commands.rs` and `crates/squad-cli/tests/export.rs`.
  - [ ] M8.P11.L1.T6 — Confirm composed-stream and twelve-stream TUI benchmarks hold their budgets with the nine widgets registered, in crate `squad-tui`, files `crates/squad-tui/benches/composed_streams.rs` and `crates/squad-tui/benches/twelve_streams.rs`.

- **M8.P11.L2 — Documentation**
  - [ ] M8.P11.L2.T1 — Fold the nine `docs/modes/*.md` sections into `README.md`: the commands table, the `/set` and `/export` rows, **Contest modes at a glance** (sixteen modes), the contents list and one section per mode.
  - [ ] M8.P11.L2.T2 — Update the seven-mode wording, the verb and export lists and the variable list in `docs/prd/01-product-overview.md`, `docs/prd/03-command-grammar-and-parser.md` and `crates/squad-protocol/src/parser/PARSER.md`, and add the nine modes' visibility rows to `docs/prd/02-message-protocol-store-and-bus.md`.
  - [ ] M8.P11.L2.T3 — Add the new templates, their placeholders and word budgets to `docs/prd/09-prompt-pack.md` and `docs/prompt-pack.md`.
  - [ ] M8.P11.L2.T4 — Add call-count and cost examples for the nine modes to `docs/costs.md` and a changelog entry to `CHANGELOG.md`.

- **M8.P11.L3 — Review and live check**
  - [ ] M8.P11.L3.T1 — One review pass over the whole milestone against **New modes** and D73 to D87, with findings fixed or recorded.
  - [ ] M8.P11.L3.T2 — Run one live smoke of each mode on low-cost models with the release binary and record calls, cost and any deviation in `docs/live-smoke-<date>.md`.
