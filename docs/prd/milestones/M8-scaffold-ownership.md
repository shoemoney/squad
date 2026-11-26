# M8 scaffold: mode-worker ownership

All eleven switches remain `ENABLED = false`. The first nine follow section 11; tune and bass_ackwards_prd follow section 12, decisions D88–D100 and M9.P1. No mode worker edits a shared registry. Bring any shared-file need back to the integrator.

The lists below are exact owned paths, including tests and user documentation the worker creates. `debug` uses module `debugging`; its prompts, export and numbered migration keep wire name `debug`. The F12 widget remains `widgets/debug.rs`.

## Shared scaffold interfaces

- Protocol modules own their status, state, transition, case material, artifact validation and `ENABLED` switch.
- Router modules own `prepare`, `resolve`, `transition`, `produce`, `barrier`, `accepted_work`, `human_answer`, `stop`, `recover`, and `load_state`. Admission receives the frozen bindings, configuration, budget, confirmation threshold and preconfirmation in `NewModeAdmission`.
- Orchestrator modules own `prepare` and `drive(&mut Router)`, plus script and pure tally functions. Runtime modules own `execute`; prompt adapters own `inputs`; export modules own `render`; widgets own `View::apply` and `render`; fx modules own their effects hook.
- The existing `ContestArtifact` has additive optional new-mode headers so earlier modes retain their exact serialized records. New modes must supply `kind`, `round`, `actor`, `audience`, and an object `body`; `validate_header` checks these before the mode validator. Legacy `data`, `workId` and `sourceRefs` remain available. New mode artifacts use `contest_artifacts`; reads require a frozen work assignment plus audience authorization.
- Human steps have no timer. Shared code commits version-bound answers, releases ordinary work while waiting, and closes through `WAITING_FOR_PARTICIPANTS`. Mode callbacks determine the next script move.
- The renderer selects compositions dynamically from the manifest; mode input dispatch is explicit. No extra enum arm is required per template or artifact.

## Registry files confirmed beyond the original table

`ContestTransition` already lives in `squad-protocol/src/contest.rs`, not envelopes.rs. `ContestArtifact` and its existing envelope kind already existed; changing their old fields would break shipped records. Store dispatch is implemented in `src/leases.rs` and re-exported as `dispatch`, not a physical dispatch.rs. Its SQL attempt lists and per-attempt deadlines also need registration. Other required shared sites are agents `registry.rs`, core `state.rs`, `room.rs`, `router/authorization.rs`, `router/contests.rs`, `orchestrator/driver.rs`, runtime `member.rs` and `judge.rs`, store `clear.rs`, `judge_repairs.rs`, TUI `actions.rs`, `composed.rs`, `room_adapter.rs` and `widgets/case.rs`. These are integrator-owned.

The original 166 parser rows and snapshot are retained. Additional rows use a test-local enable override, reset after every fixture. Public typeahead and help derive their mode lists from the switches. Optional environment seeds are absent from `.env.example`.

## $forge — Forge

- `crates/squad-protocol/src/forge.rs`
- `crates/squad-protocol/tests/forge.rs`
- `crates/squad-store/src/forge.rs`
- `crates/squad-store/migrations/0010_forge.sql`
- `crates/squad-store/tests/forge.rs`
- `crates/squad-agents/src/agents/forge.rs`
- `crates/squad-agents/tests/agents_forge.rs`
- `crates/squad-core/src/orchestrator/forge.rs`
- `crates/squad-core/src/router/forge.rs`
- `crates/squad-core/src/runtime/forge.rs`
- `crates/squad-core/src/export/forge.rs`
- `crates/squad-core/src/prompts/inputs_forge.rs`
- `crates/squad-core/prompts/forge.draft.md`
- `crates/squad-core/prompts/forge.rank.md`
- `crates/squad-core/prompts/forge.fuse.md`
- `crates/squad-core/prompts/forge.judge.md`
- `crates/squad-core/tests/forge_workflows.rs`
- `crates/squad-core/tests/forge_tallies.rs`
- `crates/squad-core/tests/forge_export.rs`
- `crates/squad-tui/src/widgets/forge.rs`
- `crates/squad-tui/src/fx/forge.rs`
- `crates/squad-tui/tests/forge_view.rs`
- `docs/modes/forge.md`

