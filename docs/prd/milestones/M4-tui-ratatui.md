# M4 — TUI (Ratatui)

**Goal:** Deliver the Ratatui interface for streamed chat, editing, quotes, debates, tournaments, grills, census, crosscheck, survey, probe, question-first picks, judging, history, recovery, daily accounting, and Markdown export through `RoomHandle`. **Exit criteria:**

- The initial 12-stream spike passes its measured drawing, input-latency, and CPU gate on both release platforms using a throwaway renderer with effects off; after M4.P5, the real transcript widget passes the same gate with `SQUAD_FX=full`.
- Both viewports preserve input, selection, readable participant information, and anchored scrolling.
- Markdown caches finalized blocks, highlights fenced code, and renders only visible transcript rows.
- Keyboard enhancement and fallbacks, bracketed paste, mouse scrolling, search, OSC 52 copy, and terminal restoration pass verification.
- Quotes, standalone cases, follow-ups, judging, regeneration, exclusions, and room clearing match shared contracts.
- Preflight, queue states, scoreboard, and daily-spend displays match authoritative state without duplicate accounting.
- Snapshot, integration, and PTY tests pass, including M5 contest and judge integration.
- Startup and export integrate with Foundations. Repository public (D61); release publication requires the owner's M7 decision.
- Round scorecards, unscored states, authoritative round tallies, and tally override reasons match shared contracts without changing participant win/loss records.
- Banners replay from state transitions and pick requests, use core-owned embedded licensed fonts and width fallbacks, and export as default-font fenced code blocks.
- The render loop exposes the post-render effects pass and effects manager hook; static rendering is complete and correct with effects off.
- The single event loop prioritizes input, bounds feed draining, and stops its frame timer when idle.
- Tournament rankings, eliminations, trophies, champion, and grill turns, passes, plan versions, final plan, and appendix match authoritative state.
- Question-first picks preserve ordinary room use, exact validation, restart restoration, and retained answers after cancellation.
- The daily spend bar stays visible, distinguishes actual, pending, and unknown cost, and presents daily-max refusal without canceling in-flight calls.
- Census, crosscheck, survey, and probe render their banners, steps, authoritative preflight, committed artifacts, skipped work, and inline collapsible reports at both viewports.
- Census estimate ranges, caveats, discovery curves, and quiet guidance; crosscheck evidence, audits, explicit risk acceptance, and bounded stop guidance; survey's five bands; and probe's decision tree and prediction matrix match accepted core results.
- All seven modes use the shared contest presentation and recovery path without client-side script execution or changes to existing mode behavior.
- New-mode inspection does not route material to members; restart and lag recovery preserve artifacts without duplicate effects; static layouts remain complete with effects off.
- The four new export scopes preserve complete core-generated reports and failure status, and report success only after destination writes.

## Phase M4.P1 — Performance spike, application shell, and input

- **M4.P1.L1 — Run the 12-stream throwaway spike**
  - [x] M4.P1.L1.T1 — Build a throwaway workload harness and renderer in `crates/squad-tui/benches/twelve_streams.rs` with effects off, 12 concurrent synthetic streams at 40 token deltas per second each, mixed prose and fenced code, and 10,000 preloaded messages; do not depend on the later transcript widget or effects implementation.
  - [x] M4.P1.L1.T2 — Measure a five-minute release-build run at both viewports with effects off, including typing, history scrolling, and resize, in `crates/squad-tui/benches/twelve_streams.rs`; require p95 draw duration at most 8 ms, p99 at most 16.7 ms, p95 key-to-visible-update latency at most 50 ms, and average process CPU at most 25% of one logical core.
  - [ ] M4.P1.L1.T3 — Record hardware, terminal, OS, dependency versions, workload, CPU measurement method, memory use, startup time, and results for macOS arm64 and Linux x64 in `crates/squad-tui/README.md`; block subsequent implementation until both platform runs pass. (open: README records Linux evidence; the required five-minute macOS arm64 run and memory/startup record remain pending)

- **M4.P1.L2 — Build the application loop and terminal lifecycle**
  - [x] M4.P1.L2.T1 — Declare Ratatui 0.30.x, crossterm, tokio, Markdown, highlighting, and Unicode dependencies in `crates/squad-tui/Cargo.toml`.
  - [x] M4.P1.L2.T2 — Implement `App` in `crates/squad-tui/src/app.rs` and the single `tokio::select!` loop in `crates/squad-tui/src/event_loop.rs`, prioritizing terminal input, draining at most 256 feed items per iteration, and arming the frame timer from `FxManager::next_wake()` and pending UI deadlines, capped at 60 Hz and disarmed when idle.
  - [x] M4.P1.L2.T3 — Implement pure render functions and both viewport geometries in `crates/squad-tui/src/ui.rs` and `crates/squad-tui/src/layout.rs`.
  - [x] M4.P1.L2.T4 — Implement terminal setup, capability queries, synchronized-output closure, and cleanup on exit or failure in `crates/squad-tui/src/terminal.rs`.
  - [x] M4.P1.L2.T5 — Implement missing-key, room-locked, and terminal-too-small presentation in `crates/squad-tui/src/screens.rs`.
  - [x] M4.P1.L2.T6 — Expose the post-render effects pass and effects manager hook in `crates/squad-tui/src/event_loop.rs` and `crates/squad-tui/src/ui.rs`, following TUI effects and motion while keeping static rendering complete and correct with effects off.

- **M4.P1.L3 — Spike and implement grapheme-aware input**
  - [x] M4.P1.L3.T1 — Compare a custom input prototype with compatible textarea candidates for soft wrap, Unicode cursor movement, selection, paste, and resize in `crates/squad-tui/examples/input_spike.rs`; record the evidence for writing Squad's widget in `crates/squad-tui/README.md`.
  - [x] M4.P1.L3.T2 — Implement grapheme segmentation, pinned cell widths, tab expansion, CRLF normalization, and source mappings in `crates/squad-tui/src/text.rs`.
  - [x] M4.P1.L3.T3 — Implement four-to-eight-row editing, cursor following, overflow labels, and draft-preserving input history in `crates/squad-tui/src/widgets/input.rs`.
  - [x] M4.P1.L3.T4 — Implement Kitty keyboard negotiation, fallback newline keys, bracketed paste, and duplicate-event prevention in `crates/squad-tui/src/input_events.rs`.
  - [x] M4.P1.L3.T5 — Verify pasted code preservation, UTF-8 byte validation, and grapheme-boundary editing in `crates/squad-tui/tests/input.rs`.

- **M4.P1.L4 — Integrate completion and shared validation**
  - [x] M4.P1.L4.T1 — Connect the shared parser, exact errors, and recipient preview in `crates/squad-tui/src/validation.rs`.
  - [x] M4.P1.L4.T2 — Implement six-row caret-aware mention, exclusion, variable, and command completion in `crates/squad-tui/src/widgets/completion.rs`.
  - [x] M4.P1.L4.T3 — Implement Ctrl+R recipient recall, pending variable display, and rejected-draft preservation in `crates/squad-tui/src/app.rs`.
  - [x] M4.P1.L4.T4 — Verify protected code, assignments, invalid exclusions, completion replacement, and recipient previews in `crates/squad-tui/tests/completion.rs`.

- **M4.P1.L5 — Integrate contest input and safe dismissal**
  - [x] M4.P1.L5.T1 — Add `$tournament`, `$grill`, and `GRILL_ROUNDS` completion and authoritative assignment display in `crates/squad-tui/src/widgets/completion.rs` and `crates/squad-tui/src/validation.rs`, consuming the shared parser's UTF-8 spans and Rust `Result`.
  - [x] M4.P1.L5.T2 — Implement eligible-first mention completion and pending-pick input placeholders in `crates/squad-tui/src/widgets/completion.rs` and `crates/squad-tui/src/widgets/input.rs`.
  - [x] M4.P1.L5.T3 — Implement transient-first Escape, the confirming second Escape within 2 s during a contest, input-preserving animation dispatch, and `q` shutdown outside input in `crates/squad-tui/src/input_events.rs` and `crates/squad-tui/src/app.rs`.
  - [x] M4.P1.L5.T4 — Verify contest completion, variable confirmation, ordinary input during picks, transient dismissal without stopping, stop-confirmation expiry, and shutdown keys in `crates/squad-tui/tests/input.rs` and `crates/squad-tui/tests/completion.rs`.

