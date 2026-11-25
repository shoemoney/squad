# M3 — Room API and CLI

**Goal:** Provide one typed room API and complete command behavior over the in-process bus and SQLite state.

**Exit criteria:**

- `RoomHandle` opens, recovers, serves requests, pages history, reports state, and shuts down through the owning contracts.
- A competing room-opening process receives the exact lock diagnostic.
- Read-only export succeeds while another process holds the room lock.
- State projections preserve shared presence, contest, preflight, model, scoreboard, trophy, and daily-spend semantics.
- Quotes, exclusions, variables, stop, retry, regeneration, judge actions, clear, and export use shared validation and execution paths.
- Send text and JSON lines output match their specified behavior and exit codes.
- Doctor checks and bus diagnostics never reveal credentials or internal prompts.
- M3 uses shared fixtures and test doubles; runtime contest behavior is verified with M5.
- The milestone receives one review pass and an owner check-in.
- History, feed, state, and command output preserve late and unscored round results and final tally fields without changing the verdict-only scoreboard.
- Markdown exports include round results and default-font banners derived from committed state transitions.
- Doctor reports `SQUAD_BANNERS` and `SQUAD_FX` with defaults, effective values, sources, and validation failures.
- `OneShot` opens never dispatch unrelated recovered work; `models` and `doctor` never open the room.
- `send --no-wait` commits without dispatch, prints the accepted cursor, and leaves work for the next interactive open.
- API and JSON output use `CommittedEnvelope`, `Presence`, `Score`, and the shared request-result schema without alternate identity fields or fabricated cursors.
- Export and banner rendering live in `squad-core` and are available to both TUI and offline CLI callers without a crate cycle.
- Daily accounting projections include the local day, reservations, unknown costs, and effective max; `/clear` preserves shared daily spend.
- Active-contest projections include collecting answers and persistent pending picks; trophy reads remain idempotent.
- Tournament and grill output, export scopes, and pick events preserve their shared schemas using committed-event fixtures.
- Doctor reports unknown-cost calls, response-model mismatches, and every new environment variable.
- `RoomVars` exposes persisted `CENSUS_PASSES`; its seed applies only when no room value is stored.
- The same `RoomHandle` paths preserve all seven modes’ shared states, preflights, artifacts, stop behavior, and recovery projections without another contest engine.
- Census, crosscheck, survey, and probe text and JSON output preserve script-derived call estimates, report artifacts, skipped members, and human-visible metadata without granting member-context access.
- All four new export scopes work through both `RoomHandle::export` and offline export, including partial runs, deterministic banners, and mode-specific reports.
- Census exports retain estimate ranges and caveats; crosscheck exports retain explicit risk acceptance; survey exports retain all five bands; probe exports retain the unexpected-result branch.

## Phase M3.P1 — Room lifecycle and requests

- **M3.P1.L1 — Implement RoomHandle**
  - [ ] M3.P1.L1.T1 — Implement the public API signatures in `crates/squad-core/src/room.rs`.
  - [ ] M3.P1.L1.T2 — Wire lock acquisition, migration, recovery, model-resolution readiness, and shutdown in `crates/squad-core/src/room.rs`.
  - [ ] M3.P1.L1.T3 — Test competing processes and the exact lock diagnostic in `crates/squad-core/tests/room_lifecycle.rs`.
  - [ ] M3.P1.L1.T4 — Test startup failure cleanup and lock release in `crates/squad-core/tests/room_lifecycle.rs`.
  - [ ] M3.P1.L1.T5 — Implement `OpenMode::Interactive` and `OpenMode::OneShot` scheduling in `crates/squad-core/src/room.rs`; verify one-shot recovery never re-wakes unrelated work in `crates/squad-core/tests/room_lifecycle.rs`.
  - [ ] M3.P1.L1.T6 — Implement commit-only submission scheduling for no-wait sends in `crates/squad-core/src/room.rs`; verify shutdown preserves accepted work without a provider attempt and the next interactive open resumes it in `crates/squad-core/tests/room_lifecycle.rs`.
  - [ ] M3.P1.L1.T7 — Extend lifecycle fixture coverage to census, crosscheck, survey, and probe in `crates/squad-core/tests/room_lifecycle.rs`, preserving shared recovery, one-shot isolation, and no-wait scheduling through the existing `RoomHandle` methods.

