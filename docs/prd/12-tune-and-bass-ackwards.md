# Two more contest modes: tune, bass_ackwards_prd

### Part 1: Specification

### Scope, ownership and naming

This section specifies two contest modes added after the nine in [New modes](10-new-modes.md): `$tune` and `$bass_ackwards_prd`. The names are final (D88). Cross-references call this section **Tune and bass_ackwards**.

Everything in [Contests](08-contests-debate-tournament-grill-census-crosscheck-survey-probe.md) and every shared contract in **New modes** still applies unless this section says otherwise: the shared engine and its moves, one active-contest claim per room, structural blindness, blind text redaction, preflight and admission, participant holds, the daily cap, `/stop`, restart recovery, the implicit judge, the D75 attempt policy, strict generated objects, barriers, private randomness, the `verbatim` matcher, the reservation sizing notation, the `contest.artifact` carrier (D74), the `ENABLED` switch, and the human step (D76). The two modes are scripts over the same moves. They add two capabilities, both specified below: the **executor call** (`$tune`) and the **automatic save** (`$bass_ackwards_prd`).

Ownership follows **New modes**:

- **Tune and bass_ackwards (this section):** the two scripts, their statuses, typed payloads, aggregation maths, call counts, caps, the human step of `$bass_ackwards_prd`, the saved file and the reports.
- **Command grammar and parser:** the two verbs, their argument shapes and every exact error string. This section proposes the strings; once landed, the parser fixtures are normative.
- **Prompt pack:** prompt text. This section names each template, its output and its word budget.
- **Protocol, store and bus:** carriers, `Work`, storage schema and visibility rows.

Vocabulary is that of **New modes**: m is the number of participating members; *accepted*, *skipped* and *label* mean what they mean there. G is the frozen value of `TUNE_GENERATIONS`.

### When to use which mode

| Need | Mode | Why not an existing mode |
|---|---|---|
| A prompt or system instruction that measurably behaves better, checked on cases its authors never saw | `$tune` | `$forge` fuses text and is judged on opinion; no other mode executes its artifact. |
| A PRD for a stated end result, derived backward from that result and saved as a file | `$bass_ackwards_prd` | `$grill` and `$census` improve a forward plan that already exists; none starts from the finished state or defines done first. |

### Shared contracts for the two modes

**Lifecycle, judge, attempts, limits, generated objects, barriers, private randomness, report and export** are exactly as in **New modes → Shared contracts for the nine modes**. `@judge` in a start command is refused with `@judge cannot be used with ${mode}.`

**Additional limits.** `V1` is 1,024 visible tokens (`TUNE_EXEC_VISIBLE_TOKENS`, a constant, not a room variable). `V8` is 8,192 visible tokens (`PRD_VISIBLE_TOKENS`, a constant). `W_prd` is 2,400 words, the cap on the prose of one PRD version.

**The executor call (D90).** `$tune` runs candidate prompts on test inputs. That call is a third work class beside member and judge calls:

| Call | Deadline and attempts |
|---|---|
| Executor call (`tune.execute`) | 180 s; 2 attempts. Both are plain attempts: the output is free text, nothing is validated except that it is not empty, and there is no repair. After two failures the execution is `missing`. |

An executor request is bare. It carries no Squad system prompt, no persona, no template, no room history, no redaction and no hint that it is a test. If the candidate prompt contains the token `{{input}}`, the request is one user message: the prompt with the token replaced by the case input. Otherwise the candidate prompt is the system message and the case input is the user message. Both are sent byte for byte. The visible cap is `V1`. An output cut off at the cap is accepted and marked `truncated`.

Reserved attempts for the two modes are `2 × member calls + 2 × executor calls + 3 × judge calls`.

### Registry changes and scaffold

One scaffold worker lands every shared-file change first (milestone phase M9.P1), on top of the M8 scaffold. After that each mode worker edits only the files it owns. A mode worker that finds it needs a shared-file change reports it to the orchestrator.

**Rust identifiers.**

| Verb | Wire name | Module name | Rust prefix | Note |
|---|---|---|---|---|
| `$tune` | `tune` | `tune` | `Tune` | |
| `$bass_ackwards_prd` | `bass_ackwards_prd` | `bass_ackwards_prd` | `BassAckwardsPrd` | `rename_all = "lowercase"` would give `bassackwardsprd`, so every enum carries an explicit `#[serde(rename = "bass_ackwards_prd")]`, as `blow_it_up` does. |

**What the scaffold lands.** The same registries as **New modes → What the scaffold lands**, for two modes:

| Registry | Change | File |
|---|---|---|
| `ContestKind`, `ContestState`, `ContestTransition`, `CaseMaterial` | Two variants each, with the explicit rename. | `crates/squad-protocol/src/contest.rs`, `envelopes.rs` |
| Status enums, state structs, enable switch | `TuneStatus` and `BassAckwardsPrdStatus` exactly as listed below, `<Mode>State` with `validate()`, and `pub const ENABLED: bool = false;`. | `crates/squad-protocol/src/tune.rs`, `bass_ackwards_prd.rs` (created by the scaffold, then owned by the mode) |
| Envelope kinds | `<mode>.setup`, `<mode>.state` and `<mode>.preflight` for both. Artifacts use the existing `contest.artifact`; the human step uses the existing `contest.human_requested` and `contest.human_answered`. | `crates/squad-protocol/src/envelopes.rs` |
| `WorkKind` | The kinds in the table below. `tune.execute` is a new executor class: `attempt_limit` 2, no repair, its own deadline class, bare request composition. | `crates/squad-protocol/src/work.rs`, `constants.rs`; `crates/squad-agents/src/agents/mod.rs` |
| Variables (D89) | `TUNE_GENERATIONS` (default 3, 1 to 6) and `PRD_HUMAN_REVIEW` (default 0, 0 or 1) in `VariableName`, `/set`, inline `$NAME=n`, `SquadConfig` and the optional `SQUAD_<NAME>` seeds. Not added to `.env.example`. An admitted contest keeps its frozen value. | `crates/squad-protocol/src/parser/types.rs`, `parser.rs`, `config.rs`; `crates/squad-core/src/router/variables.rs`; `crates/squad-cli/src/env-help.md` |
| Parser verbs and AST | Two verbs and two `ParsedInput` variants with the shapes in the grammar table; `is_contest` and `input()` arms. | `crates/squad-protocol/src/parser/types.rs`, `parser.rs`, `errors.rs` |
| `ExportScope` | `tune` and `bass_ackwards_prd`; `/export` usage strings; scope-to-kind mapping. | `crates/squad-protocol/src/parser/types.rs`, `parser.rs`; `crates/squad-core/src/export/mod.rs`; `crates/squad-cli/src/args.rs` |
| Parser fixtures | `examples.json` cases for every form in the grammar table, one refusal per string, the busy error for each kind, and the two exports. Existing cases and snapshots stay unchanged. | `crates/squad-protocol/src/parser/examples.json`, `validation_strings.json`, `validation_codes.json`, `parser_tests.rs`, `round_trip_tests.rs` |
| Store | Registered migrations `0019_tune.sql` and `0020_bass_ackwards_prd.sql`. Each carries its mode's kind and work-kind CHECK extensions and variable row from the scaffold; the mode worker then adds its own tables to the same file. Stub store modules exported from `lib.rs`. | `crates/squad-store/migrations/`, `src/schema.rs`, `src/lib.rs` |
| Engine dispatch | Match arms to `tune::` and `bass_ackwards_prd::` entry points with the M8 signatures, each a stub. | the M8 dispatch files in `crates/squad-core/src/router/`, `orchestrator/`, `runtime/`, `export/` |
| Executor path | Bare request composition and dispatch for the executor class, with the frozen executor model. | `crates/squad-core/src/router/executor.rs`, `crates/squad-agents/src/agents/mod.rs` |
| Export directory | `export_dir(data_dir) -> <data_dir>/exports`, created on first use with mode `0700`, and an atomic `write_export_file`. | `crates/squad-core/src/export/mod.rs` |
| Prompt registration | Every template named below, pointing at placeholder files. | `crates/squad-core/prompts/manifest.json`, `src/prompts/embedded.rs`, `provenance.rs`, `inputs.rs`, `render.rs`, `mod.rs` |
| TUI | Two `MODES` entries and one TIP example, gated on `ENABLED`; view maps; widget and fx registration; banner words; the preflight call-count line. | the M8 TUI registration files; `crates/squad-core/src/banner/mod.rs` |
| CLI | Two export scopes; `squad send` prints the saved path for `bass_ackwards_prd` and emits `export.ready` with `scope`, `filename` and `path` in JSON mode. | `crates/squad-cli/src/args.rs`, `send.rs`, `output.rs`, `adapter.rs` |
| MCP (D100) | No change. | none |

**Work kinds.** Member kinds first, the executor kind underlined, judge kinds in italics.

| Mode | Work kinds |
|---|---|
| tune | `tune.cases`, *`tune.freeze`*, <u>`tune.execute`</u>, *`tune.score`*, `tune.mutate` |
| bass_ackwards_prd | `bass_ackwards_prd.done`, *`bass_ackwards_prd.freeze`*, `bass_ackwards_prd.tree`, *`bass_ackwards_prd.merge`*, `bass_ackwards_prd.review`, *`bass_ackwards_prd.revise`*, `bass_ackwards_prd.must_have`, *`bass_ackwards_prd.finalize`*, *`bass_ackwards_prd.apply`* |

**Start grammar.**

| Mode | Forms | AST fields | Members |
|---|---|---|---|
| tune | `@all $tune <prompt to optimise>` `--` `<task description>`, and with a third segment `--` `@nick` naming the executor. If the text before the first `--` is empty and exactly one message is quoted, the quoted message is the prompt. | `prompt`, `task`, `executor: Option<Nick>`, `generations` | 3 to 12, not counting a named executor |
| bass_ackwards_prd | `@all $bass_ackwards_prd <end result>`, and with `--` `<where things stand today: resources, constraints, dates>` | `endResult`, `today: Option` | 3 to 12 |

The scaffold's provisional forms are kept. The two refinements are additive: the optional executor segment for `$tune` and the optional starting-point segment for `$bass_ackwards_prd`. A prompt that contains a line starting with `--` cannot be typed inline; quote it instead.

**Proposed parser strings.**

| Rule | String |
|---|---|
| Count | `${mode} needs at least three members.` |
| Count with executor | `$tune needs at least three members besides the executor.` |
| Judge refused | `@judge cannot be used with ${mode}.` |
| Missing text | `Add the prompt to optimise after $tune.` · `Add the task after -- for $tune.` · `Add the end result after $bass_ackwards_prd.` |
| Tune segments | `$tune takes the prompt, then -- and the task, then an optional -- @nick executor.` |
| Tune executor | `The $tune executor must be one roster member, like -- @nick.` |
| Tune prompt size (admission) | `$tune takes a prompt of at most 1,200 words.` |
| Tune placeholder (admission) | `$tune allows {{input}} at most once in the prompt.` |
| Variables | `TUNE_GENERATIONS must be an integer from 1 to 6.` · `PRD_HUMAN_REVIEW must be 0 or 1.` |
| PRD note | `Add your note after /answer, or use /continue to save the PRD as it is.` |

**Typeahead and TIP lists.** `MODES` gains, after `$rank`:

| Token | Hint |
|---|---|
| `$tune` | Evolve a prompt against a hidden test set |
| `$bass_ackwards_prd` | Plan backward from the end result into a saved PRD |

`examples()` gains `@all $bass_ackwards_prd our CLI is installed by 1,000 teams and has a paid tier`, kept only when it parses in the live context and the mode is enabled.

**Files each mode worker owns.** The table in **New modes → Files each mode worker owns**, with `<module>` `tune` or `bass_ackwards_prd`. Migration numbers: tune 0019, bass_ackwards_prd 0020. `$tune` has no template for `tune.execute`.

