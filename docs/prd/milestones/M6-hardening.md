# M6 — Hardening

**Goal:** Verify Squad’s isolation, recovery, cost controls, performance, and terminal safety against the owning contracts.

**Exit criteria:**

- Deterministic suites and compile-fail documentation tests pass without live credentials.
- Canary checks find no unauthorized deliveries, history entries, or provider inputs.
- Public agent capabilities cannot select another context, access the room event log, or subscribe to the feed; `squad-agents` cannot depend on store implementation code.
- Canonical and extended workflows pass through `RoomHandle`.
- Performance, memory, storage, daily spending, and failure checks meet this specification.
- Terminal sanitisation, key handling, database permissions, and PTY checks pass.
- Logs, artifacts, SQLite records, debug output, and exports contain no sentinel credential.
- Round-scoring isolation, late and failed results, tally arithmetic, verdict delivery, and preflight cost checks pass.
- Banner snapshots, width fallbacks, replay, export, and environment-mode checks pass.
- The 12-stream gate passes with `SQUAD_FX=full`; governor shedding and idle-frame checks pass.
- Tournament isolation, blind judging, ranking validation, forfeits, elimination counts, recovery, and trophy commits pass.
- Grill shared-floor isolation, turn order, passing, context compaction, plan versions, failure handling, and complete exports pass.
- Question-first collection, pick validation, normal room use during pending picks, cancellation, and restart restoration pass.
- Daily accounting, unknown costs, local-day rollover, concurrent reservations, and continued completion of in-flight calls at the max pass.
- One-shot commands preserve unrelated unfinished work, and `--no-wait` defers execution until the next interactive open.
- All seven scripts pass shared-move admission, payload, barrier, call-count, and recovery checks without changing debate, tournament, or grill behavior.
- Census, crosscheck, survey, and probe pass their blind-step canary gates, tally and report checks, retry and skip paths, stopping, restart recovery, and exports.
- New-mode medium-reasoning judge requests, script-derived preflights, daily reservations, banners, report layouts, and reduced-effects behavior pass.

## Release measurement follow-up — 2026-11-25

On dev3, three release latency runs passed: first-delta p95 min/median/max was
203.921/204.788/216.170 ms (6 workers) and 301.917/304.778/307.108 ms (12),
against 500 ms. Replay and sampled RSS also passed. Initial uptime load was
0.41/4.19/4.74; one-minute load was below 2 before each gate invocation.
The third latency run ended at 3.16; a subsequent separate build was allowed to
finish before TUI measurement. Three full 300-second-per-viewport real-widget,
12-stream full-effects release runs passed at 140x42, 80x24 and 200x60, after
repairing stale input-cell and undersized-resize assumptions in the harness.
The missing-cost reservation check failed all three explicit release runs and
remains ignored pending ordinary dispatch/pricing integration. No release-wide
checkbox is inferred from this narrower evidence. Commands, complete metric
min/median/max tables, host/load qualifications, retained failures and outstanding
scope are in [docs/performance.md](../../performance.md). Final Clippy and format
checks passed, but workspace validation stalled in `m6_operational` and returned
101 after that test process was terminated at 4m31s. A 90-second serial retry also
stalled; this unresolved validation failure is retained in the report.

## Phase M6.P1 — Deterministic verification

- **M6.P1.L1 — Protocol and routing coverage**
  - [x] M6.P1.L1.T1 — Add owning-type, parser-string, canonical-limit, and generated parser checks in `crates/squad-protocol/tests/contracts.rs` and `crates/squad-protocol/tests/parser_properties.rs`.
  - [x] M6.P1.L1.T2 — Verify persistence before notification, producer authorization, deduplication, and stale-work fencing in `crates/squad-core/tests/router.rs`.
  - [x] M6.P1.L1.T3 — Verify that direct debate submissions create no ordinary reply work, question-first submissions create the prescribed collection replies, and replay follows integer `Cursor` ordering in `crates/squad-core/tests/router.rs`.
  - [x] M6.P1.L1.T4 — Verify environment precedence, roster rules, stored `ROUNDS`, and compile-time limits in `crates/squad-protocol/tests/config.rs` and `crates/squad-core/tests/configuration.rs`.
  - [x] M6.P1.L1.T5 — Verify `SQUAD_BANNERS` and `SQUAD_FX` defaults and accepted values against Foundations in `crates/squad-protocol/tests/config.rs`.
  - [x] M6.P1.L1.T6 — Verify UTF-8 byte offsets, plain Rust parser results, strict JSON Schema validation before deserialization, required nullable fields, and canonical shared types in `crates/squad-protocol/tests/contracts.rs` and `crates/squad-protocol/tests/parser_properties.rs`.
  - [x] M6.P1.L1.T7 — Verify daily max, persistent `GRILL_ROUNDS`, plan words, per-member reasoning settings, effects debug settings, reasoning budgets, judge deadlines, plan expiry, work leases, and channel capacities in `crates/squad-protocol/tests/config.rs` and `crates/squad-core/tests/configuration.rs`.
  - [x] M6.P1.L1.T8 — Verify the four new verbs, argument rules, exact parser errors, variable reads and assignments, exclusions, and UTF-8 offsets in `crates/squad-protocol/tests/parser_properties.rs`.
  - [x] M6.P1.L1.T9 — Verify persistent `CENSUS_PASSES`, default 2, bounds 1 to 5, optional environment seeding and precedence, independence from other variables, and absence of `SQUAD_CENSUS_PASSES` from `.env.example` in `crates/squad-protocol/tests/config.rs` and `crates/squad-core/tests/configuration.rs`.
  - [x] M6.P1.L1.T10 — Verify the four modes’ envelope kinds, work kinds, visibility rows, ErrorCodes, durable artifacts, and RoomHandle state against their owning contracts in `crates/squad-protocol/tests/contracts.rs` and `crates/squad-core/tests/router.rs`.