- **M3.P1.L2 — Preserve shared request behavior**
  - [ ] M3.P1.L2.T1 — Route typed requests and parser results through the shared router path in `crates/squad-core/src/room.rs`.
  - [ ] M3.P1.L2.T2 — Test quotes, solution selections, exclusions, sticky variables, and receipt identity in `crates/squad-core/tests/room_requests.rs`.
  - [ ] M3.P1.L2.T4 — Test stop-all, stop-one, permitted ordinary retry, and regeneration in `crates/squad-core/tests/room_actions.rs`.
  - [ ] M3.P1.L2.T5 — Test standalone judge questions, verdict follow-ups, and explicit debate judging with shared case fixtures in `crates/squad-core/tests/room_judge.rs`.
  - [ ] M3.P1.L2.T8 — Route contest picks and kind-specific confirmation and cancellation through shared request contracts in `crates/squad-core/src/room.rs`; test preserved request identities and errors with protocol fixtures in `crates/squad-core/tests/room_requests.rs`.
  - [ ] M3.P1.L2.T9 — Verify `Accepted` results preserve `requestId`, `eventIds`, and optional `cursor` without invented `messageId` or `commandId` result variants in `crates/squad-core/tests/room_requests.rs`.
  - [ ] M3.P1.L2.T10 — Route the four new mode commands, selected inputs, census plan updates, and explicit crosscheck risk acceptances through their shared request contracts in `crates/squad-core/src/room.rs`; verify validation, idempotency, and preserved receipts using fixtures in `crates/squad-core/tests/room_requests.rs`.
  - [ ] M3.P1.L2.T11 — Verify mode submissions preserve script-controlled delivery rather than automatically exposing quoted plans, source answers, finder metadata, or rival explanations in `crates/squad-core/tests/room_requests.rs`; use owning router and context-delivery fixtures.

## Phase M3.P2 — History, state, and feed

- **M3.P2.L1 — Deliver durable history**
  - [ ] M3.P2.L1.T1 — Implement backward page selection and ascending page results in `crates/squad-core/src/room.rs`.
  - [ ] M3.P2.L1.T2 — Test empty history, page limits, exclusive boundaries, and initial history/feed overlap in `crates/squad-core/tests/room_history.rs`.
  - [ ] M3.P2.L1.T3 — Test invalid and future cursors and the owning clear-boundary behavior in `crates/squad-core/tests/room_history.rs`.
  - [ ] M3.P2.L1.T4 — Test startup recovery of durable events without fabricated token recovery in `crates/squad-core/tests/room_history.rs`.
  - [ ] M3.P2.L1.T5 — Verify committed round scores, late round identities, unscored markers, and final tally fields survive paging and recovery in `crates/squad-core/tests/room_history.rs`; verify banners require no additional events.
  - [ ] M3.P2.L1.T6 — Verify tournament, grill, collected-answer, and pick events preserve committed order, attribution, and owning payloads through paging in `crates/squad-core/tests/room_history.rs`.
  - [ ] M3.P2.L1.T7 — Verify census, crosscheck, survey, and probe setup, state, artifact, and report events survive paging and recovery with their human-visible metadata, skipped members, and partial results in `crates/squad-core/tests/room_history.rs`.