### Tune execution

```rust
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TuneStatus {
    Preflight,
    WaitingForParticipants,
    BuildingCases,
    Freezing,
    Executing,
    Scoring,
    Mutating,
    HoldoutExecuting,
    HoldoutScoring,
    Reporting,
    Complete,
    Stopped,
    Failed,
}
```

- `TuneSetup` — Frozen original prompt, task, participants, executor model, G, length budget and the mutator rotation.
- `TuneCaseProposal` — One member's blind test cases and rubric criteria.
- `TuneSuite` — The frozen cases, the frozen rubric, the train and held-out split and private provenance.
- `TuneCandidate` — One prompt: the original, or a mutation with its parent, generation, author and stated changes.
- `TuneExecution` — One executor output for one candidate on one case, or `missing`.
- `TuneScoreSheet` — The judge's scores for one pass, with the per-case label maps.
- `TuneDigest` — The failure digest delivered to one generation's mutators.
- `TuneGeneration` — App-computed scores, the generation's best candidate and the promotion result.
- `TuneHoldout` — App-computed held-out scores and the final pick.
- `TuneReport` — The winning prompt, the diff, the score tables and the lineage.
- `TuneCaseMaterial` — Step-scoped freeze or scoring inputs.

`@all $tune <prompt> -- <task>` improves a prompt or system instruction by measurement. The members first build a test set that no prompt author is ever shown. The app splits it into train cases and held-out cases. Then, for G generations, three members each write one blind mutation of the current champion, a fixed executor runs every candidate on the train cases, and a blind judge scores the outputs against the rubric. The best candidate of a generation replaces the champion only if it measurably beats it. The final pick is made on the held-out cases only. `TUNE_GENERATIONS` (G, default 3) bounds the loop.

**The executor (D92).** By default the executor is the judge's resolved model, called bare as specified above. Every output in a contest comes from that one model, and the judge scores outputs without knowing which candidate produced them, so any preference the judge has for its own model's style is the same for every candidate and cannot move the comparison. A third segment `-- @nick` names a roster member as the executor instead; use it to tune a prompt for the model that will run it. A named executor is removed from the participants, is not counted in m, and writes no case and no mutation. The executor's model is frozen in `TuneSetup`.

**Cost caps (D93).** These are constants. None is a variable.

| Cap | Value |
|---|---|
| Cases a member may propose | 3 |
| Rubric criteria a member may propose | 3 |
| Frozen cases `C` | 4 to 6 |
| Held-out cases `H` | `min(3, ⌊C / 2⌋)`: 2, 2 or 3 |
| Train cases `T` | `C − H`: 2 or 3 |
| Frozen rubric criteria | 2 to 4 |
| Mutators, and so new candidates, per generation `K` | 3 |
| Generations `G` | 1 to 6 |
| Finalists on the held-out cases `F` | at most 3, the original included |
| Executor calls | at most `T + 3GT + 3H`, so at most `9G + 12`: 39 at the default, 66 at G of 6 |
| Executor output | `V1` (1,024 visible tokens) |
| Original prompt | 1,200 words |
| Candidate prompt | `Lmax = min(1,800, max(⌈1.5 × W0⌉, W0 + 150))` words, where `W0` is the original's word count |
| Case input | 150 words |

No candidate is ever executed on a case twice. Executions are stored under `(candidateId, caseId)` and reused in later sheets and after restart.

| State | Move and actors | Visibility, template and output | Completion and failure |
|---|---|---|---|
| `BUILDING_CASES` | `fanout` to every participant, fresh context. | Original prompt and task only; `tune.cases.md`; `{cases: [1 to 3 {localId, kind, input, expect: [1 to 3], mustNot: [0 to 2]}], criteria: [1 to 3 {localId, name, description}]}`. `kind` is `typical`, `edge` or `adversarial`. `input` is at most 150 words; each `expect` and `mustNot` at most 25 words; `name` at most 6 words; `description` at most 40 words. | Barrier. Fewer than two accepted proposals, or fewer than four cases in total, fails with `insufficient_cases`. |
| `FREEZING` | *Blind judge.* | Prompt, task and the proposals under labels; `tune.freeze.md`; `{cases: [4 to 6 caseRef], dropped: [{caseRef, why}], rubric: [2 to 4 {name, description, sourceRefs}]}`. | The judge selects and removes duplicates. It writes no case and edits none: a frozen case is byte-identical to its accepted proposal. At most two frozen cases may come from one member. Every criterion needs at least one source reference. Judge exhaustion fails with `judge_failed`. |
| Split | `tally`. | Shuffle the frozen cases with the persisted private seed; the first `H` are held out, the rest are train. Mint opaque case labels `T1…` and `H1…`. | Persist `TuneSuite` before any execution. The judge has no say in the split. |
| `EXECUTING`, generation 0 | Executor, one call per train case. | The original prompt and one case input, bare. | A train case whose baseline execution is `missing` is dropped from the train set and reported. Fewer than two train cases left fails with `insufficient_cases`. |
| `SCORING`, generation 0 | *Blind judge.* | Task, rubric, each train case with its input, `expect` and `mustNot`, and the original's output under a label; `tune.score.md`; the sheet schema below. | Baseline score `S(c0)`. If it is 1.0 the mode completes with reason `nothing_to_fix` and no mutation. Judge exhaustion fails with `judge_failed`. |
| `MUTATING`, each generation | `fanout` to that generation's three mutators, fresh contexts, mutually invisible. | Task, the current champion prompt, the champion's `TuneDigest`, `Lmax` and the placeholder rule; `tune.mutate.md`; `{prompt, changes: [1 to 5 {what, why, targets: [caseLabel]}]}`; 4,096 visible tokens. | A mutation is invalid when it is over `Lmax`, equals its parent after whitespace collapsing, or has a different count of `{{input}}` than the original. Invalid output is repaired once. A skipped mutator contributes no candidate. With no accepted candidate the generation is recorded as empty and the loop continues. |
| `EXECUTING`, each generation | Executor, one call per new candidate and train case. | One candidate prompt and one case input, bare. | A candidate with any `missing` train execution is `incomplete`: it is not scored and cannot be promoted. |
| `SCORING`, each generation | *Blind judge.* | Task, rubric, each train case, and for each case the stored outputs of the original, the champion and every complete new candidate, under letters shuffled per case; `tune.score.md`. | Then `tally`: scores, the generation's best, promotion. Judge exhaustion fails with `judge_failed`; everything committed stays exportable. |
| `HOLDOUT_EXECUTING` | Executor, one call per finalist and held-out case. | One finalist prompt and one held-out input, bare. | A held-out case on which any finalist is `missing` is dropped for all finalists. No held-out case left fails with `holdout_unavailable`. |
| `HOLDOUT_SCORING` | *Blind judge.* | Task, rubric, each held-out case, and the finalists' outputs under letters shuffled per case; `tune.score.md`. | Then `tally`: the final pick. Judge exhaustion fails with `judge_failed`; the report says the train champion was not verified. |
| `REPORTING` | `tally`, then `report`. No model call. | Assemble from committed records. | Commit `tune.report`, enter `COMPLETE`, release resources. |

