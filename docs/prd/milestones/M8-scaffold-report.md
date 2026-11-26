# M8.P1 and M9.P1 scaffold handoff

All eleven modes are registered with `ENABLED = false`. Disabled starts return
`COMMAND_NOT_IMPLEMENTED` with the mode named. The seven shipped modes retain
their original parser fixtures and runtime suites. No live API calls, main pushes
or tags were used. This handoff implements only the two scaffold phases.

## Validation

On dev3 with `CARGO_BUILD_JOBS=4`:

- `cargo test --workspace --no-fail-fast`: 1326 passed summaries, zero failed,
  4 ignored entries: three manual performance/memory checks and the PTY
  subprocess entry point (exercised by its parent test). The WAL subprocess repeats one test
  in Cargo's summaries, so the unique passing-test count is 1325.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo fmt --all -- --check`: passed.
- Original 166 parser fixture objects and their snapshot are unchanged; 129 added
  fixtures exercise the new grammar, errors and round trips through an isolated
  test-only enable override.
- Tested code head: `23bec2bc7a2afc65fa65f8c2773fb99696eb5d32`.
- Merged release audit branch at `59e9de6ed6591c687d17e746a612afa0d0179bc5`; all gates above were rerun after the merge.
- Merged the corrected tune/bass specification branch at `842e982`. The initially
  advertised spec commit had already been imported before the owner corrected its
  provenance; both copies have identical specification content apart from the
  corrected section-11 reference. The corrected ref is in this branch's ancestry.

## Contract corrections and implementation choices

- `ContestTransition` lives in protocol `contest.rs`, rather than envelopes.rs.
  Store `dispatch` is a re-export of physical `leases.rs`; its SQL attempt lists,
  deadlines and recovery resets needed explicit registration.
- The generic artifact carrier and `contest.artifact` already existed for shipped
  modes. New header/body fields are additive optional fields, checked as required
  for new-mode records, while legacy `data`, `workId` and `sourceRefs` keep their
  released serialization. This avoids changing the seven existing modes' wire data.
- Additional shared registry sites include core state/room/driver, authorization,
  contest projections and runtime member/judge dispatch; store work reservation
  metadata, readers, clear, repairs and the integrated prompt-provenance registry; and TUI actions/composition/adapters/case
  projections. Their exact paths appear below and in the ownership handoff.
- Prompt rendering already selects manifest compositions dynamically. Owned input
  adapters and manifest/embedded/provenance registrations provide the required seam
  without adding a per-template match to render.rs.
- The release audit introduced store schema provenance. It dispatches to each
  owned protocol module's `output_schema(&WorkKind)` callback, initially returning
  none while disabled, so mode workers need no edits to that shared registry.
- Quoted tune prompts use `PlanSource` to preserve the quoted record's identity
  until admission. The bare executor has no prompt template. The atomic export
  helpers return I/O errors so callers can commit `save_failed`.
- M9's final binding spec supersedes the earlier request for empty 0019/0020:
  those migrations contain their mode's registry CHECK extensions and variable row.
  The nine M8 per-mode migrations remain empty placeholders.
- Frozen generic mode plans use `ContestPreflight`, positive `reservedAttempts`,
  per-kind `attemptShares` and `stepInputBounds` in settings. Core dependencies
  for randomness and SHA-256 are ready for owned implementations.

## Exact worker-owned files

[Scaffold ownership](M8-scaffold-ownership.md) enumerates every owned path for
forge, forecast, decide, negotiate, blow_it_up, readback, debugging, trace, rank,
tune and bass_ackwards_prd, including their exact prompt assets, migration, tests
and documentation paths. `debugging` is the Rust module; `debug` is the wire and
prompt name. Shared-file changes return to the integrator.

## Files touched per crate

This inventory is the scaffold diff against the integrated release audit branch;
release-only audit changes are excluded.

| Crate | Changed paths |
|---|---:|
| `squad-agents` | 17 |
| `squad-cli` | 8 |
| `squad-core` | 145 |
| `squad-protocol` | 33 |
| `squad-store` | 36 |
| `squad-tui` | 39 |

### squad-agents

