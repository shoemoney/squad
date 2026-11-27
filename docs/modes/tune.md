# Tune

`$tune` evolves a prompt against hidden cases, executes every candidate on one fixed model, and chooses a winner on unseen cases. Use it when prompt behavior can be measured against a task.

**Status: enabled.**

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

One release `squad send` ran on dev3 on 2026-11-26 with the default six-model roster, default executor (`anthropic/claude-fable-5.2`), `SQUAD_DAILY_MAX_USD=150`, `TUNE_GENERATIONS=1`, `--yes`, `--env-file /dev/null` and a fresh private data directory. The credential was fetched into the process environment off-screen from the documented aigate vault API, with its connection environment sourced privately (`aigate-key` was absent on this box); no real project `.env` was read.

```text
$TUNE_GENERATIONS=1 @all $tune Reply with one helpful sentence.
--
Classify a support message as exactly one lowercase label: billing for payments, refunds or invoices; technical for errors, access or setup; other for everything else. If billing and technical both apply, billing wins. Return only the label.
```

The CLI exited **0**, reason **`tuned`**, after **149.53 seconds**. Actual reported spend was **$0.520068** across **31 Tune work calls / 31 Tune provider attempts**. Tune work cost $0.518620; 4 startup capability probes added $0.001448, for 35 total provider attempts. The initial worst-case reservation was **$24.999033** for 72 attempts; the final reservation was **$0.000000**, with **0 unknown-cost calls**. The retained suite had 3 train and 3 held-out cases. Skipped work: 0.

| Work | Calls | Attempts | Actual reported cost |
|---|---:|---:|---:|
| `tune.cases` | 6 | 6 | $0.122862 |
| `tune.execute` | 18 | 18 | $0.088840 |
| `tune.freeze` | 1 | 1 | $0.093448 |
| `tune.mutate` | 3 | 3 | $0.057406 |
| `tune.score` | 3 | 3 | $0.156064 |
| Startup capability probes | 4 | 4 | $0.001448 |

Short excerpt from the actual exported report:

> Tuned: 0.14 to 1.00 on held-out cases.
>
> Reason: `tuned`

A conditional G=3 cost projection is **$0.827763**: the measured $0.520068, plus two generations of three mutations, 9 executions and one scoring call, priced using the observed per-work costs for those classes (all attempts included). This assumes the same 3 train cases, similar response lengths and retries, and no early saturation. It is an extrapolation, not another live run or a price guarantee. This run's train champion reached 1.00; if identical results recurred at G=3, saturation would skip the extra generations and keep the cost at the one-generation amount.

The generated suite is a smoke check of this task, not a general prompt-quality benchmark. Local evidence on dev3: [full report and reusable suite](/tmp/squad-tune-r2-live-7ainb5hq/report.md), [accounting summary](/tmp/squad-tune-r2-live-7ainb5hq/summary.json), [CLI records](/tmp/squad-tune-r2-live-7ainb5hq/stdout.jsonl) and [run metadata](/tmp/squad-tune-r2-live-7ainb5hq/metadata.json).

## Integration notes

The owner explicitly authorized the minimal shared-file exception needed for enablement. `runtime/judge.rs` forwards new-mode grants to the existing runner, including bare `tune.execute`. The default launcher and public parser/room submission path are exercised in interactive and one-shot workflows. Parser/TUI fixtures now use enabled inventories, Tune's segmented grammar and additive export usage while keeping the original parser snapshots unchanged. The tournament audit waits for the final champion event in its captured page, fixing a race between reading earlier results and newer terminal state.

The compatibility seams inherited from the original worker remain functional: scoped typed Tune JSON is carried in a standalone case envelope because the shared `CaseMaterial::Tune` validator is still disabled; repair sources freeze the literal step name because the shared renderer rejects that prefix in `mode_step`; structured member failures use `JUDGE_INVALID_MODE_OUTPUT` to enter D75 repair handling. No additional shared runtime change is required to run Tune. Optional integration cleanup:

- `crates/squad-protocol/src/envelopes.rs`: validate `CaseMaterial::Tune` instead of rejecting it, then replace the owned standalone carrier with `CaseMaterial::Tune(input)`.
- `crates/squad-core/src/prompts/render.rs`: accept the new-mode prefixes in `mode_step`, then remove the frozen-literal repair workaround.
- The integrator now classifies `MODE_INVALID_OUTPUT` as `FailureClass::InvalidOutput` alongside the existing judge diagnostic. The owned Tune compatibility diagnostic remains accepted.

Authorized shared changes in this branch are `crates/squad-core/src/runtime/judge.rs`; the protocol parser fixtures `new_modes_tests.rs` and `parser_tests.rs`; and the TUI fixtures `actions_shell.rs`, `contest_actions.rs`, `typeahead.rs` and `m4b_tournament_runtime_audit.rs`.

## Validation

On dev3, 2026-11-26, implementation/test head `4682a4ec3645f9bf1d51ca27535458ac26fe9967` passed with `CARGO_BUILD_JOBS=4`:

- `cargo test --workspace --no-fail-fast`: **1,360 unique passing tests**, zero failures, 4 ignored entries. Cargo prints 1,361 passing summary entries because the WAL child repeats one test.
- `cargo clippy --workspace --all-targets -- -D warnings`.
- `cargo fmt --all -- --check` and `git diff --check`.
- The workspace release gate built `cargo build --release --locked -p squad-cli` and exercised the optimized CLI. The paid smoke used that release binary.

There are **35 Tune checks**, including **12 production-launcher workflows**; eight checks were added during the resume. Captures verify mutation/scoring privacy and bare executor requests; fixtures cover repairs, skipped members with failed step names, proposal quorum, failed judges/executions, partial case drops, incomplete and empty generations, perfect-baseline/no-improvement exits, named-executor completion, one-shot driving, export and ledger conservation.

Restart and stop checks cover preflight, waiting for earlier ordinary replies, all seven paid phases (including both held-out phases), accepted execution pairs, immutable setup and accepted sheet maps. Reporting and completion share one transaction; reopening a completed run preserves its export and makes no new provider call. Stop releases unstarted reservations while preserving accounted holds for in-flight calls whose billing remains unknown. The failed-step journal field defaults to empty for earlier saved runs. Enabled parser, typeahead, TIP rotation and help checks pass.

## Integrated release smoke (2026-11-26)

Release 0.2.0 source `c7c20ff` ran once on dev2 with the built-in six-member
roster. CLI exit 0, `COMPLETE`, `tuned`; 34 planned mode calls and
35 total provider attempts including startup. Spend was $0.522077, observed
wall time 134.272 seconds, and initial/sample-peak reservation
$24.970323/$24.970323. Final reserved funds were $0.000000 and unknown
calls 0. Credential-free offline export passed. Evidence: `/tmp/squad-v020-tune-voxbepir/`;
see the [integration record](../live-smoke-2026-11-26-v0.2.0.md).