- **M6.P1.L2 — Isolation release gate**
  - [x] M6.P1.L2.T1 — Add independent recipient expectations and generated canary sequences in `crates/squad-core/tests/isolation.rs`.
  - [x] M6.P1.L2.T2 — Assert context-bound reader behavior, mailbox destinations, feed exclusion, and captured provider inputs in `crates/squad-core/tests/isolation.rs`.
  - [x] M6.P1.L2.T3 — Add compile-fail examples for cross-context reads, backing-database access, and feed access in `crates/squad-agents/src/context.rs`; verify the crate excludes `squad-store` and `rusqlite` dependencies in `crates/squad-agents/Cargo.toml`.
  - [x] M6.P1.L2.T4 — Verify quote delivery, forged-output rejection, and absence of system authority in `crates/squad-core/tests/isolation.rs`.
  - [x] M6.P1.L2.T5 — Verify parallel barriers in both completion orders and unique ordered committed identifiers in `crates/squad-core/tests/isolation.rs`.
  - [x] M6.P1.L2.T6 — Verify new judge cases, follow-ups, substitution, and unrelated-case exclusion in `crates/squad-core/tests/isolation.rs`.
  - [x] M6.P1.L2.T7 — Repeat canary assertions after retry, restart, context trimming, regeneration, and replay in `crates/squad-core/tests/isolation.rs`.
  - [x] M6.P1.L2.T8 — Verify that orchestrator execution delegates prompt assembly to context-bound workers in `crates/squad-core/tests/isolation.rs`.
  - [x] M6.P1.L2.T9 — Verify round scores reach only `me` and the judge case context before the verdict, remain absent from debater histories and provider requests, and reach both debaters with the verdict announcement in `crates/squad-core/tests/isolation.rs`.
  - [x] M6.P1.L2.T10 — Verify deferred history writes for held messages and frozen round boundaries across retries, including quotes of current-round opponent output, in `crates/squad-core/tests/isolation.rs`.
  - [x] M6.P1.L2.T11 — Verify concrete-model judge conflicts for debates and standalone rulings, fallback skipping, and the prescribed substitution warning in `crates/squad-core/tests/judge_workflows.rs`.
  - [x] M6.P1.L2.T12 — Verify case-bound repair-data access, rejected-output exclusion from accepted history, and judge evidence rendered once in captured requests in `crates/squad-core/tests/isolation.rs`.
  - [x] M6.P1.L2.T13 — Verify shared moves deliver each actor’s own payload without union broadcasts, undeclared shared floors, or human-report leakage in `crates/squad-core/tests/isolation.rs`.
  - [x] M6.P1.L2.T14 — Run the four modes’ blind-step canaries through actual router commits, bound readers, mailboxes, repair payloads, and captured requests after retries, cancellation, restart, trimming, and replay in `crates/squad-core/tests/isolation.rs`.
  - [x] M6.P1.L2.T15 — Seed ordinary histories, prior passes, unrelated contests, and judge cases with forbidden material and verify fresh or restricted contexts exclude it according to each step’s contract in `crates/squad-core/tests/isolation.rs`.

