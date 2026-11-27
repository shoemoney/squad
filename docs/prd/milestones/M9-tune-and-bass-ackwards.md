# M9 — Tune and bass_ackwards

**Goal:** Add `$tune` and `$bass_ackwards_prd` as scripts over the shared contest engine, exactly as specified in [Tune and bass_ackwards](../12-tune-and-bass-ackwards.md), with one registry scaffold landing first so two mode workers can build in separate files.

**Depends on:** M8.P1 (the registry scaffold, `contest.artifact`, the human step, the `verbatim` matcher and the D75 repair attempt) merged.

**Exit criteria:**

- The scaffold lands every shared registry change in **Tune and bass_ackwards → Registry changes and scaffold**; all existing parser fixtures and their snapshots are unchanged, and every existing suite passes with both `ENABLED` switches false.
- No mode phase edits a file outside its ownership list. Shared-file needs found during a mode phase are routed back to the orchestrator.
- `$tune` makes at most `m + T + 3H + 3 + G(3T + 4)` calls and reserves `m + 13G + 15` normal calls and `2m + 27G + 33` attempts at admission; `$bass_ackwards_prd` makes `4m + 4` calls and reserves `8m + 12` attempts, or `4m + 5` and `8m + 15` with human review. Both match the **Call counts at a glance** table for six members and for three.
- Every `$tune` cap in the **Cost caps** table is a constant enforced by application code, and no `(candidateId, caseId)` is ever executed twice.
- Every aggregate (the train and held-out split, case and candidate scores, promotion, finalists, the final pick, DoD support counts and the freeze, the lens deal, the four PRD checks, levels and the critical path) is computed by application code from committed records and matches hand-computed fixtures. No judge schema has a field that could override one.
- Blind steps deliver only the authorized projection: no `tune.mutate` request carries a case, expectation, rubric description, output or held-out fact; no `tune.score` request carries a prompt; an executor request carries the candidate prompt and the case input and nothing else.
- Every PRD comment is quote-checked by the shared matcher and dropped without repair on failure; revision integrity and the add-only rule are enforced byte for byte.
- The PRD is saved to `<data_dir>/exports/prd-<slug>-<contestId>.md`, the path is shown in the TUI and printed by `squad send`, the save is idempotent across a crash, and `/export bass_ackwards_prd [id]` and `/export tune [id]` work.
- Member steps get two attempts with one repair after invalid output, executor calls two plain attempts, judge steps three; skipped members are named with their failed step in every report.
- Each mode stops cleanly at every state and recovers at every state without rerunning an accepted step or stored execution and without redrawing private randomness.
- Typeahead, TIP rotation, `/help` and the README list each mode only once its switch is true; MCP tools are unchanged (D100).
- One review pass covers the milestone, as D63 requires.

Heavy builds and full test suites for this milestone run on dev2 or dev3, never on dev1.

## Phase M9.P1 — Registry scaffold

One worker. Nothing in P2 or P3 starts until this phase is merged.

- **M9.P1.L1 — Protocol registries**
  - [x] M9.P1.L1.T1 — Add the `Tune` and `BassAckwardsPrd` variants to `ContestKind` and the typed `ContestState`, with the explicit `#[serde(rename = "bass_ackwards_prd")]`, in crate `squad-protocol`, file `crates/squad-protocol/src/contest.rs`.
  - [x] M9.P1.L1.T2 — Create the two protocol modules with `ENABLED = false`, the status enum from each mode's section, the `<Mode>State` struct with `validate()`, an empty transition enum and a stub `validate_artifact`, in crate `squad-protocol`, files `crates/squad-protocol/src/{tune,bass_ackwards_prd}.rs`, and export them from `crates/squad-protocol/src/lib.rs`.
  - [x] M9.P1.L1.T3 — Add the per-mode `<mode>.setup`, `<mode>.state` and `<mode>.preflight` envelope kinds, two `ContestTransition` variants and two `CaseMaterial` variants in crate `squad-protocol`, file `crates/squad-protocol/src/envelopes.rs`.
  - [x] M9.P1.L1.T4 — Add every work kind in the **Work kinds** table, with member, judge and the new executor classification, `attempt_limit` 2 and no repair for `tune.execute`, and the constants `TUNE_EXEC_VISIBLE_TOKENS` (1,024) and `PRD_VISIBLE_TOKENS` (8,192), in crate `squad-protocol`, files `crates/squad-protocol/src/work.rs` and `crates/squad-protocol/src/constants.rs`, and the deadline classes in crate `squad-agents`, file `crates/squad-agents/src/agents/mod.rs`.
  - [x] M9.P1.L1.T5 — Add `TUNE_GENERATIONS` (default 3, 1 to 6) and `PRD_HUMAN_REVIEW` (default 0, 0 or 1) to `VariableName`, `SquadConfig`, the optional environment seeds and the env help in crate `squad-protocol`, files `crates/squad-protocol/src/parser/types.rs` and `crates/squad-protocol/src/config.rs`, crate `squad-core`, file `crates/squad-core/src/router/variables.rs`, and crate `squad-cli`, file `crates/squad-cli/src/env-help.md`.
  - [x] M9.P1.L1.T6 — Cover the new kinds, the rename, the executor class and the variable bounds in crate `squad-protocol`, files `crates/squad-protocol/tests/contests.rs` and `crates/squad-protocol/tests/config.rs`.

