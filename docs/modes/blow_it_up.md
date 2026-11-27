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

The TUI uses severity colours; press F7 to expand or collapse the killed appendix. The report contains ranked findings, lone discoveries, additions, skipped members, unchallenged vectors, and a killed appendix with counts, defeating quotes and judge notes. Export includes the target snapshot, lens draw, all accepted attack sets, pool/provenance, ballots and app tallies. Stopped and failed contests retain their accepted evidence for export. No report establishes that a target is safe.

A normal six-member run makes 13 calls and reserves 27 attempts (`2m + 1` calls, `4m + 3` attempts). Member deadlines are 180 seconds with two attempts; judge deadlines are 300 seconds with three attempts and at most one schema repair. Attack prose is capped at 700 words, challenges and the ruling at configured `planWords`. Provider cost depends on the frozen models and target size. Admission shows the normal estimate and the conservative reservation; actual usage replaces started reservations and unstarted reservations release on completion or stop.

### Example run

One real release `squad send` completed on 2026-11-26 America/Chicago (started **2026-11-27 00:09:11 UTC**) using the default six-member roster, `SQUAD_DAILY_MAX_USD=150`, `--env-file /dev/null`, `--yes`, and a fresh private data directory. Its target was an unauthenticated upload/download service with root writes, caller-selected absolute paths and permanent public logs. The credential was fetched directly into the process environment.

The run reached **COMPLETE**, exited **0**, and produced its ranked report in **170.052 seconds**. Preflight declared **13 normal calls / 27 reserved attempts**, estimated **$4.505912**, and reserved **$20.460108**. Recorded actual spend was **$1.449895**, including **$0.001538** for startup resolution (**$1.448357** for the mode). At completion the ledger had **$0 reserved**, **zero unknown-cost calls**, and **zero unresolved attempts**. All twenty recorded provider starts settled: sixteen mode attempts plus four startup probes.

Five attack sets were accepted, including two that needed private repair. Grok's attack refused twice and was disclosed as skipped; it still supplied an accepted challenge ballot. All six challenge ballots and the first ruling were accepted. The report has **13 ranked findings**, including **three lone findings**, **nine challenge additions**, and **nine unchallenged additions**; no vector was killed. Missing attacks did not become votes or finder credit.

Short excerpt from the real completed export:

```text
BLOW IT UP
WHAT SURVIVED
### Critical / Likely · Unauthenticated root arbitrary file write enables host takeover · score 12
Mitigation: Require authentication and authorization. Run the service unprivileged, ignore caller absolute paths, and confine writes to a canonicalized dedicated upload directory with server-generated names.
Rebuttal: Survived. Residual: Exact execution route and timing depend on host configuration; single-request persistence is not established.
Blind finders: 5
```

Offline `/export blow_it_up` also exited 0 without credentials or additional provider calls. Local raw evidence is under `/tmp/squad-blow-it-up-r2-live-s6k0tsps/`: `result.json`, `report.md`, stdout/stderr, the private SQLite ledger and `credential-scan.json`. Nine evidence files were scanned with **zero credential matches**. These temporary artifacts are not committed.

The earlier worker's separate 209.411-second run stopped at the old judge-dispatch bug. Its $0.458827 known spend and three unresolved attempts remain historical partial evidence in `/tmp/squad-blow-it-up-live-10z4wyli/`; they are not included in the completed run's totals above.

### Integration and verification

The resumed `mode-blow-it-up-r2` branch includes the required shared integration fixes, authorized on 2026-11-26:

- New-mode judge work reaches the structured mode runtime. Both complete-report RoomHandle tests are enabled; all nine mode workflows pass, including the real latest-member-reply source form, repairs, own-vector exclusion, skipped members, stop fencing and recovery.
- Challenge work declares its 4,096-token ceiling. Invalid member outputs use `MODE_INVALID_OUTPUT` and receive the private D75 repair path. Repair templates accept the mode's `mode_step` value.
- Rulings emit validated typed `CaseOpened` envelopes. Frozen prompts, bindings and stage projections remain persisted per assignment.
- The transcript preserves severity styles and wraps findings at the available width. **F7** expands or collapses the killed appendix, which is collapsed by default. A completed-report replay test exercises this shared composition path, including its styles and details control.
- Public completion, help and the parser-checked TIP rotation include the enabled mode. Shared scaffold assertions follow built modes; original parser fixtures retain a scoped disabled override. The tournament audit takes a fresh terminal event page before restoring its view.

Mode-owned acceptance coverage includes protocol/schema validation, private storage and visibility, the provider adapter, direct router/executor workflows, full RoomHandle completion, hand-computed tally/ranking fixtures, export, and TUI report replay. The two full-report workflows are no longer ignored. The completed live run above verifies the default six-model path through the real judge and offline export.

Final gates on dev2, with `CARGO_BUILD_JOBS=4` and sequential builds:

- `cargo test --workspace --no-fail-fast`: passed; Cargo summaries total **1,358 passed, zero failed, four ignored**, including the repeated passing WAL child summary. The ignored entries are three existing manual performance/memory measurements and a PTY child entry exercised by its parent. All **32 mode-owned checks pass**, with no ignored mode cases.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo build --release -p squad-cli`: passed; the smoke used a private copy of this binary.
- `cargo fmt --all -- --check` and `git diff --check`: passed.

All fourteen M8.P6 tasks are checked. Required shared changes are included; no integration fix remains deferred. No main push, tag or PR was made.