## $forecast — Forecast

- `crates/squad-protocol/src/forecast.rs`
- `crates/squad-protocol/tests/forecast.rs`
- `crates/squad-store/src/forecast.rs`
- `crates/squad-store/migrations/0011_forecast.sql`
- `crates/squad-store/tests/forecast.rs`
- `crates/squad-agents/src/agents/forecast.rs`
- `crates/squad-agents/tests/agents_forecast.rs`
- `crates/squad-core/src/orchestrator/forecast.rs`
- `crates/squad-core/src/router/forecast.rs`
- `crates/squad-core/src/runtime/forecast.rs`
- `crates/squad-core/src/export/forecast.rs`
- `crates/squad-core/src/prompts/inputs_forecast.rs`
- `crates/squad-core/prompts/forecast.frame.md`
- `crates/squad-core/prompts/forecast.estimate.md`
- `crates/squad-core/prompts/forecast.revise.md`
- `crates/squad-core/prompts/forecast.cruxes.md`
- `crates/squad-core/tests/forecast_workflows.rs`
- `crates/squad-core/tests/forecast_tallies.rs`
- `crates/squad-core/tests/forecast_export.rs`
- `crates/squad-tui/src/widgets/forecast.rs`
- `crates/squad-tui/src/fx/forecast.rs`
- `crates/squad-tui/tests/forecast_view.rs`
- `docs/modes/forecast.md`

## $decide — Decide

- `crates/squad-protocol/src/decide.rs`
- `crates/squad-protocol/tests/decide.rs`
- `crates/squad-store/src/decide.rs`
- `crates/squad-store/migrations/0012_decide.sql`
- `crates/squad-store/tests/decide.rs`
- `crates/squad-agents/src/agents/decide.rs`
- `crates/squad-agents/tests/agents_decide.rs`
- `crates/squad-core/src/orchestrator/decide.rs`
- `crates/squad-core/src/router/decide.rs`
- `crates/squad-core/src/runtime/decide.rs`
- `crates/squad-core/src/export/decide.rs`
- `crates/squad-core/src/prompts/inputs_decide.rs`
- `crates/squad-core/prompts/decide.options.md`
- `crates/squad-core/prompts/decide.shortlist.md`
- `crates/squad-core/prompts/decide.criteria.md`
- `crates/squad-core/prompts/decide.freeze.md`
- `crates/squad-core/prompts/decide.score.md`
- `crates/squad-core/prompts/decide.explain.md`
- `crates/squad-core/tests/decide_workflows.rs`
- `crates/squad-core/tests/decide_tallies.rs`
- `crates/squad-core/tests/decide_export.rs`
- `crates/squad-tui/src/widgets/decide.rs`
- `crates/squad-tui/src/fx/decide.rs`
- `crates/squad-tui/tests/decide_view.rs`
- `docs/modes/decide.md`

## $negotiate — Negotiate