- **M4.P1.L6 — Integrate the four new mode commands**
  - [x] M4.P1.L6.T1 — Add `$census`, `$crosscheck`, `$survey`, `$probe`, `$CENSUS_PASSES`, and `$CENSUS_PASSES=` completion in `crates/squad-tui/src/widgets/completion.rs` using the shared parser's recognized forms.
  - [ ] M4.P1.L6.T2 — Present census pass assignment and lookup confirmation, verbatim new-mode validation errors, source previews, and rejected-draft and quote preservation in `crates/squad-tui/src/validation.rs` and `crates/squad-tui/src/app.rs`. (covered by crates/squad-tui/tests/m4a_audit.rs::census_pass_assignment_and_lookup_use_authoritative_parser and four_mode_completion_protected_text_and_rejected_drafts_use_the_parser; source previews in src/widgets/preflight.rs) (open: Foundations requires standalone bare $CENSUS_PASSES lookup, but the shared parser rejects input without recipients; inline lookup and assignment presentation are covered, and no local grammar was added)
  - [x] M4.P1.L6.T3 — Extend help and export command presentation from the authoritative parser and Foundations contracts in `crates/squad-tui/src/actions.rs`, without adding local grammar. (covered by crates/squad-tui/tests/contest_actions.rs::help_is_parser_owned_and_covers_every_new_verb_and_export and every_help_export_example_parses_to_an_export_scope)
  - [x] M4.P1.L6.T4 — Verify completion, pass-variable confirmation, protected text, exact parser-error display, and draft preservation for all four verbs in `crates/squad-tui/tests/completion.rs` and `crates/squad-tui/tests/input.rs`. (covered by crates/squad-tui/tests/m4a_audit.rs::four_mode_completion_protected_text_and_rejected_drafts_use_the_parser and census_pass_assignment_and_lookup_use_authoritative_parser)

## Phase M4.P2 — Transcript, Markdown, and room state

- **M4.P2.L1 — Integrate RoomHandle**
  - [x] M4.P2.L1.T1 — Implement subscription, state initialization, request acknowledgements, submission gating, and shutdown integration in `crates/squad-tui/src/session.rs`.
  - [x] M4.P2.L1.T2 — Implement durable paging, canonical identity reconciliation, feed-lag recovery, and pending-request preservation in `crates/squad-tui/src/session.rs`.
  - [x] M4.P2.L1.T3 — Verify initialization races, repeated events, invalid-cursor presentation, and authoritative room reset in `crates/squad-tui/tests/session.rs`.

- **M4.P2.L2 — Implement virtualised history and stable streaming**
  - [x] M4.P2.L2.T1 — Implement visible-row rendering, bounded overscan, wrapped-height caches, and cumulative height lookup in `crates/squad-tui/src/widgets/transcript.rs`.
  - [x] M4.P2.L2.T2 — Implement stable reply cards, attempt replacement, 50 ms token batching, and immediate terminal outcomes in `crates/squad-tui/src/transcript.rs`.
  - [x] M4.P2.L2.T3 — Implement anchored paging, mouse wheel scrolling, suspended live following, and new-activity counts in `crates/squad-tui/src/transcript.rs`. (covered by crates/squad-tui/tests/m4_paging.rs: page, wheel, live-follow and new-activity regressions)
  - [x] M4.P2.L2.T4 — Implement coalesced resize reflow and source-grapheme anchor preservation in `crates/squad-tui/src/layout.rs`.
  - [x] M4.P2.L2.T5 — Verify offscreen streams do not move the viewport or cause full-history drawing in `crates/squad-tui/tests/transcript.rs`.
  - [x] M4.P2.L2.T6 — Include banner heights, font fallbacks, banner mode, and scorecard expansion in presentation caches and preserve anchors during late score arrival and banner reflow in `crates/squad-tui/src/widgets/transcript.rs` and `crates/squad-tui/src/transcript.rs`.

- **M4.P2.L3 — Implement incremental Markdown**
  - [x] M4.P2.L3.T1 — Implement `pulldown-cmark` block parsing, finalized-block caching, and revisable live-tail boundaries in `crates/squad-tui/src/markdown.rs`.
  - [x] M4.P2.L3.T2 — Implement `syntect` fenced-code highlighting and plain-code fallback outside the drawing path in `crates/squad-tui/src/highlight.rs`.
  - [x] M4.P2.L3.T3 — Implement terminal-control sanitization for message bodies and metadata in `crates/squad-tui/src/sanitize.rs`.
  - [x] M4.P2.L3.T4 — Verify table and fence transitions, cache reuse, width invalidation, and attempt replacement in `crates/squad-tui/tests/markdown.rs`.

- **M4.P2.L4 — Implement search, selection, and copy**
  - [x] M4.P2.L4.T1 — Implement transcript search, incremental history coverage, match navigation, and collapsed-content reveal in `crates/squad-tui/src/search.rs`.
  - [x] M4.P2.L4.T2 — Implement focused-message navigation, distinct action eligibility, and durable selection state in `crates/squad-tui/src/selection.rs`.
  - [x] M4.P2.L4.T3 — Implement user-initiated OSC 52 message and code-block copy with unavailable-copy feedback in `crates/squad-tui/src/clipboard.rs`.
  - [x] M4.P2.L4.T4 — Verify search offsets, selection persistence, copy sanitization, and keyboard action access in `crates/squad-tui/tests/navigation.rs`.

- **M4.P2.L5 — Display participants, accounting, and color**
  - [x] M4.P2.L5.T1 — Render nick-adjacent versions, complete model detail, presence, queues, contest participation, usage, cost, win-loss records, and tournament trophies in `crates/squad-tui/src/widgets/participants.rs`.
  - [x] M4.P2.L5.T2 — Render today's spending, pending reservations, daily max, remaining daily amount, and unavailable values in `crates/squad-tui/src/widgets/budget.rs`.
  - [x] M4.P2.L5.T3 — Implement the effects-owned palette's stable author colors, specified 16-colour mapping, default-foreground body text, gold chrome, and Foundations no-color behavior in `crates/squad-tui/src/theme.rs`.
  - [x] M4.P2.L5.T4 — Verify 12-member navigation, long nicks, persona-based duplicate models, human naming, and repeated accounting events in `crates/squad-tui/tests/participants.rs`.

- **M4.P2.L6 — Display authoritative daily spend**
  - [x] M4.P2.L6.T1 — Add `DailySpendView` and consume authoritative daily spend, reservations, unknown-call count, effective max, and local-day changes in `crates/squad-tui/src/app.rs` and `crates/squad-tui/src/session.rs` without computing totals from transcript events.
  - [x] M4.P2.L6.T2 — Implement the always-visible spend bar, lighter pending segment, `~` unknown-cost prefix, explicit amounts, footer-to-status placement, and undersized-terminal display in `crates/squad-tui/src/widgets/budget.rs`, `crates/squad-tui/src/layout.rs`, and `crates/squad-tui/src/screens.rs`.
  - [x] M4.P2.L6.T3 — Present `DAILY_MAX_REACHED`, the exact effective-max message, refused preflight, continued in-flight work, and 75% and 90% heat hooks in `crates/squad-tui/src/widgets/budget.rs` and `crates/squad-tui/src/widgets/preflight.rs`.
  - [x] M4.P2.L6.T4 — Verify actual-cost replacement, billed failures and cancellations, unknown costs, duplicate events, day rollover, a raised max, limit boundaries, no-color labels, and persistent visibility in `crates/squad-tui/tests/daily_spend.rs`.
  - [x] M4.P2.L6.T5 — Verify authoritative room clearing preserves billed daily totals and that judge usage remains distinct from member `fable` in `crates/squad-tui/tests/daily_spend.rs`.

- **M4.P2.L7 — Restore contest views and tag feed origin**
  - [x] M4.P2.L7.T1 — Add `ContestView`, `PickView`, and the TUI-internal `FeedOrigin` tag in `crates/squad-tui/src/app.rs` and `crates/squad-tui/src/session.rs`, restoring static contest presentation without changing protocol feed items.
  - [x] M4.P2.L7.T2 — Extend presentation caches for tournament rankings, grill plans and appendix, pick candidates, and contest expansion in `crates/squad-tui/src/widgets/transcript.rs`. (covered by crates/squad-tui/tests/m4_expansion.rs and src/composed.rs: ranking/plan/appendix expansion and pick_revision cache keys)
  - [x] M4.P2.L7.T3 — Verify restart and lag recovery restore active contests, pending picks without expiry, trophies, elimination markers, and plan versions without duplicate effects or lost anchors in `crates/squad-tui/tests/session.rs`.

- **M4.P2.L8 — Restore and virtualise the four new reports**
  - [x] M4.P2.L8.T1 — Extend `ContestView` and authoritative session reconciliation for new-mode setup, state, committed artifacts, reports, skipped members, and terminal states in `crates/squad-tui/src/app.rs` and `crates/squad-tui/src/session.rs`.
  - [x] M4.P2.L8.T2 — Restore census pass and plan references, crosscheck audit and acceptance records, survey vote stages, and probe role split and predictions from shared state in `crates/squad-tui/src/session.rs`, without rerunning script moves or random selections. (covered by crates/squad-tui/src/session.rs::restore and typed Census/Crosscheck/Survey/Probe reducers; crates/squad-tui/tests/probe.rs::only_committed_live_reports_emit_tree_cues_and_replay_is_quiet and tests/survey.rs::reducer_emits_only_live_committed_cues_and_replay_is_static)
  - [x] M4.P2.L8.T3 — Add report, row, band, matrix, and branch expansion to presentation caches and anchored reflow in `crates/squad-tui/src/widgets/transcript.rs` and `crates/squad-tui/src/transcript.rs`. (covered by crates/squad-tui/src/composed.rs report_revision/expanded keys and invalidate_artifacts; crates/squad-tui/tests/m4a_audit.rs::census_report_search_reveals_copy_and_quote_without_delivering and tests/probe.rs::matrix_expands_without_hiding_tree_or_skipped_evidence)
  - [x] M4.P2.L8.T4 — Integrate report search reveal, source copying, keyboard detail actions, and draft-preserving quote preparation in `crates/squad-tui/src/search.rs`, `crates/squad-tui/src/selection.rs`, and `crates/squad-tui/src/clipboard.rs`. (covered by crates/squad-tui/tests/m4a_audit.rs::census_report_search_reveals_copy_and_quote_without_delivering; src/app.rs and src/session.rs share core-rendered report sources for search, copy and quotes)
  - [x] M4.P2.L8.T5 — Show new-mode participant work status and probe sit-out without score or trophy changes in `crates/squad-tui/src/widgets/participants.rs`.

