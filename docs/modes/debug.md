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
squad send --yes '@all $debug Requests time out after deploy -- Read-only checks only'
squad send '/answer O2 The resolver returned the old address'
squad export debug <contest-id> -o debug-report.md
```

The Markdown export includes the brief and limits, original hypothesis tables and attribution, all proposals and predictions, each selection, unanchored quotes, the full raw human answers, the differential and the report. Export while waiting includes the committed evidence and pending instruction. It contains no frozen provider prompts or credentials.

## Integration status

This branch keeps the exact mode ownership boundary. The scaffold's outer judge executor currently omits new-mode judge routing in `crates/squad-core/src/runtime/judge.rs`; the integrator must route `new_mode_judge()` to the already registered structured runner. The owned executor is exercised through real leased grants, scoped readers, accounting and fake OpenRouter calls in `debugging_workflows.rs`.

The shared typeahead inventory and help follow the mode switch. The shared idle TIP example list still needs a debug example. The shared string-only transcript adapter also needs a styling hook to grey eliminated differential rows; this branch renders their explicit eliminated status.

## Example run

Live smoke evidence will be recorded here after the release invocation. A run stopped at the shared judge dispatch seam cannot establish the complete live workflow.