- `crates/squad-protocol/src/negotiate.rs`
- `crates/squad-protocol/tests/negotiate.rs`
- `crates/squad-store/src/negotiate.rs`
- `crates/squad-store/migrations/0013_negotiate.sql`
- `crates/squad-store/tests/negotiate.rs`
- `crates/squad-agents/src/agents/negotiate.rs`
- `crates/squad-agents/tests/agents_negotiate.rs`
- `crates/squad-core/src/orchestrator/negotiate.rs`
- `crates/squad-core/src/router/negotiate.rs`
- `crates/squad-core/src/runtime/negotiate.rs`
- `crates/squad-core/src/export/negotiate.rs`
- `crates/squad-core/src/prompts/inputs_negotiate.rs`
- `crates/squad-core/prompts/negotiate.map.md`
- `crates/squad-core/prompts/negotiate.brief.md`
- `crates/squad-core/prompts/negotiate.package.md`
- `crates/squad-core/prompts/negotiate.move.md`
- `crates/squad-core/prompts/negotiate.counter.md`
- `crates/squad-core/prompts/negotiate.playbook.md`
- `crates/squad-core/tests/negotiate_workflows.rs`
- `crates/squad-core/tests/negotiate_tallies.rs`
- `crates/squad-core/tests/negotiate_export.rs`
- `crates/squad-tui/src/widgets/negotiate.rs`
- `crates/squad-tui/src/fx/negotiate.rs`
- `crates/squad-tui/tests/negotiate_view.rs`
- `docs/modes/negotiate.md`

## $blow_it_up — BlowItUp

- `crates/squad-protocol/src/blow_it_up.rs`
- `crates/squad-protocol/tests/blow_it_up.rs`
- `crates/squad-store/src/blow_it_up.rs`
- `crates/squad-store/migrations/0014_blow_it_up.sql`
- `crates/squad-store/tests/blow_it_up.rs`
- `crates/squad-agents/src/agents/blow_it_up.rs`
- `crates/squad-agents/tests/agents_blow_it_up.rs`
- `crates/squad-core/src/orchestrator/blow_it_up.rs`
- `crates/squad-core/src/router/blow_it_up.rs`
- `crates/squad-core/src/runtime/blow_it_up.rs`
- `crates/squad-core/src/export/blow_it_up.rs`
- `crates/squad-core/src/prompts/inputs_blow_it_up.rs`
- `crates/squad-core/prompts/blow_it_up.attack.md`
- `crates/squad-core/prompts/blow_it_up.challenge.md`
- `crates/squad-core/prompts/blow_it_up.rule.md`
- `crates/squad-core/tests/blow_it_up_workflows.rs`
- `crates/squad-core/tests/blow_it_up_tallies.rs`
- `crates/squad-core/tests/blow_it_up_export.rs`
- `crates/squad-tui/src/widgets/blow_it_up.rs`
- `crates/squad-tui/src/fx/blow_it_up.rs`
- `crates/squad-tui/tests/blow_it_up_view.rs`
- `docs/modes/blow_it_up.md`

## $readback — Readback

- `crates/squad-protocol/src/readback.rs`
- `crates/squad-protocol/tests/readback.rs`
- `crates/squad-store/src/readback.rs`
- `crates/squad-store/migrations/0015_readback.sql`
- `crates/squad-store/tests/readback.rs`
- `crates/squad-agents/src/agents/readback.rs`
- `crates/squad-agents/tests/agents_readback.rs`
- `crates/squad-core/src/orchestrator/readback.rs`
- `crates/squad-core/src/router/readback.rs`
- `crates/squad-core/src/runtime/readback.rs`
- `crates/squad-core/src/export/readback.rs`
- `crates/squad-core/src/prompts/inputs_readback.rs`
- `crates/squad-core/prompts/readback.interpret.md`
- `crates/squad-core/prompts/readback.diff.md`
- `crates/squad-core/prompts/readback.rewrite.md`
- `crates/squad-core/tests/readback_workflows.rs`
- `crates/squad-core/tests/readback_tallies.rs`
- `crates/squad-core/tests/readback_export.rs`
- `crates/squad-tui/src/widgets/readback.rs`
- `crates/squad-tui/src/fx/readback.rs`
- `crates/squad-tui/tests/readback_view.rs`
- `docs/modes/readback.md`

## $debug — Debugging