- **M9.P1.L2 — Parser and fixtures**
  - [x] M9.P1.L2.T1 — Register the two verbs in the `verbs!` table and add two `ParsedInput` variants with the fields in the **Start grammar** table, gated on each mode's `ENABLED`, in crate `squad-protocol`, file `crates/squad-protocol/src/parser/types.rs`.
  - [x] M9.P1.L2.T2 — Parse the start forms: for `$tune` the prompt and task segments, the optional `-- @nick` executor segment, the quoted-message prompt and removal of the executor from the participants; for `$bass_ackwards_prd` the end result and the optional starting-point segment, in crate `squad-protocol`, file `crates/squad-protocol/src/parser/parser.rs`.
  - [x] M9.P1.L2.T3 — Add the `tune` and `bass_ackwards_prd` `ExportScope` variants with the `/export` usage strings in crate `squad-protocol`, files `crates/squad-protocol/src/parser/types.rs` and `parser.rs`.
  - [x] M9.P1.L2.T4 — Add every rule in the **Proposed parser strings** table with its canonical `ErrorCode` in crate `squad-protocol`, files `crates/squad-protocol/src/parser/errors.rs`, `validation_strings.json` and `validation_codes.json`.
  - [x] M9.P1.L2.T5 — Add `examples.json` cases for every start form, one refusal per string, the busy error for each new kind and the two exports, leaving existing cases and snapshots unchanged, in crate `squad-protocol`, file `crates/squad-protocol/src/parser/examples.json`; extend `parser_tests.rs` and `round_trip_tests.rs` under the test-only switch override.
  - [x] M9.P1.L2.T6 — Map the new parsed inputs to requests in crate `squad-protocol`, file `crates/squad-protocol/src/requests.rs`.

- **M9.P1.L3 — Store**
  - [x] M9.P1.L3.T1 — Create and register migrations `0019_tune.sql` and `0020_bass_ackwards_prd.sql`, each carrying its mode's kind and work-kind CHECK extensions and variable row, and stub store modules, in crate `squad-store`, files `crates/squad-store/migrations/`, `crates/squad-store/src/schema.rs`, `crates/squad-store/src/{tune,bass_ackwards_prd}.rs` and `crates/squad-store/src/lib.rs`.
  - [x] M9.P1.L3.T2 — Verify the migrations preserve all earlier released records and that clear and stop fencing cover every new work kind, including `tune.execute`, in crate `squad-store`, files `crates/squad-store/tests/mode_migrations.rs` and `crates/squad-store/tests/clear.rs`.

- **M9.P1.L4 — Engine dispatch, executor call and export directory**
  - [x] M9.P1.L4.T1 — Add dispatch arms to stub `tune` and `bass_ackwards_prd` modules for prepare, accepted work, human answer, stop, recover, load state and render export, in crate `squad-core`, files `crates/squad-core/src/router/mod.rs`, `router/stop.rs`, `router/recovery.rs`, `router/preflight.rs`, `orchestrator/mod.rs`, `orchestrator/preflight.rs`, `runtime/mod.rs` and `export/mod.rs`, creating the two stub files in each of `orchestrator/`, `router/`, `runtime/` and `export/`.
  - [x] M9.P1.L4.T2 — Implement the executor call (D90): bare request composition with the system and user form and the `{{input}}` single-message form, the frozen executor model, two plain attempts, no repair, the `V1` cap and the `truncated` mark, in crate `squad-core`, file `crates/squad-core/src/router/executor.rs`, and crate `squad-agents`, file `crates/squad-agents/src/agents/mod.rs`.
  - [x] M9.P1.L4.T3 — Price executor attempts in the reservation as `2 × executor calls` in crate `squad-core`, file `crates/squad-core/src/router/bounds.rs`.
  - [x] M9.P1.L4.T4 — Add `export_dir(data_dir)` and the atomic `write_export_file` (temporary file, rename, mode `0600`, directory `0700`) in crate `squad-core`, file `crates/squad-core/src/export/mod.rs`.
  - [x] M9.P1.L4.T5 — Register every template named in **Tune and bass_ackwards** in the manifest, embedded table and provenance list with placeholder files, add the two `inputs_<module>.rs` stubs and their composition dispatch, in crate `squad-core`, files `crates/squad-core/prompts/manifest.json`, `crates/squad-core/src/prompts/embedded.rs`, `provenance.rs`, `inputs.rs`, `render.rs` and `mod.rs`.
  - [x] M9.P1.L4.T6 — Test the executor call (both request forms byte for byte, no template or redaction, plain retry, `missing` after two failures, truncation) and the export file writer (atomic replace, modes, unwritable directory) in crate `squad-core`, files `crates/squad-core/tests/executor_call.rs` and `crates/squad-core/tests/export_file.rs`.
  - [x] M9.P1.L4.T7 — Extend the one-active-claim test to the two new kinds in crate `squad-core`, file `crates/squad-core/tests/contest_storage.rs`.