## Phase M4.P3 — Debates, cases, and recovery actions

- **M4.P3.L1 — Render and control inline debates**
  - [x] M4.P3.L1.T1 — Implement collapsible debate blocks, round 0, rounds, parallel turns, retries, forfeits, closing statements, and waiting states in `crates/squad-tui/src/widgets/debate.rs`.
  - [x] M4.P3.L1.T2 — Implement authoritative Protocol-owned preflight estimate display, model-call count, debaters, round count, judge and opening-round flags, auto-confirmation status, and explicit Confirm and Cancel controls bound to plan identity in `crates/squad-tui/src/widgets/preflight.rs`. (covered by crates/squad-tui/tests/preflight.rs::controls_bind_to_plan_identity_and_pending_state_blocks_them and debate_shows_core_count_with_round_scoring_per_configured_round; source/control visibility in tests/m4a_audit.rs)
  - [x] M4.P3.L1.T3 — Implement solution previews, distinct debate selection, and explicit solution references in `crates/squad-tui/src/selection.rs`. (covered by crates/squad-tui/tests/selection.rs::sources::debate_solutions_need_two_distinct_authors_and_use_reply_to_only; committed event IDs are carried separately from message IDs)
  - [x] M4.P3.L1.T4 — Connect global stop, targeted stop, acknowledged cancellation, and eligible later judging in `crates/squad-tui/src/session.rs`.
  - [x] M4.P3.L1.T5 — Consume `debate.round_score` by debate, case, and round identity and render pending, scored, unscored, and canceled scoring states, expandable reasons, and authoritative tallies in `crates/squad-tui/src/session.rs` and `crates/squad-tui/src/widgets/debate.rs`.
  - [x] M4.P3.L1.T6 — Present late scores without blocking later turns, outstanding scoring before final judging, no scoring for round 0 or unjudged debates, and no retroactive scoring when judging a stopped debate in `crates/squad-tui/src/widgets/debate.rs`.
  - [x] M4.P3.L1.T7 — Verify the displayed authoritative preflight includes one round-scoring judge call per configured round when judging is requested in `crates/squad-tui/src/widgets/preflight.rs` and `crates/squad-tui/tests/debate.rs`. (covered by crates/squad-tui/tests/preflight.rs::debate_shows_core_count_with_round_scoring_per_configured_round and tests/m4a_conversation_audit.rs::question_first_chat_pick_preflight_debate_verdict_and_restart)
  - [x] M4.P3.L1.T8 — Present judge-only preflight, prior-status restoration after cancellation or expiry, and multiple associated cases using shared contest transitions in `crates/squad-tui/src/widgets/debate.rs` and `crates/squad-tui/tests/debate.rs`. (covered by crates/squad-tui/tests/preflight.rs::judge_only_preflight_has_no_round_scoring and cancel_and_expiry_restore_prior_status_but_confirmation_does_not; tests/widget_debate.rs::stopped_judging_does_not_restart_canceled_scoring and session.rs case_ids restoration)

- **M4.P3.L2 — Render verdicts and judge cases**
  - [x] M4.P3.L2.T1 — Render actual judge identity, the exact substitution warning, accepted scores, weighted totals, winner, reasoning, and loser takeaways in `crates/squad-tui/src/widgets/verdict.rs`.
  - [x] M4.P3.L2.T2 — Implement quote previews, removal, attachment count, limits, and shared quote submission in `crates/squad-tui/src/quotes.rs`.
  - [x] M4.P3.L2.T3 — Render standalone free-form rulings, case identity, and verdict-linked follow-ups in `crates/squad-tui/src/widgets/case.rs`.
  - [x] M4.P3.L2.T4 — Connect shared judging requests and verbatim judge-validation errors in `crates/squad-tui/src/session.rs`. (covered by crates/squad-tui/tests/contest_actions.rs::judge_without_quote_shows_the_parser_string_verbatim; src/validation.rs::request uses the shared mapper and src/app.rs displays parser/core rejections unchanged)
  - [x] M4.P3.L2.T5 — Render accepted `roundTally`, non-null `tallyOverrideReason`, and available per-round scores with unscored markers in `crates/squad-tui/src/widgets/verdict.rs`, preserving core validation and verdict-only participant win/loss updates.
  - [ ] M4.P3.L2.T6 — Verify concrete-model conflict warnings substitute the reported nick, standalone judging follows the shared conflict contract, and tournaments never display a substitution warning in `crates/squad-tui/tests/cases.rs`. (covered by crates/squad-tui/tests/widget_verdict.rs::protocol_arithmetic_and_exact_conflict_warning, src/widgets/case.rs verbatim warning, and tests/contest_actions.rs::tournament_never_shows_a_substitution_warning) (open: core router/cases.rs::standalone binds the judge without publishing a conflict notice; helper formatting and tournament suppression are covered, but standalone conflict presentation cannot be verified from shared events)

- **M4.P3.L3 — Integrate recovery and Foundations actions**
  - [x] M4.P3.L3.T1 — Connect ordinary-reply retry actions and regeneration without restarting successful siblings in `crates/squad-tui/src/session.rs`.
  - [x] M4.P3.L3.T2 — Render incomplete output, superseded replies, replacement work, and authoritative clear results in `crates/squad-tui/src/transcript.rs`. (covered by crates/squad-tui/tests/m4_incomplete.rs and tests/session_shell.rs::authoritative_clear_preserves_daily_spend_and_reset_removes_contest)
  - [x] M4.P3.L3.T3 — Connect Foundations command presentation, help, roster detail, and export destination handling in `crates/squad-tui/src/actions.rs`. (covered by crates/squad-tui/src/actions.rs and tests/contest_actions.rs help/export tests; tests/actions_shell.rs::file_export_writes_core_text_and_preserves_existing_destination and tests/ui_composed.rs::startup_identities_and_focused_details_are_available_before_replies)
  - [x] M4.P3.L3.T4 — Implement the F12 debug pane using shared redacted diagnostics in `crates/squad-tui/src/widgets/debug.rs`. (covered by crates/squad-tui/tests/m4a_audit.rs::expanded_input_and_f12_redacted_diagnostics_snapshot_at_both_viewports; src/ui.rs invokes widgets/debug.rs redacted counters)
  - [x] M4.P3.L3.T5 — Obtain shared Markdown through `RoomHandle::export(scope, id) -> String`, write it to the selected destination, and report success only after writing in `crates/squad-tui/src/actions.rs`, preserving core-rendered scores, reasons, unscored markers, tallies, override reasons, and default-font fenced banners.

- **M4.P3.L4 — Render ASCII round banners**
  - [x] M4.P3.L4.T1 — Integrate the M3 core-owned in-process banner renderer and embedded fonts in `crates/squad-tui/src/widgets/banner.rs` without adding a TUI font copy or moving banner generation out of `squad-core`.
  - [x] M4.P3.L4.T2 — Derive `OPENING`, `ROUND n`, `FINAL ROUND`, and `VERDICT` boundaries from live and replayed `debate.state` transitions in `crates/squad-tui/src/widgets/debate.rs` without creating banner events, placing `VERDICT` on entering `JUDGING`.
  - [x] M4.P3.L4.T3 — Implement centred presentation of core-rendered default-font, compact-font, and one-line banner fallbacks, theme accent, plain `NO_COLOR` output, and the Foundations `SQUAD_BANNERS` modes in `crates/squad-tui/src/widgets/banner.rs`.
  - [x] M4.P3.L4.T4 — Consume core-owned default-font export banners through the shared export operation in `crates/squad-tui/src/actions.rs`, independent of terminal width and banner mode and without external commands.

- **M4.P3.L5 — Render tournaments**
  - [x] M4.P3.L5.T1 — Implement inline collapsible tournament blocks, authoritative states, stable parallel entry cards, retry and forfeit labels, and inspectable previous rounds in `crates/squad-tui/src/widgets/tournament.rs`.
  - [x] M4.P3.L5.T2 — Consume tournament setup, state, entry, ranking, result, and champion events by canonical identity in `crates/squad-tui/src/session.rs`, rendering accepted ranks, scores, critiques, final placements, replay, and failure without local ranking decisions.
  - [x] M4.P3.L5.T3 — Render eliminated participants and authoritative trophy counts in `crates/squad-tui/src/widgets/participants.rs`, deduplicating updates and retaining elimination marks for the rest of the tournament.
  - [x] M4.P3.L5.T4 — Present the core's k + (k−1) + ... + 2 answer calls, k−1 judge calls, total estimate, remaining daily amount, and confirmation controls in `crates/squad-tui/src/widgets/preflight.rs`. (covered by crates/squad-tui/tests/preflight.rs::tournament_shows_k_plus_down_to_two_answers_and_k_minus_one_judges and controls_bind_to_plan_identity_and_pending_state_blocks_them)
  - [x] M4.P3.L5.T5 — Add `TOURNAMENT`, `ROUND r · N REMAIN`, `FINAL`, and `CHAMPION` boundaries and bottom-up ranking and champion effect hooks in `crates/squad-tui/src/widgets/tournament.rs` and `crates/squad-tui/src/widgets/banner.rs`, retaining complete static views with effects off.
  - [x] M4.P3.L5.T6 — Connect `/export tournament [id]` to shared core export and destination writes in `crates/squad-tui/src/actions.rs`.