- `crates/squad-protocol/src/debugging.rs`
- `crates/squad-protocol/tests/debugging.rs`
- `crates/squad-store/src/debugging.rs`
- `crates/squad-store/migrations/0016_debug.sql`
- `crates/squad-store/tests/debugging.rs`
- `crates/squad-agents/src/agents/debugging.rs`
- `crates/squad-agents/tests/agents_debugging.rs`
- `crates/squad-core/src/orchestrator/debugging.rs`
- `crates/squad-core/src/router/debugging.rs`
- `crates/squad-core/src/runtime/debugging.rs`
- `crates/squad-core/src/export/debugging.rs`
- `crates/squad-core/src/prompts/inputs_debugging.rs`
- `crates/squad-core/prompts/debug.hypothesize.md`
- `crates/squad-core/prompts/debug.merge.md`
- `crates/squad-core/prompts/debug.test.md`
- `crates/squad-core/prompts/debug.select.md`
- `crates/squad-core/prompts/debug.report.md`
- `crates/squad-core/tests/debugging_workflows.rs`
- `crates/squad-core/tests/debugging_tallies.rs`
- `crates/squad-core/tests/debugging_export.rs`
- `crates/squad-tui/src/widgets/debugging.rs`
- `crates/squad-tui/src/fx/debugging.rs`
- `crates/squad-tui/tests/debugging_view.rs`
- `docs/modes/debug.md`

## $trace — Trace

- `crates/squad-protocol/src/trace.rs`
- `crates/squad-protocol/tests/trace.rs`
- `crates/squad-store/src/trace.rs`
- `crates/squad-store/migrations/0017_trace.sql`
- `crates/squad-store/tests/trace.rs`
- `crates/squad-agents/src/agents/trace.rs`
- `crates/squad-agents/tests/agents_trace.rs`
- `crates/squad-core/src/orchestrator/trace.rs`
- `crates/squad-core/src/router/trace.rs`
- `crates/squad-core/src/runtime/trace.rs`
- `crates/squad-core/src/export/trace.rs`
- `crates/squad-core/src/prompts/inputs_trace.rs`
- `crates/squad-core/prompts/trace.extract.md`
- `crates/squad-core/prompts/trace.merge.md`
- `crates/squad-core/prompts/trace.verify.md`
- `crates/squad-core/prompts/trace.assemble.md`
- `crates/squad-core/tests/trace_workflows.rs`
- `crates/squad-core/tests/trace_tallies.rs`
- `crates/squad-core/tests/trace_export.rs`
- `crates/squad-tui/src/widgets/trace.rs`
- `crates/squad-tui/src/fx/trace.rs`
- `crates/squad-tui/tests/trace_view.rs`
- `docs/modes/trace.md`

## $rank — Rank

- `crates/squad-protocol/src/rank.rs`
- `crates/squad-protocol/tests/rank.rs`
- `crates/squad-store/src/rank.rs`
- `crates/squad-store/migrations/0018_rank.sql`
- `crates/squad-store/tests/rank.rs`
- `crates/squad-agents/src/agents/rank.rs`
- `crates/squad-agents/tests/agents_rank.rs`
- `crates/squad-core/src/orchestrator/rank.rs`
- `crates/squad-core/src/router/rank.rs`
- `crates/squad-core/src/runtime/rank.rs`
- `crates/squad-core/src/export/rank.rs`
- `crates/squad-core/src/prompts/inputs_rank.rs`
- `crates/squad-core/prompts/rank.compare.md`
- `crates/squad-core/prompts/rank.explain.md`
- `crates/squad-core/tests/rank_workflows.rs`
- `crates/squad-core/tests/rank_tallies.rs`
- `crates/squad-core/tests/rank_export.rs`
- `crates/squad-tui/src/widgets/rank.rs`
- `crates/squad-tui/src/fx/rank.rs`
- `crates/squad-tui/tests/rank_view.rs`
- `docs/modes/rank.md`

## $tune — Tune