- **M9.P1.L5 — TUI and CLI registration**
  - [x] M9.P1.L5.T1 — Add the two `MODES` entries and the TIP example, each gated on `ENABLED`, in crate `squad-tui`, file `crates/squad-tui/src/typeahead.rs`.
  - [x] M9.P1.L5.T2 — Add per-mode view maps, widget and fx module registration and dispatch with stub files, the banner words `TUNE` and `BASS ACKWARDS`, and the preflight call-count line with member, executor and judge calls shown separately, in crate `squad-tui`, files `crates/squad-tui/src/session.rs`, `widgets/mod.rs`, `widgets/contest_status.rs`, `widgets/preflight.rs`, `fx/mod.rs` and `fx/dispatch.rs`, and crate `squad-core`, file `crates/squad-core/src/banner/mod.rs`.
  - [x] M9.P1.L5.T3 — Add the two export scopes, generic report output, and the saved-path line and `export.ready` JSON record for a `bass_ackwards_prd.saved` artifact in `squad send`, in crate `squad-cli`, files `crates/squad-cli/src/args.rs`, `send.rs`, `output.rs` and `adapter.rs`.
  - [x] M9.P1.L5.T4 — Verify typeahead and `/help` hide the disabled modes and that MCP tool discovery is unchanged in crate `squad-tui`, file `crates/squad-tui/tests/actions_shell.rs`, and crate `squad-cli`, file `crates/squad-cli/tests/mcp.rs`.

### M9.P1 evidence and handoff

- All two-mode variants, exact statuses, 14 work kinds, explicit underscore serde names, executor classification, V1/PRD caps and both room-variable bounds are covered by `squad-protocol/tests/contests.rs` and `tests/config.rs`.
- Additional parser fixtures cover inline and quoted prompts, optional executor removal, the PRD starting point, busy/disabled errors, exports and validation strings. `parser/new_modes_tests.rs` checks round trips, prompt admission caps, PRD note caps and enabled export usage. A quoted tune prompt uses `PlanSource` to retain its source identity until admission.
- Migrations 0019 and 0020 extend their own kind/work/event CHECKs and variable rows. `squad-store/tests/mode_migrations.rs` verifies both released and populated M8 upgrades, preserved work/indexes/FKs, default-off PRD review and bounds. `squad-core/tests/human_step.rs` fences every new work kind on stop and removes them on clear.
- `squad-core/tests/executor_call.rs` captures both bare request forms byte-for-byte, frozen binding, absence of template/persona/history/redaction/reasoning/schema inputs, two plain attempts then missing, retained truncated output, and the hand-computed two-attempt reservation. `tests/export_file.rs` verifies atomic replacement, stable destination, Unix modes, failed destination and invalid paths. `tests/contest_storage.rs` checks all eighteen claim kinds.
- `squad-cli/tests/output.rs` verifies the saved-path line and exact `export.ready` record. Existing MCP discovery and workflows remain unchanged; TUI action tests verify disabled help/typeahead/TIP filtering. Prompt inventory/provenance tests include the four tune and nine PRD placeholders, with no template for the bare executor.
- Shared dispatch hooks and the worker-owned paths are listed in [Scaffold ownership](M8-scaffold-ownership.md). The corrected `origin/tune-bass-spec` commit 842e982 is merged. Both modes remain disabled; P2/P3 implementation work is untouched.
- Final gates, test totals and per-crate changed paths are recorded in [Scaffold report](M8-scaffold-report.md).

## Phase M9.P2 — Tune

Owns only the `tune` module files listed in **New modes → Files each mode worker owns**, migration `0019_tune.sql` and `docs/modes/tune.md`.