- **M4.P3.L6 — Render grills**
  - [x] M4.P3.L6.T1 — Implement eligible leader-plan preview and explicit source selection using the shared selected-or-quoted-message contract in `crates/squad-tui/src/selection.rs`. (covered by crates/squad-tui/tests/selection.rs::sources::grill_plan_source_is_exactly_one_selected_or_quoted_message; metadata uses the committed source event ID)
  - [x] M4.P3.L6.T2 — Implement inline grill blocks, shared-floor participant scope, authoritative states, sequential turn headings and angles, questions, answers, `Plan changes`, passes, skipped turns, and counters in `crates/squad-tui/src/widgets/grill.rs`.
  - [x] M4.P3.L6.T3 — Consume grill setup, state, question, answer, pass, plan, and completion events in `crates/squad-tui/src/session.rs`, preserving all plan versions and the last good plan after leader failure.
  - [x] M4.P3.L6.T4 — Render collapsible plan versions with the newest expanded, final plan, question-and-answer appendix, and `survived` completion in `crates/squad-tui/src/widgets/grill.rs`, without judge, winner, or scoreboard updates.
  - [x] M4.P3.L6.T5 — Present the authoritative rounds × (grillers × 2 + 1) model-call count, estimate, participants, and remaining daily amount in `crates/squad-tui/src/widgets/preflight.rs`. (covered by crates/squad-tui/tests/preflight.rs::grill_shows_rounds_times_two_grillers_plus_one and tests/grill_runtime.rs::real_runtime_preflight_actions_committed_snapshots_and_export)
  - [x] M4.P3.L6.T6 — Add `GRILL`, `GRILL ROUND r`, and `FINAL PLAN` boundaries and turn-handoff, plan-version, and survived effect hooks in `crates/squad-tui/src/widgets/grill.rs` and `crates/squad-tui/src/widgets/banner.rs`.
  - [x] M4.P3.L6.T7 — Connect `/export grill [id]` and its plan versions and complete appendix to shared core export and destination writes in `crates/squad-tui/src/actions.rs`.

- **M4.P3.L7 — Render question-first collection and picks**
  - [x] M4.P3.L7.T1 — Consume `contest.collecting`, `contest.pick_requested`, and `contest.picked`, preserving ordinary answer cards and authoritative candidate eligibility in `crates/squad-tui/src/session.rs`.
  - [x] M4.P3.L7.T2 — Render inline `CHOOSE YOUR DEBATE BROS` and `CHOOSE WHO GETS GRILLED` banners, exact hints, candidate lists, and `pick pending` status in `crates/squad-tui/src/widgets/pick.rs` and `crates/squad-tui/src/widgets/banner.rs`.
  - [x] M4.P3.L7.T3 — Route accepted mentions-only picks through `ContestPick { contestId, nicks }`, preserve unrelated ordinary messages, and display exact wrong-pick and insufficient-answer strings in `crates/squad-tui/src/validation.rs` and `crates/squad-tui/src/session.rs`. (covered by crates/squad-tui/tests/contest_actions.rs::mentions_only_pick_routes_to_contest_pick_and_ordinary_text_is_preserved, wrong_pick_and_ineligible_pick_use_the_exact_parser_strings and insufficient_answers_text_matches_the_prd_string)
  - [x] M4.P3.L7.T4 — Present post-pick preflight with selected debate solutions or grill leadership, default judging for question-first debates, and exclusion of already billed collection answers from the estimate in `crates/squad-tui/src/widgets/preflight.rs`.
  - [x] M4.P3.L7.T5 — Restore pending picks without expiry and reflect acknowledged `pick_cancelled` while retaining ordinary answers in `crates/squad-tui/src/session.rs` and `crates/squad-tui/src/widgets/pick.rs`.
  - [x] M4.P3.L7.T6 — Expose eligible-candidate row and banner-entrance effect hooks in `crates/squad-tui/src/widgets/participants.rs` and `crates/squad-tui/src/widgets/pick.rs`, keeping text eligibility complete with effects off. (covered by crates/squad-tui/tests/contest_actions.rs::pick_hooks_expose_eligible_rows_and_text_is_complete_without_effects and tests/fx_pick.rs)

- **M4.P3.L8 — Present shared engine progress for the new modes**
  - [x] M4.P3.L8.T1 — Extend shared contest presentation for script step labels, stable parallel work cards, accepted batches, application-only tally progress, and committed reports in `crates/squad-tui/src/session.rs` and `crates/squad-tui/src/app.rs`, without executing engine moves in the client. (covered by crates/squad-tui/src/composed.rs shared step/work progress rows and tests/m4a_audit.rs::source_references_and_shared_script_progress_reach_the_ui; committed cards in tests/contest_actions.rs::session_tracks_steps_and_cards_per_contest)
  - [x] M4.P3.L8.T2 — Present authoritative source references, participants, call counts, estimates, reservations, remaining daily amount, confirmation status, and crosscheck's optional extraction call in `crates/squad-tui/src/widgets/preflight.rs`. (covered by crates/squad-tui/tests/preflight.rs census/crosscheck/survey/probe cases and tests/m4a_audit.rs::source_references_and_shared_script_progress_reach_the_ui; src/widgets/preflight.rs::source_preview consumes committed setups)
  - [x] M4.P3.L8.T3 — Integrate `CENSUS`, `CROSSCHECK`, `SURVEY`, and `PROBE` banners and script step dividers with core-owned fonts, width fallbacks, replay deduplication, and banners-off labels in `crates/squad-tui/src/widgets/banner.rs`.
  - [x] M4.P3.L8.T4 — Present new-mode visibility labels, retries, skipped-member details, acknowledged stop, failed state, and incomplete reports through shared contest controls in `crates/squad-tui/src/session.rs`. (covered by crates/squad-tui/src/composed.rs shared report_header/visibility/work rows, tests/contest_actions.rs::stopped_and_failed_runs_are_incomplete_and_name_skipped_members and tests/m4a_audit.rs::source_references_and_shared_script_progress_reach_the_ui)
  - [x] M4.P3.L8.T5 — Connect the four new report export scopes to shared core Markdown and destination writes in `crates/squad-tui/src/actions.rs`. (covered by crates/squad-tui/tests/contest_actions.rs::export_scopes_name_files_and_new_reports_are_flagged and tests/actions_shell.rs::export_success_requires_destination_write_and_flush)
  - [x] M4.P3.L8.T6 — Integrate fixtures in D71 order—debate, grill, census, crosscheck, tournament, survey, probe—while preserving existing task IDs and the existing modes' presentation behavior in `crates/squad-tui/tests/conversation.rs`. (covered by crates/squad-tui/tests/m4a_conversation_audit.rs::d71_mode_fixtures_restore_preflight_and_cancel_in_order; complete mode results remain covered by census/crosscheck/tournament/survey/probe widget suites and grill_runtime.rs)

- **M4.P3.L9 — Render census**
  - [x] M4.P3.L9.T1 — Implement plan-source preview, participating members, pass count, per-pass plan references, assigned lenses, and hunt, merge, rate, and finalise step presentation in `crates/squad-tui/src/widgets/census.rs`.
  - [x] M4.P3.L9.T2 — Render ranked ledger rows, singleton labels, consequences, lenses, finder counts, ratings, additions, and covered citations with a human-only member-by-gap drilldown in `crates/squad-tui/src/widgets/census.rs`.
  - [x] M4.P3.L9.T3 — Render the exact authoritative estimate-range line, adjacent shared-blind-spots caveat, discovery curve, and per-pass plan versions in `crates/squad-tui/src/widgets/census.rs`, without calculating bounds or finder counts locally.
  - [x] M4.P3.L9.T4 — Present between-pass continuation, patched-plan quote preparation, the singleton-to-grill hint, and core-supplied quiet guidance separately from pass-limit completion in `crates/squad-tui/src/widgets/census.rs` and `crates/squad-tui/src/actions.rs`.
  - [x] M4.P3.L9.T5 — Expose census banner and estimate count-down hooks for M4.P5 in `crates/squad-tui/src/widgets/census.rs`, retaining the complete static ledger and final estimate with effects off.

