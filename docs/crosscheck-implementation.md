# Crosscheck implementation

Branch: `mode-crosscheck`, based on the live-runtime merge `f2debf9`.

Crosscheck now runs through the shared router/contest driver, using frozen step assignments, transactional barriers, scoped readers, the normal executor grants and daily accounting. At six members it schedules 16 calls, or 17 when the judge must extract the goal and constraints. It does not create a separate scheduler or provider client.

## Files and behavior

- `crates/squad-protocol/src/crosscheck.rs`: typed setup, state, goal, inventory, normalization, grid, reviews, audit samples/findings, rulings, human acceptance, reports and scoped case material. Provider responses deny unknown fields and cannot accept risks or supply application stop results. Schemas instantiate assigned IDs and cardinalities; semantic validation checks complete source/cell coverage.
- `crates/squad-core/src/orchestrator/crosscheck.rs`: script transitions, all singles/cross-lens pairs, deterministic 200-cell cap, contributor-aware balanced assignments, frozen auditor selection/samples, retained audit additions, residual risks and bounded-stop recomputation. The default unmerged six-lens grid has 153 cells.
- `crates/squad-core/src/orchestrator/crosscheck_redaction.rs`: deterministic case-insensitive, Unicode-boundary redaction from frozen roster/model/persona tokens, matching longer phrases first. Human source text is retained.
- `crates/squad-core/src/router/crosscheck.rs`: admission, confirmation, authoritative payload validation, step/work allocation, barrier commits and COMPLETE-only human acceptance. Acceptance checks current plan/grid references and optimistic state version. Confirmations retain request causality and distinguish automatic, explicit and preconfirmed approval.
- `crates/squad-store/migrations/0002_crosscheck.sql` and `src/crosscheck.rs`: upgrade released event/work CHECK constraints without changing `0001_initial.sql`; persist cells, evidence, fixed samples and acceptance references. Grid and ruling origins are exclusive; accepted cells require a retained human acceptance event.
- `crates/squad-agents/src/agents/crosscheck.rs`, `crates/squad-core/src/runtime/crosscheck.rs` and `src/prompts/inputs_crosscheck.rs`: one structured request per authorized batch, fresh inputs, scoped judge repair and strict result validation. Member calls use 2,048 visible tokens; judge calls use medium reasoning and 4,096 visible tokens.
- `crates/squad-core/src/export/crosscheck.rs`: committed Markdown with provenance, normalized items, every retained/omitted cell, evidence, samples, rulings, explicit acceptances, failures and the bounded-stop result.
- `crates/squad-tui/src/widgets/crosscheck.rs`: committed plan/goal, inventory/lens labels, cells, evidence, audit findings, omissions, acceptance and stop guidance. F5 toggles cell details. The session reducer reconstructs state from replay, binds `/accept`, and emits live banner/cell cues only for committed artifacts. Accepted partial reviews remain visible before a barrier; the later typed carrier does not duplicate the effect.
- `crates/squad-core/tests/crosscheck_room.rs`, `tests/crosscheck_policy.rs`, `crates/squad-protocol/tests/crosscheck.rs`, `crates/squad-store/tests/crosscheck.rs`, and `crates/squad-tui/tests/crosscheck.rs`: live fake-provider workflows, isolation, recovery, transactional fault injection, strict payload validation and committed-state snapshots.

COMPLETE means execution finished. Bounded stop additionally requires every retained cell and residual risk to be closed by supported evidence, justified exclusion or explicit human acceptance, and all material audit findings to be resolved. Missing auditors prevent a clean-audit claim. Accepted audit findings cannot be erased by the cap; they become open cells or explicit residual risks.

## Shared registration edits