- **M9.P2.L1 — Contracts and storage**
  - [x] M9.P2.L1.T1 — Define `TuneSetup`, `TuneCaseProposal`, `TuneSuite`, `TuneCandidate`, `TuneExecution`, `TuneScoreSheet`, `TuneDigest`, `TuneGeneration`, `TuneHoldout`, `TuneReport`, `TuneCaseMaterial`, the transition enum and `validate_artifact`, with the cap constants from the **Cost caps** table, in crate `squad-protocol`, file `crates/squad-protocol/src/tune.rs`.
  - [x] M9.P2.L1.T2 — Add the tables to the mode migration and commit and read proposals, the frozen suite with its split and case labels, the mutator rotation, candidates with parent and generation, executions unique on `(candidateId, caseId)`, sheets with per-case label maps, digests and the champion chain, in crate `squad-store`, files `crates/squad-store/migrations/0019_tune.sql` and `crates/squad-store/src/tune.rs`.
  - [x] M9.P2.L1.T3 — Add valid and invalid fixtures, including four cases from one proposal, a freeze with an invented case or three cases from one member, a criterion with no source, a score of 5, a sheet missing a cell, a low score with no note and a mutation equal to its parent, in crate `squad-protocol`, file `crates/squad-protocol/tests/tune.rs`, and step-visibility tests proving the suite's audience excludes members in crate `squad-store`, file `crates/squad-store/tests/tune.rs`.

- **M9.P2.L2 — Script and tally**
  - [x] M9.P2.L2.T1 — Declare the script steps `BUILDING_CASES`, `FREEZING`, the split, then `EXECUTING` and `SCORING` for generation 0, the `MUTATING`, `EXECUTING`, `SCORING` loop bounded by `TUNE_GENERATIONS`, then `HOLDOUT_EXECUTING`, `HOLDOUT_SCORING` and `REPORTING`, with the reasons `insufficient_cases`, `nothing_to_fix`, `no_improvement`, `holdout_unavailable`, `tuned` and `original_kept`, in crate `squad-core`, file `crates/squad-core/src/orchestrator/tune.rs`, with commits and transitions in `crates/squad-core/src/router/tune.rs`.
  - [x] M9.P2.L2.T2 — Implement the seeded split (`H = min(3, ⌊C/2⌋)`), the mutator rotation, `Lmax`, case score `q`, candidate score `S`, the generation's best with its tie-breaks, the 0.05 promotion rule, saturation, finalist selection and the held-out pick as pure functions in crate `squad-core`, file `crates/squad-core/src/orchestrator/tune.rs`.
  - [x] M9.P2.L2.T3 — Enforce the freeze rules (byte-identical cases, at most two per member, sourced criteria), the mutation rules (`Lmax`, differs from parent, `{{input}}` count), the admission refusals for prompt size and placeholder count, and the six-word leak check on notes, in crate `squad-core`, file `crates/squad-core/src/router/tune.rs`.
  - [x] M9.P2.L2.T4 — Dispatch executor calls through the shared executor path, store each result once, reuse stored outputs in later sheets, drop a train case with a missing baseline output, mark candidates `incomplete`, and drop a held-out case missing for any finalist, in crate `squad-core`, files `crates/squad-core/src/router/tune.rs` and `crates/squad-core/src/runtime/tune.rs`.
  - [x] M9.P2.L2.T5 — Build each sheet's blind projection (original, champion and complete candidates under letters shuffled per case, no prompts) and each `TuneDigest` (champion only, labels, criterion names, scores, notes) in crate `squad-core`, file `crates/squad-core/src/orchestrator/tune.rs`.
  - [x] M9.P2.L2.T6 — Supply the `router/bounds.rs` step list from the mode's reservation table and derive `m + 13G + 15` normal calls and `2m + 27G + 33` reserved attempts at the caps, releasing the difference after the freeze, on early ends and on empty generations, in crate `squad-core`, file `crates/squad-core/src/router/tune.rs`.

- **M9.P2.L3 — Prompts**
  - [x] M9.P2.L3.T1 — Write `tune.cases.md`, `tune.freeze.md`, `tune.score.md` with its fixed 0 to 4 anchors, and `tune.mutate.md`, with the outputs and word budgets in **Tune and bass_ackwards**, replace the scaffold placeholders, and map typed inputs to placeholders, in crate `squad-core`, files `crates/squad-core/prompts/tune.*.md` and `crates/squad-core/src/prompts/inputs_tune.rs`; add the agent work handler in crate `squad-agents`, file `crates/squad-agents/src/agents/tune.rs`.

- **M9.P2.L4 — Report, export and TUI**
  - [x] M9.P2.L4.T1 — Assemble the report and `/export tune [id]`: result line, the winning prompt byte-identical to its artifact, the app-computed unified line diff, the lineage's stated changes, the held-out and train tables, regressions, per-generation scores, remaining failures, and in export the reusable suite, every candidate, output, sheet and digest, in crate `squad-core`, file `crates/squad-core/src/export/tune.rs`.
  - [x] M9.P2.L4.T2 — Render the `TUNE` banner, the five dividers, the collapsed test set, the per-generation candidate rows with the promotion mark, execution progress and the four result lines, in crate `squad-tui`, files `crates/squad-tui/src/widgets/tune.rs` and `crates/squad-tui/src/fx/tune.rs`, with view tests in `crates/squad-tui/tests/tune_view.rs`.