- **M3.P2.L2 — Project authoritative state**
  - [ ] M3.P2.L2.T1 — Implement `RoomState` with shared `Presence`, `Score`, contest types, and camelCase projection fields in `crates/squad-core/src/state.rs`.
  - [ ] M3.P2.L2.T2 — Test presence, queue depth, resolved versions, active contest, and pending preflight projections in `crates/squad-core/tests/room_state.rs`.
  - [ ] M3.P2.L2.T3 — Test authoritative scoreboard and daily-spend snapshots, including unknown charges and repeated verdict observation, in `crates/squad-core/tests/room_state.rs`.
  - [ ] M3.P2.L2.T4 — Verify clear invalidates transcript, provisional text, preflights, pending picks, and stale submissions while preserving shared daily accounting in `crates/squad-core/tests/room_clear.rs`.
  - [ ] M3.P2.L2.T5 — Preserve the owning round-score and tally projections in `crates/squad-core/src/state.rs`; verify late and unscored rounds never increment wins or losses in `crates/squad-core/tests/room_state.rs`.
  - [ ] M3.P2.L2.T6 — Project local-day actual spend, reservations, unknown-call counts, and effective daily max from typed store readers in `crates/squad-core/src/state.rs`; test billed failures, cancellations, unknown costs, and day changes using committed accounting fixtures in `crates/squad-core/tests/room_state.rs`.
  - [ ] M3.P2.L2.T7 — Project tournament trophies and active debate, tournament, or grill state in `crates/squad-core/src/state.rs`; test repeated observation, clearing, and absence of duplicate trophy increments in `crates/squad-core/tests/room_state.rs`.
  - [ ] M3.P2.L2.T8 — Preserve collecting and awaiting-pick state, eligible nicks, original question, and committed answer references in `crates/squad-core/src/state.rs`; test restart projections without expiring a pending pick in `crates/squad-core/tests/room_state.rs`.
  - [ ] M3.P2.L2.T9 — Project persisted `GRILL_ROUNDS` independently of `ROUNDS` in `crates/squad-core/src/state.rs`; verify environment seeds apply only when room values are absent in `crates/squad-core/tests/room_state.rs`.
  - [ ] M3.P2.L2.T10 — Add `RoomVars.census_passes` serialized as `CENSUS_PASSES` in `crates/squad-core/src/state.rs`; test persisted-value precedence, default seeding, restart, and independence from existing variables in `crates/squad-core/tests/room_state.rs`.
  - [ ] M3.P2.L2.T11 — Preserve the four new modes’ shared active states, step progress, artifact references, skipped members, and terminal outcomes in `crates/squad-core/src/state.rs`; test census pass and estimate projections, crosscheck acceptance status, survey report references, and probe role and tree projections without recomputing results in `crates/squad-core/tests/room_state.rs`.
  - [ ] M3.P2.L2.T12 — Verify repeated observation and recovery of the four new modes never changes debate scores, tournament trophies, coverage acceptances, or committed report values in `crates/squad-core/tests/room_state.rs`.

- **M3.P2.L3 — Preserve stream identities**
  - [ ] M3.P2.L3.T1 — Expose shared feed items without changing chunk or attempt identities in `crates/squad-core/src/room.rs`.
  - [ ] M3.P2.L3.T2 — Test stalled-consumer recovery through durable history in `crates/squad-core/tests/room_feed.rs`.
  - [ ] M3.P2.L3.T3 — Test higher-attempt replacement and final-event ordering in `crates/squad-core/tests/room_feed.rs`.
  - [ ] M3.P2.L3.T4 — Verify `debate.round_score` feed delivery preserves recipients, round identity, payload, and committed sequence order when scores arrive late in `crates/squad-core/tests/room_feed.rs`.
  - [ ] M3.P2.L3.T5 — Verify tournament, grill, and pick-event feed delivery preserves shared identities and payloads without exposing blind-judge mappings in `crates/squad-core/tests/room_feed.rs`.
  - [ ] M3.P2.L3.T6 — Test recovery after feed broadcast lag by re-reading committed events after the last seen cursor in `crates/squad-core/tests/room_feed.rs`.
  - [ ] M3.P2.L3.T7 — Verify the four new modes’ feed delivery and lag recovery preserve shared step and artifact identities and committed order in `crates/squad-core/tests/room_feed.rs`; confirm human feed access does not become an agent-history capability.

## Phase M3.P3 — Commands and output

- **M3.P3.L1 — Implement command parsing and lifecycle**
  - [ ] M3.P3.L1.T1 — Define the complete command and global-flag set in `crates/squad-cli/src/args.rs`.
  - [ ] M3.P3.L1.T2 — Wire interactive startup, command dispatch, shutdown, and exit codes in `crates/squad-cli/src/main.rs`.
  - [ ] M3.P3.L1.T3 — Test help, version, argument errors, interruption, and lock failures in `crates/squad-cli/tests/commands.rs`.
  - [ ] M3.P3.L1.T4 — Select interactive or one-shot open mode and bypass room opening for models, doctor, and offline export in `crates/squad-cli/src/main.rs`; test absence of unrelated provider calls in `crates/squad-cli/tests/commands.rs`.
  - [ ] M3.P3.L1.T5 — Add census, crosscheck, survey, and probe export choices to command usage in `crates/squad-cli/src/args.rs`; test scope arguments and preserved one-shot and offline lifecycle behavior in `crates/squad-cli/tests/commands.rs`.