- **M4.P3.L10 — Render crosscheck**
  - [x] M4.P3.L10.T1 — Implement plan, goal, and constraint preview, optional extraction progress, plan-hidden inventory labels, lens assignments, normalization, and grid-building presentation in `crates/squad-tui/src/widgets/crosscheck.rs`.
  - [x] M4.P3.L10.T2 — Render the coverage grid summary, stable cell IDs, actual retained count, cap and trimming disclosure, and expandable single-item or cross-lens-pair records in `crates/squad-tui/src/widgets/crosscheck.rs`.
  - [x] M4.P3.L10.T3 — Render cited coverage, justified exclusions, open gaps and proposed fixes, pending or skipped reviews, and cell history without treating probable coverage as closure in `crates/squad-tui/src/widgets/crosscheck.rs`.
  - [x] M4.P3.L10.T4 — Render fresh-context auditor details, sampled evidence, challenges, proposed missing cases, and accepted rulings without dropping cells in `crates/squad-tui/src/widgets/crosscheck.rs`.
  - [x] M4.P3.L10.T5 — Connect eligible explicit risk-acceptance actions through the shared core contract, wait for acknowledgement, and show bounded stop guidance and unresolved audit findings in `crates/squad-tui/src/actions.rs` and `crates/squad-tui/src/widgets/crosscheck.rs`.
  - [x] M4.P3.L10.T6 — Expose crosscheck banner and cell-filling hooks for M4.P5 in `crates/squad-tui/src/widgets/crosscheck.rs`, retaining textual statuses and complete cell detail with effects off.

- **M4.P3.L11 — Render survey**
  - [x] M4.P3.L11.T1 — Implement new-question and existing-answer previews, optional collection, neutral claim extraction, and stable claim identity across steps in `crates/squad-tui/src/widgets/survey.rs`.
  - [x] M4.P3.L11.T2 — Render blind-vote and re-vote progress, AGREE / DISAGREE / UNSURE, reasons, confidence, selected split claims, and evidence that would change a member's mind in `crates/squad-tui/src/widgets/survey.rs`.
  - [x] M4.P3.L11.T3 — Render the five-band map, empty-band labels, split reasons, switchers, origins, settling questions, and labeled vote bars with first-vote/re-vote drilldowns in `crates/squad-tui/src/widgets/survey.rs`, using core classifications and preserving missing votes separately.
  - [x] M4.P3.L11.T4 — Expose survey banner and bar-splitting hooks for M4.P5 in `crates/squad-tui/src/widgets/survey.rs`, retaining all bands, counts, reasons, and no-winner presentation with effects off.

- **M4.P3.L12 — Render probe**
  - [x] M4.P3.L12.T1 — Implement brief preview, authoritative random role split, odd-member sit-out notice, and separate blind explanation and check groups in `crates/squad-tui/src/widgets/probe.rs`.
  - [x] M4.P3.L12.T2 — Render explanation assumptions, exact check measurements and outcomes, prediction-locking progress, accepted predictions, cannot-predict entries, and skipped work in `crates/squad-tui/src/widgets/probe.rs`.
  - [x] M4.P3.L12.T3 — Render the accepted decision tree with box characters, wrapped branch conditions, remaining explanations, next checks, and a visible unexpected-result branch in `crates/squad-tui/src/widgets/probe.rs`.
  - [x] M4.P3.L12.T4 — Render the expandable prediction matrix, dropped-check reasons, and inseparable explanations without declaring one explanation true or executing checks in `crates/squad-tui/src/widgets/probe.rs`.
  - [x] M4.P3.L12.T5 — Expose probe banner and branch-drawing hooks for M4.P5 in `crates/squad-tui/src/widgets/probe.rs`, retaining the full static tree and matrix with effects off.

## Phase M4.P4 — Verification

- **M4.P4.L1 — Verify deterministic rendering and interaction**
  - [x] M4.P4.L1.T1 — Add `TestBackend` and `insta` snapshots for both viewports, expanded input, startup screens, participants, completion, debates, verdicts, and no-color output in `crates/squad-tui/tests/snapshots.rs`. (covered by crates/squad-tui/tests/snapshots.rs, screens.rs, participants.rs, ui_composed.rs and m4a_audit.rs::expanded_input_and_f12_redacted_diagnostics_snapshot_at_both_viewports)
  - [x] M4.P4.L1.T2 — Verify Unicode widths, cursor mappings, resize anchors, hostile escapes, input history, search, and focus in `crates/squad-tui/tests/interaction.rs`.
  - [x] M4.P4.L1.T3 — Verify drawing uses cached finalized Markdown and visible transcript rows in `crates/squad-tui/tests/render_budget.rs`.
  - [x] M4.P4.L1.T4 — Add `insta` snapshots for every banner at 120, 80, and 40 available columns and with `NO_COLOR`; cover default and compact fonts, one-line fallbacks, banner modes, replay deduplication, and height-cache reflow in `crates/squad-tui/tests/banners.rs`. (covered by crates/squad-tui/tests/banners.rs::every_banner_snapshots_at_120_80_and_40_columns_in_every_mode_without_colour, four_new_banners_and_step_dividers_snapshot_with_default_and_compact_fonts and replay_hits_the_cache_and_resize_reflows_heights)
  - [x] M4.P4.L1.T5 — Verify complete static layouts with effects off and integration of the post-render effects manager hook in `crates/squad-tui/tests/render_budget.rs`, leaving individual effects tests to TUI effects and motion. (covered by crates/squad-tui/tests/render_budget.rs::effects_off_layout_is_static_and_the_post_hook_is_inert and full_effects_run_through_the_post_render_hook_and_settle_to_the_static_layout)

- **M4.P4.L2 — Verify integrated conversation flows**
  - [x] M4.P4.L2.T1 — Run the canonical question-first chat-to-debate flow against the M2 fake provider and temporary SQLite data in `crates/squad-tui/tests/conversation.rs`. (covered by crates/squad-tui/tests/m4a_conversation_audit.rs::question_first_chat_pick_preflight_debate_verdict_and_restart using M2 FakeOpenRouter and a temporary SQLite data directory)
  - [x] M4.P4.L2.T2 — Verify threshold boundaries, auto-confirmation, cancellation, plan expiry, queued debaters, round 0, and manual judging in `crates/squad-tui/tests/debate.rs`. (covered by crates/squad-tui/tests/m4b_debate_audit.rs and preflight.rs; core tests/orchestrator/admission.rs verifies waiting debaters)
  - [x] M4.P4.L2.T3 — Verify quotes, standalone cases, isolated follow-ups, malformed verdict presentation, retries, and supersession in `crates/squad-tui/tests/cases.rs`. (covered by crates/squad-tui/tests/widget_case.rs, actions_shell.rs, m4_reply_keys.rs and m4_incomplete.rs; core tests/router_debates.rs::case_followup_keeps_the_original_decision_sealed)
  - [x] M4.P4.L2.T4 — Verify state reconciliation, confirmed room clearing, scoreboard deduplication, daily-max refusal with in-flight completion, and unavailable costs in `crates/squad-tui/tests/recovery.rs`. (covered by crates/squad-tui/tests/m4b_recovery_audit.rs, session_shell.rs, event_loop_shell.rs::feed_lag_recovers_authoritative_state_without_losing_draft and budget.rs; core tests/agents_integration.rs::actual_inflight_stream_finishes_after_other_call_reaches_daily_max)
  - [x] M4.P4.L2.T5 — Verify room and debate export presentation, successful-write reporting, and credential exclusion in `crates/squad-tui/tests/export.rs`. (covered by crates/squad-tui/tests/m4b_export_audit.rs::all_scopes_preserve_core_markdown_and_report_success_only_after_flush and actions_shell.rs)
  - [x] M4.P4.L2.T6 — Verify pending, late, repeated, equal, failed, and canceled round-score presentation, expandable reasons, authoritative tallies, anchored updates, and unchanged participant win/loss records in `crates/squad-tui/tests/debate.rs`. (covered by crates/squad-tui/tests/m4b_debate_audit.rs::equal_and_late_scores_preserve_authoritative_tally_records_and_anchor and widget_debate.rs)
  - [x] M4.P4.L2.T7 — Verify no scorecards for round 0 or unjudged debates, waiting for outstanding scores before final judging, stopped-debate judging without retroactive scoring, and accepted round tally and override-reason presentation in `crates/squad-tui/tests/debate.rs`. (covered by crates/squad-tui/tests/widget_debate.rs and widget_verdict.rs; core tests/orchestrator/judge.rs::final_waits_for_all_scores_and_does_not_allocate_twice)
  - [x] M4.P4.L2.T8 — Verify exported round scores, reasons, unscored markers, authoritative tallies, override reasons, and default-font fenced banners regardless of terminal width or banner mode in `crates/squad-tui/tests/export.rs`. (covered by crates/squad-core/tests/export.rs::debate_snapshot_and_late_round_identity and crates/squad-tui/tests/m4b_export_audit.rs)

- **M4.P4.L3 — Verify the real terminal lifecycle**
  - [x] M4.P4.L3.T1 — Add one TUI PTY-driven end-to-end test covering input, newline fallbacks, bracketed paste, streaming, resize, search, copy initiation, preflight, and terminal restoration in `crates/squad-tui/tests/pty_e2e.rs`. (covered by crates/squad-tui/tests/m4b_pty_e2e_audit.rs::fake_room_terminal_input_and_restoration and m4b_pty_support/drive.py)
  - [ ] M4.P4.L3.T2 — Record direct terminal checks for synchronized output, Kitty keyboard enhancement, mode 2027, OSC 52, tmux, SSH, and both viewports in `crates/squad-tui/README.md`. (open: dev3 PTY checks do not establish direct Kitty, mode 2027, OSC 52, tmux and SSH behavior; physical-terminal matrix remains unavailable)
  - [x] M4.P4.L3.T3 — Verify Foundations startup and shutdown integration from the built binary and document `./setup.sh` entry guidance in `crates/squad-tui/tests/startup.rs` and `crates/squad-tui/README.md`. (covered by crates/squad-cli/tests/pty.rs startup, streaming, signal and restoration tests; setup guidance in crates/squad-tui/README.md)
  - [x] M4.P4.L3.T4 — Record passing workspace checks and M5 integration results before closing M4 in `crates/squad-tui/README.md`. (workspace gates and offline M5 fixture results recorded in crates/squad-tui/README.md; cross-platform exit gates remain explicitly open)