- **M9.P2.L5 — Verification and enable**
  - [x] M9.P2.L5.T1 — Verify the tune acceptance tests in **Tune and bass_ackwards**, including captured mutate, score and executor requests, the named-executor form, no repeated execution and recovery at every state, plus stop fencing and export, in crate `squad-core`, files `crates/squad-core/tests/tune_workflows.rs` and `crates/squad-core/tests/tune_export.rs`, and handler tests in crate `squad-agents`, file `crates/squad-agents/tests/agents_tune.rs`.
  - [x] M9.P2.L5.T2 — Verify the pure functions against hand-computed fixtures: splits for four, five and six cases, rotation for m of 3, 4 and 12, score arithmetic, promotion at 0.04 and 0.05, each tie-break, and every final-pick case, in crate `squad-core`, file `crates/squad-core/tests/tune_tallies.rs`.
  - [x] M9.P2.L5.T3 — Write the user-facing mode section in `docs/modes/tune.md` in the README mode-section format, then set `ENABLED = true` in crate `squad-protocol`, file `crates/squad-protocol/src/tune.rs`.

### Tune worker evidence

All fifteen M9.P2 tasks are complete. The owned contracts, durable journal, pure script/tallies, caps, prompts, report/export and human view are implemented. The explicit owner-approved shared exception routes production judge and executor grants through the existing runner and updates enabled-mode parser/TUI fixtures. Other mode switches stay unchanged.

There are 35 Tune checks, eight added during the resume, including twelve wiremock workflows through the default launcher and public parser/room submission path. They prove privacy projections, bare execution, named-executor removal/completion, one-shot driving, no duplicate execution, repairs/skips/quorum, empty/incomplete generations, baseline and held-out case drops, final picks, export, immutable seed/suite/sheet maps and accepted-work attempt counts, stop at every paid phase, preflight and prior-reply waiting recovery. Reporting/completion are atomic, and a terminal restart makes no provider call. Stop conserves unknown-charge holds while releasing unstarted work. Reports name each skipped member's failed step.

**Final gates (dev3, 2026-11-26):** on `4682a4ec3645f9bf1d51ca27535458ac26fe9967`, `CARGO_BUILD_JOBS=4 cargo test --workspace --no-fail-fast` passed 1,360 unique tests, zero failures and 4 ignored entries (1,361 passing summaries including the duplicate WAL child). All-target workspace Clippy with warnings denied, formatting and whitespace checks pass. The release CLI/package gates pass.

**Approved live smoke:** one optimized `squad send`, default six-model roster/default executor, fresh private data, `SQUAD_DAILY_MAX_USD=150`, `TUNE_GENERATIONS=1`, `--env-file /dev/null`. Exit 0, `tuned`, 31 Tune calls/31 Tune attempts plus 4 startup probes, **$0.520068 actual**, **149.53s**, **$24.999033 initial reservation**, **$0.000000 final reservation**, 0 unknown-cost calls. The command, per-kind measured costs, real report excerpt and qualified default-G projection are in `docs/modes/tune.md`. `ENABLED = true`; parser/typeahead/TIP/help checks pass. No required runtime integration change remains; optional shared compatibility cleanup is documented there. Broader M9.P4 work remains the integrator's scope.

## Phase M9.P3 — Bass_ackwards_prd

Owns only the `bass_ackwards_prd` module files listed in **New modes → Files each mode worker owns**, migration `0020_bass_ackwards_prd.sql` and `docs/modes/bass_ackwards_prd.md`.

- **M9.P3.L1 — Contracts and storage**
  - [x] M9.P3.L1.T1 — Define `BassAckwardsPrdSetup`, `BassAckwardsPrdDoneList`, `BassAckwardsPrdDone`, `BassAckwardsPrdTree`, `BassAckwardsPrdDocument`, `BassAckwardsPrdReview`, `BassAckwardsPrdMustHaves`, `BassAckwardsPrdDecisions`, `BassAckwardsPrdChecks`, `BassAckwardsPrdSaved`, `BassAckwardsPrdReport`, `BassAckwardsPrdCaseMaterial`, the transition enum and `validate_artifact`, with explicit serde renames, in crate `squad-protocol`, file `crates/squad-protocol/src/bass_ackwards_prd.rs`.
  - [x] M9.P3.L1.T2 — Add the tables to the mode migration and commit and read done lists, the grouping and frozen definition of done, trees, PRD versions with permanent keys, the lens deal and readback assignments, quote checks, decisions, human notes, check results and the saved record, in crate `squad-store`, files `crates/squad-store/migrations/0020_bass_ackwards_prd.sql` and `crates/squad-store/src/bass_ackwards_prd.rs`.
  - [x] M9.P3.L1.T3 — Add valid and invalid fixtures, including a reworded DoD group, a tree with a cycle, depth five or a node with no `serves`, a document with a dangling dependency, a dependency on a later milestone or an unsourced line item, a must-have list of four, and a decision list that omits a comment, in crate `squad-protocol`, file `crates/squad-protocol/tests/bass_ackwards_prd.rs`, and step-visibility tests in crate `squad-store`, file `crates/squad-store/tests/bass_ackwards_prd.rs`.