- **M6.P1.L3 — Provider and TUI integration**
  - [x] M6.P1.L3.T1 — Run the exact canonical inputs through `RoomHandle` using the M2 fixture and verify the authorized transcript in `crates/squad-core/tests/canonical_flow.rs`.
  - [x] M6.P1.L3.T2 — Verify round 0, waiting debaters, held ordinary work, and unrelated-member progress in `crates/squad-core/tests/debate_workflows.rs`.
  - [x] M6.P1.L3.T3 — Verify optional judging, later `/judge`, standalone rulings, case follow-ups, winner selection, and computed totals in `crates/squad-core/tests/judge_workflows.rs`.
  - [x] M6.P1.L3.T4 — Verify regeneration, export, scoreboard updates, persistent variables, personas, and environment-driven participants in `crates/squad-core/tests/room_workflows.rs`.
  - [x] M6.P1.L3.T5 — Add `TestBackend` and `insta` checks for streaming, quotes, recipient recall, search, resizing, daily spending, and debate states in `crates/squad-tui/tests/interaction.rs`.
  - [x] M6.P1.L3.T6 — Add one PTY-driven end-to-end test covering the canonical flow and terminal lifecycle in `crates/squad-cli/tests/pty.rs`. (covered by crates/squad-cli/tests/pty.rs::at_all_hello_streams_replies_and_ctrl_c_restores_the_terminal)
  - [x] M6.P1.L3.T7 — Verify round-scoring work creation after barriers, accumulated judge context, no scoring of round 0, nonblocking late scores, and final-judge gating in `crates/squad-core/tests/judge_workflows.rs`.
  - [x] M6.P1.L3.T8 — Verify round-score validation, equal scores, two-attempt exhaustion, unscored records, final prompt disclosure, tally arithmetic, and override-reason repair in `crates/squad-core/tests/judge_workflows.rs`.
  - [x] M6.P1.L3.T9 — Verify pending, scored, late, and unscored scorecards, expandable reasons, running tallies, and verdict-only win-loss updates in `crates/squad-tui/tests/interaction.rs`.
  - [x] M6.P1.L3.T10 — Add `insta` snapshots for every banner at 120, 80, and 40 columns and with `NO_COLOR`; verify font fallbacks, centring, accent colour, row-cache height, environment modes, and state-derived replay in `crates/squad-tui/tests/banners.rs`.
  - [x] M6.P1.L3.T11 — Verify fenced default-font banners in markdown export and embedded font licences without external rendering commands in `crates/squad-core/tests/export.rs` and `crates/squad-core/tests/banners.rs`. (covered by crates/squad-core/tests/banner.rs::export_is_fixed_while_viewport_and_modes_change; tests/export.rs snapshots and font_licences_audit.rs::vendored_font_licence_and_notice_ship_with_both_embedded_fonts)
  - [x] M6.P1.L3.T12 — Verify visible ceilings plus explicit supported reasoning budgets, omitted reasoning parameters for unsupported models, and rejection of explicitly unsupported settings in `crates/squad-agents/tests/openrouter.rs`.
  - [x] M6.P1.L3.T13 — Verify alias probes, the single 400 fallback, concrete-slug completions, per-attempt response models, mismatch warnings, and doctor reporting in `crates/squad-agents/tests/model_registry.rs` and `crates/squad-cli/tests/doctor.rs`.
  - [x] M6.P1.L3.T14 — Verify judge-only plans from terminal debate states, cancellation and expiry restoring prior status, and retained case lists in `crates/squad-core/tests/judge_workflows.rs`.
  - [x] M6.P1.L3.T15 — Verify every new-mode judge request uses the judge system prompt, its mode task template, strict schema, medium reasoning effort, and 4,096-token visible ceiling in `crates/squad-agents/tests/openrouter.rs`.
  - [x] M6.P1.L3.T16 — Add banner, step-divider, collapsible-report, width-fallback, replay, and reduced-effects snapshots for census, crosscheck, survey, and probe in `crates/squad-tui/tests/banners.rs` and `crates/squad-tui/tests/effects.rs`.

- **M6.P1.L4 — Tournament verification**
  - [x] M6.P1.L4.T1 — Verify named entrants, `@all` exclusions, implied judging, minimum-three validation, and shared contest admission in `crates/squad-core/tests/tournament_workflows.rs` and `crates/squad-protocol/tests/parser_properties.rs`.
  - [x] M6.P1.L4.T2 — Verify parallel entries, own-result-only histories and provider requests, final placing lists without answer text, and non-entrant exclusion in `crates/squad-core/tests/tournament_isolation.rs`.
  - [x] M6.P1.L4.T3 — Verify shuffled labels each round, a server-only label map, no entrant identity metadata in judge context, and no judge swap when `fable` competes in `crates/squad-core/tests/tournament_isolation.rs`.
  - [x] M6.P1.L4.T4 — Verify strict ranking permutations, no ties, integer scores from 1 to 10, critiques, three-attempt repair, one round replay, and terminal judging failure in `crates/squad-core/tests/tournament_workflows.rs`.
  - [x] M6.P1.L4.T5 — Verify forfeits ranked last, exactly one elimination per round, k−1 rounds, champion completion, and one trophy increment without changing debate win-loss records in `crates/squad-core/tests/tournament_workflows.rs`.
  - [x] M6.P1.L4.T6 — Verify tournament banners, bottom-up ranking reveals, eliminated rows, champion celebration, replay behavior, and reduced-effects modes in `crates/squad-tui/tests/tournament.rs` and `crates/squad-tui/tests/effects.rs`.
  - [x] M6.P1.L4.T7 — Verify complete tournament markdown through `RoomHandle::export` and offline export in `crates/squad-core/tests/export.rs` and `crates/squad-cli/tests/export.rs`.