- **M3.P3.L2 — Implement send**
  - [ ] M3.P3.L2.T1 — Implement message submission, sequence-based quotes, preconfirmation, and request-scoped waiting in `crates/squad-cli/src/send.rs`.
  - [ ] M3.P3.L2.T2 — Implement labeled text output and the exact JSON lines record shapes in `crates/squad-cli/src/send.rs`.
  - [ ] M3.P3.L2.T3 — Test partial member failure, debate completion, preflight-before-admission output, and noninteractive confirmation failure in `crates/squad-cli/tests/send.rs`.
  - [ ] M3.P3.L2.T4 — Test all shared slash-command results and preserved validation strings in `crates/squad-cli/tests/send_commands.rs`.
  - [ ] M3.P3.L2.T5 — Implement commit-only no-wait submission and accepted-cursor output in `crates/squad-cli/src/send.rs`; verify no dispatch, preserved work, and execution at the next interactive open in `crates/squad-cli/tests/send_no_wait.rs`.
  - [ ] M3.P3.L2.T6 — Render round scores, tallies, late-round labels, unscored markers, and final tally fields in `crates/squad-cli/src/send.rs`; verify JSON preserves the shared event and verdict schemas in `crates/squad-cli/tests/send.rs`.
  - [ ] M3.P3.L2.T8 — Match JSON acknowledgements to shared `requestId`, `eventIds`, and optional `cursor` in `crates/squad-cli/src/send.rs`; test durable and local results without fabricated identity fields or cursors in `crates/squad-cli/tests/send.rs`.
  - [ ] M3.P3.L2.T9 — Render tournament, grill, collecting, and pick events through shared committed-event fixtures in `crates/squad-cli/src/send.rs` and `crates/squad-cli/tests/send.rs`.
  - [ ] M3.P3.L2.T10 — Render kind-specific preflight call counts, estimated costs, remaining daily amount, and preserved admission errors in `crates/squad-cli/src/send.rs`; verify output with shared preflight fixtures in `crates/squad-cli/tests/send.rs`.
  - [ ] M3.P3.L2.T11 — Render census, crosscheck, survey, and probe steps and reports, and add their preflight JSON record shapes in `crates/squad-cli/src/send.rs`; verify script-derived estimates, reused inputs, odd-member sit-out notices, human-visible metadata, and unchanged shared event payloads in `crates/squad-cli/tests/send.rs`.
  - [ ] M3.P3.L2.T12 — Test the four new modes’ skipped-member reporting, partial failures, request-scoped waiting, cancellation, and no-wait persistence in `crates/squad-cli/tests/send.rs` and `crates/squad-cli/tests/send_no_wait.rs`; verify `--yes` does not accept crosscheck residual risks.