- `crates/squad-agents/src/agents/bass_ackwards_prd.rs`
- `crates/squad-agents/src/agents/blow_it_up.rs`
- `crates/squad-agents/src/agents/census.rs`
- `crates/squad-agents/src/agents/crosscheck.rs`
- `crates/squad-agents/src/agents/debugging.rs`
- `crates/squad-agents/src/agents/decide.rs`
- `crates/squad-agents/src/agents/forecast.rs`
- `crates/squad-agents/src/agents/forge.rs`
- `crates/squad-agents/src/agents/grill.rs`
- `crates/squad-agents/src/agents/mod.rs`
- `crates/squad-agents/src/agents/negotiate.rs`
- `crates/squad-agents/src/agents/rank.rs`
- `crates/squad-agents/src/agents/readback.rs`
- `crates/squad-agents/src/agents/trace.rs`
- `crates/squad-agents/src/agents/tune.rs`
- `crates/squad-agents/src/openrouter.rs`
- `crates/squad-agents/src/registry.rs`

### squad-cli

- `crates/squad-cli/src/adapter.rs`
- `crates/squad-cli/src/args.rs`
- `crates/squad-cli/src/env-help.md`
- `crates/squad-cli/src/output.rs`
- `crates/squad-cli/src/room_api.rs`
- `crates/squad-cli/src/send.rs`
- `crates/squad-cli/tests/output.rs`
- `crates/squad-cli/tests/snapshots/output__state_daily_spend_and_trophies.snap`

### squad-core