- **M6.P1.L5 — Grill verification**
  - [x] M6.P1.L5.T1 — Verify latest, selected, and quoted leader plans, default and explicit grillers, first-mention leader selection, missing-plan validation, and independent `GRILL_ROUNDS` in `crates/squad-core/tests/grill_workflows.rs`.
  - [x] M6.P1.L5.T2 — Verify shared-floor delivery to exactly the participants, router-only history writes, and non-participant canary exclusion in `crates/squad-core/tests/grill_isolation.rs`.
  - [x] M6.P1.L5.T3 — Verify sequential roster order rotated by one each round, rotating primary angles, up to three questions, direct answers, and plan changes in `crates/squad-core/tests/grill_workflows.rs`.
  - [x] M6.P1.L5.T4 — Verify exact `PASS` handling, all-pass completion with reason `survived`, required final rebuild behavior, and rebuild skipping when no questions were asked at all in `crates/squad-core/tests/grill_workflows.rs`.
  - [x] M6.P1.L5.T5 — Verify latest-plan and current-round working context, earlier-question lists, no repeated questions, plan word limits, retained versions, and final appendix completeness in `crates/squad-core/tests/grill_workflows.rs` and `crates/squad-core/tests/export.rs`.
  - [x] M6.P1.L5.T6 — Verify skipped failed grillers, leader failure after retry, last-good-plan preservation, and absence of judge work, winners, or scoreboard changes in `crates/squad-core/tests/grill_workflows.rs`.
  - [x] M6.P1.L5.T7 — Verify grill banners, turn headings, question and pass counters, newest-expanded plan versions, handoffs, version bumps, and survived effects in `crates/squad-tui/tests/grill.rs` and `crates/squad-tui/tests/effects.rs`.
  - [x] M6.P1.L5.T8 — Verify `/export grill [id]`, `squad export grill`, all plan versions, and the final plan with its complete question-and-answer appendix in `crates/squad-cli/tests/export.rs` and `crates/squad-core/tests/export.rs`.

- **M6.P1.L6 — Question-first contests and picks**
  - [x] M6.P1.L6.T1 — Verify the parser-owned debate and grill decision table, required-question strings, direct shortcuts, and the no-text explicit-griller exception in `crates/squad-protocol/tests/parser_properties.rs`.
  - [x] M6.P1.L6.T2 — Verify `COLLECTING_ANSWERS`, ordinary mutually invisible replies, completion after every addressed member answers, errors, or stops, and eligible-answer selection in `crates/squad-core/tests/contest_picks.rs`.
  - [x] M6.P1.L6.T3 — Verify `ContestPick { contestId, nicks }`, `contest.collecting`, `contest.pick_requested` addressed to `me`, and `contest.picked` using the Protocol contracts in `crates/squad-core/tests/contest_picks.rs`.
  - [x] M6.P1.L6.T4 — Verify two eligible debate picks, one eligible grill leader with every other answering member as griller, original question and answer references, default debate judging, post-pick preflight, and no tournament pick in `crates/squad-core/tests/contest_picks.rs`.
  - [x] M6.P1.L6.T5 — Verify invalid-pick strings and pending-state preservation, normal messages while a pick is pending, one active contest, and insufficient-answer failure in `crates/squad-core/tests/contest_picks.rs`.
  - [x] M6.P1.L6.T6 — Verify pick banners, hints, placeholder, `pick pending`, eligible-first autocomplete, candidate highlights, and narrow-terminal fallbacks in `crates/squad-tui/tests/contest_picks.rs` and `crates/squad-tui/tests/effects.rs`.
  - [x] M6.P1.L6.T7 — Verify `/stop` and confirmed Escape cancellation produce `STOPPED` with reason `pick_cancelled`, preserve ordinary answers and private histories, and release the active-contest slot in `crates/squad-core/tests/contest_picks.rs` and `crates/squad-tui/tests/contest_picks.rs`.
  - [x] M6.P1.L6.T8 — Verify pending picks never expire and survive restart with their eligible answers and original references, without regenerating answers or applying preflight expiry to the pick, in `crates/squad-core/tests/recovery.rs`.

- **M6.P1.L7 — Shared engine verification**
  - [x] M6.P1.L7.T1 — Verify all seven scripts use the shared moves with their declared actors, payload visibility, prompt templates, schemas, word limits, and failure rules in `crates/squad-core/tests/contest_engine.rs`.
  - [x] M6.P1.L7.T2 — Verify one active contest across every mode pairing, including collection, picks, and between-pass states, using the existing busy-contest error in `crates/squad-core/tests/contest_engine.rs`. (covered by crates/squad-core/tests/contest_engine.rs::one_active_contest_blocks_every_other_mode_in_every_state)
  - [x] M6.P1.L7.T3 — Verify parallel completion orders, atomic barrier commits, exact delivery destinations, and replay without duplicate artifacts or histories in `crates/squad-core/tests/contest_engine.rs`. (covered by crates/squad-core/tests/contest_engine.rs::barriers_commit_atomically_deliver_exactly_and_replay_without_duplicates)
  - [x] M6.P1.L7.T4 — Verify model-call accounting for `collect`, `fanout`, `turns`, `judge`, and report variants, with no provider calls for `pick`, `tally`, or `barrier`, in `crates/squad-core/tests/contest_engine.rs`. (covered by crates/squad-core/tests/contest_engine.rs::only_generation_moves_reach_the_provider_and_accounting_matches_calls; census_workflows.rs, survey_workflows.rs and probe_workflows.rs)
  - [x] M6.P1.L7.T5 — Run existing debate, tournament, grill, and pick fixtures through the scripts and verify unchanged visibility, timing dependencies, retries, judging, scoring, and accounting in `crates/squad-core/tests/contest_engine.rs`. (covered by crates/squad-core/tests/contest_engine.rs plus router_debates.rs, tournament_workflows.rs, grill_workflows.rs and room_actions.rs workflow fixtures)