**What a prompt author never sees.** No member step after `BUILDING_CASES` receives a case input, an `expect` or `mustNot` line, a rubric description, an executor output, another member's cases, another candidate, or any held-out information at all. Each step runs in a fresh context, so a member's own cases are not in its mutation request either. The suite's audience is the human and the judge case. The one channel from the test set to the authors is the digest.

**Score sheet.** `{cases: [{caseLabel, outputs: [{label, scores: [{criterion, score}], note}]}]}`. Every supplied case, label and criterion appears exactly once. `score` is an integer from 0 to 4, with the anchors fixed in the template: 0 fails the criterion, 2 meets it in part, 4 meets it fully. `note` is at most 30 words, describes what the output did, and is required when any of that output's scores is 2 or lower. The judge never sees a candidate prompt, a nick, a generation number, or which output belongs to the original or the champion. A truncated output carries the marker `[cut off at the length cap]`.

**Leak check.** A note that shares six or more consecutive words with its case's input, `expect` or `mustNot` lines, or with a rubric description, is invalid, and the sheet is repaired once. The comparison uses the `verbatim` normalisation and is application code.

**Digest.** For the champion only, per train case: the case label, the kind, each criterion's name and score from the latest sheet, and the note. Nothing else. Held-out cases never produce a digest.

**Scores.** With `R` frozen criteria, the case score is `q(x, c) = Σ scores / (4R)`, from 0 to 1. A candidate's score in a sheet is `S(x)`, the mean of `q` over that sheet's cases. Values are stored unrounded and shown to two decimals. Scores from different sheets are never compared: the original's and the champion's stored outputs are scored again inside every sheet, so each comparison is made within one judge call.

**Promotion.** The generation's best `b_g` is the complete new candidate with the highest `S`. Ties break by fewer words, then the persisted seed. `b_g` becomes champion only when `S(b_g) − S(champion) ≥ 0.05` in that sheet; otherwise the champion stands. `b_g` is recorded for every generation whether or not it was promoted. If the champion's `S` in a sheet is 1.0 after promotion, the remaining generations are skipped and released.

**Mutator rotation.** The participants are shuffled once with the private seed. Generation g takes the next three in that order, wrapping around, so turns differ by at most one across the room.

**Finalists and the final pick.** The finalists are the original `c0`, the final champion, and the champion before it when that is not the original. If no candidate was ever promoted, there is no held-out pass and the mode completes with reason `no_improvement`. Otherwise the held-out sheet gives `Sh(x)`. A finalist other than the original wins only when `Sh(x) − Sh(c0) ≥ 0.05`. If two qualify the higher `Sh` wins, and a tie goes to the earlier champion. The reason is `tuned`. If none qualifies, the original is kept with reason `original_kept` and the report says the train gain did not carry to unseen cases. Train scores nominate finalists and decide nothing else.

**Judge limits.** The judge selects cases, freezes the rubric and fills score sheets. It writes no case, no prompt and no mutation, never sees a prompt it is scoring the outputs of, and has no field in which to name a winner. Scores, promotion, the split and the final pick are application code.

**Calls.** At most `m + T + 3H + 3 + G(3T + 4)`: m case proposals, one freeze, `T` baseline executions and one baseline sheet; per generation three mutations, `3T` executions and one sheet; then at most `3H` held-out executions and one held-out sheet. Preflight reserves at the caps `T = 3` and `H = 3`:

- Normal calls: `m + 13G + 15`. Six members at the default G of 3 cost at most 60: 15 member calls, 39 executor calls and 6 judge calls.
- Reserved attempts: `2(m + 3G) + 2(9G + 12) + 3(G + 3) = 2m + 27G + 33`. Six members at G of 3 reserve 126.

A frozen set smaller than the caps, an early end and an empty generation release what they do not use. Preflight shows the three call classes separately, because an executor call is small: one prompt, one case input and at most `V1` out.

| Step | Calls × attempts | Data bound |
|---|---|---|
| `tune.cases` | m × 2 | `J(prompt) + J(task)` |
| `tune.freeze` | 1 × 3 | `B(prompt) + J(task) + m · P(L(V2, W_room))` |
| `tune.execute` | (9G + 12) × 2 | `L(V4, Lmax) + L(V2, 150)`, no template bytes |
| `tune.score` | (G + 2) × 3 | `J(task) + 4 · L(V2, 50) + 3 · L(V2, 250) + 15 · A(V1)` |
| `tune.mutate` | 3G × 2 | `J(task) + L(V4, Lmax) + 3 · L(V2, 100)` |

**Prompts.** `tune.cases.md` (output `W_room`), `tune.freeze.md` (`W_room`), `tune.score.md` (600), `tune.mutate.md` (`Lmax` for the prompt plus 150 for the changes). There is no template for `tune.execute`.