- Protocol: `lib.rs`, `envelopes.rs`, `contest.rs`, `work.rs`. Work wire names are `crosscheck.goal`, `crosscheck.inventory`, `crosscheck.normalize`, `crosscheck.cell`, `crosscheck.audit`, and `crosscheck.rule`.
- Store: `lib.rs`, `schema.rs`, `writer.rs`, `work.rs`, `leases.rs`, `readers.rs`, `judge_repairs.rs`, `contest_steps.rs`, `clear.rs`.
- Core engine/router: orchestrator `mod.rs`, `driver.rs`, `rounds.rs`; router `mod.rs`, `contests.rs`, `executor.rs`, `preflight.rs`, `stop.rs`.
- Core runtime/prompts/export: runtime `mod.rs`, `member.rs`, `judge.rs`; prompts `mod.rs`, `inputs.rs`; export `mod.rs`; `state.rs`.
- Agents: `agents/mod.rs`, `registry.rs`.
- TUI: widgets `mod.rs`, `session.rs`, `composed.rs`, `ui_fx.rs`, `app.rs`.
- CLI: `adapter.rs`, `send.rs`. Existing command tests were updated for the installed runtime and implemented crosscheck export; the protocol built-kind expectation now includes crosscheck.
- Bus: `bus/in_process.rs` now acknowledges after committed feed publication and coalesced mailbox wakeups. Workspace testing exposed the previous acknowledgement/wakeup race.
- Core adds the already-used `regex` dependency explicitly for Unicode-aware blind-text projection.

## Completed checklist scope

- `M5.P8.L1.T1`–`T4`, `M5.P8.L2.T1`–`T4`, `M5.P8.L3.T1`–`T3`.
- `M5.P8.L4.T1`–`T10`, `M5.P8.L4.T12`–`T14`.
- `M2.P2.L9.T1`–`T6`.
- `M3.P3.L1.T5`, `M3.P3.L3.T16`.
- `M4.P1.L6.T1`, `M4.P3.L10.T1`–`T6`, `M4.P4.L6.T3`–`T4`.

Only checkbox characters are changed in milestone files.

## Deferred joint verification

`M5.P8.L4.T11` remains unchecked because it explicitly requires report/artifact export and banner reconstruction across census, crosscheck, survey and probe. This branch verifies crosscheck replay and export; the other three runtime/type migrations are absent from its base.

The remaining crosscheck-named multi-mode rows stay unchecked until the corresponding mode branches are combined: `M2.P4.L1.T12`, `M2.P4.L2.T7`; `M3.P1.L1.T7`, `M3.P1.L2.T10`, `M3.P2.L1.T7`, `M3.P2.L2.T11`, `M3.P3.L2.T11`, `M3.P3.L2.T12`, `M3.P3.L3.T14`, `M3.P3.L5.T8`; `M4.P2.L8.T2`, `M4.P3.L8.T2`, `M4.P3.L8.T3`, `M4.P3.L8.T6`, `M4.P4.L6.T5`, `M4.P5.L15.T1`. Their crosscheck paths are implemented here; these rows also require other modes.

## Verification

On dev3, with `CARGO_BUILD_JOBS=8`:

```sh
cargo build --workspace
RUST_TEST_THREADS=1 cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
```

Final gates passed on 2026-11-25: workspace build, 734 tests passed (three existing unrelated tests ignored), all-target Clippy with warnings denied, and format checking. Wiki update `5315267` was linted and pushed.

The crosscheck suites run without ignored cases and use the existing wiremock provider. Coverage includes 16/17-call execution; quote and ordinary-history plan canaries; exact assignment coverage and no self-review; member failure; semantic judge repair and exhaustion; stop and clear; export of accepted partial reviews after stopping mid-step; daily-max refusal; no-wait confirmation; fixed samples after restart without repeating accepted steps; human-only version-bound acceptance; all seven kinds competing for one active claim; and rollback/reopen at assignment, artifact, barrier and next-work boundaries. TUI tests cover committed states, evidence, audit challenges, acceptance, duplicate delivery, replay cue suppression and snapshot-bound acceptance requests.

## Open questions and integration notes

- The PRD fixes judge output at 4,096 visible tokens while requiring a full ruling record for every retained cell. A large grid may exhaust that ceiling. This implementation retains the cells and uses the authorized repair/failure policy; compacting the ruling contract or changing the ceiling requires a PRD decision. Fake-provider tests verify request ceilings and workflow behavior, not real-model output length feasibility.
- Migration `0002` follows a base containing only `0001`. Assign subsequent migration versions when combining independently implemented mode branches.
- The wiki is hosted on dev1; its index, rules and relevant Squad pages were consulted over SSH. Crosscheck integration notes are recorded in `wiki/projects/squad-crosscheck.md`.