- **M6.P1.L8 — Census verification**
  - [x] M6.P1.L8.T1 — Verify plan-source selection, minimum participants, pass limits, lens order, repeated and rotated lenses, and up to eight omissions with consequences in `crates/squad-core/tests/census_workflows.rs`.
  - [x] M6.P1.L8.T2 — Verify mutually invisible hunts and rating ledgers without finder names, model names, counts, or provenance tables in histories and captured requests in `crates/squad-core/tests/census_isolation.rs`.
  - [x] M6.P1.L8.T3 — Verify duplicate merging, distinct-member finder counts, rating validation, required covered citations, majority removal, retained citations, and added-gap handling in `crates/squad-core/tests/census_workflows.rs`.
  - [x] M6.P1.L8.T4 — Verify application-computed Chao1 values for `f2 > 0`, `f2 = 0`, and empty finds, plus the owning range rule, exact estimate headline, floor caveat, and no fabricated finder counts for rating additions in `crates/squad-core/tests/census_workflows.rs`.
  - [x] M6.P1.L8.T5 — Verify plan versions and human patches between passes, discovery curves, singleton highlights, ranked ledgers, the human-only attribution table, and both conditions for the exact quiet message in `crates/squad-core/tests/census_workflows.rs`.
  - [x] M6.P1.L8.T6 — Verify skipped-member disclosure, incomplete-evidence behavior, report and export evidence, banner and ledger rendering, estimate effects, and the non-executing singleton-to-grill hint in `crates/squad-core/tests/export.rs` and `crates/squad-tui/tests/census.rs`.

- **M6.P1.L9 — Crosscheck verification**
  - [x] M6.P1.L9.T1 — Verify plan and goal selection, optional extraction, six inventory lenses, three items per member, and plan-blind inventory requests despite prior ordinary plan history in `crates/squad-core/tests/crosscheck_workflows.rs` and `crates/squad-core/tests/crosscheck_isolation.rs`.
  - [x] M6.P1.L9.T2 — Verify inventory provenance, every single and cross-lens pair, the 18-item and 153-cell default fixture, the 200-cell priority cap, balanced assignment, and no self-review in `crates/squad-core/tests/crosscheck_workflows.rs`.
  - [x] M6.P1.L9.T3 — Verify reviewers receive only their cells, closure citations and exclusion reasons are required, gaps include proposed fixes, and “probably covered” remains open in `crates/squad-core/tests/crosscheck_workflows.rs` and `crates/squad-core/tests/crosscheck_isolation.rs`.
  - [x] M6.P1.L9.T4 — Verify persisted random auditor selection and closure samples, fresh context for each auditor, five-closure checks under the owning small-sample rule, missing-case proposals, and no reviewer identity or earlier work in auditor requests in `crates/squad-core/tests/crosscheck_isolation.rs`.
  - [x] M6.P1.L9.T5 — Verify audit challenges reopen false closures, rulings retain all admitted cells, proposed additions are accounted for, and only explicit human acceptance can accept residual risks in `crates/squad-core/tests/crosscheck_workflows.rs`.
  - [x] M6.P1.L9.T6 — Verify the bounded stop condition, skipped-member disclosure, complete coverage ledger and export, grid summary, and cell-filling effects in `crates/squad-core/tests/export.rs` and `crates/squad-tui/tests/crosscheck.rs`.

- **M6.P1.L10 — Survey verification**
  - [x] M6.P1.L10.T1 — Verify question-first collection and reuse of the most recent parallel answer set for one question without mixing questions or recollecting existing answers in `crates/squad-core/tests/survey_workflows.rs`.
  - [x] M6.P1.L10.T2 — Verify up to 25 neutral deduplicated atomic claims, private provenance, no peer-answer delivery, and initial blind votes without identities or counts in `crates/squad-core/tests/survey_isolation.rs`.
  - [x] M6.P1.L10.T3 — Verify vote enums, confidence bounds, reasons, application tallies, split ordering and tie rules, and selection of up to five claims under the owning contract in `crates/squad-core/tests/survey_workflows.rs`.
  - [x] M6.P1.L10.T4 — Verify revote requests contain only selected claims and anonymous opposing reasons, with no authorship, vote counts, peer votes, or confidence aggregates, in `crates/squad-core/tests/survey_isolation.rs`.
  - [x] M6.P1.L10.T5 — Verify switches, consensus, contested claims, lone insights, lone dissents, rejected outliers, settling questions, skipped votes, and non-six-member rosters without declaring truth or a winner in `crates/squad-core/tests/survey_workflows.rs`.
  - [x] M6.P1.L10.T6 — Verify five-band report rendering, bar-splitting effects, human-visible counts and switches, and complete markdown export in `crates/squad-tui/tests/survey.rs` and `crates/squad-core/tests/export.rs`.