**Report.** The result line and reason; the winning prompt, byte-identical to its accepted artifact; a unified line diff from the original, computed by the app; what changed, as the stated `changes` of every mutation in the winner's lineage with author and generation; the held-out table, original against winner per case and criterion, from the one held-out sheet; the train table, original against winner per case, from the last train sheet; cases where the winner scores at least 0.25 lower than the original, listed as regressions; the score of the champion and of `b_g` per generation; what the winner still fails, from the held-out notes; dropped cases, incomplete candidates and skipped members. `/export tune [id]` adds every proposal, the whole frozen suite with its split so that it can be reused, every candidate, every executor output, every sheet with its label maps and every digest.

**TUI.** Banner `TUNE`; dividers `TEST SET`, `BASELINE`, `GENERATION n`, `HELD OUT`, `RESULT`. The test set is collapsed by default and shown to the human only. Each generation shows one row per candidate with its score and a mark on the promoted one. Execution progress shows as a count. The result line reads `Tuned: 0.58 to 0.81 on held-out cases.`, `The original stands. The best variant did not hold up on unseen cases.`, `The original stands. No variant beat it.` or `Nothing to fix: the original already passes every train case.`

**Acceptance tests.**

- Six members at G of 3 with six frozen cases make at most 60 calls, of which 39 are executor calls, and reserve 126 attempts. Three members at G of 1 make at most 31 and reserve 66.
- With four frozen cases the split is 2 and 2, with five it is 3 and 2, with six it is 3 and 3; the same seed gives the same split after restart.
- No captured `tune.mutate` request contains a case input, an `expect` or `mustNot` line, a rubric description, an executor output, a held-out label or another candidate.
- A captured executor request contains exactly the candidate prompt and the case input: no template, persona, room history or redaction. With `{{input}}` it is one user message.
- No captured `tune.score` request contains a prompt, a nick or a marker for the original or the champion; label order differs between cases.
- A note that copies six consecutive words of its case is rejected and the sheet repaired once.
- A frozen case is byte-identical to its proposal; a freeze output with three cases from one member, an invented case or a criterion with no source is rejected.
- Promotion fixtures: a gain of 0.04 does not promote, 0.05 does, each tie-break level, an incomplete candidate, an empty generation.
- Final-pick fixtures: `tuned`, `original_kept`, two qualifying finalists, a tie, a dropped held-out case, `holdout_unavailable`.
- A baseline of 1.0 ends with `nothing_to_fix` after `m + T + 2` calls; no promotion in any generation ends with `no_improvement` and makes no held-out call.
- No execution is repeated for a stored `(candidateId, caseId)`, in a later sheet or after restart.
- A named executor writes no case and no mutation and is not counted in m; with `@all` and three other members the mode starts, with two it is refused.
- Stop and restart at every state repeat no accepted step and redraw no seed, split, rotation or label.

### Bass_ackwards_prd execution

Rust identifiers use the `BassAckwardsPrd` prefix and the module name `bass_ackwards_prd`.

```rust
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BassAckwardsPrdStatus {
    Preflight,
    WaitingForParticipants,
    DefiningDone,
    FreezingDone,
    BuildingTrees,
    Merging,
    Reviewing,
    Revising,
    MustHaves,
    Finalizing,
    AwaitingHuman,
    Applying,
    Checking,
    Saving,
    Complete,
    Stopped,
    Failed,
}
```

- `BassAckwardsPrdSetup` — Frozen end result, starting point, participants, the human-review flag and the lens rotation.
- `BassAckwardsPrdDoneList` — One member's blind definition-of-done items.
- `BassAckwardsPrdDone` — The frozen definition of done, `D1` to `Dn`, with support counts and private provenance.
- `BassAckwardsPrdTree` — One member's blind precondition tree.
- `BassAckwardsPrdDocument` — One PRD version: overview, milestones, phases, line items with dependencies and DoD mapping, provenance.
- `BassAckwardsPrdReview` — One member's lens comments, with the app's quote check per comment.
- `BassAckwardsPrdMustHaves` — One member's must-have additions.
- `BassAckwardsPrdDecisions` — The judge's accept or reject for every comment or addition of one round, each with a one-line reason.
- `BassAckwardsPrdChecks` — App-computed check results, critical path and parallel levels.
- `BassAckwardsPrdSaved` — File name, path, byte count and SHA-256 of the saved file.
- `BassAckwardsPrdReport` — The final PRD, the decision logs, the checks and the path.
- `BassAckwardsPrdCaseMaterial` — Step-scoped freeze, merge, revise, finalize or apply inputs.

`@all $bass_ackwards_prd <end result>` takes the finished state and works back to today. The room first agrees what done means. Each member then builds, blind, a tree of what must be true immediately before the end result, and before that, and so on. The judge merges the trees into one dependency graph and writes the PRD from it. The members review the PRD through assigned lenses, the judge revises, the members get one capped round of must-haves, the app checks the result, and the PRD is saved as a Markdown file. The optional text after `--` says where things stand today; without it each member states its own assumptions about the starting point.