- **M4.P4.L4 — Verify tournament, grill, and pick presentation**
  - [x] M4.P4.L4.T1 — Snapshot tournament parallel entries, strict accepted rankings, scores, critiques, eliminated rows, trophies, champion, round replay, stopped state, and failed judging at both viewports in `crates/squad-tui/tests/tournament.rs`.
  - [x] M4.P4.L4.T2 — Verify tournament preflight call counts, k−1 round presentation, forfeit labels, unchanged blind judge identity, deduplicated trophies, and complete static ranking without effects in `crates/squad-tui/tests/tournament.rs`.
  - [x] M4.P4.L4.T3 — Snapshot grill participant scope, rotated sequential turns, headings, primary angles, questions, answers, plan changes, passes, counters, skipped turns, plan versions, appendix, survived completion, and leader failure in `crates/squad-tui/tests/grill.rs`.
  - [x] M4.P4.L4.T4 — Verify grill call-count presentation, explicit plan sources, newest-plan expansion, last-good-plan retention, and absence of judge or scoreboard changes in `crates/squad-tui/tests/grill.rs`. (covered by crates/squad-tui/tests/grill_runtime.rs, widget_grill.rs and m4_expansion.rs::grill_plans_and_appendix_expand_through_the_presentation_cache)
  - [x] M4.P4.L4.T5 — Verify question-first debate and grill collection, eligible candidates, exact banners and hints, placeholders, `pick pending`, valid picks, wrong-pick strings, and insufficient-answer failure in `crates/squad-tui/tests/pick.rs`. (covered by crates/squad-tui/tests/widget_pick.rs snapshots and exact feedback, contest_actions.rs and validation_shell.rs)
  - [x] M4.P4.L4.T6 — Verify ordinary messages remain usable during a pending pick, eligible-first completion, Escape and `/stop` cancellation, retained answers, pending-pick restart without expiry, post-pick preflight, and absence of tournament pick state in `crates/squad-tui/tests/pick.rs`. (covered by crates/squad-tui/tests/validation_shell.rs, widget_pick.rs, session_shell.rs::restored_pick_has_no_expiry_and_no_entrance, preflight.rs and m4b_fx_input_audit.rs)
  - [x] M4.P4.L4.T7 — Verify tournament, grill, and pick banners at 120, 80, and 40 available columns, no-color and effects-off views, replay deduplication, and anchor preservation during ranking, plan, and candidate updates in `crates/squad-tui/tests/banners.rs` and `crates/squad-tui/tests/transcript.rs`. (covered by crates/squad-tui/tests/banners.rs, transcript.rs::cache_noop_and_source_anchor_survive_reflow_and_offscreen_preceding_growth and m4_expansion.rs)
  - [x] M4.P4.L4.T8 — Verify tournament and grill export scopes, retained plan versions, complete appendix, rankings, champion, core-owned banners, and success only after destination writes in `crates/squad-tui/tests/export.rs`. (covered by crates/squad-core/tests/export.rs::tournament_and_grill_public_projections and typed_grill_export_retains_versions_and_attributed_appendix; TUI destination coverage in tests/m4b_export_audit.rs)
  - [x] M4.P4.L4.T9 — Exercise the new contest views against M5's fake-provider integration fixtures in `crates/squad-tui/tests/conversation.rs`, leaving context isolation, blind-label validation, shared-floor delivery, rotation logic, and no-repeat enforcement to Contests and Hardening. (covered by crates/squad-tui/tests/m4b_tournament_runtime_audit.rs and grill_runtime.rs against loopback fake providers, SQLite, static widgets and core exports)

- **M4.P4.L5 — Verify scheduling and the final performance gate**
  - [x] M4.P4.L5.T1 — Verify input-first handling, feed batches bounded to 256, drawing during sustained feed bursts, token-batch deadlines, a 60 Hz cap, and a disarmed idle timer in `crates/squad-tui/tests/event_loop.rs`.
  - [x] M4.P4.L5.T2 — Verify transient-first Escape, second-Escape confirmation within 2 s, expired confirmation, animation input preservation, and `q` outside input in `crates/squad-tui/tests/interaction.rs`.
  - [ ] M4.P4.L5.T3 — After M4.P5, rerun the five-minute 12-stream workload with the real transcript widget and `SQUAD_FX=full` at both viewports on macOS arm64 and Linux x64 in `crates/squad-tui/benches/twelve_streams.rs`, requiring the M4.P1.L1 latency and CPU limits. (open: Linux full-effects measurements exist in docs/reports/perf-gates-2026-11-25; required five-minute macOS arm64 measurements are missing)
  - [ ] M4.P4.L5.T4 — Record final full-effects hardware, versions, measurement methods, results, and the completed M4.P5 dependency in `crates/squad-tui/README.md`; block M4 exit until both platforms pass. (open: README records Linux hardware and results, but M4 exit remains blocked on the missing macOS arm64 release gate)

- **M4.P4.L6 — Verify census and crosscheck presentation**
  - [x] M4.P4.L6.T1 — Snapshot census source previews, pass and lens labels, ranked ledger, singletons, covered citations, member-by-gap details, skipped members, and collapsed reports at both viewports in `crates/squad-tui/tests/census.rs`.
  - [x] M4.P4.L6.T2 — Verify exact authoritative census range text, adjacent caveat, discovery curve, plan-version changes, quiet guidance, and distinct pass-limit, stopped, and failed completion in `crates/squad-tui/tests/census.rs`, without testing estimate arithmetic in the TUI.
  - [x] M4.P4.L6.T3 — Snapshot crosscheck optional extraction, blind-inventory labels, grid summaries, retained count and trimming, every cell disposition, citations, exclusions, gaps, fixes, and skipped reviews in `crates/squad-tui/tests/crosscheck.rs`.
  - [x] M4.P4.L6.T4 — Verify auditor sample and challenge details, accepted rulings, retained cells, explicit risk-acceptance acknowledgement, and bounded stop guidance in `crates/squad-tui/tests/crosscheck.rs`.
  - [x] M4.P4.L6.T5 — Verify census and crosscheck display the authoritative call counts, pass count, optional extraction call, estimates, and remaining daily amount from M5 fixtures in `crates/squad-tui/tests/census.rs` and `crates/squad-tui/tests/crosscheck.rs`. (covered by crates/squad-tui/tests/m4b_conversation_audit.rs::new_mode_preflights_display_the_actual_m5_plan_and_enforce_one_claim using real M5 SQLite admission, both extraction paths and displayed estimates)

- **M4.P4.L7 — Verify survey and probe presentation**
  - [x] M4.P4.L7.T1 — Snapshot survey question-first and existing-answer paths, stable claim IDs, vote reasons and confidence, selected re-vote claims, five bands including empty bands, switchers, and settling questions at both viewports in `crates/squad-tui/tests/survey.rs`.
  - [x] M4.P4.L7.T2 — Verify survey's authoritative counts and band assignments, first-vote/re-vote comparison, missing-vote distinction, rejected-outlier wording, and absence of winner or scoreboard changes in `crates/squad-tui/tests/survey.rs`.
  - [x] M4.P4.L7.T3 — Snapshot probe role split, odd-member sit-out, explanation assumptions, check measurements, locked predictions, cannot-predict and skipped entries, tree, matrix, and unexpected-result branch at both viewports in `crates/squad-tui/tests/probe.rs`.
  - [x] M4.P4.L7.T4 — Verify probe branch wrapping, dropped-check reasons, inseparable explanations, authoritative participating count, and absence of execution or truth declarations in `crates/squad-tui/tests/probe.rs`.
  - [x] M4.P4.L7.T5 — Verify survey's new-answer and existing-answer preflight counts and probe's adjusted participant count and estimate against M5 fixtures in `crates/squad-tui/tests/survey.rs` and `crates/squad-tui/tests/probe.rs`. (covered by crates/squad-tui/tests/m4b_conversation_audit.rs new-mode and existing-answer preflights; real M5 boolean collection flag and adjusted probe count)