- **M6.P1.L11 — Probe verification**
  - [x] M6.P1.L11.T1 — Verify required brief fields, at least four active members, odd-count last-member sit-out with transcript notice, and a persisted equal random role split shown to the human in `crates/squad-core/tests/probe_workflows.rs`.
  - [x] M6.P1.L11.T2 — Verify blind explanation and check proposals, key assumptions, exact measurements and outcomes, and no explanations reaching checkers in `crates/squad-core/tests/probe_isolation.rs`.
  - [x] M6.P1.L11.T3 — Verify each explainer receives all checks and only its own explanation, records every prediction or inability to predict, and cannot see rival explanations or predictions before the matrix locks in `crates/squad-core/tests/probe_isolation.rs`.
  - [x] M6.P1.L11.T4 — Verify non-distinguishing checks are removed, useful checks are ordered, inseparable explanations remain flagged, failed members are named, and the report declares no explanation true in `crates/squad-core/tests/probe_workflows.rs`.
  - [x] M6.P1.L11.T5 — Verify runnable tree branches, prediction matrix, unexpected-result branch, proposed-check-only behavior, tree-drawing effects, and complete markdown export in `crates/squad-tui/tests/probe.rs` and `crates/squad-core/tests/export.rs`.

## Phase M6.P2 — Operational controls

- **M6.P2.L1 — Tracing and limits**
  - [x] M6.P2.L1.T1 — Verify file-only JSON tracing, required fields, and correlation across retries and judging in `crates/squad-core/tests/observability.rs`.
  - [x] M6.P2.L1.T2 — Verify request, commit, and wake-up inspection with redacted bodies in `crates/squad-core/tests/bus_log.rs`.
  - [x] M6.P2.L1.T3 — Verify `--bus-log`, explicit body capture, and F12 inspection against the owning contract in `crates/squad-cli/tests/debug.rs` and `crates/squad-tui/tests/debug.rs`. (covered by crates/squad-cli/tests/release.rs::bus_log_is_written_and_bodies_require_the_explicit_flag; crates/squad-tui/tests/contest_actions.rs::debug_diagnostics_are_redacted_codes_and_counters and ui_composed.rs F12 exercise)
  - [x] M6.P2.L1.T4 — Add boundary checks for message size, queues, context selection, bounded delivery, and durable paging in `crates/squad-core/tests/limits.rs`.
  - [x] M6.P2.L1.T5 — Verify `STORAGE_LIMIT`, history preservation, and transactional `/clear` followed by `VACUUM` in `crates/squad-store/tests/storage.rs`.
  - [x] M6.P2.L1.T6 — Add repeatable latency and whole-process memory measurements in `crates/squad-cli/tests/performance.rs`.
  - [ ] M6.P2.L1.T7 — Record streaming and transcript memory results for `squad-cli` and `squad-tui` in `docs/performance.md`.
  - [x] M6.P2.L1.T8 — Run the 12-stream latency and memory gate with `SQUAD_FX=full` in `crates/squad-cli/tests/performance.rs` and record results for `squad-cli` and `squad-tui` in `docs/performance.md`.
  - [x] M6.P2.L1.T9 — Verify the frame-budget governor sheds effects under load and that `SQUAD_FX=off` and `NO_COLOR` each produce no animation frames when idle in `crates/squad-tui/tests/effects.rs`.
  - [x] M6.P2.L1.T10 — Verify capacity-1 synchronous mailbox `try_send`, Full as a pending wake-up, no router waits on agent-owned channels, bounded ingress, and feed-lag recovery from the last cursor in `crates/squad-core/tests/limits.rs`.
  - [x] M6.P2.L1.T11 — Verify work lease expiry and 20-second renewals under the 60-second lease contract in `crates/squad-core/tests/recovery.rs`.
  - [x] M6.P2.L1.T12 — Verify the single TUI event loop’s effect-driven timer, 60 Hz cap, idle disarming, input priority, and feed batches of at most 256 in `crates/squad-tui/tests/event_loop.rs`.
  - [x] M6.P2.L1.T13 — Verify palette ownership, internal live/replay tagging, inline settling, colour-only clear, resolved-state roster effects, verdict timing, and unset-only automatic lite mode in `crates/squad-tui/tests/effects.rs`.
  - [x] M6.P2.L1.T14 — Verify new-mode tracing correlates script steps, work attempts, barriers, and reports without default body capture or reasoning disclosure in `crates/squad-core/tests/observability.rs`. (covered by crates/squad-core/src/router/observability_tests.rs::step_attempt_barrier_and_report_traces_exclude_bodies; router/mod.rs::trace_outcome)
  - [x] M6.P2.L1.T15 — Exercise maximum census passes, a capped crosscheck grid, 25 survey claims, and a 12-member probe while verifying paging, bounded delivery, context-fit failures, responsive input, and effects shedding in `crates/squad-core/tests/limits.rs` and `crates/squad-tui/tests/effects.rs`.