- **M9.P3.L2 — Script and tally**
  - [x] M9.P3.L2.T1 — Declare the script steps `DEFINING_DONE`, `FREEZING_DONE`, `BUILDING_TREES`, `MERGING`, the coverage tally, `REVIEWING`, `REVISING`, `MUST_HAVES`, `FINALIZING`, the optional `AWAITING_HUMAN` and `APPLYING`, `CHECKING` and `SAVING`, with the reasons `insufficient_done`, `no_shared_done`, `insufficient_trees`, `saved`, `saved_with_findings` and `save_failed`, and the skip of a judge call when a round has nothing to decide, in crate `squad-core`, file `crates/squad-core/src/orchestrator/bass_ackwards_prd.rs`, with commits and transitions in `crates/squad-core/src/router/bass_ackwards_prd.rs`.
  - [x] M9.P3.L2.T2 — Implement the DoD freeze (distinct-member support of at least two, cap of twelve, ID order), the seeded lens deal and readback milestone rotation, tree validation, coverage, the four checks, levels and the critical path as pure functions in crate `squad-core`, file `crates/squad-core/src/orchestrator/bass_ackwards_prd.rs`.
  - [x] M9.P3.L2.T3 — Mint permanent keys on every committed version, validate document structure on every judge output, and enforce the integrity rule after `REVISING` and `APPLYING` and the add-only rule after `FINALIZING`, in crate `squad-core`, file `crates/squad-core/src/router/bass_ackwards_prd.rs`.
  - [x] M9.P3.L2.T4 — Render each PRD version to the house-style Markdown, run `verbatim` on every comment quote against the rendered text that member was sent, and drop failures without repair, in crate `squad-core`, files `crates/squad-core/src/export/bass_ackwards_prd.rs` and `crates/squad-core/src/router/bass_ackwards_prd.rs`.
  - [x] M9.P3.L2.T5 — Implement the optional human step: enter `AWAITING_HUMAN` when `PRD_HUMAN_REVIEW` is 1, accept `/answer` notes within the ten-note and 500-word caps, close on `/continue`, and run `APPLYING` only when a note exists, in crate `squad-core`, file `crates/squad-core/src/router/bass_ackwards_prd.rs`.
  - [x] M9.P3.L2.T6 — Implement the save: slug, the path under `export_dir`, report commit before the write, the atomic write through `write_export_file`, the `bass_ackwards_prd.saved` commit, the idempotent restart, the `save_failed` path, and saving the last good version after a revision or final-assembly judge failure, in crate `squad-core`, file `crates/squad-core/src/runtime/bass_ackwards_prd.rs`.
  - [x] M9.P3.L2.T7 — Supply the `router/bounds.rs` step list from the mode's reservation table and derive `4m + 4` calls and `8m + 12` reserved attempts, or `4m + 5` and `8m + 15` with human review, in crate `squad-core`, file `crates/squad-core/src/router/bass_ackwards_prd.rs`.

- **M9.P3.L3 — Prompts**
  - [x] M9.P3.L3.T1 — Write `bass_ackwards_prd.done.md`, `freeze.md`, `tree.md`, `merge.md`, `review.md` with its three lens blocks, `revise.md`, `must_have.md`, `finalize.md` and `apply.md`, with the outputs and word budgets in **Tune and bass_ackwards**, replace the scaffold placeholders, and map typed inputs to placeholders, in crate `squad-core`, files `crates/squad-core/prompts/bass_ackwards_prd.*.md` and `crates/squad-core/src/prompts/inputs_bass_ackwards_prd.rs`; add the agent work handler in crate `squad-agents`, file `crates/squad-agents/src/agents/bass_ackwards_prd.rs`.

