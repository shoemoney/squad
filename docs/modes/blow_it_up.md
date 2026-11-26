## Blow It Up

`$blow_it_up` red-teams a plan, specification, prompt, configuration or diff. It asks how the target breaks or can be exploited, then challenges the attacks before reporting what survived. Use census or crosscheck to look for omissions.

````text
@all $blow_it_up ```All requests are accepted without authentication.```
@all $blow_it_up <paste a plan here>
--
Focus on abuse, cost and operational failure.
@claude $blow_it_up
/export blow_it_up
/export blow_it_up <contest-id>
````

The single-member mention selects that member's latest reply as the target; the whole room attacks it. You can also attach one quoted message. A source is frozen at admission and never silently refreshed. Starts require at least three room members; at least two usable attack sets must survive validation.

1. Squad freezes the target, scope, roster, models, prompts and cost reservation. It draws and persists a private participant-to-lens permutation across outside attackers, malicious insiders, operator error, dependencies, incentives/cost/fraud, and legal/reputational failure. Lenses wrap after six participants.
2. Each participant independently proposes up to five ordinary attack vectors plus exactly one diversity finding: something it believes the others will miss, using its assigned lens. Every vector quotes at least 12 contiguous characters of the target. Invalid structured output receives one private repair attempt; missing or failed attack sets are skipped.
3. Squad assigns opaque shuffled vector IDs and freezes the pool with private provenance. Each member receives a fresh anonymous pool excluding its own vectors. It must challenge every supplied vector, and can add up to two discoveries. False-positive verdicts require a verbatim defeating quote; duplicate verdicts name another supplied vector. Reasons are limited to 20 words.
4. Squad counts accepted ballots. At least two verdicts and a strict majority of false-positive votes kill a vector. A strict majority of weak plus false-positive votes otherwise makes it contested. Exact halves remain standing. Fewer than two verdicts means unchallenged: the false-positive filter did not run. Missing ballots never become votes.
5. A blind judge deduplicates surviving evidence, assigns severity and likelihood, and writes mitigations, rebuttal status and residual risk. It cannot invent findings or reinstate killed vectors. Contested evidence requires a weakened finding or an explanation for rejecting the challenges.
6. Squad ranks by severity × likelihood, then distinct blind attackers, then judge order. Critical/high/medium/low weigh 4/3/2/1 and likely/possible/unlikely weigh 3/2/1. A surviving finding from one blind attacker is highlighted as a lone finding. Challenge additions carry no finder count.

The report contains ranked findings, lone discoveries, additions, skipped members, unchallenged vectors, and a killed appendix with counts, defeating quotes and judge notes. Export includes the target snapshot, lens draw, all accepted attack sets, pool/provenance, ballots and app tallies. Stopped and failed contests retain their accepted evidence for export. No report establishes that a target is safe.

A normal six-member run makes 13 calls and reserves 27 attempts (`2m + 1` calls, `4m + 3` attempts). Member deadlines are 180 seconds with two attempts; judge deadlines are 300 seconds with three attempts and at most one schema repair. Attack prose is capped at 700 words, challenges and the ruling at configured `planWords`. Provider cost depends on the frozen models and target size. Admission shows the normal estimate and the conservative reservation; actual usage replaces started reservations and unstarted reservations release on completion or stop.

### Example run

On 2026-11-26, one real release `squad send` used the default six-member roster, `SQUAD_DAILY_MAX_USD=150`, a fresh private data directory, `--env-file /dev/null`, and `--yes`. The target was a public unauthenticated upload/download service running as root, with client-selected absolute paths and permanent raw-content logs. The API key was fetched off-screen from the credential helper; scanning 18 local evidence/source files found zero credential matches.