- **M6.P2.L2 — Daily spending enforcement**
  - [x] M6.P2.L2.T1 — Verify idempotent reservation reconciliation using the owning accounting contract in `crates/squad-core/tests/budget.rs`.
  - [x] M6.P2.L2.T2 — Test concurrent reservations, permitted attempts, interrupted requests, and retained uncertain charges in `crates/squad-core/tests/budget.rs`.
  - [x] M6.P2.L2.T3 — Verify round-count and model-call estimates, threshold auto-confirmation, explicit confirmation, cancellation, changed plans, and 10-minute plan expiry in `crates/squad-core/tests/budget.rs`.
  - [x] M6.P2.L2.T4 — Verify `squad send --yes` and TUI confirmation behavior in `crates/squad-cli/tests/send.rs` and `crates/squad-tui/tests/budget.rs`.
  - [x] M6.P2.L2.T5 — Test `DAILY_MAX_REACHED`, prevention of queued or subsequent dispatch, and continued completion of in-flight calls in `crates/squad-core/tests/budget.rs`.
  - [x] M6.P2.L2.T6 — Verify today’s spent, reserved, remaining, and unknown-call amounts after replay and restart in `crates/squad-core/tests/budget.rs` and `crates/squad-tui/tests/budget.rs`.
  - [x] M6.P2.L2.T7 — Verify preflight includes one round-scoring call per round and reservations cover two attempts, accumulated judge context, low reasoning effort with its 1,024-token budget, and the 512-token visible ceiling at `ROUNDS` default 10, min 1, and max 20 in `crates/squad-core/tests/budget.rs`.
  - [x] M6.P2.L2.T8 — Verify no round-scoring reservations for debates without automatic judging, no retroactive scoring charges for later `/judge`, and correct reconciliation after failed or cancelled round scoring in `crates/squad-core/tests/budget.rs`.
  - [x] M6.P2.L2.T9 — Verify actual reported costs, including billed failed and cancelled calls, update `spend_daily` atomically with completion events and never double count on replay in `crates/squad-store/tests/spend_daily.rs` and `crates/squad-core/tests/budget.rs`.
  - [x] M6.P2.L2.T10 — Verify local calendar-day rollover, shared totals across rooms, restart persistence, and `/clear` preserving daily accounting in `crates/squad-store/tests/spend_daily.rs` and `crates/squad-core/tests/budget.rs`.
  - [x] M6.P2.L2.T11 — Verify missing-cost reconciliation, retained uncertain reservations, `cost unknown`, the bar’s `~` prefix, and doctor counts in `crates/squad-core/tests/budget.rs`, `crates/squad-tui/tests/budget.rs`, and `crates/squad-cli/tests/doctor.rs`.
  - [x] M6.P2.L2.T12 — Verify supported reasoning budgets are priced alongside visible output, unavailable pricing blocks dispatch with `COST_UNKNOWN`, and no pricing override bypass exists in `crates/squad-core/tests/budget.rs`.
  - [x] M6.P2.L2.T13 — Verify the always-visible spend bar, lighter pending segment replaced by actual cost, 75% and 90% colour thresholds, configured max, and narrow-layout status-line placement in `crates/squad-tui/tests/budget.rs`.
  - [x] M6.P2.L2.T14 — Verify tournament and grill model-call counts, retry and replay reservations, no grill judge cost, and question-first collection charges excluded from post-pick estimates in `crates/squad-core/tests/budget.rs`.
  - [x] M6.P2.L2.T15 — Verify the exact daily-max message, preflight refusal above remaining capacity, equality boundaries, and new-request eligibility after a local-day change or raised max in `crates/squad-core/tests/budget.rs` and `crates/squad-cli/tests/send.rs`.
  - [x] M6.P2.L2.T16 — Verify script-derived new-mode counts for participant bounds and default fixtures: census 14 per pass and 28 for two passes, crosscheck 16 or 17 with extraction, survey 20 or 14 with reused answers, and probe 10 after sit-out resolution in `crates/squad-core/tests/budget.rs`.
  - [x] M6.P2.L2.T17 — Verify new-mode judge reservations use medium supported reasoning budgets, 4,096 visible tokens, actual script evidence, and permitted repair attempts without adding calls for app-only moves in `crates/squad-core/tests/budget.rs`.
  - [x] M6.P2.L2.T18 — Verify plan patches, later census passes, early quiet completion, skipped members, optional extraction, survey reuse, and report variants adjust remaining work and reservations without charging twice in `crates/squad-core/tests/budget.rs`.
  - [x] M6.P2.L2.T19 — Verify all four modes refuse unaffordable preflights, block undispatched paid steps at the daily max, retain uncertain charges, and reconcile stop and restart paths without duplicate spending in `crates/squad-core/tests/budget.rs`.

## Phase M6.P3 — Failure and security checks

