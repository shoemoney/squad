# `$debug` — narrow a failure with tests you run

Use debug for a persistent failure with competing explanations and observable tests. Members work independently; the judge merges the differential and selects a safe test. You run the test and paste the result. Squad runs no commands and applies no patches.

```text
@all $debug Requests time out after a deploy. Health checks pass. Rolling back fixes it.
--
Read-only checks only; production restarts are unavailable.
```

Select at least three members with mentions, or use `@all` (up to twelve). `@judge` cannot participate. Set `/set DEBUG_ROUNDS 3` before starting; the allowed range is 1–6. The default is three rounds.

1. Every member independently proposes up to four hypotheses with supporting and contradicting quotes. Quotes that do not match the brief are retained as **unanchored**, never treated as evidence.
2. A blind judge merges duplicate causes into up to eight hypotheses, preserving every source, opposing quote and prior spread.
3. Each member independently proposes up to two tests and predicts an outcome for every live hypothesis. A prediction may explicitly say it cannot predict. Proposals include effort and safety.
4. A blind judge selects a proposed test whose safety is `read_only` or `reversible` and which splits at least two live hypotheses. It cannot invent a test, alter the procedure, or select a `risky` test.
5. `RUN THE TEST` shows the instruction and labels `O1`, `O2`, etc. The room waits without a timer and releases the participants for ordinary chat. Closing and reopening preserves the same instruction and labels.
6. You answer. Application code updates the differential from the committed predictions and your result, then repeats or reports.

```text
/answer O2 The lookup returned the old address; here is the resolver output...
/answer other The command returned a third result: permission denied.
/answer unknown
/continue
```

A listed outcome eliminates only hypotheses that predicted a different listed outcome. Matching predictions gain one consistent result. `disputed` and `cannot_predict` do not change. `other` eliminates nothing, records an unexpected result, and permits members to propose a new hypothesis next round. `unknown` and `/continue` record an inconclusive or unrun test and change nothing. Silence applies no default. Ordinary messages preserve the pending step.

The loop ends with one live hypothesis (`isolated`), none (`all_eliminated`), no proposals (`no_tests`), no discriminating safe test (`no_discriminating_test`), or the round limit (`rounds_complete`). Failed members are named with the step and round; fewer than two initial hypothesis tables fails the run. Judge exhaustion fails with `judge_failed`. `/stop` fences outstanding work and retains committed evidence.

A patch is advice for you to apply. It is allowed only when exactly one hypothesis remains live and at least one test you ran produced a consistent result for it. Otherwise the report states:

> No patch: the evidence does not yet isolate a cause.

The report contains the differential, live/eliminated status and consistent counts, every test and pasted result, failed predictions, the next test, risky unselected proposals, unanchored quotes and skipped members. No status means that a cause has been proved.

## Cost

For `m` members and `R` rounds the upper bound is `(R + 1)(m + 1) + 1` calls, reserving `2m(R + 1) + 3R + 6` attempts. Six members and three rounds plan 29 calls and 63 attempts; twelve members plan 53 calls and 111 attempts. Early termination releases unused reservations. Members get two attempts, including one invalid-output repair; judges get three attempts and at most one repair. Human waiting costs nothing.

Preflight shows the estimate and worst-case reservation based on resolved model pricing and frozen prompts. Actual cost depends on models, output length and repairs. Later prompts include at most the first 400 words of each pasted result; the full answer remains in the transcript and export.

## Report and export

```text
/export debug
/export debug <contest-id>
```

From the shell:

```sh
squad send --yes '@all $debug Requests time out after deploy
--
Read-only checks only'
squad send '/answer O2 The resolver returned the old address'
squad export debug <contest-id> -o debug-report.md
```

The Markdown export includes the brief and limits, original hypothesis tables and attribution, all proposals and predictions, each selection, unanchored quotes, the full raw human answers, the differential and the report. Export while waiting includes the committed evidence and pending instruction. It contains no frozen provider prompts or credentials.

## Integration status

This branch keeps the exact mode ownership boundary. The scaffold's outer judge executor currently omits new-mode judge routing in `crates/squad-core/src/runtime/judge.rs`; the integrator must route `new_mode_judge()` to the already registered structured runner. The owned executor is exercised through real leased grants, scoped readers, accounting and fake OpenRouter calls in `debugging_workflows.rs`.

The shared typeahead inventory and help follow the mode switch. The shared idle TIP example list still needs a debug example. The shared string-only transcript adapter also needs a styling hook to grey eliminated differential rows; this branch renders their explicit eliminated status.

## Example run