| State | Move and actors | Visibility, template and output | Completion and failure |
|---|---|---|---|
| `DEFINING_DONE` | `fanout` to every participant, fresh context. | End result and starting point only; `bass_ackwards_prd.done.md`; `{items: [3 to 8 {localId, test, verifiedBy}]}`. `test` is one observable acceptance test of at most 30 words; `verifiedBy` at most 15 words. | Barrier. Fewer than two accepted lists fails with `insufficient_done`. |
| `FREEZING_DONE` | *Blind judge*, then `tally`. | End result and the labelled lists; `bass_ackwards_prd.freeze.md`; `{groups: [{text, sourceRefs}]}` where every accepted item appears in exactly one group and `text` is byte-identical to one of the group's items. | The judge only groups items that mean the same thing. The app then freezes the set by the rule below. Fewer than two frozen items fails with `no_shared_done`. Judge exhaustion fails with `judge_failed`. |
| `BUILDING_TREES` | `fanout` to every participant, fresh context. | End result, starting point and the frozen DoD; `bass_ackwards_prd.tree.md`; `{nodes: [3 to 20 {nodeId, parent, precondition, deliverable, ownerRole, risk, serves: [1 or more DoD IDs]}], reachesToday, gapNote, todayAssumptions: [0 to 5]}`; 4,096 visible tokens. | Tree rules below. Barrier. Fewer than two accepted trees fails with `insufficient_trees`. |
| `MERGING` | *Blind judge.* | End result, starting point, frozen DoD and the labelled trees; `bass_ackwards_prd.merge.md`; a `BassAckwardsPrdDocument`; `V8`. | Document rules below. Every line item cites at least one source tree node, so the merge adds no work of the judge's own. Judge exhaustion fails with `judge_failed`; the DoD and trees stay exportable and nothing is saved. |
| Coverage | `tally`. | DoD items that no line item serves in version 1. | Persist. The list is delivered to the next two member steps. |
| `REVIEWING` | `fanout` to every participant, fresh context, mutually invisible. | End result, starting point, the rendered PRD version 1, the uncovered DoD list, and the member's one lens; `bass_ackwards_prd.review.md`; `{build, comments: [0 to 4 {localId, quote, comment, proposed}]}`. `build` is required for the readback lens and null otherwise. | The app runs `verbatim` on every `quote` against the rendered PRD the member was sent. A comment whose quote fails is dropped, with no repair, and counted in the report. An empty comment list is a valid answer, not a skip. |
| `REVISING` | *Blind judge.* Skipped when no comment survived. | Version 1, the frozen DoD, and the surviving comments under labels with their lens, quote and the readback builds; `bass_ackwards_prd.revise.md`; `{decisions, document}`; `V8`. | Decision and integrity rules below. Judge exhaustion ends in `FAILED` with `judge_failed` after the save step has written the last good version. |
| `MUST_HAVES` | `fanout` to every participant, fresh context, mutually invisible. | End result, starting point, the rendered current version and its uncovered DoD list; `bass_ackwards_prd.must_have.md`; `{additions: [0 to 3 {localId, text, under, dependsOn, serves, ownerRole, risk, why}]}`. | The schema has no field for a change or a removal. `under` names an existing milestone key; `serves` names at least one DoD ID. An empty list is valid. |
| `FINALIZING` | *Blind judge.* Skipped when no addition was submitted. | The current version, the frozen DoD and the additions under labels; `bass_ackwards_prd.finalize.md`; `{decisions, document}`; `V8`. | Add-only rule below. Judge exhaustion as in `REVISING`. |
| `AWAITING_HUMAN` | Human step. Only when `PRD_HUMAN_REVIEW` is 1. | Banner `LAST LOOK`. Shows the final PRD and the check results so far. | See the human step below. |
| `APPLYING` | `judge`. Only when the human gave at least one note. | The current version, the frozen DoD and the human's notes; `bass_ackwards_prd.apply.md`; `{applied: [{note, appliedTo, conflict}], document}`; `V8`. | The judge applies every note and cannot reject one. A note it cannot apply carries a `conflict` line and is reported as not applied. The integrity rule holds. |
| `CHECKING` | `tally`. No model call. | The four checks, the critical path and the parallel levels. | Persist `BassAckwardsPrdChecks`. |
| `SAVING` | `report`, then the file write. No model call. | Commit `bass_ackwards_prd.report`, write the file, commit `bass_ackwards_prd.saved`. | Enter `COMPLETE` and show the path. See saving below. |

**Freezing the definition of done.** The app counts, for each group, the distinct members among its sources. A group with a count of two or more is frozen; the rest are single-source items and are listed in the PRD's appendix, not in the definition of done. If more than 12 groups qualify, the 12 with the highest count are kept, ties in the judge's order. Frozen items get IDs `D1` to `Dn` in that order. The judge cannot add an item, reword one or decide the cut, and the frozen text is never edited by any later step.

**Tree rules.** `parent` is null for a node that must be true immediately before the end result, otherwise the `nodeId` of the node it must precede. A tree has at most 20 nodes and depth at most 4, at least one root, no unknown parent and no cycle. `precondition` and `deliverable` are at most 40 words each, `ownerRole` at most 5, `risk` at most 30. When `reachesToday` is false, `gapNote` says what is missing between the deepest nodes and the starting point.

**Document.** `{title, overview, milestones: [2 to 8 {key, localId, title, goal, phases: [1 to 4 {key, localId, title, lineItems: [1 to 8 {key, localId, text, ownerRole, precondition, risk, serves, dependsOn, sources}]}], provenance: {sourceLabels, note}}], overviewProvenance, openQuestions: [0 to 8]}`. At most 60 line items and `W_prd` words in all; `overview` at most 200 words; each provenance `note` at most 30 words. Milestones are listed forward, from today to the end result, although they were derived backward.

- **Keys.** The app mints a permanent key for every milestone, phase and line item when a version is committed. A judge output names existing things by `key` and new things by a `localId` with `key` null. The display numbers `M1`, `M1.P2`, `M1.P2.L3` are computed from position at render time and are never stored as identity.
- **Structure, validated on every version.** Every `serves` entry is a frozen DoD ID and every line item has at least one. Every `dependsOn` entry is a line item in the same version. The dependency graph has no cycle. No line item depends on a line item in a later milestone. Each milestone has a provenance entry with at least one valid source label. An output that breaks any of these is invalid and uses the judge's own repair attempt.
- **Sources.** In version 1 every line item's `sources` names at least one tree node. A line item added later names the comment, addition or human note that caused it.

**Lenses (D95).** The participants are shuffled once with the private seed and dealt the lenses in the order `GAPS`, `ATTACK`, `READBACK`, repeating, so every lens has at least one member in any room of three or more. A member knows only its own lens.

| Lens | The member's job |
|---|---|
| `GAPS` | Name preconditions and steps that are missing, including for any uncovered DoD item. |
| `ATTACK` | Say how following this PRD exactly still fails to reach the end result. |
| `READBACK` | The app assigns one milestone, rotating through the milestones. The member states concretely in `build`, in at most 150 words, what it would build from that milestone, then comments where the text allowed more than one reading. |