The run lasted **209.411 seconds** and recorded **$0.458827** known spend, including **$0.001468** startup resolution. Preflight estimated **$4.525920**, reserved **$20.539587**, and declared **13 calls / 27 attempts**. At shutdown **$12.458302** remained reserved locally and **three attempts were unresolved** (two challenge attempts and the judge's pre-dispatch start). The known spend is not a verified final provider total; unresolved costs were not invented or reported as zero.

Five attack sets and four challenge ballots were accepted. Grok's attack refused without a visible answer, Gemini's challenge returned HTTP 403, and Grok's challenge failed after retries. The app committed 20 pooled vectors and tallies: 11 standing, one contested, and eight unchallenged challenge additions. The run reached `RULING` and was interrupted at the confirmed shared outer judge-dispatch blocker (exit 130). It produced **no completed ruling or ranked report**. Offline export succeeded without credentials or further provider calls.

Short excerpt from that real partial export:

```text
# BLOW IT UP

Partial evidence; the contest has no completed report.

Status: Ruling
```

Accepted, still-unruled attack titles included “Unauthenticated root write to an arbitrary path” and “Insider harvests permanent logs of all uploads.” These are model findings pending adjudication, not verified exploits.

Local raw evidence is under `/tmp/squad-blow-it-up-live-10z4wyli/` (`result.json`, `report.md`, stdout/stderr and the private SQLite ledger). This directory is temporary and is not committed. The complete credential-free Wiremock workflow separately verifies the final report, ranking, accounting and export with 13 calls; the live run is partial evidence only.

### Integration handoff

Only mode-owned paths were changed. Shared fixes still required:

- `crates/squad-core/src/runtime/judge.rs`: the outer `Executor::execute` must route `grant.work.kind.new_mode_judge()` to `self.crosscheck.execute(...)` alongside probe/census/survey. The inner Runner already dispatches new modes; the outer fallback currently sends them to `JudgeExecutor`, which rejects them. Two complete-report RoomHandle workflow tests are explicitly ignored with this reason; direct owned-callback workflow tests remain executable.
- `crates/squad-protocol/src/work.rs`: add `BlowItUpChallenge` to both 4,096-token ceiling matches (`Work::validate` and `WorkKind::new_mode_visible_tokens`). Until then the owned allocator leaves that optional Work ceiling unset and freezes the correct 4,096 limit in its authorized input; bounds and the actual request both use 4,096.
- `crates/squad-core/src/router/executor.rs`: include `MODE_INVALID_OUTPUT` in invalid-output classification. Until then the mode adapter uses `JUDGE_INVALID_MODE_OUTPUT` for invalid member outputs as well, enabling the required private D75 repair through the existing mode-repair path.
- `crates/squad-protocol/src/envelopes.rs`: replace the unconditional rejection of `CaseMaterial::BlowItUp` with typed case validation. The owned allocator currently stores the typed case against a human-only `blow_it_up.case` artifact instead of emitting the rejected `CaseOpened` envelope.
- `crates/squad-tui/src/composed.rs`: compose `widgets::blow_it_up::styled_lines` instead of joining plain strings, and connect the mode's `View::toggle_killed` to the details interaction. The widget supplies severity colors and a collapsed appendix; the shared renderer currently strips styles and has no toggle wiring for these modes.
- `crates/squad-core/src/prompts/render.rs`: allow `blow_it_up` in the `mode_step` whitelist. The mode currently freezes its literal composition name into the repair suffix to keep D75 repair functional without changing this shared file.
- `crates/squad-tui/src/typeahead.rs`: add a parser-valid `$blow_it_up` example to `examples()` so the TIP rotation includes this mode; its command list and help already derive enablement from the switch.
- Shared disabled-mode tests must track built modes: `crates/squad-core/tests/router_validation.rs`, `crates/squad-protocol/src/parser/{new_modes_tests,parser_tests}.rs`, `crates/squad-protocol/tests/contests.rs`, and `crates/squad-tui/tests/{actions_shell,contest_actions}.rs` currently assert that this implemented mode stays disabled. Update their expectations or add a scoped force-disabled test override. Public typeahead snapshots may also need updates when the mode list grows.

- `crates/squad-tui/tests/m4b_tournament_runtime_audit.rs`: re-page events after observing the terminal room state before restoring the TUI. The current test captures events first, then observes a newer terminal state and can omit the champion event. It failed twice at the champion assertion. The owned `enabled_mode_preserves_tournament_champion_with_a_fresh_terminal_snapshot` test uses the same fixture with a fresh terminal event page and passes, preserving the champion and export with this mode enabled.

New ordinary replies are held by the owned engine before dispatch and released at terminal states. Stage contexts always receive frozen authorized projections. Prompt source and catalog capabilities are frozen per assignment, so restarts do not redraw lenses or re-read changed prompt overrides.

### Verification handoff

All commands used `CARGO_BUILD_JOBS=4` on dev2, with builds run sequentially.

- Owned protocol/store/agents/core/TUI suites and blinding unit tests: **28 passed, zero failed, two ignored**. The two ignored full-report RoomHandle tests name the missing shared judge route. Direct owned-callback workflows still execute the actual scoped readers, provider streaming, retries, private repairs, accounting, barriers, report and export, including 13 calls / 27 reserved attempts, two-set quorum, insufficient attacks, skipped ballots, judge failure, own-vector exclusion, stop and persisted recovery.
- `cargo build --release -p squad-cli`: passed; the real smoke used a private copy of the built release binary.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed, including a final rerun after fixture and view-test corrections.
- `cargo fmt --all -- --check`: passed, including the final source state.
- `cargo test --workspace --no-fail-fast`: **not green**. Cargo summaries totaled 1,340 passed, 13 failed and six ignored, including the existing repeated WAL subprocess summary. Four failures were an owned store-fixture history metadata error, corrected and verified by all four passing store tests. Eight failures are shared disabled-mode/help/export/menu assertions that must be updated for enablement. The remaining failure is the shared tournament audit's stale event-page race described above; its fresh-snapshot guard passes. The shared tests were left untouched as required by ownership.

M8.P6 has eleven tasks checked with evidence. L4.T2 remains open for shared severity-style/details wiring; L5.T1 remains open for the complete RoomHandle integration and full workspace gate; L5.T3 remains open for final integration sign-off and the TIP example. `ENABLED = true` on the worker branch permits parser/typeahead/help and smoke verification, and must not be read as a green integration gate. No main push, tag, PR, shared source edit, delegate, or extra live mode run was made.

### Resume audit on `mode-blow-it-up-r2`

The resumed branch retains `ENABLED = true`; the mode still requires the shared integration fixes above before release. Two additional acceptance tests pass:

- `nick_source_freezes_latest_reply_and_attacks_with_the_whole_roster` starts an ordinary member reply through RoomHandle, selects it with `@member0 $blow_it_up`, verifies the frozen target and source event, and checks all six attack assignments before stopping.
- `report_replay_preserves_severity_and_collapses_killed_evidence` replays a completed report twice, verifies critical severity styling, and checks that toggling the killed appendix reveals and hides its counts, defeating quote and judge note.

Explicitly running the two ignored completion workflows with `--include-ignored` reproduces the judge-dispatch blocker: six workflows pass and the two full-report workflows time out in `RULING`, with ruling work running at attempt one. This is not a passing completion gate. No additional paid smoke was started while that route remains broken. The earlier 209.411-second partial run, its $0.458827 known cost and its unresolved reservations remain the live evidence; the shared-file ownership restriction in the task brief prevents repairing that route within the mode-owned files.