- **M3.P3.L3 — Implement exports and model output**
  - [ ] M3.P3.L3.T1 — Implement read-only room and contest export delegation to `squad-core`, standard output, and `-o` handling in `crates/squad-cli/src/export.rs`.
  - [ ] M3.P3.L3.T2 — Verify concurrent export, attribution, contest structure, supersession labels, and excluded secrets in `crates/squad-cli/tests/export.rs`.
  - [ ] M3.P3.L3.T3 — Implement resolved roster and judge-selection output without opening the room in `crates/squad-cli/src/models.rs`.
  - [ ] M3.P3.L3.T4 — Test roster order, concrete versions, sanitized resolution failures, and absence of room opening or unrelated work dispatch in `crates/squad-cli/tests/models.rs`.
  - [ ] M3.P3.L3.T5 — Export committed round scores, reasons, tallies, unscored markers, and final tally fields in `crates/squad-core/src/export/mod.rs`; verify late scores remain associated with their rounds in `crates/squad-cli/tests/export.rs`.
  - [ ] M3.P3.L3.T6 — Derive export banners from committed state transitions in `crates/squad-core/src/export/mod.rs` and render fenced code blocks with the embedded default FIGlet font from `crates/squad-core/src/banner/mod.rs`.
  - [ ] M3.P3.L3.T7 — Add `insta` snapshots for `OPENING`, `ROUND n`, `FINAL ROUND`, and `VERDICT` exports in `crates/squad-cli/tests/export.rs`; verify output is independent of terminal width, banner mode, effects mode, and `NO_COLOR`.
  - [x] M3.P3.L3.T8 — Implement shared banner text rendering and embed compatible fonts in `crates/squad-core/src/banner/`; record asset credits in `NOTICE` and verify deterministic rendering in `crates/squad-core/tests/banner.rs`.
  - [ ] M3.P3.L3.T9 — Implement `RoomHandle::export` in `crates/squad-core/src/room.rs` and `export_offline` with a consistent read snapshot in `crates/squad-core/src/export/mod.rs`; test identical output and concurrent offline reads in `crates/squad-core/tests/export.rs`.
  - [ ] M3.P3.L3.T10 — Add tournament and grill scopes in `crates/squad-cli/src/args.rs` and `crates/squad-cli/src/export.rs`; verify required IDs and shared slash-export routing in `crates/squad-cli/tests/export.rs`.
  - [ ] M3.P3.L3.T11 — Render tournament rankings, critiques, eliminations, placements, champion, and banners in `crates/squad-core/src/export/mod.rs`; snapshot committed fixtures and verify exclusion of the blind-judge map in `crates/squad-core/tests/export.rs`.
  - [ ] M3.P3.L3.T12 — Render grill final plans, all plan versions, questions, answers, plan changes, appendix, and banners in `crates/squad-core/src/export/mod.rs`; snapshot committed fixtures in `crates/squad-core/tests/export.rs`.
  - [ ] M3.P3.L3.T13 — Preserve question-first collection, pending-pick, cancellation, and selected-answer attribution in `crates/squad-core/src/export/mod.rs`; verify exports from committed fixtures in `crates/squad-core/tests/export.rs`.
  - [ ] M3.P3.L3.T14 — Add census, crosscheck, survey, and probe to `ExportScope`, CLI export delegation, shared slash-export routing, and `export.ready` output in `crates/squad-core/src/export/mod.rs`, `crates/squad-cli/src/args.rs`, and `crates/squad-cli/src/export.rs`; verify required contest IDs and scope matching in `crates/squad-cli/tests/export.rs`.
  - [ ] M3.P3.L3.T15 — Render census plan versions, passes, ranked ledger, finder counts, singletons, covered citations, member-by-gap table, estimate range and caveat, discovery curve, quiet-stop result, and grill hint in `crates/squad-core/src/export/mod.rs`; snapshot committed fixtures in `crates/squad-core/tests/export.rs`.
  - [ ] M3.P3.L3.T16 — Render crosscheck inventory, every grid cell and disposition, coverage citations, exclusions, proposed fixes, audit findings, explicit risk acceptances, and bounded stop result in `crates/squad-core/src/export/mod.rs`; verify export cannot close cells or accept risks in `crates/squad-core/tests/export.rs`.
  - [ ] M3.P3.L3.T17 — Render survey claims, votes, confidence, re-vote reasons, switches, all five report bands, and settling questions in `crates/squad-core/src/export/mod.rs`; snapshot committed fixtures in `crates/squad-core/tests/export.rs`.
  - [ ] M3.P3.L3.T18 — Render probe briefs, role assignments, sit-out notice, explanations, checks, prediction matrix, discarded checks, inseparable explanations, and decision tree with unexpected-result branch in `crates/squad-core/src/export/mod.rs`; snapshot committed fixtures in `crates/squad-core/tests/export.rs`.
  - [ ] M3.P3.L3.T19 — Verify identical live and offline exports for all four modes, room-export inclusion, stopped and failed partial runs, skipped-member disclosures, deterministic mode banners and dividers, secret exclusion, and absence of provider calls in `crates/squad-core/tests/export.rs` and `crates/squad-cli/tests/export.rs`.