- `crates/squad-core/Cargo.toml`
- `crates/squad-core/prompts/bass_ackwards_prd.apply.md`
- `crates/squad-core/prompts/bass_ackwards_prd.done.md`
- `crates/squad-core/prompts/bass_ackwards_prd.finalize.md`
- `crates/squad-core/prompts/bass_ackwards_prd.freeze.md`
- `crates/squad-core/prompts/bass_ackwards_prd.merge.md`
- `crates/squad-core/prompts/bass_ackwards_prd.must_have.md`
- `crates/squad-core/prompts/bass_ackwards_prd.review.md`
- `crates/squad-core/prompts/bass_ackwards_prd.revise.md`
- `crates/squad-core/prompts/bass_ackwards_prd.tree.md`
- `crates/squad-core/prompts/blow_it_up.attack.md`
- `crates/squad-core/prompts/blow_it_up.challenge.md`
- `crates/squad-core/prompts/blow_it_up.rule.md`
- `crates/squad-core/prompts/debug.hypothesize.md`
- `crates/squad-core/prompts/debug.merge.md`
- `crates/squad-core/prompts/debug.report.md`
- `crates/squad-core/prompts/debug.select.md`
- `crates/squad-core/prompts/debug.test.md`
- `crates/squad-core/prompts/decide.criteria.md`
- `crates/squad-core/prompts/decide.explain.md`
- `crates/squad-core/prompts/decide.freeze.md`
- `crates/squad-core/prompts/decide.options.md`
- `crates/squad-core/prompts/decide.score.md`
- `crates/squad-core/prompts/decide.shortlist.md`
- `crates/squad-core/prompts/forecast.cruxes.md`
- `crates/squad-core/prompts/forecast.estimate.md`
- `crates/squad-core/prompts/forecast.frame.md`
- `crates/squad-core/prompts/forecast.revise.md`
- `crates/squad-core/prompts/forge.draft.md`
- `crates/squad-core/prompts/forge.fuse.md`
- `crates/squad-core/prompts/forge.judge.md`
- `crates/squad-core/prompts/forge.rank.md`
- `crates/squad-core/prompts/manifest.json`
- `crates/squad-core/prompts/negotiate.brief.md`
- `crates/squad-core/prompts/negotiate.counter.md`
- `crates/squad-core/prompts/negotiate.map.md`
- `crates/squad-core/prompts/negotiate.move.md`
- `crates/squad-core/prompts/negotiate.package.md`
- `crates/squad-core/prompts/negotiate.playbook.md`
- `crates/squad-core/prompts/rank.compare.md`
- `crates/squad-core/prompts/rank.explain.md`
- `crates/squad-core/prompts/readback.diff.md`
- `crates/squad-core/prompts/readback.interpret.md`
- `crates/squad-core/prompts/readback.rewrite.md`
- `crates/squad-core/prompts/trace.assemble.md`
- `crates/squad-core/prompts/trace.extract.md`
- `crates/squad-core/prompts/trace.merge.md`
- `crates/squad-core/prompts/trace.verify.md`
- `crates/squad-core/prompts/tune.cases.md`
- `crates/squad-core/prompts/tune.freeze.md`
- `crates/squad-core/prompts/tune.mutate.md`
- `crates/squad-core/prompts/tune.score.md`
- `crates/squad-core/src/banner/mod.rs`
- `crates/squad-core/src/export/bass_ackwards_prd.rs`
- `crates/squad-core/src/export/blow_it_up.rs`
- `crates/squad-core/src/export/debugging.rs`
- `crates/squad-core/src/export/decide.rs`
- `crates/squad-core/src/export/forecast.rs`
- `crates/squad-core/src/export/forge.rs`
- `crates/squad-core/src/export/mod.rs`
- `crates/squad-core/src/export/negotiate.rs`
- `crates/squad-core/src/export/rank.rs`
- `crates/squad-core/src/export/readback.rs`
- `crates/squad-core/src/export/trace.rs`
- `crates/squad-core/src/export/tune.rs`
- `crates/squad-core/src/orchestrator/bass_ackwards_prd.rs`
- `crates/squad-core/src/orchestrator/blow_it_up.rs`
- `crates/squad-core/src/orchestrator/debugging.rs`
- `crates/squad-core/src/orchestrator/decide.rs`
- `crates/squad-core/src/orchestrator/driver.rs`
- `crates/squad-core/src/orchestrator/forecast.rs`
- `crates/squad-core/src/orchestrator/forge.rs`
- `crates/squad-core/src/orchestrator/human.rs`
- `crates/squad-core/src/orchestrator/mod.rs`
- `crates/squad-core/src/orchestrator/negotiate.rs`
- `crates/squad-core/src/orchestrator/preflight.rs`
- `crates/squad-core/src/orchestrator/rank.rs`
- `crates/squad-core/src/orchestrator/readback.rs`
- `crates/squad-core/src/orchestrator/rounds.rs`
- `crates/squad-core/src/orchestrator/trace.rs`
- `crates/squad-core/src/orchestrator/tune.rs`
- `crates/squad-core/src/orchestrator/verbatim.rs`
- `crates/squad-core/src/prompts/embedded.rs`
- `crates/squad-core/src/prompts/inputs.rs`
- `crates/squad-core/src/prompts/inputs_bass_ackwards_prd.rs`
- `crates/squad-core/src/prompts/inputs_blow_it_up.rs`
- `crates/squad-core/src/prompts/inputs_debugging.rs`
- `crates/squad-core/src/prompts/inputs_decide.rs`
- `crates/squad-core/src/prompts/inputs_forecast.rs`
- `crates/squad-core/src/prompts/inputs_forge.rs`
- `crates/squad-core/src/prompts/inputs_negotiate.rs`
- `crates/squad-core/src/prompts/inputs_rank.rs`
- `crates/squad-core/src/prompts/inputs_readback.rs`
- `crates/squad-core/src/prompts/inputs_trace.rs`
- `crates/squad-core/src/prompts/inputs_tune.rs`
- `crates/squad-core/src/prompts/mod.rs`
- `crates/squad-core/src/prompts/provenance.rs`
- `crates/squad-core/src/prompts/tests.rs`
- `crates/squad-core/src/room.rs`
- `crates/squad-core/src/router/authorization.rs`
- `crates/squad-core/src/router/bass_ackwards_prd.rs`
- `crates/squad-core/src/router/blow_it_up.rs`
- `crates/squad-core/src/router/bounds.rs`
- `crates/squad-core/src/router/contests.rs`
- `crates/squad-core/src/router/debugging.rs`
- `crates/squad-core/src/router/decide.rs`
- `crates/squad-core/src/router/executor.rs`
- `crates/squad-core/src/router/forecast.rs`
- `crates/squad-core/src/router/forge.rs`
- `crates/squad-core/src/router/human.rs`
- `crates/squad-core/src/router/mod.rs`
- `crates/squad-core/src/router/negotiate.rs`
- `crates/squad-core/src/router/preflight.rs`
- `crates/squad-core/src/router/rank.rs`
- `crates/squad-core/src/router/readback.rs`
- `crates/squad-core/src/router/recovery.rs`
- `crates/squad-core/src/router/stop.rs`
- `crates/squad-core/src/router/trace.rs`
- `crates/squad-core/src/router/tune.rs`
- `crates/squad-core/src/router/variables.rs`
- `crates/squad-core/src/runtime/bass_ackwards_prd.rs`
- `crates/squad-core/src/runtime/blow_it_up.rs`
- `crates/squad-core/src/runtime/debugging.rs`
- `crates/squad-core/src/runtime/decide.rs`
- `crates/squad-core/src/runtime/forecast.rs`
- `crates/squad-core/src/runtime/forge.rs`
- `crates/squad-core/src/runtime/judge.rs`
- `crates/squad-core/src/runtime/member.rs`
- `crates/squad-core/src/runtime/mod.rs`
- `crates/squad-core/src/runtime/negotiate.rs`
- `crates/squad-core/src/runtime/rank.rs`
- `crates/squad-core/src/runtime/readback.rs`
- `crates/squad-core/src/runtime/trace.rs`
- `crates/squad-core/src/runtime/tune.rs`
- `crates/squad-core/src/state.rs`
- `crates/squad-core/tests/contest_storage.rs`
- `crates/squad-core/tests/executor_call.rs`
- `crates/squad-core/tests/export_file.rs`
- `crates/squad-core/tests/grill_workflows.rs`
- `crates/squad-core/tests/human_step.rs`
- `crates/squad-core/tests/m5b_replay_audit.rs`
- `crates/squad-core/tests/provenance_prompts.rs`
- `crates/squad-core/tests/router_debates.rs`
- `crates/squad-core/tests/router_validation.rs`
- `crates/squad-core/tests/verbatim.rs`