Every comment quotes the passage it targets. `comment` is at most 60 words; `proposed` is nullable and at most 60 words.

**Decisions.** `decisions` holds exactly one entry per surviving comment or addition: `{ref, verdict, reason, appliedTo}`. `verdict` is `accept` or `reject`; `reason` is one line of at most 25 words; an accepted entry lists at least one key or `localId` in `appliedTo`, and may list keys in `removes`. Two comments that say the same thing are each decided.

**Integrity rule.** After `REVISING` and `APPLYING`, every key that no accepted decision lists in `appliedTo` or `removes` must be byte-identical in the new version, and no key may vanish unless an accepted decision removes it. The app checks this and rejects a version that fails it.

**Add-only rule.** After `FINALIZING`, every key of the previous version is still present, in the same order, with every text field byte-identical. An existing line item's `dependsOn` may gain keys of newly added items and may lose none. New line items exist only for accepted additions, each under the milestone it named.

**The human step (D99).** Off by default. With `PRD_HUMAN_REVIEW=1` the contest enters `AWAITING_HUMAN` after `FINALIZING` and follows **New modes → The human step**. `/answer <text>` adds one note, up to ten notes and 500 words together. `/continue` closes the step. With no note there is no judge call. It is off by default because the owner's flow is one command in and one file out, and a human step waits without expiry, holds the room's contest claim and stops a one-shot `squad send` before the file exists. It is offered because the human is the only party who knows whether the end result was understood, and correcting the PRD before the file is written is cheaper than editing the file afterwards. A human note is an instruction, so the judge applies it and has no verdict over it.

**Checks before save (D97).** Application code, run on the version about to be saved:

1. Every line item maps to at least one DoD item.
2. Every DoD item is served by at least one line item.
3. No dependency names a missing line item.
4. The dependency graph has no cycle.

Checks 1, 3 and 4 are also validation rules on every judge output, so a version that breaks one is rejected when it is produced and repaired inside that call's three attempts. Check 2 is not forced on the judge: when no member's tree, comment or addition serves a DoD item, making the judge cover it would make the judge the author of unreviewed work. Instead the uncovered list is computed after the merge and handed to the `GAPS` reviewers and to the must-have round, so the repair is done by members in rounds that already exist. There is no extra repair call. Whatever still fails at this gate is written into the PRD's appendix under `Checks` and stated on the report's first line, and the terminal reason is `saved_with_findings` instead of `saved`.

**Critical path and parallel work.** Also application code. A line item's level is 1 when it depends on nothing, otherwise one more than the highest level among its dependencies. Items on the same level can run in parallel. The critical path is the longest dependency chain by number of line items; ties go to the chain that comes first in document order. No durations are estimated. The app renders both in the PRD; the judge does not state them.

**Saving (D98).** The file is `<data_dir>/exports/prd-<slug>-<contestId>.md`. `slug` is the PRD title in lowercase ASCII with every run of other characters replaced by one hyphen, trimmed, cut to 48 characters, and `prd` when that leaves nothing. The path is a pure function of committed records, so a restart during `SAVING` writes the same bytes to the same path. The write goes to a temporary file in the same directory and is renamed into place; the file has mode `0600`. The report is committed before the write and `bass_ackwards_prd.saved` after it. The data directory is used because it always exists, is writable, and is the same after a restart or from `squad send`, where the working directory is not. If the write fails, the mode still completes, with reason `save_failed`, the OS error, and the line `Could not save the PRD. Use /export bass_ackwards_prd <id>.` The PRD is in the store either way.

The saved file is the PRD in Squad's house style:

```markdown
# <title>

**End result:** <the end result as given>
**Starting point:** <the starting point as given, or "Not stated.">

## Overview
<overview brief>

## Definition of done
- D1 — <test> (verified by: <verifiedBy>)

## Dependency order
**Critical path:** M1.P1.L1 → M1.P2.L1 → M2.P1.L2
**Can run in parallel:** level 1: M1.P1.L1, M1.P1.L2 · level 2: ...

## Milestones

### M1 — <title>
**Goal:** <goal>
**Serves:** D1, D4

#### Phase M1.P1 — <title>
- [ ] M1.P1.L1 — <text> (owner: <role>; needs: <precondition>; after: <dependencies>; done: D1; risk: <risk>)

## Open questions

## Appendix
### Provenance
### Changes after review
### Last must-haves
### Single-source done items
### Checks
```

Provenance names, per section, the members whose trees it came from, with the judge's note. The two change lists show accepted items only, each with its author and the judge's reason.

**Judge limits.** The judge groups DoD items, merges trees, revises to comments and reassembles. It adds no DoD item, adds no line item without a member or human source, cannot edit the frozen definition of done, cannot touch a passage no accepted decision names, cannot reject a human note, and states neither the critical path nor any check result.

**Calls.** `4m + 4`: m done lists and one freeze, m trees and one merge, m reviews and one revision, m must-have lists and one final assembly. Six members cost 28. Reserved attempts are `8m + 12`; six members reserve 60. A round with nothing to decide makes no judge call and releases it. With `PRD_HUMAN_REVIEW=1` the reservation adds one judge call: at most `4m + 5` calls and `8m + 15` reserved attempts.

| Step | Calls × attempts | Data bound |
|---|---|---|
| `bass_ackwards_prd.done` | m × 2 | `J(endResult) + J(today)` |
| `bass_ackwards_prd.freeze` | 1 × 3 | `B(endResult) + m · P(L(V2, W_room))` |
| `bass_ackwards_prd.tree` | m × 2 | `J(endResult) + J(today) + 12 · L(V2, 50)` |
| `bass_ackwards_prd.merge` | 1 × 3 | `B(endResult) + J(today) + 12 · L(V2, 50) + m · P(L(V4, W_plan))` |
| `bass_ackwards_prd.review` | m × 2 | `J(endResult) + J(today) + A(V8) + 12 · L(V2, 50)` |
| `bass_ackwards_prd.revise` | 1 × 3 | `B(endResult) + A(V8) + 12 · L(V2, 50) + m · P(L(V2, W_room))` |
| `bass_ackwards_prd.must_have` | m × 2 | `J(endResult) + J(today) + A(V8) + 12 · L(V2, 50)` |
| `bass_ackwards_prd.finalize` | 1 × 3 | `B(endResult) + A(V8) + 12 · L(V2, 50) + m · P(L(V2, 300))` |
| `bass_ackwards_prd.apply` | 1 × 3, only with human review | `J(endResult) + A(V8) + 12 · L(V2, 50) + L(V2, 500)` |