- `crates/squad-protocol/src/tune.rs`
- `crates/squad-protocol/tests/tune.rs`
- `crates/squad-store/src/tune.rs`
- `crates/squad-store/migrations/0019_tune.sql`
- `crates/squad-store/tests/tune.rs`
- `crates/squad-agents/src/agents/tune.rs`
- `crates/squad-agents/tests/agents_tune.rs`
- `crates/squad-core/src/orchestrator/tune.rs`
- `crates/squad-core/src/router/tune.rs`
- `crates/squad-core/src/runtime/tune.rs`
- `crates/squad-core/src/export/tune.rs`
- `crates/squad-core/src/prompts/inputs_tune.rs`
- `crates/squad-core/prompts/tune.cases.md`
- `crates/squad-core/prompts/tune.freeze.md`
- `crates/squad-core/prompts/tune.score.md`
- `crates/squad-core/prompts/tune.mutate.md`
- `crates/squad-core/tests/tune_workflows.rs`
- `crates/squad-core/tests/tune_tallies.rs`
- `crates/squad-core/tests/tune_export.rs`
- `crates/squad-tui/src/widgets/tune.rs`
- `crates/squad-tui/src/fx/tune.rs`
- `crates/squad-tui/tests/tune_view.rs`
- `docs/modes/tune.md`

## $bass_ackwards_prd — BassAckwardsPrd

- `crates/squad-protocol/src/bass_ackwards_prd.rs`
- `crates/squad-protocol/tests/bass_ackwards_prd.rs`
- `crates/squad-store/src/bass_ackwards_prd.rs`
- `crates/squad-store/migrations/0020_bass_ackwards_prd.sql`
- `crates/squad-store/tests/bass_ackwards_prd.rs`
- `crates/squad-agents/src/agents/bass_ackwards_prd.rs`
- `crates/squad-agents/tests/agents_bass_ackwards_prd.rs`
- `crates/squad-core/src/orchestrator/bass_ackwards_prd.rs`
- `crates/squad-core/src/router/bass_ackwards_prd.rs`
- `crates/squad-core/src/runtime/bass_ackwards_prd.rs`
- `crates/squad-core/src/export/bass_ackwards_prd.rs`
- `crates/squad-core/src/prompts/inputs_bass_ackwards_prd.rs`
- `crates/squad-core/prompts/bass_ackwards_prd.done.md`
- `crates/squad-core/prompts/bass_ackwards_prd.freeze.md`
- `crates/squad-core/prompts/bass_ackwards_prd.tree.md`
- `crates/squad-core/prompts/bass_ackwards_prd.merge.md`
- `crates/squad-core/prompts/bass_ackwards_prd.review.md`
- `crates/squad-core/prompts/bass_ackwards_prd.revise.md`
- `crates/squad-core/prompts/bass_ackwards_prd.must_have.md`
- `crates/squad-core/prompts/bass_ackwards_prd.finalize.md`
- `crates/squad-core/prompts/bass_ackwards_prd.apply.md`
- `crates/squad-core/tests/bass_ackwards_prd_workflows.rs`
- `crates/squad-core/tests/bass_ackwards_prd_tallies.rs`
- `crates/squad-core/tests/bass_ackwards_prd_export.rs`
- `crates/squad-tui/src/widgets/bass_ackwards_prd.rs`
- `crates/squad-tui/src/fx/bass_ackwards_prd.rs`
- `crates/squad-tui/tests/bass_ackwards_prd_view.rs`
- `docs/modes/bass_ackwards_prd.md`

The worker owns any additional `prompts/<wire name>.*.md` assets for its mode. One integration worker assembles the user-facing mode pages into README.md; README.md is not a concurrent worker file. Tune registers five work kinds, including the bare executor `tune.execute` which has no template. Bass_ackwards_prd registers nine. The shared executor, reservation pricing and atomic export writer are implemented. The scaffold adds core dependencies for persisted randomness and SHA-256 so mode workers need no Cargo manifest edits. A quoted tune prompt is represented by `PlanSource`, preserving its identity until admission.
