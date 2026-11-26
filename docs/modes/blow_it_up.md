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

Live smoke evidence will be recorded here after the release build and isolated run. The credential-free six-member fixture exercises the actual owned router, executor, streaming transport, accounting and export with a target that accepts requests without authentication.

### Integration handoff

Only mode-owned paths were changed. Shared fixes still required:

- `crates/squad-core/src/runtime/judge.rs`: the outer `Executor::execute` must route `grant.work.kind.new_mode_judge()` to `self.crosscheck.execute(...)` alongside probe/census/survey. The inner Runner already dispatches new modes; the outer fallback currently sends them to `JudgeExecutor`, which rejects them. Two complete-report RoomHandle workflow tests are explicitly ignored with this reason; direct owned-callback workflow tests remain executable.
- `crates/squad-protocol/src/work.rs`: add `BlowItUpChallenge` to both 4,096-token ceiling matches (`Work::validate` and `WorkKind::new_mode_visible_tokens`). Until then the owned allocator leaves that optional Work ceiling unset and freezes the correct 4,096 limit in its authorized input; bounds and the actual request both use 4,096.
- `crates/squad-core/src/router/executor.rs`: include `MODE_INVALID_OUTPUT` in invalid-output classification. Until then the mode adapter uses `JUDGE_INVALID_MODE_OUTPUT` for invalid member outputs as well, enabling the required private D75 repair through the existing mode-repair path.
- `crates/squad-protocol/src/envelopes.rs`: replace the unconditional rejection of `CaseMaterial::BlowItUp` with typed case validation. The owned allocator currently stores the typed case against a human-only `blow_it_up.case` artifact instead of emitting the rejected `CaseOpened` envelope.
- `crates/squad-tui/src/composed.rs`: compose `widgets::blow_it_up::styled_lines` instead of joining plain strings, and connect the mode's `View::toggle_killed` to the details interaction. The widget supplies severity colors and a collapsed appendix; the shared renderer currently strips styles and has no toggle wiring for these modes.
- `crates/squad-core/src/prompts/render.rs`: allow `blow_it_up` in the `mode_step` whitelist. The mode currently freezes its literal composition name into the repair suffix to keep D75 repair functional without changing this shared file.
- `crates/squad-tui/src/typeahead.rs`: add a parser-valid `$blow_it_up` example to `examples()` so the TIP rotation includes this mode; its command list and help already derive enablement from the switch.
- Shared parser/typeahead fixtures must track enabled modes. In particular `crates/squad-protocol/src/parser/new_modes_tests.rs` currently expects its `condition=disabled` fixture to fail even after this mode is enabled; snapshot expectations also change when the public mode list grows.

New ordinary replies are held by the owned engine before dispatch and released at terminal states. Stage contexts always receive frozen authorized projections. Prompt source and catalog capabilities are frozen per assignment, so restarts do not redraw lenses or re-read changed prompt overrides.