- **M6.P3.L1 — Recovery**
  - [x] M6.P3.L1.T1 — Verify OpenRouter 429, 5xx, timeout, partial-stream, and single-model failures against the owning attempt policies in `crates/squad-core/tests/provider_failures.rs`.
  - [x] M6.P3.L1.T2 — Inject SQLite busy, corrupt, and disk-full failures and verify failed commits remain unacknowledged in `crates/squad-store/tests/failures.rs`.
  - [x] M6.P3.L1.T3 — Verify atomic recovery after crashes before, during, and after commit, including lost post-commit wake-ups, in `crates/squad-core/tests/recovery.rs`.
  - [x] M6.P3.L1.T4 — Verify debate recovery at dispatch, result acceptance, and barrier boundaries in `crates/squad-core/tests/recovery.rs`.
  - [x] M6.P3.L1.T5 — Verify global and targeted stopping, late-result rejection, ordinary retry actions, and later manual judging in `crates/squad-core/tests/cancellation.rs`.
  - [x] M6.P3.L1.T6 — Verify the exact same-room lock message and concurrent read-only export in `crates/squad-cli/tests/room_lock.rs`.
  - [x] M6.P3.L1.T7 — Verify terminal closure during streaming, terminal restoration, and recovery on the next start in `crates/squad-cli/tests/pty.rs`. (covered by crates/squad-cli/tests/pty.rs::hangup_mid_stream_exits_leaves_the_store_sound_and_the_next_start_recovers)
  - [x] M6.P3.L1.T8 — Verify round-scoring recovery preserves attempt counts, accepted scores, tallies, and final-judge gating without duplicate committed effects in `crates/squad-core/tests/recovery.rs`.
  - [x] M6.P3.L1.T9 — Verify `/stop` cancels outstanding round-scoring work, rejects late results, and later `/judge` uses existing scores without retroactive scoring in `crates/squad-core/tests/cancellation.rs`; verify `COMPLETE_UNJUDGED` debates create no round-scoring work in `crates/squad-core/tests/judge_workflows.rs`.
  - [x] M6.P3.L1.T10 — Verify `OpenMode::Interactive` and `OpenMode::OneShot`, unrelated-work preservation, models and doctor avoiding room open, and `--no-wait` printing the accepted cursor and deferring execution in `crates/squad-core/tests/recovery.rs` and `crates/squad-cli/tests/send.rs`.
  - [x] M6.P3.L1.T11 — Verify tournament and grill recovery at dispatch, result, elimination, plan-version, and terminal boundaries, with no duplicate trophy, result, history, or accounting effects, in `crates/squad-core/tests/recovery.rs`.
  - [x] M6.P3.L1.T12 — Verify router recovery spawns case executors for unfinished work, preserves attempt-scoped repair data, and retires finished executors under the owning lifecycle contract in `crates/squad-core/tests/recovery.rs`.
  - [x] M6.P3.L1.T13 — Crash before, during, and after every new-mode step and barrier; verify committed artifacts, state, attempt counts, histories, reports, and accounting recover atomically in `crates/squad-core/tests/recovery.rs`.
  - [x] M6.P3.L1.T14 — Verify census plan versions and lenses, crosscheck cell assignments and audit samples, survey claim provenance and votes, and probe roles and locked predictions survive restart without resampling or duplicate dispatch of completed work in `crates/squad-core/tests/recovery.rs`.
  - [x] M6.P3.L1.T15 — Verify member retry exhaustion skips and names the failed step, missing evidence is not fabricated, judge schema-repair exhaustion follows the script, and accepted artifacts remain available in `crates/squad-core/tests/provider_failures.rs`.
  - [x] M6.P3.L1.T16 — Verify `/stop`, targeted cancellation, and confirmed Escape at each new-mode step reject late results, preserve partial evidence and charges, release holds and admission, and never produce a false successful stop indication in `crates/squad-core/tests/cancellation.rs`.

- **M6.P3.L2 — Terminal and credential safety**
  - [x] M6.P3.L2.T1 — Test escape-sequence and OSC 52 sanitisation, including split chunks and alternate control encodings, in `crates/squad-tui/tests/terminal_safety.rs` and `crates/squad-cli/tests/output_safety.rs`.
  - [x] M6.P3.L2.T2 — Verify that model output and pasted content cannot trigger key actions in `crates/squad-tui/tests/interaction.rs`.
  - [x] M6.P3.L2.T3 — Verify database mode 0600 and private SQLite auxiliary files in `crates/squad-store/tests/permissions.rs`.
  - [x] M6.P3.L2.T4 — Verify sentinel-key absence from artifacts, logs, SQLite, debug output, doctor output, and exports in `crates/squad-cli/tests/credential_safety.rs`.
  - [x] M6.P3.L2.T5 — Verify that logging and accounting never forward reasoning text or emit telemetry in `crates/squad-core/tests/privacy.rs`.
  - [x] M6.P3.L2.T6 — Verify the exact shipped example and Foundations setup requirements for `squad-cli` using `.env.example` and `setup.sh`.
  - [x] M6.P3.L2.T7 — Verify Escape dismisses the topmost transient first, contest stopping requires a second Escape within 2 s, expired confirmation does not stop, and animation skipping consumes no Escape or input keys in `crates/squad-tui/tests/interaction.rs`.
  - [x] M6.P3.L2.T8 — Inject routing instructions, forged author metadata, terminal controls, automatic risk acceptance, and executable-looking checks into new-mode artifacts; verify they cannot grant visibility, authorize spending, accept risks, execute checks, or control the terminal in `crates/squad-core/tests/isolation.rs` and `crates/squad-tui/tests/terminal_safety.rs`.