### squad-protocol

- `crates/squad-protocol/src/bass_ackwards_prd.rs`
- `crates/squad-protocol/src/blow_it_up.rs`
- `crates/squad-protocol/src/config.rs`
- `crates/squad-protocol/src/constants.rs`
- `crates/squad-protocol/src/contest.rs`
- `crates/squad-protocol/src/debugging.rs`
- `crates/squad-protocol/src/decide.rs`
- `crates/squad-protocol/src/envelopes.rs`
- `crates/squad-protocol/src/forecast.rs`
- `crates/squad-protocol/src/forge.rs`
- `crates/squad-protocol/src/lib.rs`
- `crates/squad-protocol/src/negotiate.rs`
- `crates/squad-protocol/src/parser/context.rs`
- `crates/squad-protocol/src/parser/errors.rs`
- `crates/squad-protocol/src/parser/examples.json`
- `crates/squad-protocol/src/parser/mod.rs`
- `crates/squad-protocol/src/parser/new_modes_tests.rs`
- `crates/squad-protocol/src/parser/parser.rs`
- `crates/squad-protocol/src/parser/parser_tests.rs`
- `crates/squad-protocol/src/parser/review_tests.rs`
- `crates/squad-protocol/src/parser/round_trip_tests.rs`
- `crates/squad-protocol/src/parser/types.rs`
- `crates/squad-protocol/src/parser/validate_context.rs`
- `crates/squad-protocol/src/parser/validation_codes.json`
- `crates/squad-protocol/src/parser/validation_strings.json`
- `crates/squad-protocol/src/rank.rs`
- `crates/squad-protocol/src/readback.rs`
- `crates/squad-protocol/src/requests.rs`
- `crates/squad-protocol/src/trace.rs`
- `crates/squad-protocol/src/tune.rs`
- `crates/squad-protocol/src/work.rs`
- `crates/squad-protocol/tests/config.rs`
- `crates/squad-protocol/tests/contests.rs`

### squad-store