- **M4.P4.L8 — Verify shared new-mode interaction and recovery**
  - [x] M4.P4.L8.T1 — Snapshot all four new banners and step dividers at 120, 80, and 40 available columns with default and compact fonts, fallbacks, all banner modes, `NO_COLOR`, and effects off in `crates/squad-tui/tests/banners.rs`. (covered by crates/squad-tui/tests/banners.rs::four_new_banners_and_step_dividers_snapshot_with_default_and_compact_fonts and no_color_has_no_foreground_and_colour_tiers_do)
  - [ ] M4.P4.L8.T2 — Verify report expansion, keyboard detail actions, search reveal, source copy, anchored resize, and visible-row rendering for large ledgers, grids, maps, trees, and matrices in `crates/squad-tui/tests/navigation.rs` and `crates/squad-tui/tests/render_budget.rs`. (open: composed report models have empty source fields and Ctrl+F/Ctrl+Y currently inspect ordinary message cards only; report navigation depends on the earlier M4.P2.L8 work owned by the other M4 worker)
  - [x] M4.P4.L8.T3 — Verify restart, lag recovery, repeated artifacts, stable role assignments and audit samples, acceptance-record restoration, banner deduplication, and replay without entrance effects in `crates/squad-tui/tests/session.rs`. (covered by crates/squad-tui/tests/m4b_session_audit.rs::typed_role_audit_and_acceptance_records_survive_restore_and_duplicate_replay, event_loop_shell.rs lag recovery and survey.rs/probe.rs replay tests)
  - [x] M4.P4.L8.T4 — Verify new-mode rejection preserves drafts and quotes, `/stop` waits for acknowledgement, skipped members remain named, incomplete reports cannot show successful stop guidance, and daily-max refusal preserves committed content in `crates/squad-tui/tests/recovery.rs`. (covered by crates/squad-tui/tests/m4b_recovery_audit.rs and contest_actions.rs::stopped_and_failed_runs_are_incomplete_and_name_skipped_members; core refusal workflows preserve committed content)
  - [ ] M4.P4.L8.T5 — Verify inspection, expansion, search, and copying issue no member-delivery requests, while explicit quote preparation requires normal submission in `crates/squad-tui/tests/navigation.rs`, leaving structural isolation and blind-step tests to Contests and Hardening. (open: report inspection source-copy and search actions are incomplete in M4.P2.L8; cannot prove the complete navigation/submission boundary before that other-worker implementation lands)
  - [x] M4.P4.L8.T6 — Verify the four new export scopes preserve core-generated reports, provenance, citations, ranges and caveats, risk acceptances, voting details, prediction matrices, trees, skipped work, and stopped or failed status, with success only after destination writes in `crates/squad-tui/tests/export.rs`. (covered by core tests/census_export.rs, probe_export.rs, export.rs and room_export.rs; crates/squad-tui/tests/m4b_export_audit.rs verifies byte-identical destination writes and flush failures for every scope)
  - [ ] M4.P4.L8.T7 — Exercise all seven mode views against M5 integration fixtures and verify one active contest, unchanged existing-mode behavior, authoritative estimates and results, and complete static presentation with the M4.P5 hooks disabled in `crates/squad-tui/tests/conversation.rs`. (open: current TUI fixtures cover all four real M5 preflights and tournament/grill runtime, but a single seven-mode completed-result gate still depends on unfinished report-navigation integration in M4.P2.L8)

## Phase M4.P5 — Effects and motion

- **M4.P5.L1 — Fx level switch and terminal capability detection**
  - [x] M4.P5.L1.T1 — `squad-tui/src/fx/level.rs`: `FxLevel { Full, Lite, Off }` parsed from `SQUAD_FX`, default `Full`, unit tests for every value and for garbage input.
  - [x] M4.P5.L1.T2 — `squad-tui/src/fx/caps.rs`: colour tier from `NO_COLOR`/`COLORTERM`/`TERM`, sync-output probe with 100 ms timeout, SSH and tmux flags, auto-downgrade to `Lite` only when `SQUAD_FX` is unset; disable full-buffer passes without synchronized output at every level.
  - [x] M4.P5.L1.T3 — `squad-tui/src/fx/palette.rs`: per-model hues, OKLCH interpolation, 256 and 16-colour mapping, slots 7 to 12 generator, snapshot of all tiers.
  - [x] M4.P5.L1.T4 — `squad-tui/src/fx/caps.rs`: test unset versus explicit `full`, `lite` and `off` over SSH and without synchronized output; assert the canonical six-member ANSI mapping and default-foreground human body text.
- **M4.P5.L2 — Clock, effects manager and cue queue**
  - [x] M4.P5.L2.T1 — `squad-tui/src/fx/clock.rs`: `Clock` trait, `SystemClock`, `ManualClock`.
  - [x] M4.P5.L2.T2 — `squad-tui/src/fx/cue.rs`: `FxCue` enum and bounded queue; `squad-tui/src/feed.rs` emits cues for internally tagged `Live` delivery only, without adding an origin field to Protocol's `FeedItem`.
  - [x] M4.P5.L2.T3 — `squad-tui/src/fx/manager.rs`: `FxManager::{pre, post, next_wake}`, `FxKey` replacement, 48-effect cap, 2 s stale-cue drop, protected-rect clipping.
  - [x] M4.P5.L2.T4 — `squad-tui/Cargo.toml`: add `tachyonfx = "0.25"`; `squad-tui/src/fx/pass.rs` adapter wrapping `tachyonfx::Effect`. Consume banner text from `squad-core`; `figlet-rs` and embedded fonts belong to that crate.
  - [x] M4.P5.L2.T5 — `squad-tui/src/fx/cue.rs` and `squad-tui/src/feed.rs`: add the daily-spend, tournament, grill and pick cues; derive persistent markers from reduced state and suppress entrances during replay.
- **M4.P5.L3 — Frame-budget governor and idle halt**
  - [x] M4.P5.L3.T1 — `squad-tui/src/fx/governor.rs`: EWMA, byte-rate input, G0 to G3 thresholds, 3 s hysteresis, table-driven tests.
  - [x] M4.P5.L3.T2 — `squad-tui/src/event_loop.rs`: integrate `next_wake()` with the TUI-owned event loop; test zero draws over 5 s after idle effects end and continued drawing during bounded feed batches.
  - [x] M4.P5.L3.T3 — `squad-tui/benches/fx_frame.rs`: 12 streams plus verdict at 200x60 under 6 ms.
- **M4.P5.L4 — Test harness**
  - [x] M4.P5.L4.T1 — `squad-tui/tests/fx_support/mod.rs`: `render_at(cues, t)` helper over `TestBackend` and `ManualClock`.
  - [x] M4.P5.L4.T2 — `squad-tui/tests/fx_invariants.rs`: end-state equals `off` render; protected rects keep glyphs; replay of 1,000 events yields zero effects.
- **M4.P5.L5 — Chrome and participants effects**
  - [x] M4.P5.L5.T1 — `squad-tui/src/fx/chrome.rs`: logo reveal, roster resolve from the resolved state at first draw, border breath, quit power-down on TUI quit actions, resize hard cut; snapshots.
  - [x] M4.P5.L5.T2 — `squad-tui/src/fx/status.rs`: orbit glyph, throughput bar from real token rate, backoff ring, pip stack; snapshots.
  - [x] M4.P5.L5.T3 — `squad-tui/src/fx/meter.rs`: cost odometer, daily-spend heat states at 75/90/100%, record tick; snapshots.
- **M4.P5.L6 — Transcript effects**
  - [x] M4.P5.L6.T1 — `squad-tui/src/fx/bubble.rs`: rail drop, comet cursor, seal, rail fracture; snapshots at four timestamps each.
  - [x] M4.P5.L6.T2 — `squad-tui/src/fx/dispatch.rs`: launch streak, spoke burst with narrow fallback, quote lasso.
  - [x] M4.P5.L6.T3 — `squad-tui/src/fx/command.rs`: `/stop` brake, colour-only `/clear` wipe followed by a hard cut; preserve protected message-body glyphs during the pass.
- **M4.P5.L7 — Debate effects**
  - [x] M4.P5.L7.T1 — `squad-tui/src/fx/particles.rs`: fixed pool of 256, gravity integrator, blank-cell-only drawing, seeded determinism test.
  - [x] M4.P5.L7.T2 — `squad-tui/src/fx/debate.rs`: versus card settling inline, score slam, tug bar, lead flip; snapshots.
  - [x] M4.P5.L7.T3 — `squad-tui/src/fx/filter.rs`: gay, metal and duotone filters as pure `(x, y, t) -> Color`, with 16-colour and `NO_COLOR` variants.
- **M4.P5.L8 — Showpiece: round banner transition**
  - [x] M4.P5.L8.T1 — `squad-tui/src/fx/banner.rs`: consume the shared `squad-core` banner text cache per (text, font, width); horizon unfold, glint, hold, settle, immediate settle on first token.
  - [x] M4.P5.L8.T2 — `squad-tui/tests/fx_banner.rs`: snapshots at 0, 120, 280, 600, 900, 1600, 1900 ms for `ROUND 3`, `FINAL ROUND`, `OPENING` at 120x40 and 80x24, in all colour tiers.
  - [x] M4.P5.L8.T3 — `squad-tui/tests/fx_banner.rs`: assert `VERDICT` entrance occurs on entering `JUDGING`, the verdict arrival does not insert a duplicate banner, and early tokens leave permanent inline art.
- **M4.P5.L9 — Showpiece: verdict reveal**
  - [x] M4.P5.L9.T1 — `squad-tui/src/fx/verdict.rs`: timeline state machine (blackout, criteria rows, suspense hold, winner, confetti, loser section), skip on keypress without consuming the key.
  - [x] M4.P5.L9.T2 — `squad-tui/tests/fx_verdict.rs`: snapshots at each storyboard timestamp, an equal-round-tally case, and the `lite` and narrow variants.
  - [x] M4.P5.L9.T3 — `squad-tui/examples/fx_reel.rs`: scripted demo that plays launch, a three-round debate and a verdict from canned data, for recording.
