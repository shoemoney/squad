# Grill implementation checkpoint

This branch is **incomplete**. Grill is still rejected by normal command admission;
`ContestKind::is_built()` intentionally remains false for grill. No milestone checkbox
has been ticked. This is a foundation checkpoint, not an end-to-end mode release.

## Implemented and tested components

- Typed grill payload shapes, status/transition declarations, work kinds, bounds,
  exact question coverage, prompt identity and fresh step-context validation.
- Deterministic maximum-path script using shared turns/barrier/report moves, roster
  rotation, angle assignment, pass/skip distinction, rebuild/stop/failure decisions,
  and appendix assembly from committed records.
- Separate frozen grill prompt-input adapter for the existing compositions.
- Migration 0002_grill rebuilds the CHECK-constrained event/work tables while retaining
  old rows and foreign-key references; adds grill plan/turn tables and kind triggers.
- Store helpers retain typed floor records to exactly the declared participants and
  build compact projections retaining the latest plan, current round, and earlier
  questions. Full earlier answers remain in durable histories.
- Typed committed events update the existing TUI grill widget and effect cues; replay
  deduplicates events and suppresses cues. Typed events render in the shared export API.

## Required continuation

1. Add grill admission/preflight/confirmation and frozen model/prompt provenance to
   the real router and driver. Resolve direct/quoted plans and question-first picks;
   enforce shared active claims, participant draining/holds and daily reservation.
2. Extend ContestRouter's existing hooks (currently debate-specific internally) for
   grill state, transitions, authorized projections, sequential assignments and barriers.
3. Implement the grill executor and canonical generation/result identity validation,
   including structured question/answer validation, exact PASS, retries and deadlines.
   Work kinds currently exist but no grill executor dispatch is installed.
4. Bind store projections through the actual executor-bound readers; fence accepted
   results, stop/recovery, release holds/reservations and freeze prompts on restart.
5. Add full RoomHandle + SQLite + wiremock workflow tests, especially failures/skips,
   stop, restart without duplicate provider calls, daily refusal and question-first picks.
6. Complete TUI state/preflight/export action integration and committed-state snapshots.
   Current rendering consumes typed feed events, but no runtime emits those yet.

## Validation

Targeted protocol, script, prompt, store, export and TUI tests pass. Workspace build,
Clippy with warnings denied and formatting pass. Full workspace tests reach all targets
with --no-fail-fast; the existing squad-cli commands target has three stale assertions
expecting "Room runtime integration is not installed" after the real-runtime merge.
No CLI implementation or command tests were changed on this branch.

## Shared registration edits

- protocol/lib.rs: grill module and re-export.
- protocol/envelopes.rs: seven typed payload variants and validators.
- protocol/work.rs: three grill work kinds and validation/visible ceilings.
- protocol/contest.rs: Grill transition variant; debate conversion rejects other kinds.
- core/orchestrator/mod.rs and prompts/mod.rs: new module declarations.
- core/prompts/inputs.rs: sibling visibility for two existing input helpers.
- agents/registry.rs and agents/mod.rs: grill token/deadline classification only.
- store/lib.rs, schema.rs and clear.rs: module, migration and cleanup registration.
- core/export/mod.rs: typed export classification/rendering.
- tui/session.rs and composed.rs: typed feed reducer and widget rendering.

## Deferred requested todo IDs

Every requested milestone ID remains deferred; individual parts above do not establish
end-to-end completion of any listed todo.

- M1-protocol-store-and-bus.md: M1.P3.L1.T9.
- M2-agents-and-openrouter.md: M2.P2.L7.T4, M2.P2.L7.T6, M2.P2.L7.T7, M2.P3.L2.T7, M2.P4.L1.T11, M2.P4.L2.T6.
- M3-room-api-and-cli.md: M3.P2.L1.T6, M3.P2.L2.T7, M3.P2.L3.T5, M3.P3.L2.T9, M3.P3.L3.T10, M3.P3.L3.T12, M3.P3.L3.T15, M3.P3.L4.T8, M3.P3.L5.T5.
- M4-tui-ratatui.md: M4.P2.L7.T2, M4.P3.L6.T3, M4.P3.L6.T5, M4.P3.L6.T6, M4.P3.L6.T7, M4.P3.L7.T2, M4.P3.L7.T4, M4.P3.L8.T6, M4.P3.L9.T4, M4.P4.L4.T3, M4.P4.L4.T4, M4.P4.L4.T5, M4.P4.L4.T7, M4.P4.L4.T8, M4.P5.L2.T5.
- M5.P6: M5.P6.L1.T1, M5.P6.L1.T2, M5.P6.L1.T3, M5.P6.L1.T4, M5.P6.L1.T5, M5.P6.L2.T1, M5.P6.L2.T2, M5.P6.L2.T3, M5.P6.L2.T4, M5.P6.L2.T5, M5.P6.L2.T6, M5.P6.L3.T1, M5.P6.L3.T2, M5.P6.L3.T3, M5.P6.L3.T4, M5.P6.L3.T5, M5.P6.L3.T6, M5.P6.L4.T1, M5.P6.L4.T2, M5.P6.L4.T3, M5.P6.L4.T4, M5.P6.L4.T5, M5.P6.L4.T6, M5.P6.L5.T1, M5.P6.L5.T2, M5.P6.L6.T1, M5.P6.L6.T2, M5.P6.L6.T3, M5.P6.L6.T4, M5.P6.L6.T5, M5.P6.L6.T6.
