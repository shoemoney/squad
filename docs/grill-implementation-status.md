# Grill runtime implementation

GRILL is enabled in normal command admission and runs through the real RoomHandle,
SQLite writer, executor-bound readers and OpenRouter transport. Tests use wiremock
only. The continuation items below replace the foundation-only checkpoint inherited
from wave 4.

## Required continuation — implementation and evidence

1. **Admission, preflight and frozen provenance:** `router/grill.rs` admits direct
   leader plans, parser-selected/quoted sources, restricted floors and question-first
   picks. It freezes source text and identity, focus, rounds, limits, concrete model
   bindings, reasoning effort, catalog pricing/context facts, rendered compositions
   and prompt hashes. Shared active claims, daily admission, confirmation/cancellation/
   expiry, participant draining and later ordinary-work holds use existing contracts.
   Maximum-path normal calls and both attempts are included in preflight.
2. **Shared engine hooks:** `ContestRouter` exposes grill state. The existing typed
   transition, allocation and barrier hooks dispatch grill operations. Assignments
   commit sequentially with fresh step contexts; the router derives their projections
   and validates the target, work kind, prompt identity, ordinal and source trigger.
   Barriers resolve durable work once before advancing the rotated order and angle.
3. **Executor and validation:** `squad-agents/src/agents/grill.rs` makes one provider
   call per granted attempt, using the shared 180-second turn deadline and two-attempt
   policy. Question JSON contains one to three bounded, nonrepeated questions; only
   exact `PASS` is voluntary. Answer JSON must cover every question index once with
   nonempty answers and plan changes. Rebuilds obey the plan word/token limit.
   Truncated/filtered output is rejected. Router acceptance validates the canonical
   generation identity and rederives the typed output from frozen input.
4. **Bound reads and lifecycle:** allocation freezes the latest plan, current-round
   floor and compact prior questions into the work's authorized context. Full earlier
   answers remain in participant history/export. Work readers check ownership,
   assignment, epoch and lease; accepted results retain the ordinary fence check.
   Stop fences outstanding work. Terminal states release unstarted plan reservations
   and deferred ordinary history. Restart uses durable assignments and frozen prompts,
   preserves accepted results and consumes interrupted attempt ordinals.
5. **Real workflow tests:** `squad-core/tests/grill_workflows.rs` covers a complete
   two-round floor, voluntary passes, invalid PASS retries and attributed skips,
   leader failure with the last good plan, confirmation, cancellation/expiry, daily
   refusal, question-first picks without duplicate ordinary calls, restricted-floor
   isolation, stop with held-work release, and restart without repeated accepted
   questions. Restart also changes prompt files/personas and verifies frozen prompts.
   Accounting assertions reconcile daily totals with attempts and reservations.
   Protocol tests separately exercise exact PASS, repeated questions, strict root
   shapes, complete answer coverage and limits. Existing script/store tests retain
   rotation, bounds, migration and compact-projection coverage.
6. **TUI and export:** real typed feed events update authoritative active state,
   preflight actions and the shared grill widget. `squad-tui/tests/grill_runtime.rs`
   confirms a real pending preflight through TUI actions, consumes the committed feed,
   checks replay/duplicate cue suppression and restored state, snapshots completion
   at 120/80/40 columns, and saves real core-owned Markdown export.

## Validation

The initial runtime checkpoint passed its two-round RoomHandle workflow. Subsequent
focused validation passed all 11 core GRILL workflows and the real-runtime TUI test.
Final workspace gates are recorded after completion in the branch's final report.

The earlier foundation suites remain enabled: typed payloads and work validation,
maximum-path script, migration 0006 with later carrier rebuilds, floor records,
compact projections, export allowlists, widgets and effects. Other contest modes
retain their existing registration and runtime paths.

## Milestone bookkeeping

This file records the continuation implementation and its evidence. Historical
milestone checkboxes have not been bulk-ticked: several specify individual test
locations or broader cross-milestone checks beyond this runtime continuation.