- **M9.P3.L4 — Report, export and TUI**
  - [x] M9.P3.L4.T1 — Assemble the report and `/export bass_ackwards_prd [id]`: the saved path, the check line, the final PRD with its appendix, the definition of done with support, provenance resolved to nicks, the review and must-have logs, and in export every list, tree, version, comment and decision, in crate `squad-core`, file `crates/squad-core/src/export/bass_ackwards_prd.rs`.
  - [x] M9.P3.L4.T2 — Render the `BASS ACKWARDS` banner, the seven dividers, collapsed trees, the decision table, the `PRD saved to <path>` line and the `LAST LOOK` human step through the shared widget, in crate `squad-tui`, files `crates/squad-tui/src/widgets/bass_ackwards_prd.rs` and `crates/squad-tui/src/fx/bass_ackwards_prd.rs`, with view tests in `crates/squad-tui/tests/bass_ackwards_prd_view.rs`.

- **M9.P3.L5 — Verification and enable**
  - [x] M9.P3.L5.T1 — Verify the bass_ackwards_prd acceptance tests in **Tune and bass_ackwards**, including blind captured requests, one lens per review request, dropped unquoted comments, byte-level integrity, the crash between report and write, and the human step, plus stop fencing and export, in crate `squad-core`, files `crates/squad-core/tests/bass_ackwards_prd_workflows.rs` and `crates/squad-core/tests/bass_ackwards_prd_export.rs`, and handler tests in crate `squad-agents`, file `crates/squad-agents/tests/agents_bass_ackwards_prd.rs`.
  - [x] M9.P3.L5.T2 — Verify the pure functions against hand-computed fixtures: the DoD freeze, the lens deal for m of 3, 6 and 7, the four checks, levels and the critical path for a chain, a diamond and two equal chains, and the slug, in crate `squad-core`, file `crates/squad-core/tests/bass_ackwards_prd_tallies.rs`.
  - [x] M9.P3.L5.T3 — Write the user-facing mode section in `docs/modes/bass_ackwards_prd.md` in the README mode-section format, then set `ENABLED = true` in crate `squad-protocol`, file `crates/squad-protocol/src/bass_ackwards_prd.rs`.

### M9.P3 evidence and handoff

- All 16 P3 tasks are implemented in the owned protocol, storage, agent, script/router/runtime, prompt, export and TUI files. The mode is enabled. The owner explicitly authorized the one-line shared judge-dispatch fix (`ce2d07a`), which routes new-mode case work to the registered handler.
- Seven mode-specific test targets pass 54 executions with zero failures or ignores (33 distinct functions, with shared contract fixtures reused). `bass_ackwards_prd_workflows.rs` exercises both the real `RoomHandle` runtime and a scoped engine/handler harness, including 28/29/26-call paths, D75 repair and quorum failure, failed-revision last-good saving, quote dropping, one-lens and identity-free requests, human conflicts and stop, disk failure, and reopen at every state through report-before-write. Protocol fixtures cover grouping, tree and document rejection; store tests cover durable records, cascade and scoped-reader refusal; tally/export/view tests cover counts, graph arithmetic, keys, house style and saved path.
- The owner-approved D101 follow-up smoke used the default six-model roster, release binary, fresh private data directory, `/dev/null` environment file and `SQUAD_DAILY_MAX_USD=150`. It completed `COMPLETE/saved`, CLI exit 0, with final PRD version 3 and all four checks passing. Confirmed cost was $1.992752, wall time 495.946 seconds, 28 logical mode calls / 30 attempts plus 4 startup probes. Preflight reserved $50.383359 across 60 attempts. Two late receipts were retrieved without model calls and applied through the existing accounting writer: zero unknown charges and zero remaining reservation. File bytes, SHA-256, mode 0600, CLI path and the exact first 40 lines are in [the mode docs](../../modes/bass_ackwards_prd.md). Revision and final assembly both succeeded on their first attempt. This replaces the earlier failed full-document revision example.
- The final compact-key implementation uses a persisted monotonic counter, never reuses removed keys, and preserves M/P/L display numbering independently. Assembly prompts request compact JSON; regression tests preserve byte integrity and counter state.
- With `CARGO_BUILD_JOBS=4`, workspace Clippy (`--all-targets -- -D warnings`), format and release checks pass. The full `cargo test --workspace --no-fail-fast` gate passed 1,373 tests, failed seven stale disabled-mode/list-count assertions across five shared files, and ignored four tests. All 54 mode-specific executions pass. Shared expectation updates remain for the integrator; no shared production change beyond the previously authorized judge dispatch was made.

D101 follow-up replaces full-document judge revisions with permanent-key edits, validates the reconstructed candidate, and applies explicit fields to stored bytes. The input reservation includes accumulated patches; the 8,192-token visible ceiling is unchanged. New tests cover the large 60-task overflow regression, sparse save through the real runtime, rejected identity/target mutations, add-only provenance and placement, redacted-field preservation, structured-output schema compatibility, and truncation without partial application.

Shared test maintenance still needed under the ownership boundary (reviewable commit `e7fc4b9` on `origin/mode-bass-ackwards-prd-shared-tests`):