On dev2 on 2026-11-26, exactly one paid release `squad send` used the repository's default **six** seats (the owner confirmed six), `SQUAD_DAILY_MAX_USD=150`, `--env-file /dev/null` and a fresh private directory. The key was fetched directly into the process environment through `aigate-key openrouter` on dev1, never printed or saved. The brief described deployment-related client timeouts, passing server health checks and rollback success; it requested read-only checks.

Five hypothesis tables passed on their first attempt. Grok exhausted both provider attempts without a usable output. The barrier recorded the skipped seat and moved to `MERGING`, where the shared outer judge adapter refused the work before sending a provider request. The invocation was interrupted after approximately **301.8 seconds**. No merge, selected test or human-result loop completed. No output-token-limit failure occurred.

The plan estimated **$9.208942**, reserved **$40.625187**, and allowed 29 calls / 63 attempts. Real provider cost was **$0.151144**: $0.001542 for four startup probes, $0.112316 for five accepted tables, and $0.037286 for Grok's two failed generations, verified through read-only generation lookups. The ledger had recorded $0.113858 and retained three unresolved records: the two Grok calls and the judge attempt started before the dispatch failure. After closing the temporary contest through the public router stop transaction offline, unused reservations were released; **$1.565995** remained reserved against those three records. No additional generation request or second mode send was made. This is a partial smoke, not a successful end-to-end run.

The initial evidence collector used the wrong attempt-column name after the process exited. Cost and outcome were reconstructed offline from SQLite and provider generation records; wall time is approximate from the invocation's log timestamps. Evidence is in `/tmp/squad-debug-smoke.e5wzca1q/` on dev2: `summary.json`, `generation-costs.json`, `debug.md`, `stdout.log` and `stderr.log`. The read-only CLI export exited zero. The smoke used source `30e7407`; later validation and recovery hardening was verified with fake-provider tests; no second paid smoke was run.

Excerpt from the exported partial report:

```text
# DEBUG
Result: stopped
No patch: the evidence does not yet isolate a cause.
Unanchored quote (not evidence): stale DNS
Skipped: grok · Hypothesizing · round 1

Table 1 · @muse
- port: Wrong target port in new client config · High
  Supporting: ["a wrong target port"]
  Contradicting: ["Health checks on the server pass."]
```

## Verification and integrator handoff

The seven owned test targets run 25 tests without live calls: protocol validation and parser examples; scoped store access, repairs and cascade deletion; executor contracts; leased workflow/accounting/recovery tests; hand-computed elimination and patch gates; export/reference validation; and TUI state/discovery checks. Workspace Clippy for all targets with warnings denied and formatting pass.

The final workspace gate recorded 1,339 passing executions, four existing manual ignores and 11 failures across eight shared targets: ten fixture/enablement failures plus the inherited tournament snapshot race. Shared fixtures still assume all scaffold modes are disabled, and the human-step fixture creates no debug run row. These files remain integrator-owned:

- `crates/squad-core/src/runtime/judge.rs`: include `grant.work.kind.new_mode_judge()` in `Executor::execute`'s direct-runner condition, so the already registered mode executor receives judge grants.
- `crates/squad-tui/src/typeahead.rs`: add a parser-checked debug example to `examples()` so idle TIP rotation includes it.
- `crates/squad-tui/src/composed.rs`: preserve differential-row styles instead of flattening every mode projection into strings, allowing eliminated rows to be greyed.
- `crates/squad-core/tests/human_step.rs`: seed `debugging_runs` with setup, differential, a round and selection before invoking the real debug answer callback.
- `crates/squad-core/tests/router_validation.rs`, `crates/squad-protocol/src/parser/new_modes_tests.rs` and `crates/squad-protocol/tests/contests.rs`: derive disabled-mode assertions from each mode's switch.
- `crates/squad-protocol/src/parser/examples.json`: update the two legacy `/export` usage examples for the enabled debug scope.
- `crates/squad-tui/tests/actions_shell.rs`, `crates/squad-tui/tests/contest_actions.rs` and `crates/squad-tui/tests/typeahead.rs`: update enabled-mode membership, export/help/menu counts and `$deb` prefix matches.
- `crates/squad-tui/tests/m4b_tournament_runtime_audit.rs`: wait for a committed `TournamentChampion` before snapshotting; waiting only for `TournamentResult` races the champion projection and intermittently unwraps `None`.

The mode switch is enabled and parser/help/completion tests pass in the owned targets. Production judge dispatch, TIP inclusion, grey row styling and a fully green workspace gate remain integration work. The paid smoke does not establish the human-result loop in the production room runtime.