- **M4.P5.L10 — Daily spend effects**
  - [x] M4.P5.L10.T1 — `squad-tui/src/fx/meter.rs`: animate authoritative daily spend and pending reservations; preserve unknown-cost `~` and the always-visible spend/max labels; apply heat thresholds to actual spend.
  - [x] M4.P5.L10.T2 — `squad-tui/tests/fx_meter.rs`: snapshots and state-change tests for 75/90/100%, pending-to-actual replacement, unknown costs, local-day rollover and a raised max, including `lite`, `off` and `NO_COLOR`.
- **M4.P5.L11 — Tournament effects**
  - [x] M4.P5.L11.T1 — `squad-tui/src/fx/tournament.rs`: reveal accepted rankings bottom-up in fixed row positions, stamp the committed elimination and retain the eliminated-row marker through the tournament.
  - [x] M4.P5.L11.T2 — `squad-tui/src/fx/tournament.rs` and `squad-tui/src/fx/verdict.rs`: reuse the celebration for `CHAMPION`, with one trophy tick and no debate W-L tick.
  - [x] M4.P5.L11.T3 — `squad-tui/tests/fx_tournament.rs`: snapshot ranking, elimination and champion sequences; test duplicate cues, resize, replay, rejected judge output and all effect levels against the static render.
- **M4.P5.L12 — Grill effects**
  - [x] M4.P5.L12.T1 — `squad-tui/src/fx/grill.rs`: turn handoff, plan version badge bump and `SURVIVED` treatment keyed to the authoritative completion reason; preserve plan and Q&A glyphs.
  - [x] M4.P5.L12.T2 — `squad-tui/tests/fx_grill.rs`: snapshots and state-change tests for sequential handoff, `PASS`, rebuild, survived early end, normal completion and leader failure retaining the last good plan.
- **M4.P5.L13 — Question-first pick effects**
  - [x] M4.P5.L13.T1 — `squad-tui/src/fx/banner.rs` and `squad-tui/src/fx/pick.rs`: animate `CHOOSE YOUR DEBATE BROS` and `CHOOSE WHO GETS GRILLED` inline; retain the TUI-owned hint and highlight only eligible candidate rows while the pick is pending.
  - [x] M4.P5.L13.T2 — `squad-tui/src/fx/pick.rs`: remove candidate effects on accepted pick, cancellation or failure; use static markers in `lite`, `off` and `NO_COLOR`, and after replay restoration.
  - [x] M4.P5.L13.T3 — `squad-tui/tests/fx_pick.rs`: snapshots of both banners and hints at wide and narrow sizes; rejected picks retain highlights, ordinary chat remains usable, accepted picks clear highlights and restored pending picks produce no entrance.
- **M4.P5.L14 — Effects integration gates**
  - [x] M4.P5.L14.T1 — `squad-tui/tests/fx_input.rs`: assert animation skip preserves typed keys and the TUI-owned Escape priority and contest-stop confirmation, including a pending pick. (covered by crates/squad-tui/tests/m4b_fx_input_audit.rs for all 18 catalogue families, transient Escape, second-Escape stop and pending picks)
  - [x] M4.P5.L14.T2 — `squad-tui/tests/fx_invariants.rs`: extend end-state, protected-glyph, resize and replay properties to daily spend, tournament, grill and pick effects.
  - [ ] M4.P5.L14.T3 — `squad-tui/benches/fx_frame.rs` and `squad-tui/tests/fx_integration.rs`: re-run the real-widget 12-stream full-effects gate after M4.P5 on both release platforms, including sustained feed bursts and idle timer disarming, as the M4 exit gate referenced by M6.P2.L1.T8. (open: Linux renderer results and sustained-feed/idle tests exist, but the required macOS arm64 real-widget release rerun is missing)
- **M4.P5.L15 — Shared engine cues and four-mode banners**
  - [x] M4.P5.L15.T1 — `squad-tui/src/fx/cue.rs` and `squad-tui/src/feed.rs`: add `ModeBannerReady`, `CensusEstimateReady`, `CrosscheckCellsReady`, `SurveyTallyReady` and `ProbeTreeReady`; emit them only for accepted live state or committed artifacts, never judge streams, rejected output or repair attempts.
  - [x] M4.P5.L15.T2 — `squad-tui/src/fx/manager.rs`: key effects by contest and artifact identifiers, suppress duplicate entrances, replace superseded tweens and settle report effects on collapse, stop, failure or resize; retain partial committed state.
  - [x] M4.P5.L15.T3 — `squad-tui/src/fx/banner.rs`: reuse the shared entrance for `CENSUS`, `CROSSCHECK`, `SURVEY` and `PROBE` in gold, with step-divider accents, immediate settlement on early output and existing `SQUAD_BANNERS` behavior.
  - [x] M4.P5.L15.T4 — `squad-tui/tests/fx_banner.rs`: snapshot all four banners at wide and narrow sizes in all levels and colour tiers; cover disabled banners, early output, duplicate setup/state delivery and static restart restoration.
- **M4.P5.L16 — Census effects**
  - [x] M4.P5.L16.T1 — `squad-tui/src/fx/census.rs`: implement the pass-to-pass estimate count-down in a dedicated graphic rect using stored range endpoints; preserve the exact headline, shared-blind-spots caveat, ledger, citations, singleton markers and discovery curve.
  - [x] M4.P5.L16.T2 — `squad-tui/tests/fx_census.rs`: snapshot first-pass, falling, rising and unavailable estimates; assert no invented zero, no forced decline, and no quiet message unless present in the authoritative report.
  - [x] M4.P5.L16.T3 — `squad-tui/tests/fx_census.rs`: cover partial member failure, stop, replay, collapse, resize and all effect levels against the static census report.
- **M4.P5.L17 — Crosscheck effects**
  - [x] M4.P5.L17.T1 — `squad-tui/src/fx/crosscheck.rs`: fill committed cell results in fixed grid order with one bounded staggered batch effect; keep statuses and evidence readable and retain open, pending, skipped, challenged and accepted-risk distinctions.
  - [x] M4.P5.L17.T2 — `squad-tui/tests/fx_crosscheck.rs`: snapshot single-cell and 200-cell batches, audit challenges and rulings; assert no cell removal or reordering and no false closed or complete appearance.
  - [x] M4.P5.L17.T3 — `squad-tui/tests/fx_crosscheck.rs`: cover duplicate batches, superseding rulings, stop, replay, collapse, resize, narrow summary fallback and every effect level.
- **M4.P5.L18 — Survey effects**
  - [x] M4.P5.L18.T1 — `squad-tui/src/fx/survey.rs`: animate initial AGREE / DISAGREE / UNSURE bar splits and revised tally boundaries from stored counts; use explicit labels and patterned fallbacks without moving claim rows.
  - [x] M4.P5.L18.T2 — `squad-tui/tests/fx_survey.rs`: snapshot all five bands and vote changes; preserve reasons, confidence, switch details and settling questions, distinguish missing votes from UNSURE, and assert no winner or record effects.
  - [x] M4.P5.L18.T3 — `squad-tui/tests/fx_survey.rs`: cover partial votes, duplicate tallies, stop, replay, collapse, resize, narrow rendering and every effect level against the static survey report.
- **M4.P5.L19 — Probe effects**
  - [x] M4.P5.L19.T1 — `squad-tui/src/fx/probe.rs`: trace accepted decision-tree connectors branch by branch in stored root-first order within 900 ms; leave all node text and prediction-matrix entries visible and unchanged.
  - [x] M4.P5.L19.T2 — `squad-tui/tests/fx_probe.rs`: snapshot branch progression, unexpected results and inseparable explanations; preserve role and sit-out notices and assert that no explanation becomes a winner.
  - [x] M4.P5.L19.T3 — `squad-tui/tests/fx_probe.rs`: cover rejected report output, partial failure, stop, replay, collapse, resize, narrow wrapping and every effect level against the static probe report.
- **M4.P5.L20 — Four-mode integration gates**
  - [x] M4.P5.L20.T1 — `squad-tui/tests/fx_invariants.rs`: extend final-render, protected-glyph, dropped-cue, duplicate-cue, resize and replay properties to the four modes, including restoration of partial committed reports.
  - [x] M4.P5.L20.T2 — `squad-tui/tests/fx_input.rs` and `squad-tui/tests/fx_integration.rs`: verify normal input, skip and Escape handling during each new effect; assert that animation never delays a shared move or barrier and leaves member deliveries and Markdown exports unchanged. (covered by crates/squad-tui/tests/m4b_fx_input_audit.rs and m4b_fx_integration_audit.rs::new_effects_do_not_gate_moves_barriers_or_change_deliveries_and_export)
  - [ ] M4.P5.L20.T3 — `squad-tui/benches/fx_frame.rs`: include the 200-cell grid, survey bars and probe tree in real-widget effect workloads; preserve the 256-cue queue, 48-effect cap, under-6-ms frame gate, governor degradation and idle timer disarming on both release platforms. (open: fx_frame.rs includes catalogue grid/bars/tree effects, but full real-widget workloads and the under-6-ms macOS release measurement are not established)