- `crates/squad-store/migrations/0009_new_modes.sql`
- `crates/squad-store/migrations/0010_forge.sql`
- `crates/squad-store/migrations/0011_forecast.sql`
- `crates/squad-store/migrations/0012_decide.sql`
- `crates/squad-store/migrations/0013_negotiate.sql`
- `crates/squad-store/migrations/0014_blow_it_up.sql`
- `crates/squad-store/migrations/0015_readback.sql`
- `crates/squad-store/migrations/0016_debug.sql`
- `crates/squad-store/migrations/0017_trace.sql`
- `crates/squad-store/migrations/0018_rank.sql`
- `crates/squad-store/migrations/0019_tune.sql`
- `crates/squad-store/migrations/0020_bass_ackwards_prd.sql`
- `crates/squad-store/src/bass_ackwards_prd.rs`
- `crates/squad-store/src/blow_it_up.rs`
- `crates/squad-store/src/clear.rs`
- `crates/squad-store/src/contest_steps.rs`
- `crates/squad-store/src/debugging.rs`
- `crates/squad-store/src/decide.rs`
- `crates/squad-store/src/forecast.rs`
- `crates/squad-store/src/forge.rs`
- `crates/squad-store/src/judge_repairs.rs`
- `crates/squad-store/src/leases.rs`
- `crates/squad-store/src/lib.rs`
- `crates/squad-store/src/negotiate.rs`
- `crates/squad-store/src/prompt_provenance.rs`
- `crates/squad-store/src/rank.rs`
- `crates/squad-store/src/readback.rs`
- `crates/squad-store/src/readers.rs`
- `crates/squad-store/src/schema.rs`
- `crates/squad-store/src/trace.rs`
- `crates/squad-store/src/tune.rs`
- `crates/squad-store/src/work.rs`
- `crates/squad-store/tests/clear.rs`
- `crates/squad-store/tests/event_pages.rs`
- `crates/squad-store/tests/mode_migrations.rs`
- `crates/squad-store/tests/new_mode_visibility.rs`

### squad-tui

- `crates/squad-tui/src/actions.rs`
- `crates/squad-tui/src/composed.rs`
- `crates/squad-tui/src/fx/bass_ackwards_prd.rs`
- `crates/squad-tui/src/fx/blow_it_up.rs`
- `crates/squad-tui/src/fx/debugging.rs`
- `crates/squad-tui/src/fx/decide.rs`
- `crates/squad-tui/src/fx/dispatch.rs`
- `crates/squad-tui/src/fx/forecast.rs`
- `crates/squad-tui/src/fx/forge.rs`
- `crates/squad-tui/src/fx/mod.rs`
- `crates/squad-tui/src/fx/negotiate.rs`
- `crates/squad-tui/src/fx/rank.rs`
- `crates/squad-tui/src/fx/readback.rs`
- `crates/squad-tui/src/fx/trace.rs`
- `crates/squad-tui/src/fx/tune.rs`
- `crates/squad-tui/src/room_adapter.rs`
- `crates/squad-tui/src/session.rs`
- `crates/squad-tui/src/typeahead.rs`
- `crates/squad-tui/src/widgets/bass_ackwards_prd.rs`
- `crates/squad-tui/src/widgets/blow_it_up.rs`
- `crates/squad-tui/src/widgets/case.rs`
- `crates/squad-tui/src/widgets/completion.rs`
- `crates/squad-tui/src/widgets/contest_status.rs`
- `crates/squad-tui/src/widgets/debugging.rs`
- `crates/squad-tui/src/widgets/decide.rs`
- `crates/squad-tui/src/widgets/forecast.rs`
- `crates/squad-tui/src/widgets/forge.rs`
- `crates/squad-tui/src/widgets/human_step.rs`
- `crates/squad-tui/src/widgets/mod.rs`
- `crates/squad-tui/src/widgets/negotiate.rs`
- `crates/squad-tui/src/widgets/preflight.rs`
- `crates/squad-tui/src/widgets/rank.rs`
- `crates/squad-tui/src/widgets/readback.rs`
- `crates/squad-tui/src/widgets/trace.rs`
- `crates/squad-tui/src/widgets/tune.rs`
- `crates/squad-tui/tests/actions_shell.rs`
- `crates/squad-tui/tests/crosscheck.rs`
- `crates/squad-tui/tests/room_adapter.rs`
- `crates/squad-tui/tests/snapshots/typeahead__popup_commands.snap`

## Other files

The workspace lockfile includes Unicode normalization/segmentation and the shared
core randomness/hash dependencies. Documentation renames section 10 to section 11,
fixes references, integrates section 12 and decisions D88–D100, records both P1
checklists with evidence, and supplies this report and the exact ownership list.