- `crates/squad-protocol/src/parser/new_modes_tests.rs`: test a disabled fixture as a successful parse when that mode is now built.
- `crates/squad-protocol/src/parser/parser_tests.rs`: compare export-usage strings with the enabled scope list while preserving the released grammar snapshot baseline.
- `crates/squad-tui/tests/actions_shell.rs`: compare parser/help/TIP/typeahead visibility with each mode's actual switch.
- `crates/squad-tui/tests/contest_actions.rs`: derive help/export counts and report expectations from the enabled registry.
- `crates/squad-tui/tests/typeahead.rs`: derive menu/TIP counts from enabled modes and retain the released starter-layout baseline independently of optional mode membership.

## Phase M9.P4 — Integration, docs and review

One worker, after P2 and P3 are merged.

- **M9.P4.L1 — Cross-mode verification**
  - [ ] M9.P4.L1.T1 — Verify both kinds compete for the one active claim with every other kind, each busy error names the active kind, and every new work kind is fenced by `/stop` and removed by `/clear`, in crate `squad-core`, files `crates/squad-core/tests/contest_storage.rs` and `crates/squad-core/tests/router_fencing.rs`.
  - [ ] M9.P4.L1.T2 — Run the blind-text redaction fixtures over one blind projection of each mode, confirm executor requests are exempt and byte-exact, and test guessed work IDs and alternate-context access for every new step-bound reader, in crate `squad-core`, file `crates/squad-core/tests/isolation.rs`, and crate `squad-store`, file `crates/squad-store/tests/readers.rs`.
  - [ ] M9.P4.L1.T3 — Crash and recover each mode at assignment, artifact, barrier, execution, human-step, save and next-work boundaries without redrawing private randomness or repeating an execution, in crate `squad-core`, file `crates/squad-core/tests/mode_recovery.rs`.
  - [x] M9.P4.L1.T4 — Add both modes' six-member and three-member call counts and reserved attempts to the table-driven count test, in crate `squad-core`, file `crates/squad-core/tests/new_mode_counts.rs`.
  - [ ] M9.P4.L1.T5 — Verify `squad send` output, exit codes and export for each mode, the printed PRD path and `export.ready` record, and the `AWAITING_HUMAN` return with `PRD_HUMAN_REVIEW=1`, in crate `squad-cli`, files `crates/squad-cli/tests/send_commands.rs` and `crates/squad-cli/tests/export.rs`.

- **M9.P4.L2 — Documentation**
  - [x] M9.P4.L2.T1 — Fold `docs/modes/tune.md` and `docs/modes/bass_ackwards_prd.md` into `README.md`: the commands table, the `/set` and `/export` rows, **Contest modes at a glance**, the contents list and one section per mode.
  - [x] M9.P4.L2.T2 — Update the mode count, the verb and export lists and the variable list in `docs/prd/01-product-overview.md`, `docs/prd/03-command-grammar-and-parser.md` and `crates/squad-protocol/src/parser/PARSER.md`; add the two modes' visibility rows and the executor work class to `docs/prd/02-message-protocol-store-and-bus.md`; add `<data_dir>/exports/` to the data layout in `docs/prd/05-foundations-room-api-and-cli.md`.
  - [x] M9.P4.L2.T3 — Add the new templates, their placeholders and word budgets to `docs/prd/09-prompt-pack.md` and `docs/prompt-pack.md`.
  - [ ] M9.P4.L2.T4 — Add call-count and cost examples for both modes, with executor calls priced separately, to `docs/costs.md` and a changelog entry to `CHANGELOG.md`.

- **M9.P4.L3 — Review and live check**
  - [ ] M9.P4.L3.T1 — One review pass over the whole milestone against **Tune and bass_ackwards** and D88 to D100, with findings fixed or recorded.
  - [ ] M9.P4.L3.T2 — Run one live smoke of each mode on low-cost models with the release binary, `$tune` once with the default executor and once with a named one, and record calls, cost, the saved PRD path and any deviation in `docs/live-smoke-<date>.md`.

Integration evidence for M9.P4.L1.T4 (2026-11-26): `new_mode_counts.rs` passes public preflights for both modes with three and six members, Tune at default G=3 and G=1, and PRD with human review enabled. No provider completion is dispatched. The regression also found and fixed the inline PRD review freeze ordering. Full integration gate evidence is in [v0.2.0 integration](v0.2.0-integration.md).

Integration documentation evidence (2026-11-26): README folds the currently enabled mode guides with command, variable, contents and export rows. Product overview, command grammar and parser notes enumerate eighteen designed modes, nineteen scopes and eight stored variables while preserving enable switches. The protocol adds eleven step-visibility rows; Foundations documents automatic PRD exports. Both prompt-pack guides document all fifty new templates, their two shared JSON inputs, output bounds, bare Tune execution and the twenty-four blind judge compositions.