- **M3.P3.L4 — Implement doctor and diagnostics**
  - [ ] M3.P3.L4.T1 — Implement every doctor check and `--env` documentation output in `crates/squad-cli/src/doctor.rs`.
  - [ ] M3.P3.L4.T2 — Test independent check continuation, secret redaction, terminal reporting, and doctor exit codes in `crates/squad-cli/tests/doctor.rs`.
  - [ ] M3.P3.L4.T3 — Implement bus-log file output and explicit body inclusion in `crates/squad-cli/src/debug.rs`.
  - [ ] M3.P3.L4.T4 — Verify default body redaction and unconditional credential, reasoning, and prompt exclusion in `crates/squad-cli/tests/debug.rs`.
  - [ ] M3.P3.L4.T5 — Include `SQUAD_BANNERS` and `SQUAD_FX` defaults, accepted values, effective values, and sources in `crates/squad-cli/src/doctor.rs`; test reporting and invalid-value diagnostics in `crates/squad-cli/tests/doctor.rs`.
  - [ ] M3.P3.L4.T6 — Report daily spend, reservations, effective max, and unknown-cost call count without opening the room in `crates/squad-cli/src/doctor.rs`; test read-only accounting fixtures in `crates/squad-cli/tests/doctor.rs`.
  - [ ] M3.P3.L4.T7 — Report recorded response-model mismatches through typed readers in `crates/squad-cli/src/doctor.rs`; test concrete-slug warnings without exposing credentials in `crates/squad-cli/tests/doctor.rs`.
  - [ ] M3.P3.L4.T8 — Document and report `SQUAD_DAILY_MAX_USD`, `SQUAD_GRILL_ROUNDS`, `SQUAD_PLAN_WORDS`, `SQUAD_REASONING_EFFORT_<NICK>`, and `SQUAD_FX_DEBUG` in `crates/squad-cli/src/doctor.rs`; test defaults, effective values, sources, and validation failures in `crates/squad-cli/tests/doctor.rs`.
  - [ ] M3.P3.L4.T9 — Verify contest and pick diagnostics preserve shared trace identities while excluding blind-judge maps and private context in `crates/squad-cli/tests/debug.rs`.
  - [ ] M3.P3.L4.T10 — Document and report `SQUAD_CENSUS_PASSES`, its default, range, effective value, source, exact diagnostic, and persisted room-value precedence in `crates/squad-cli/src/doctor.rs`; verify independent checks continue after invalid configuration in `crates/squad-cli/tests/doctor.rs`.
  - [ ] M3.P3.L4.T11 — Verify diagnostics for the four new modes preserve contest, step, work, attempt, and case identities while excluding internal prompts and unrestricted private contexts in `crates/squad-cli/tests/debug.rs`.

- **M3.P3.L5 — Verify command integration**
  - [ ] M3.P3.L5.T1 — Add command-output snapshots and machine-readable JSON lines fixtures in `crates/squad-cli/tests/output.rs`.
  - [ ] M3.P3.L5.T2 — Add the PTY lifecycle check for `squad-cli` and `squad-tui` in `crates/squad-cli/tests/pty.rs`.
  - [ ] M3.P3.L5.T3 — Document `RoomHandle` usage in `crates/squad-core/src/room.rs` and command behavior in `README.md`.
  - [ ] M3.P3.L5.T4 — Record the M3 API and CLI review and owner check-in in `docs/milestones/M3.md`.
  - [ ] M3.P3.L5.T5 — Add shared-schema snapshots for daily-spend state, trophies, active contests, pending picks, and tournament and grill output in `crates/squad-cli/tests/output.rs`.
  - [ ] M3.P3.L5.T6 — Document one-shot recovery, no-wait persistence, concrete model resolution, daily accounting, contest commands, and all export scopes for `squad-cli` in `README.md`.
  - [ ] M3.P3.L5.T7 — Add shared-schema snapshots for `CENSUS_PASSES`, the four new active-contest projections, preflights, artifacts, reports, and export-ready scopes in `crates/squad-cli/tests/output.rs`.
  - [ ] M3.P3.L5.T8 — Document the four new mode submission and export paths, script-derived preflight estimates, skipped-member output, explicit crosscheck risk acceptance, and census variable precedence in `README.md` and `crates/squad-core/src/room.rs`.

