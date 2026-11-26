# Tune

`$tune` evolves a prompt against hidden cases, executes every candidate on one fixed model, and chooses a winner on unseen cases. Use it when prompt behavior can be measured against a task.

**Status: implemented behind a disabled switch, pending the shared production judge dispatch fix described below.** The commands describe the enabled mode after integration.

```text
@all $tune Answer clearly and concisely.
--
Answer support questions accurately, state uncertainty, and avoid invented facts.
```

Put each `--` on its own line. An optional third segment chooses the model to optimize for:

```text
$TUNE_GENERATIONS=1 @all $tune Summarize the following input: {{input}}
--
Summarize faithfully in three sentences, preserving important qualifications.
--
@fable
```

Without an executor segment, the judge's resolved model executes all prompts. A named executor leaves the participant set and contributes no cases or mutations. Select three to twelve other members. The original prompt is limited to 1,200 words, with at most one `{{input}}` token. A quoted message can supply the prompt when no inline prompt precedes the first separator. `/set TUNE_GENERATIONS=3` controls the default; the range is 1–6 and admission freezes it.

1. Members independently propose up to three cases and three criteria. At least two proposals and four cases must survive validation.
2. The judge selects four to six existing cases, at most two per member, and freezes two to four sourced criteria. It invents no case. The app privately seeds and persists the split: two or three train cases and two or three held-out cases.
3. The fixed executor runs the original on train cases. The blind judge scores its outputs from 0 to 4 per criterion. A perfect baseline ends immediately.
4. Three members per generation write mutually blind mutations. They see only the champion, task and a digest of train labels, criterion names, scores and short notes. They never receive cases, expectations, rubric descriptions, executor outputs or held-out information. The app rejects notes that copy six consecutive hidden words.
5. The executor runs each candidate/case pair once, retaining the output across later sheets and restarts. Each judge sheet rescans the original, champion and complete variants together under fresh anonymous letters. A variant promotes only with a gain of at least 0.05 in that same sheet. Ties prefer shorter prompts, then the persisted seed.
6. The original and at most two recent champions run on held-out cases. A variant wins only if it beats the original there by at least 0.05. Ties prefer the earlier champion. No promotion means no held-out pass.

Executions are bare: one user message with `{{input}}` substituted, or the candidate as the system message and case input as the user message. Neither receives a Squad template, persona, history or redaction. The output ceiling is 1,024 visible tokens. A cut-off answer is retained and marked truncated. Executors get two plain attempts; structured member and judge calls follow D75, including one repair for invalid output. Missing baseline cases are dropped; fewer than two remaining train cases fails. Incomplete variants cannot promote. Held-out cases missing for any finalist are dropped for all finalists; no usable held-out case fails explicitly.

`/stop` fences outstanding work. Restart preserves the admitted prompt, models, generation limit, seed, split, rotation, anonymous maps and accepted outputs. Ordinary participant work submitted during a tune contest is held until the contest ends.

The human report includes the winning prompt verbatim, a unified line diff, lineage and stated changes, train and held-out score tables, regressions, generation scores, remaining failures and skipped work. `/export tune [contest-id]` adds the reusable suite with its split, every proposal, candidate, execution, scoring sheet with private label maps, and digest. Export contains the hidden suite for the human; do not send that archive to subsequent prompt authors.

Six members at the default three generations make at most **60 calls**: 15 member, 39 executor and 6 judge calls, reserving **126 attempts**. A six-member one-generation smoke is capped at 34 calls and 72 attempts. Three members at one generation are capped at 31 calls and 66 attempts. Actual cost depends on token prices and output lengths. Preflight prices executor calls separately; reservations are worst-case holds and unused amounts are released as the suite shrinks, generations empty or the run ends.

## Example run

No live result or measured dollar cost is claimed. The production wrapper rejects new-mode judge work before the first tune freeze provider call, so the approved live smoke cannot exercise the mode until integration. The fake-provider workflow checks held-out promotion, original retention, perfect-baseline and no-improvement exits, failed proposals/judges/executions, repair, stop, restart and ledger conservation. Its scores and costs are fixtures, not live evidence. At default G=3 the call cap is 60 versus 34 at G=1; **1.765× is a call-count ratio, not a measured cost projection**.

## Integration requirements

Only tune-owned files and tune's milestone tasks were edited.

- In `crates/squad-core/src/runtime/judge.rs`, `Executor::execute` must route `grant.work.kind.new_mode_kind().is_some()` through `self.crosscheck.execute`, including `TuneExecute`. `new_mode_judge()` alone misses the bare executor, whose target is also a case. The downstream runner already dispatches to `runtime::tune::execute`. The legacy `JudgeExecutor` currently rejects these grants and leaves them running.
- In `crates/squad-protocol/src/envelopes.rs`, replace the unconditional rejection of `CaseMaterial::Tune(_)` with validation of the tune material. Then replace the standalone carrier in `router/tune.rs::allocate` with `CaseMaterial::Tune(input)`. The current compatibility carrier stores the same typed JSON with its scoped source step, preserving the capability boundary.
- In `crates/squad-core/src/prompts/render.rs`, allow new-mode prefixes in the `mode_step` placeholder validator. Tune currently freezes the literal step name in the repair source and removes that unused slot before rendering, so repairs work without this shared edit.
- In `crates/squad-core/src/router/executor.rs`, treat `MODE_INVALID_OUTPUT` as `FailureClass::InvalidOutput`. Tune uses the already recognized `JUDGE_INVALID_MODE_OUTPUT` for both member and judge structured validation so D75 repair policy works.

After the judge dispatch change, enable `squad_protocol::tune::ENABLED`, switch the workflow fixture to the default launcher, verify parser/typeahead/help, rerun all gates, and run the approved release `squad send` with the default six-model roster, `SQUAD_DAILY_MAX_USD=150`, `TUNE_GENERATIONS=1`, `--env-file /dev/null` and a fresh private data directory. Fetch the credential directly into the process environment with `aigate-key openrouter`; never print it. Record measured spend, wall time, initial reservation, final reservation and report excerpt here. Project default-G cost using per-step measured class costs and the extra two generations, explicitly qualifying that estimate.

## Validation

On dev3, 2026-11-26, with `CARGO_BUILD_JOBS=4`, implementation commit `28f9c57d7a5958cc3988c6622319b65057ee20c5` passed:

- `cargo test --workspace --no-fail-fast`: 1,352 unique passing tests, zero failures, four ignored entries. Cargo prints 1,353 passing summary entries because the WAL subprocess repeats one test.
- `cargo clippy --workspace --all-targets -- -D warnings`.
- `cargo fmt --all -- --check`.
- The workspace's release CLI gate built `squad` with `cargo build --release --locked -p squad-cli` and passed its release behavior checks.

Twenty-seven tune tests were added across protocol, store, agents, core workflow/tally/export, blinding and TUI. The public mode remains disabled. Default-launcher acceptance, the remaining held-out/report crash boundaries, parser/typeahead/help enablement and the real live smoke remain integration work; green injected-runtime tests do not substitute for those checks.