**Prompts.** `bass_ackwards_prd.done.md` (output 300), `bass_ackwards_prd.freeze.md` (`W_room`), `bass_ackwards_prd.tree.md` (`W_plan`), `bass_ackwards_prd.merge.md` (`W_prd`), `bass_ackwards_prd.review.md` (`W_room`), `bass_ackwards_prd.revise.md` (`W_prd` plus 25 per decision), `bass_ackwards_prd.must_have.md` (300), `bass_ackwards_prd.finalize.md` (`W_prd` plus 25 per decision), `bass_ackwards_prd.apply.md` (`W_prd`).

**Report.** The saved path, or the save failure; the check line; the final PRD; the frozen definition of done with support counts; provenance per section resolved to nicks; the review log (lens, author, quote, comment, verdict, reason) and the count of comments dropped for a failed quote; the must-have log; the human's notes and what was applied; uncovered DoD items; single-source done items; skipped members. `/export bass_ackwards_prd [id]` returns the final PRD followed by the full record: every done list, the grouping, every tree, every PRD version, every comment including dropped ones, the readback builds, every decision, and the checks. `/export` works whether or not the automatic save succeeded.

**TUI.** Banner `BASS ACKWARDS`; dividers `DONE FIRST`, `BACKWARD TREES`, `MERGED PRD`, `REVIEW`, `LAST MUST-HAVES`, `CHECKS`, `SAVED`. Trees are collapsed by default. Decisions render as a table with accept and reject marks. The last line reads `PRD saved to <path>`. The human step, when on, uses the shared human-step widget with the placeholder `/answer <note>, or /continue to save`.

**Acceptance tests.**

- Six members make 28 calls and reserve 60 attempts; three make 16 and reserve 36. With `PRD_HUMAN_REVIEW=1` and one note, six members make 29 and reserve 63.
- No done, tree, review or must-have request contains another member's list, tree, comment or addition, a nick, a slug or a persona; a review request contains exactly one lens.
- DoD fixtures: an item named by one member is not frozen and appears in the appendix; thirteen qualifying groups freeze twelve by support; a freeze output that rewords an item or adds one is rejected.
- Tree fixtures: an unknown parent, a cycle, depth five, twenty-one nodes and a node with no `serves` are each rejected and repaired once.
- A merge output with a line item that has no source node, a dependency on a later milestone, a dangling dependency, a cycle or an unknown DoD ID is rejected.
- With three, six and seven members every lens is assigned at least once and the deal survives restart.
- A comment whose quote is not verbatim in the delivered PRD is dropped without repair and never reaches the judge.
- A revision that changes a line item no accepted decision names is rejected; a final assembly that rewords, removes or reorders an existing item is rejected.
- A must-have output cannot express a change or removal; four additions are rejected by the schema.
- Rounds with no surviving comment, and with no addition, make no judge call.
- An uncovered DoD item after the merge is delivered to the reviewers and the must-have round; still uncovered at the gate, it is in the appendix, on the report's first line, and the reason is `saved_with_findings`.
- Critical path and level fixtures: a chain, a diamond, two equal chains.
- The saved file's bytes equal the PRD part of `/export bass_ackwards_prd`; the path is `<data_dir>/exports/prd-<slug>-<id>.md`; slug fixtures cover punctuation, non-ASCII and an empty title.
- A crash between the report commit and the file write saves the same bytes to the same path on restart; an unwritable directory completes with `save_failed`.
- Judge exhaustion in `REVISING` saves version 1 and ends `FAILED` with `judge_failed`; in `MERGING` nothing is saved.
- With human review on: `AWAITING_HUMAN` releases holds, keeps the claim, survives restart and never expires; `/continue` with no note makes no call; a note the judge cannot apply is reported as not applied.

### Stop, failures and recovery for the two modes

`/stop`, the daily-cap path and quorum failures behave as in **New modes → Stop, failures and recovery for the nine modes**. A stopped `$bass_ackwards_prd` saves nothing; its last committed version stays exportable. A stopped `$tune` keeps every candidate, execution and sheet, and reports the current champion as unverified.

On restart, in addition to the earlier lists:

- Restore the tune suite and split, the mutator rotation, every candidate, every stored execution, every sheet with its label maps, digests, the champion chain and the finalists.
- Restore the bass_ackwards_prd frozen definition of done, trees, every PRD version with its keys, the lens deal and readback assignments, quote checks, decisions, a pending human step with its notes, and a pending save.

Recovery never reruns an accepted step or a stored execution and never redraws a seed, split, rotation, lens or label.

### Call counts at a glance

| Mode | Normal model calls | Reserved attempts | Six members, defaults |
|---|---|---|---|
| tune | at most `m + T + 3H + 3 + G(3T + 4)`, reserved at the caps as `m + 13G + 15` | `2m + 27G + 33` | 60, of which 39 are executor calls |
| bass_ackwards_prd | `4m + 4`, or at most `4m + 5` with human review | `8m + 12`, or `8m + 15` | 28 |

### Later support

Outside M9: human approval of the tune test set and human-supplied cases; a saved tune suite that a later run can reuse; several executor models in one run; cost and latency columns in the tune report; duration estimates on the PRD's critical path; saving the PRD to a path of the human's choice; MCP tools for either mode.

### Part 2: Milestones

The task breakdown is in [M9 — Tune and bass_ackwards](milestones/M9-tune-and-bass-ackwards.md).
