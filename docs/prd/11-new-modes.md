# New contest modes: forge, forecast, decide, negotiate, blow_it_up, readback, debug, trace, rank

### Part 1: Specification

### Scope, ownership and naming

This section specifies nine contest modes added after v0.1.2: `$forge`, `$forecast`, `$decide`, `$negotiate`, `$blow_it_up`, `$readback`, `$debug`, `$trace` and `$rank`. The names are final (D73). With the seven existing modes Squad has sixteen.

This is section 11; section 10 is [Hardening and launch](10-hardening-and-launch.md).

Everything in [Contests](08-contests-debate-tournament-grill-census-crosscheck-survey-probe.md) still applies unless this section says otherwise: the shared contest engine and its eight moves, one active-contest claim per room, structural blindness, blind text redaction, preflight and admission, participant holds, the daily cap, `/stop`, restart recovery, and the markdown export path. The nine modes are scripts over the same moves. They add one engine capability, the **human step**, specified below.

Ownership follows Contests:

- **New modes (this section):** the nine scripts, their statuses, typed payloads, aggregation maths, call counts, human steps and reports.
- **Command grammar and parser:** the nine verbs, their argument shapes, `/answer`, the extended `/continue`, and every exact error string. This section proposes the strings; once landed, the parser fixtures are normative.
- **Prompt pack:** prompt text. This section names each template, its output and its word budget.
- **Protocol, store and bus:** carriers, `Work`, storage schema and visibility rows.

Vocabulary used below: m is the number of participating members; *accepted* means an output passed validation and was committed; *skipped* means a member exhausted its attempts on a step and is named, with the failed step, in the report; *label* means a router-minted opaque identifier or shuffled letter whose map to a nick lives only in `contest_private_maps`.

### When to use which mode

| Need | Mode | Why not an existing mode |
|---|---|---|
| One finished artifact that is better than any single draft | `$forge` | Tournament crowns one whole entry and discards the rest. |
| A dated probability with spread, cruxes and signposts | `$forecast` | Survey bands claims; it produces no number. |
| A choice among two to four options on several criteria | `$decide` | Debate is two-sided and rewards rhetoric; tournament ranks answers, not options. |
| Preparation and rehearsal for a negotiation | `$negotiate` | No existing mode has hidden private information or role play. |
| Adversarial attack on a plan, spec, prompt, config or diff | `$blow_it_up` | Census and crosscheck find omissions; grill repairs one leader's plan. None attacks. |
| A spec that reads the same to every reader | `$readback` | Census finds what is missing; readback finds what is present but read differently. |
| A stubborn failure, narrowed by tests the human actually runs | `$debug` | Probe is a one-shot decision tree with no evidence loop. |
| A brief where every claim is tied to a quoted source span | `$trace` | Survey measures agreement between models, not support in sources. |
| An ordered list of the human's own items | `$rank` | Tournament ranks model answers to one prompt. |

### Shared contracts for the nine modes

**Lifecycle.** Each mode starts in `PREFLIGHT`, applies the shared confirmation and daily-admission policy, enters `WAITING_FOR_PARTICIPANTS`, and then runs its first script step once earlier ordinary work has drained. None of the nine uses question-first collection and `pick`: their first step is a contest `fanout` under admission, as in census. Terminal states are `COMPLETE`, `STOPPED` and `FAILED`, each with a `terminalReason`. `COMPLETE` means the script finished; a mode's own reason says what the result is.

**Judge.** Every mode has an implicit judge on one case context per contest, using `judge.system.md`, `NEW_MODE_JUDGE_REASONING_EFFORT` and `NEW_MODE_JUDGE_VISIBLE_TOKENS`. Where a step is marked *blind judge* the request also carries `judge.system.blind-prefix.md` and contains labels only. `@judge` in a start command is refused: `@judge cannot be used with ${mode}.` The judge is never a participant and is not counted in m.

**Attempts and repair (D75).**

| Call | Deadline and attempts |
|---|---|
| Member call in any of the nine modes | 180 s; 2 attempts. If attempt one returned output that failed schema or reference validation, attempt two is one repair using `mode.repair.md` with the validation diagnostics. After a transport failure or timeout it is a plain retry. Then the member is skipped for that step. |
| Judge call in any of the nine modes | 300 s; 3 attempts including transport retries and at most one schema repair. Exhaustion enters `FAILED` unless the step says otherwise. |

Reserved attempts are therefore `2 × member calls + 3 × judge calls` for every mode. `router/bounds.rs` already prices each attempt at its repair-resend worst case, so the repair needs no extra reservation.

**Limits.** `W_room` is `roomWords` (500) and `W_plan` is `planWords` (1,200). A step's word budget covers its prose fields together; identifiers, enums and numbers do not count. Member steps use `REPLY_VISIBLE_TOKENS` (2,048) unless the step table says 4,096, which is `PLAN_VISIBLE_TOKENS`. Judge steps use 4,096.

**Generated objects.** Strict JSON schemas, unknown properties rejected, required nullable fields explicitly required, bounded arrays, unique local IDs, and validated references to supplied inputs, as in Contests. A missing output is never converted into an empty answer, a vote, a score, a probability or a judgment.

**Barriers.** Every parallel step ends with a `barrier` that commits accepted artifacts and attributed failures and delivers only the next step's authorized projection. Reports and identity-bearing tables go to the human and the authorized judge case, never into members' ordinary histories.

**Verbatim matching.** Four modes check that a quoted span exists in a source text. They share one deterministic matcher, `verbatim(quote, source)`: apply Unicode NFKC to both; map curly quotes and apostrophes to ASCII, and en and em dashes to `-`; collapse every run of whitespace to one space; trim. The quote matches when the normalised quote is a contiguous substring of the normalised source. Matching is case-sensitive. A quote shorter than 12 normalised characters, or one containing an ellipsis used to join separate passages, never matches. The matcher returns the first match's byte span in the original source, or none. It is application code; no model decides whether a quote exists.

**Reservation sizing notation.** Each mode gives a step table for `router/bounds.rs`. Sizes are bytes in that module's unit. `A(v) = artifact(v)`; `L(v, w) = limited(v, w)`; `P(x) = project(x)`; `B(t) = blinded(t)`; `J(t) = json_text(t)`. `V2` is 2,048 and `V4` is 4,096 visible tokens. Every row also carries its frozen template and schema bytes, which the tables omit. A plan whose worst step does not fit its model fails closed with `CONTEXT_TOO_LARGE`; no mode summarises or drops required evidence to fit.

**Private randomness.** Label permutations, role draws, lens assignments, pair samples, verifier assignments and tie-break seeds are drawn once at admission or at the step that needs them, persisted in `contest_private_maps`, never derived from roster order, and reused on recovery.

**Report and export.** Each mode commits a `<mode>.report` artifact, shows a banner and step dividers in the TUI, and exports through the shared markdown export with its own scope. None updates debate records or tournament trophies.

### The human step

`$readback` and `$debug` pause for the human. The step follows the conventions of the question-first pick (D68) and of census's `WAITING_FOR_PLAN` (D76):

| State | Event or condition | Required action |
|---|---|---|
| Script reaches a human step | Judge output for the step is accepted | Commit `contest.human_requested` with the prompt material, enter `AWAITING_HUMAN`, release participant holds, deliver queued ordinary inputs, keep the active-contest claim. |
| `AWAITING_HUMAN` | Valid answer | Commit `contest.human_answered` bound to the contest ID and expected state version. If the step is now complete, reacquire holds, drain earlier ordinary work through `WAITING_FOR_PARTICIPANTS`, and enter the next script state. |
| `AWAITING_HUMAN` | Invalid answer | Return the parser-owned error and preserve the pending step. |
| `AWAITING_HUMAN` | Ordinary message or unrelated command | Handle it normally and preserve the pending step. |
| `AWAITING_HUMAN` | `/continue` | Close the step with whatever has been answered. The mode defines what an unanswered item means. |
| `AWAITING_HUMAN` | Accepted global stop | Enter `STOPPED` with reason `stopped`; the report keeps everything committed so far. |
| `AWAITING_HUMAN` | Restart | Restore the same prompt material and partial answers. No expiry, no new model call. |

A human step never expires and consumes no provider deadline. No model answers on the human's behalf, and no default is applied when the human is silent. Answers arrive in two ways:

- `/answer <text>` takes the rest of the message literally, with no mention, variable or command scanning. The pending mode validates the text.
- For `$readback` only, a message made only of choice tokens such as `1b 2a 3c` is taken as an answer, in the way a pick message is made only of mentions.

The TUI shows the mode's banner, the numbered items, an input placeholder with the accepted shape, and an `answer pending` status. Escape follows the transient-dismissal and confirmed-global-stop contract.

`squad send` returns when its contest reaches `AWAITING_HUMAN`, printing the pending items and the contest ID; a later `squad send "/answer ..."` continues it. This follows whatever the open question on a waiting `squad send` and question-first picks finally decides, and must change with it.

### Registry changes and scaffold

One scaffold worker lands every shared-file change first (milestone phase M8.P1). After that, each mode worker edits only the files it owns, so nine workers run in parallel without collisions. A mode worker that finds it needs a shared-file change reports it to the orchestrator; it does not make the edit.

**Rust identifiers.** The wire name is the verb. The Rust prefix differs in two cases.

| Verb | Wire name | Module name | Rust prefix | Note |
|---|---|---|---|---|
| `$forge` | `forge` | `forge` | `Forge` | |
| `$forecast` | `forecast` | `forecast` | `Forecast` | |
| `$decide` | `decide` | `decide` | `Decide` | |
| `$negotiate` | `negotiate` | `negotiate` | `Negotiate` | |
| `$blow_it_up` | `blow_it_up` | `blow_it_up` | `BlowItUp` | `rename_all = "lowercase"` would give `blowitup`, so every enum carries an explicit `#[serde(rename = "blow_it_up")]`. The lexer already accepts underscores in command names. |
| `$readback` | `readback` | `readback` | `Readback` | |
| `$debug` | `debug` | `debugging` | `Debugging` | Avoids `std::fmt::Debug` and the existing F12 pane in `squad-tui/src/widgets/debug.rs`. Enums carry `#[serde(rename = "debug")]`. |
| `$trace` | `trace` | `trace` | `Trace` | |
| `$rank` | `rank` | `rank` | `Rank` | |

**What the scaffold lands.**

| Registry | Change | File |
|---|---|---|
| `ContestKind` | Nine variants, with the two explicit renames. | `crates/squad-protocol/src/contest.rs` |
| `ContestState` | Nine typed variants, `Forge(ForgeState)` through `Rank(RankState)`. | `crates/squad-protocol/src/contest.rs` |
| `ContestTransition` | Nine variants carrying each mode's transition enum. | `crates/squad-protocol/src/envelopes.rs` |
| Status enums and state structs | For each mode, the `<Mode>Status` enum exactly as listed in that mode's section, and `<Mode>State { contestId, stateVersion, state, stepId, round, artifactEventIds, terminalReason }` with a `validate()`. `round` is 0 for modes without rounds. | `crates/squad-protocol/src/<module>.rs` (created by the scaffold, then owned by the mode) |
| Enable switch | `pub const ENABLED: bool = false;` in each mode's protocol module. The parser, typeahead and `/help` list a verb only when its switch is true. A mode worker flips it in its own file as its last task. | `crates/squad-protocol/src/<module>.rs` |
| Envelope kinds | `<mode>.setup`, `<mode>.state` and `<mode>.preflight` per mode, plus the shared `contest.artifact`, `contest.human_requested` and `contest.human_answered`. | `crates/squad-protocol/src/envelopes.rs` |
| Artifact carrier (D74) | `ContestArtifact { contestId, kind, artifactKind, stepId, round, actor, audience, body }`. `artifactKind` is a string such as `forge.draft`; `audience` is `human`, `case`, or `members` with an explicit recipient list; `body` is a JSON object validated by the owning mode's `validate_artifact(artifactKind, body)`. Every artifact named in this section, including each `<mode>.report`, travels in this carrier, so adding an artifact type never touches a shared enum again. The typed Rust structs in the mode's protocol module remain the normative schema. | `crates/squad-protocol/src/contest.rs`, `envelopes.rs` |
| `WorkKind` | Every work kind in the table below, with `attempt_limit` returning `FINAL_JUDGE_ATTEMPTS` for judge kinds and `DEBATE_TURN_ATTEMPTS` for member kinds, plus deadline classes. | `crates/squad-protocol/src/work.rs`, `constants.rs`; `crates/squad-agents/src/agents/mod.rs` |
| `CaseMaterial` | One variant per mode, typed in the mode's module. | `crates/squad-protocol/src/envelopes.rs` |
| Variables (D77) | `NEGOTIATE_ROUNDS` (default 3, 1 to 6), `READBACK_ROUNDS` (default 2, 1 to 4), `DEBUG_ROUNDS` (default 3, 1 to 6) in `VariableName`, `/set`, inline `$NAME=n`, `SquadConfig` and the optional `SQUAD_<NAME>` seeds. Not added to `.env.example`. An admitted contest keeps its frozen value. | `crates/squad-protocol/src/parser/types.rs`, `parser.rs`, `config.rs`; `crates/squad-core/src/router/variables.rs`; `crates/squad-cli/src/env-help.md` |
| Parser verbs and AST | The nine verbs in the `verbs!` table and nine `ParsedInput` variants with the argument shapes in the grammar table below; `is_contest` and `input()` arms; multi-segment `--` splitting for `$trace`; line-list parsing for `$decide` and `$rank`. | `crates/squad-protocol/src/parser/types.rs`, `parser.rs`, `errors.rs` |
| Human-step grammar | `SlashAction::Answer { text }`; `/continue` binds to a contest in `AWAITING_HUMAN` as well as to a waiting census; `ParseContext.pending_human: Option<PendingHuman>`; `ParsedInput::HumanChoices` for the readback choice-token message. | same parser files; `crates/squad-protocol/src/requests.rs` |
| `ExportScope` | Nine variants; `/export` usage strings; scope-to-kind mapping. | `crates/squad-protocol/src/parser/types.rs`, `parser.rs`; `crates/squad-core/src/export/mod.rs`; `crates/squad-cli/src/args.rs` |
| Parser fixtures | New `examples.json` cases for every form in the grammar table, one refusal per error string, the busy error for each new kind, `/answer`, `/continue` on a human step, a choice-token message with and without a pending step, and the nine exports. The 166 existing cases and their snapshots stay unchanged. New rules are added to `validation_strings.json` and `validation_codes.json`. | `crates/squad-protocol/src/parser/examples.json`, `validation_strings.json`, `validation_codes.json`, `parser_tests.rs`, `round_trip_tests.rs` |
| Store | Migration `0009_new_modes.sql`: kind and work-kind CHECK extensions for all nine modes, the artifact and human-step tables, and variable rows. Empty, registered per-mode migrations `0010_forge.sql` through `0018_rank.sql` that each mode worker fills. Stub `crates/squad-store/src/<module>.rs` files exported from `lib.rs`. | `crates/squad-store/migrations/`, `src/schema.rs`, `src/lib.rs` |
| Engine dispatch | Match arms that route prepare, accepted work, human answers, stop, recovery, state loading and export rendering to `<module>::` entry points with fixed signatures, each a stub the mode worker replaces. The stubs are unreachable while the mode's `ENABLED` switch is false, because the parser does not accept the verb. Stub modules are registered in each `mod.rs`. | `crates/squad-core/src/router/mod.rs`, `router/stop.rs`, `router/recovery.rs`, `router/preflight.rs`, `orchestrator/mod.rs`, `orchestrator/preflight.rs`, `runtime/mod.rs`, `export/mod.rs` |
| Human step | The `AWAITING_HUMAN` transition, hold release and reacquire, answer commit and recovery. | `crates/squad-core/src/router/human.rs` (new), `orchestrator/human.rs` (new) |
| Verbatim matcher | `verbatim(quote, source)` and its fixtures. | `crates/squad-core/src/orchestrator/verbatim.rs` (new) |
| Prompt registration | Every template name in this section registered in the manifest, the embedded table and the provenance list, each pointing at a placeholder file the mode worker replaces; the composition dispatch arms in `inputs.rs` and `render.rs`. | `crates/squad-core/prompts/manifest.json`, `src/prompts/embedded.rs`, `provenance.rs`, `inputs.rs`, `render.rs`, `mod.rs` |
| TUI | `MODES` entries and TIP examples (below), gated on each mode's `ENABLED`; view maps in `session.rs`; widget and fx module registration and dispatch; banner words; the shared human-step widget; the preflight widget's call-count line for the nine kinds. | `crates/squad-tui/src/typeahead.rs`, `session.rs`, `widgets/mod.rs`, `widgets/contest_status.rs`, `widgets/preflight.rs`, `widgets/human_step.rs` (new), `fx/mod.rs`, `fx/dispatch.rs`; `crates/squad-core/src/banner/mod.rs` |
| CLI | Export scope arguments, `squad send` output for the nine reports and for `AWAITING_HUMAN`. | `crates/squad-cli/src/args.rs`, `send.rs`, `output.rs`, `adapter.rs` |
| MCP (D87) | No change. See below. | none |

**Work kinds.** Member kinds first, judge kinds in italics.

| Mode | Work kinds |
|---|---|
| forge | `forge.draft`, `forge.rank`, `forge.fuse`, *`forge.judge`* |
| forecast | *`forecast.frame`*, `forecast.estimate`, `forecast.revise`, *`forecast.cruxes`* |
| decide | `decide.options`, *`decide.shortlist`*, `decide.criteria`, *`decide.freeze`*, `decide.score`, *`decide.explain`* |
| negotiate | `negotiate.map`, *`negotiate.brief`*, `negotiate.package`, `negotiate.move`, `negotiate.counter`, *`negotiate.playbook`* |
| blow_it_up | `blow_it_up.attack`, `blow_it_up.challenge`, *`blow_it_up.rule`* |
| readback | `readback.interpret`, *`readback.diff`*, *`readback.rewrite`* |
| debug | `debug.hypothesize`, *`debug.merge`*, `debug.test`, *`debug.select`*, *`debug.report`* |
| trace | `trace.extract`, *`trace.merge`*, `trace.verify`, *`trace.assemble`* |
| rank | `rank.compare`, *`rank.explain`* |

**Start grammar.** All forms use the existing grammar: recipients, one `$mode` verb, free text, and line-initial `--` separators. Each mode needs `@all` (exclusions allowed) or at least the stated number of named members. `PlanSource` forms (quote one message, paste text, or `@nick` for that member's latest reply) work as they do for census.

| Mode | Forms | AST fields | Members |
|---|---|---|---|
| forge | `@all $forge <brief>` and `@all $forge <brief>` `--` `<constraints and style>` | `brief`, `constraints: Option` | 3 to 12 |
| forecast | `@all $forecast <question>` and with `--` `<background>` | `question`, `background: Option` | 3 to 12 |
| decide | `@all $decide <decision and context>` `--` `<one option per line>`; without `--`, options are generated | `decision`, `options: Vec<String>` (empty, or 2 to 4) | 3 to 12 |
| negotiate | `@all $negotiate <our side>` and with `--` `<what we know of the counterpart>` | `ourSide`, `counterpart: Option` | 4 to 12 |
| blow_it_up | `@all $blow_it_up` with a `PlanSource`; optional `--` `<scope or threat focus>` | `source: PlanSource`, `scope: Option` | 3 to 12 |
| readback | `@all $readback` with a `PlanSource` | `source: PlanSource`, `rounds` | 3 to 12 |
| debug | `@all $debug <symptom, evidence, what was tried>` and with `--` `<what can be run or inspected, and limits>` | `brief`, `limits: Option`, `rounds` | 3 to 12 |
| trace | `@all $trace <question>` `--` `<source 1>` `--` `<source 2>` ...; quoted messages are appended as further sources | `question`, `sources: Vec<TraceSource>` (1 to 12) | 3 to 12 |
| rank | `@all $rank <what to rank by>` `--` `<one item per line>` | `criterion`, `items: Vec<String>` (3 to 40) | 3 to 12 |

For `@nick $blow_it_up` and `@nick $readback` the single mention names the source, not a one-member contest, exactly as `@nick $census` does.

**Proposed parser strings.**

| Rule | String |
|---|---|
| Count, eight modes | `${mode} needs at least three members.` |
| Count, negotiate | `$negotiate needs at least four members.` |
| Judge refused | `@judge cannot be used with ${mode}.` |
| Missing text | `Add a brief after $forge.` · `Add a question after $forecast.` · `Add the decision after $decide.` · `Add your side of the negotiation after $negotiate.` · `Add a symptom and the evidence after $debug.` · `Add a question after $trace.` · `Add what to rank by after $rank.` |
| Missing source | `Add something to attack for $blow_it_up: quote one message, paste it, or use @nick for that member's latest reply.` · `Add a spec for $readback: quote one message, paste it, or use @nick for that member's latest reply.` |
| Decide options | `$decide takes two to four options after --, one per line.` |
| Rank items | `$rank takes three to forty items after --, one per line.` |
| Trace sources | `Add at least one source after -- for $trace.` · `$trace takes at most twelve sources.` |
| Variables | `NEGOTIATE_ROUNDS must be an integer from 1 to 6.` · `READBACK_ROUNDS must be an integer from 1 to 4.` · `DEBUG_ROUNDS must be an integer from 1 to 6.` |
| No pending step | `Nothing is waiting for an answer.` |
| Readback answer | `Answer with entry and reading, like 1b 2a, or /answer 3: your own reading.` · `Entry {n} has no reading {x}.` |
| Debug answer | `Answer with an outcome label and what you saw, like /answer O2 p99 fell to 180 ms, or /answer other <what you saw>, or /answer unknown.` |

**Typeahead and TIP lists.** `MODES` gains, in this order after `$probe`:

| Token | Hint |
|---|---|
| `$forge` | Blind drafts fused into one best artifact |
| `$forecast` | A pooled probability with cruxes and signposts |
| `$decide` | Score options on a frozen rubric and see what flips it |
| `$negotiate` | Prepare and rehearse a negotiation |
| `$blow_it_up` | Red-team a plan and keep only attacks that survive |
| `$readback` | Find where a spec reads two ways and fix it |
| `$debug` | Narrow a failure with tests you run |
| `$trace` | Build a brief where every claim quotes its source |
| `$rank` | Order a list by pairwise comparison |

`examples()` gains, each kept only when it parses in the live context and its mode is enabled: `@all $forge write the launch post for our CLI`, `@all $forecast will we ship v1 by June 30?`, `@all $rank impact for effort` followed by a newline, `--` and three items. `KEY_TIPS` gains `A contest waiting on you: type the answer, or /continue to move on`.

**MCP exposure (D87).** M8 adds no MCP tool. The MCP server continues to expose roster discovery and the tournament workflow only, and `squad_contests_list` continues to list tournament runs. Each new tool would need a start schema, a result schema and a spend-approval path, and two of the nine modes pause for a human, which the MCP flow cannot do today. The six modes with no human step and a self-contained result (`forge`, `forecast`, `decide`, `blow_it_up`, `trace`, `rank`) are the candidates for a later milestone.

**Files each mode worker owns.** `<module>` is the module name from the identifier table.

| Crate | Files |
|---|---|
| `squad-protocol` | `src/<module>.rs`, `tests/<module>.rs` |
| `squad-store` | `src/<module>.rs`, its numbered migration, `tests/<module>.rs` |
| `squad-agents` | `src/agents/<module>.rs`, `tests/agents_<module>.rs` |
| `squad-core` | `src/orchestrator/<module>.rs` (script and pure tally functions), `src/router/<module>.rs` (commits and transitions), `src/runtime/<module>.rs`, `src/prompts/inputs_<module>.rs`, `prompts/<wire name>.*.md`, `src/export/<module>.rs`, `tests/<module>_workflows.rs`, `tests/<module>_tallies.rs`, `tests/<module>_export.rs` |
| `squad-tui` | `src/widgets/<module>.rs`, `src/fx/<module>.rs`, `tests/<module>_view.rs` |
| docs | `docs/modes/<wire name>.md`, the user-facing section in the README's mode-section format. One integration worker folds these into `README.md` at the end. |

Migration numbers: forge 0010, forecast 0011, decide 0012, negotiate 0013, blow_it_up 0014, readback 0015, debug 0016, trace 0017, rank 0018.

### Forge execution

```rust
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ForgeStatus {
    Preflight,
    WaitingForParticipants,
    Drafting,
    Ranking,
    Fusing,
    Judging,
    Reporting,
    Complete,
    Stopped,
    Failed,
}
```

- `ForgeSetup` — Frozen brief, constraints, participants and label maps.
- `ForgeDraft` — One blind draft.
- `ForgeBallot` — One member's ranking of the other drafts with a steal-worthy element and a fatal flaw per draft.
- `ForgeTally` — App-computed Borda scores, the best original and the two fusers.
- `ForgeFusion` — One fusion with its provenance and handled flaws.
- `ForgeVerdict` — The blind judge's ranking of the candidates with reasons.
- `ForgeReport` — The winning artifact, the gate outcome and the provenance table.
- `ForgeCaseMaterial` — The brief and the anonymous candidates.

`@all $forge <brief>` asks for one finished artifact: a launch post, a design doc, a function, an email. Use it when the result must be the deliverable itself. Tournament picks a winner and never improves it; forge recombines the best parts and then refuses the recombination unless it beats the best single draft.

| State | Move and actors | Visibility, template and output | Completion and failure |
|---|---|---|---|
| `DRAFTING` | `fanout` to every participant, fresh context. | Brief and constraints only; `forge.draft.md`; `{body}` within `W_plan`; 4,096 visible tokens. | Barrier after all slots resolve. Fewer than three accepted drafts fails with `insufficient_drafts`. |
| `RANKING` | `fanout` to every participant with an accepted draft, fresh context. | Brief, constraints and every other accepted draft under labels shuffled per voter; the voter's own draft is not on its ballot; `forge.rank.md`; `{ranking: [label...], notes: [{label, steal, fatalFlaw}]}` covering every supplied draft exactly once; `fatalFlaw` is nullable; each `steal` and `fatalFlaw` at most 40 words. | Barrier locks ballots. Fewer than two accepted ballots fails with `insufficient_ballots`. |
| Tally | `tally`. | Borda scores, best original `D*`, fusers `F1` and `F2`. | Persist before fusion dispatch. |
| `FUSING` | `fanout` to the two fusers, fresh contexts, mutually invisible. | Brief, constraints, every accepted draft under one shared label set, the deduplicated steal list and flaw list with labels and no vote counts or ranks, and the length budget; `forge.fuse.md`; `{body, provenance: [{element, sourceLabels}], handledFlaws: [{label, flaw, handling}]}`; 4,096 visible tokens. | The app rejects a body over the length budget; that failure is repaired once like any invalid output. A fuser that exhausts its attempts contributes no candidate. |
| `JUDGING` | *Blind judge.* Skipped when no fusion was accepted. | Brief, constraints and the candidates under fresh shuffled labels: `D*` and each accepted fusion, formatted identically with nothing marking which is an original; `forge.judge.md`; `{ranking: [label...], reasons: [{label, reason}], seamIssues: [{label, issue}]}`. | Judge exhaustion fails with `judge_failed`; the drafts, tally and fusions stay exportable. |
| `REPORTING` | `tally`, then `report`. No model call. | The gate result, the winning text, the provenance table resolved to nicks for the human, and the Borda table. | Commit `forge.report`, enter `COMPLETE`, release resources. |

**Borda.** A ballot that ranks k drafts gives the draft at position p (1 is best) the score `(k − p) / (k − 1)`. A draft's Borda score `B(d)` is the mean of its scores over the accepted ballots that contain it. Excluding each voter's own draft removes self-preference and still gives every draft the same number of potential voters. Ties break by more first places, then fewer distinct voters naming a fatal flaw, then the persisted private seed. `D*` is the draft with the highest `B`. The fusers are the authors of the two highest-ranked drafts; they wrote the two drafts the room trusts most, and they did not see the scores.

**Length budget.** `Lmax` is the word count of the longest accepted draft. A fusion's body must not exceed `Lmax` words. This stops a fusion from winning by stapling everything together.

**Gate.** The winner is the candidate the judge ranks first. If that is a fusion, the terminal reason is `fusion_won`. If it is `D*`, the reason is `original_won` and the report says the fusions did not beat the best draft. If no fusion was accepted, `D*` wins with reason `no_fusion` and no judge call is made. The judge is not told that a gate exists or which candidate is an original.

**Judge limits.** The judge ranks and explains. It does not edit, merge or patch a candidate, and the report prints the winning text exactly as its author wrote it.

**Calls.** `2m + 3`: m drafts, m ballots, two fusions and one judge call. Six members cost 15. Reserved attempts are `4m + 7`. Unused fusion and judge reservations are released.

| Step | Calls × attempts | Data bound |
|---|---|---|
| `forge.draft` | m × 2 | `J(brief) + J(constraints)` |
| `forge.rank` | m × 2 | `B(brief) + J(constraints) + (m − 1) · P(L(V4, W_plan))` |
| `forge.fuse` | 2 × 2 | `B(brief) + J(constraints) + m · P(L(V4, W_plan)) + m · P(L(V2, W_room))` |
| `forge.judge` | 1 × 3 | `B(brief) + J(constraints) + 3 · P(L(V4, W_plan))` |

**Prompts.** `forge.draft.md` (output `W_plan`), `forge.rank.md` (`W_room`), `forge.fuse.md` (`W_plan`, further capped by `Lmax`), `forge.judge.md` (`W_room`).

**Report.** Winner and gate line; the artifact; provenance table (element, source draft, author); Borda table; steal list; fatal flaws and how the winner handled them; judge reasons and seam issues; skipped members. `/export forge [id]` adds every draft, every ballot with its label map, both fusions and the judge's full output.

**TUI.** Banner `FORGE`; dividers `DRAFTS`, `RANKING`, `FUSION`, `VERDICT`. Drafts are collapsed by default. The verdict line reads `Fusion by @nick beat the best draft.` or `The best draft by @nick stands. No fusion beat it.`

**Acceptance tests.**

- Six members make 15 calls and reserve 31 attempts.
- No rank prompt contains its voter's own draft, a nick, a slug or a persona; label order differs between voters.
- Borda fixtures: complete ballots, a missing ballot, and each tie-break level.
- A fusion over `Lmax` is rejected and repaired once; a second overrun leaves one candidate.
- With both fusions failed there is no judge work and the reason is `no_fusion`.
- The judge prompt carries no marker separating fusion from original; the winning text in the report is byte-identical to its accepted artifact.
- Two accepted drafts fail with `insufficient_drafts`; stop and restart at every state repeat no accepted step and redraw no labels.

### Forecast execution

```rust
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ForecastStatus {
    Preflight,
    WaitingForParticipants,
    Framing,
    Forecasting,
    Revising,
    Aggregating,
    Explaining,
    Reporting,
    Complete,
    Stopped,
    Failed,
}
```

- `ForecastSetup` — Frozen question, background and participants.
- `ForecastFrame` — Resolution criteria, date, source and void rule.
- `ForecastEstimate` — One blind probability with base rate, factors and rationale.
- `ForecastBundle` — The anonymous rationales delivered for revision, with numbers withheld.
- `ForecastRevision` — One revised probability with its reason and cited labels.
- `ForecastAggregate` — App-computed pooled probability, spread and flags for each round.
- `ForecastCruxes` — The judge's cruxes, signposts and shared assumptions.
- `ForecastReport` — The assembled result.
- `ForecastCaseMaterial` — Frame, labelled forecasts and app aggregates.

`@all $forecast <question>` produces a dated probability. Use it for "will X happen by Y". Use survey when the output should be a map of claims.

| State | Move and actors | Visibility, template and output | Completion and failure |
|---|---|---|---|
| `FRAMING` | `judge`. | Question and background; `forecast.frame.md`; `{question, resolutionCriteria, resolutionDate, resolutionSource, voidRule, forecastable, reason}`. `resolutionDate` is an ISO 8601 date. | `forecastable: false` ends in `FAILED` with reason `not_forecastable` and the judge's reason; later reservations are released. The frame is frozen and never changes after this step. |
| `FORECASTING` | `fanout` to every participant, fresh context. | Frame and background only; `forecast.estimate.md`; `{probabilityPct, baseRate: {referenceClass, ratePct}, factors: [3 × {direction, text}], rationale}`. `probabilityPct` is an integer from 1 to 99; `ratePct` is a nullable integer from 0 to 100; `direction` is `up` or `down`. | Barrier locks first forecasts. Fewer than three accepted fails with `insufficient_forecasts`. |
| Sharing | `barrier`. | Build each member's `ForecastBundle`. | See number withholding. |
| `REVISING` | `fanout` to every member with an accepted forecast, fresh context. | Frame, the member's own first forecast including its own number, and the other members' reference classes, factors and rationales under shuffled labels, with every number withheld; `forecast.revise.md`; `{probabilityPct, changeReason, citedLabels}`. | Barrier locks revisions. A missing revision carries the first forecast forward, marked `not revised`, never reported as unchanged. |
| `AGGREGATING` | `tally`. | Round 1 and final aggregates, spread, split flag, uncited-move flags. | Persist inputs and unrounded results. |
| `EXPLAINING` | *Blind judge.* | Frame, both rounds of forecasts with numbers under labels, and the app aggregates; `forecast.cruxes.md`; `{cruxes: [up to 5 {text, higherLabels, lowerLabels}], signposts: [up to 6 {observable, direction, byWhen}], sharedAssumptions: [up to 5], weakEvidence: [up to 5]}`. | Judge exhaustion fails with `judge_failed`; the aggregate stays exportable and is still shown. |
| `REPORTING` | `report`. No model call. | Assemble from committed records. | Commit `forecast.report`, enter `COMPLETE`. |

**Number withholding.** The probability and the base-rate number are separate numeric fields and are never placed in another member's bundle. The prompt tells members to keep those numbers out of their prose. The app also replaces, in every bundled prose field, percent tokens, decimals between 0 and 1, `N in M`, `N to M` and `N:M` odds with `[number withheld]`. The bundle excludes the recipient's own entry, and nicks are redacted as in every blind projection.

**Aggregation.** Let the n final forecasts be `p_i`, the revision when accepted, otherwise the first forecast. Let `ℓ_i = ln(p_i / (1 − p_i))`, sorted ascending as `ℓ_(1) ≤ … ≤ ℓ_(n)`. Trim `k = ⌊n / 5⌋` from each end (0 for n of 3 or 4, 1 for 5 to 9, 2 for 10 to 12).

`L = (1 / (n − 2k)) × Σ ℓ_(i)` for `i = k + 1 … n − k`.

`P = 1 / (1 + e^(−L))`.

Averaging log-odds is the geometric mean of odds. The pool is not extremised: extremising assumes forecasters hold independent information, and the room's models share training data and blind spots.

**Spread.** `s` is the sample standard deviation of all n values `ℓ_i`, untrimmed. The report shows `low = 1 / (1 + e^(−(L − s)))` and `high = 1 / (1 + e^(−(L + s)))`, plus the lowest and highest individual forecast. It is called the spread of the room, never a confidence interval.

**Split.** Find the largest gap between adjacent sorted `ℓ` values. If that gap is at least 2.0 (an odds ratio of about 7.4) and at least `⌈n / 3⌉` forecasts lie on each side, set `split`. A split report leads with `The room is split.` and shows the geometric mean of odds of each side before the pooled value.

**Uncited move.** With `L1` the round 1 pooled log-odds, a member has an uncited move when `|ℓ_final − L1| < |ℓ_first − L1|` and `citedLabels` is empty. The report lists uncited moves as possible herding. They are not removed from the pool.

**Judge limits.** The judge writes the frame, and later cruxes and signposts. Neither schema has a probability field. The headline number is the app's and the judge cannot replace or adjust it. The judge does not see nicks.

**Calls.** `2m + 2`: one frame, m forecasts, m revisions and one crux call. Six members cost 14. Reserved attempts are `4m + 6`. There is exactly one revision round (D79): more rounds raise cost and herding without adding independent information.

| Step | Calls × attempts | Data bound |
|---|---|---|
| `forecast.frame` | 1 × 3 | `J(question) + J(background)` |
| `forecast.estimate` | m × 2 | `A(V4) + J(background)` |
| `forecast.revise` | m × 2 | `A(V4) + J(background) + L(V2, W_room) + (m − 1) · P(L(V2, W_room))` |
| `forecast.cruxes` | 1 × 3 | `A(V4) + J(background) + 2m · P(L(V2, W_room)) + ARTIFACT_FRAME` |

**Prompts.** `forecast.frame.md` (300 words), `forecast.estimate.md` (`W_room`), `forecast.revise.md` (200 words), `forecast.cruxes.md` (`W_plan`).

**Report.** Frame with date, source and void rule; pooled probability with spread, or the split view; before and after table per member; uncited moves; cruxes; signposts headed `Update if you see`; shared assumptions with the standing caveat that models share blind spots; skipped members. `/export forecast [id]` adds every forecast with timestamps, each bundle as delivered, and the aggregation inputs with `k`, `L`, `s` and the trimmed entries.

**TUI.** Banner `FORECAST`; dividers `FRAME`, `FIRST FORECASTS`, `REVISION`, `POOLED`. The pooled value is drawn as a bar with the spread as a band and each member as a tick. Member numbers appear to the human as they commit.

**Acceptance tests.**

- Six members make 14 calls and reserve 30 attempts.
- No revise prompt contains another member's probability, base-rate number or nick; the withholding fixtures cover percent, decimal, `N in M` and odds forms.
- Aggregation fixtures for n of 3, 5 and 10 match hand-computed `k`, `L`, `P` and `s` to 1e-9.
- `probabilityPct` of 0 or 100 is rejected and repaired once.
- A missing revision is carried forward and labelled `not revised`.
- A constructed bimodal set sets `split`; a unimodal set does not.
- The judge output cannot carry a probability; the report's number equals the stored tally.
- `forecastable: false` fails before any member call and releases the reservation.

### Decide execution

```rust
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DecideStatus {
    Preflight,
    WaitingForParticipants,
    ProposingOptions,
    Shortlisting,
    ProposingCriteria,
    Freezing,
    Scoring,
    Computing,
    Explaining,
    Reporting,
    Complete,
    Stopped,
    Failed,
}
```

- `DecideSetup` — Frozen decision, options or the generate flag, and participants.
- `DecideOptionProposal` — Two proposed options from one member.
- `DecideShortlist` — Merged options with proposer references and the kept list.
- `DecideCriteria` — One member's criteria and weights.
- `DecideRubric` — Merged criteria, kept and dropped lists, raw and renormalised weights.
- `DecideScores` — One member's cell scores with evidence lines.
- `DecideMatrix` — App-computed cell medians, totals, sensitivity and flips.
- `DecideExplanation` — The judge's flip conditions, dissents and evidence gaps.
- `DecideReport` — The assembled result.
- `DecideCaseMaterial` — Step-scoped merge or explanation inputs.

`@all $decide <decision>` followed by `--` and two to four options scores given options. Without `--`, the room generates options first. Let `g` be 1 when options are generated and 0 otherwise.

| State | Move and actors | Visibility, template and output | Completion and failure |
|---|---|---|---|
| `PROPOSING_OPTIONS`, when `g = 1` | `fanout`, fresh context. | Decision only; `decide.options.md`; exactly 2 `{localId, name, description}` with `name` at most 8 words and `description` at most 40. | Barrier. Fewer than two accepted proposals fails with `insufficient_options`. |
| `SHORTLISTING`, when `g = 1` | `judge`, then `tally`. | Anonymous proposals; `decide.shortlist.md`; merged options with source references and a priority order. The app keeps at most four: more distinct proposers first, then the judge's order. | The judge may merge duplicates and may not add an option nobody proposed. Fewer than two kept options fails. |
| `PROPOSING_CRITERIA` | `fanout`, fresh context. | Decision and the final option list; `decide.criteria.md`; up to 6 `{localId, name, definition, weight}` with integer weights summing to 100. | Barrier. Fewer than two accepted lists fails with `insufficient_criteria`. |
| `FREEZING` | `judge`, then `tally`. | Anonymous criteria lists; `decide.freeze.md`; canonical criteria `{criterionId, name, definition, sources: [{memberLabel, localId}]}` and a priority order. The app applies the support rule, the cap and the renormalisation. | The judge merges synonyms only. It cannot add a criterion, drop one, or set a weight. Fewer than two kept criteria fails with `no_shared_rubric`. |
| `SCORING` | `fanout`, fresh context. | Decision, options in an order shuffled per member, and the frozen criteria with definitions and **no weights**; `decide.score.md`; exactly one `{optionId, criterionId, score, evidence}` per cell, `score` an integer from 0 to 10, `evidence` one line of at most 25 words; 4,096 visible tokens. | Barrier locks scores. Fewer than two accepted score sheets fails with `insufficient_scores`. |
| `COMPUTING` | `tally`. | Matrix, totals, winner, margin, leave-one-out table, flip thresholds, contested cells. | Persist inputs and unrounded results. |
| `EXPLAINING` | *Blind judge.* | The app's matrix, weights, sensitivity results and the anonymous evidence lines; `decide.explain.md`; `{flipConditions: [{criterionId, rivalOptionId, text}], dissents: [{optionId, text, cellRefs}], evidenceGaps: [up to 5]}`. | Judge exhaustion fails with `judge_failed`; the matrix and sensitivity stay exportable and are still shown. |
| `REPORTING` | `report`. No model call. | Assemble from committed records. | Commit `decide.report`, enter `COMPLETE`. |

**Frozen rubric.** For each canonical criterion c, support `s(c)` is the number of distinct members with at least one source in c. Keep c when `s(c) ≥ 2`. Member i's weight `w_i(c)` is the sum of the weights of its sources in c, or 0. With n accepted lists, `W(c) = (1 / n) × Σ_i w_i(c)`. If more than seven criteria are kept, retain the seven with the highest `W`, ties broken by higher support and then the judge's order. Over the kept set K, `ŵ(c) = W(c) / Σ_{k ∈ K} W(k)`. Weights are stored unrounded and shown as whole percentages. Criteria named by one member are listed in the report as dropped, with their proposer visible to the human.

**Scores and totals.** `S(o, c)` is the median of the accepted members' scores for that cell; with an even count it is the mean of the two middle values. The median keeps one inflated or deflated scorer from moving a cell. `T(o) = Σ_{c ∈ K} ŵ(c) × S(o, c)`, on the 0 to 10 scale. The leader `a` is the option with the highest `T`; the margin is `T(a)` minus the next highest total. A cell is contested when its highest and lowest member scores differ by 4 or more.

**Too close to call.** If the margin is below 0.25, the report names no winner and says `Too close to call between {a} and {b}.`

**Leave one criterion out.** For each `c ∈ K`, recompute totals with `ŵ_{−c}(k) = ŵ(k) / (1 − ŵ(c))` for `k ≠ c`, and record the leader. The report always shows the ranking with each criterion removed, so one fashionable criterion cannot decide the result unseen.

**Flip thresholds.** For leader `a`, rival `b` and criterion `c`, let the rest score be `R(o, c) = (T(o) − ŵ(c) × S(o, c)) / (1 − ŵ(c))`, `Δ_S = S(b, c) − S(a, c)` and `Δ_R = R(a, c) − R(b, c)`. Moving c's weight to `w′` while scaling the other weights in proportion gives `T′(o) = w′ × S(o, c) + (1 − w′) × R(o, c)`. When `Δ_S` and `Δ_R` are both nonzero and share a sign, `b` overtakes `a` at:

`w* = Δ_R / (Δ_R + Δ_S)`,

above `w*` when `Δ_S > 0` and below it when `Δ_S < 0`. Otherwise changing c's weight alone never flips that pair. For each criterion the report shows the rival and threshold nearest to the current weight.

**Robustness.** `robust` when the leader survives every leave-one-out and no flip threshold lies within 0.10 of its criterion's current weight; otherwise `fragile`. The report also gives the number of members whose own scores, under the frozen weights, put the leader first.

**Judge limits.** The judge merges option and criteria wording and, at the end, writes flip conditions in plain language and dissents. Its final schema has no winner, ranking, score or weight field. Every flip condition must reference an app-computed threshold or leave-one-out change; a reference to anything else is invalid. The judge cannot pick a new winner, and the winner line is rendered by the app.

**Calls.** `2m + 2 + g × (m + 1)`: m criteria lists, one freeze, m score sheets and one explanation, plus m option proposals and one shortlist when options are generated. Six members cost 14 with options given and 21 without. Reserved attempts are `4m + 6 + g × (2m + 3)`.

| Step | Calls × attempts | Data bound |
|---|---|---|
| `decide.options` | gm × 2 | `J(decision)` |
| `decide.shortlist` | g × 3 | `B(decision) + m · P(L(V2, 100))` |
| `decide.criteria` | m × 2 | `J(decision) + 4 · L(V2, 50)` |
| `decide.freeze` | 1 × 3 | `B(decision) + m · P(L(V2, 300))` |
| `decide.score` | m × 2 | `J(decision) + 4 · L(V2, 50) + 7 · L(V2, 60)` |
| `decide.explain` | 1 × 3 | `B(decision) + m · P(L(V4, 700)) + 4 · ARTIFACT_FRAME` |

**Prompts.** `decide.options.md` (100 words), `decide.shortlist.md` (300), `decide.criteria.md` (300), `decide.freeze.md` (500), `decide.score.md` (700), `decide.explain.md` (`W_plan`).

**Report.** Winner line or too-close line with the robustness label; frozen rubric with weights and support; score matrix with contested cells marked; totals and margin; leave-one-out table; flip thresholds with the judge's plain-language conditions; dissents; dropped criteria; evidence gaps; skipped members. `/export decide [id]` adds every proposal, each member's weights and score sheet with evidence lines, the shortlist and rubric provenance, and all tally inputs.

**TUI.** Banner `DECIDE`; dividers `OPTIONS`, `RUBRIC`, `SCORES`, `WHAT FLIPS IT`. The matrix is a table with options as columns; contested cells are marked; the leave-one-out view is a second table the human can toggle.

**Acceptance tests.**

- Six members make 14 calls with options given and 21 without; reservations are 30 and 45.
- More than four options or a single option after `--` is refused by the parser.
- Rubric fixtures: a singleton criterion is dropped and listed; an eighth criterion is cut by `W`; weights renormalise to 1.
- No score prompt contains a weight; option order differs between members.
- Totals, leave-one-out leaders and `w*` match hand-computed fixtures, including the no-flip cases where the signs differ.
- A margin of 0.2 yields the too-close line and no winner.
- A judge output that names a winner or cites an unknown threshold is rejected.
- Fewer than two shared criteria fails with `no_shared_rubric` and keeps the proposals exportable.

### Negotiate execution

```rust
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NegotiateStatus {
    Preflight,
    WaitingForParticipants,
    Mapping,
    Briefing,
    Packaging,
    Rehearsing,
    WritingPlaybook,
    Reporting,
    Complete,
    Stopped,
    Failed,
}
```

- `NegotiateSetup` — Frozen our-side text, counterpart text, participants, round count, table count and role draw.
- `NegotiateMap` — One member's interests, BATNA and ZOPA estimates.
- `NegotiateBrief` — The strategy sheet, the public scenario and the hidden counterpart briefs.
- `NegotiatePackage` — One trade package.
- `NegotiateTurn` — One move or counter at one table.
- `NegotiateTable` — App-computed outcome of one table.
- `NegotiatePlaybook` — The judge's playbook.
- `NegotiateReport` — The assembled result.
- `NegotiateCaseMaterial` — Step-scoped briefing or playbook inputs.

`@all $negotiate <our side>` prepares and rehearses a negotiation: goal, fallback, red lines and what is known of the other side go in the text. Models play both sides in v1; the human reads the rehearsal and the playbook.

**Roles.** The table count is `P = min(3, ⌊m / 2⌋)`: 2 for four or five members, 3 for six or more. At admission the app draws P counterpart players and, from the rest, P negotiators, persists the draw, and shows it to the human. Remaining members take part in mapping and packaging only.

| State | Move and actors | Visibility, template and output | Completion and failure |
|---|---|---|---|
| `MAPPING` | `fanout` to every participant, fresh context. | Our-side and counterpart text; `negotiate.map.md`; `{ourInterests, ourBatna, counterpartInterests: [{text, basis}], counterpartBatna, zopa: [{issue, ourLimit, theirLikelyLimit}], tactics}` with `basis` `supplied` or `inferred`. | Barrier. Fewer than two accepted maps fails with `insufficient_maps`. |
| `BRIEFING` | `judge`. | Both texts and the anonymous maps; `negotiate.brief.md`; `{strategySheet, publicScenario, counterpartBriefs: [P × {persona, priorities, reservations: [{issue, limit}], hiddenConstraint, walkAwayRule}], assumptionsToConfirm}`. | Judge exhaustion fails the mode. `publicScenario` must contain nothing from our BATNA, limits or red lines; the prompt requires it and the brief is flagged hypothetical. |
| `PACKAGING` | `fanout` to every participant who is not a counterpart player, fresh context. | Our-side text and the strategy sheet; never a counterpart brief; `negotiate.package.md`; one `{opener, gives, gets, walkAway}`. | Barrier. A negotiator with no accepted package opens from the strategy sheet. |
| `REHEARSING` | P tables in parallel, each a `turns` sequence of up to R rounds; a round is one negotiator move and one counterpart reply. Tables are mutually invisible. | Negotiator: our-side text, strategy sheet, its own package and its table's transcript; `negotiate.move.md`; `{message, offer}`. Counterpart: the public scenario, its own hidden brief and its table's transcript; `negotiate.counter.md`; `{message, response, counterOffer, violatedInterest}` with `response` one of `accept`, `reject`, `counter`, `walk_away`. | A table ends on `accept` (`deal`), `walk_away` (`no_deal`), after R rounds (`unfinished`), or when either player exhausts its attempts (`abandoned`). A failed turn is never read as a concession. |
| `WRITING_PLAYBOOK` | `judge`. | Everything: maps, briefs, packages and transcripts; `negotiate.playbook.md`; `{opening, anchors, concessionLadder: [{give, onlyFor}], objections: [{objection, response}], walkAwayLine, zopaMap: [{issue, ourLimit, theirModelledLimit, overlap}], questionsToAsk, tableNotes: [{table, leftOnTable, unrealisticConcessions}], dependsOnAssumptions}`. | Judge exhaustion fails with `judge_failed`; transcripts stay exportable. |
| `REPORTING` | `tally`, then `report`. No model call. | Table outcomes and counts, then the playbook. | Commit `negotiate.report`, enter `COMPLETE`. |

**Hidden briefs.** A counterpart brief is delivered only to its counterpart player and the judge case. Negotiators, packagers and other counterpart players never receive it. The human sees every brief, marked `Hypothetical counterpart, not a fact`. A counterpart may walk away; its prompt says a deal outside its reservations is a failure for its side.

**Tally.** There is no numeric pooling in this mode. The app computes only counts: outcomes per table, rounds used, and whether each accepted deal's stated terms were flagged by the judge as breaching the hidden reservations.

**Judge limits.** The judge writes the briefs and the playbook. It marks every recommendation that rests on an inferred counterpart interest in `dependsOnAssumptions`, and may not present a modelled limit as known. `overlap` is `yes`, `no` or `unknown`.

**Calls.** `2m − P + 2PR + 2`: m maps, one briefing, `m − P` packages, up to `2PR` rehearsal turns and one playbook. Six members with the default `NEGOTIATE_ROUNDS` of 3 cost 29; four members cost 20. Reserved attempts are `4m − 2P + 4PR + 6`. Tables that end early release their unused reservations.

| Step | Calls × attempts | Data bound |
|---|---|---|
| `negotiate.map` | m × 2 | `J(ourSide) + J(counterpart)` |
| `negotiate.brief` | 1 × 3 | `J(ourSide) + J(counterpart) + m · P(L(V2, W_room))` |
| `negotiate.package` | (m − P) × 2 | `J(ourSide) + L(V4, 600)` |
| `negotiate.move` | PR × 2 | `J(ourSide) + L(V4, 600) + L(V2, 200) + 2R · L(V2, 200)` |
| `negotiate.counter` | PR × 2 | `L(V4, 300) + L(V4, 300) + 2R · L(V2, 200)` |
| `negotiate.playbook` | 1 × 3 | `J(ourSide) + J(counterpart) + m · P(L(V2, W_room)) + A(V4) + (m − P) · L(V2, 200) + 2PR · L(V2, 200)` |

**Prompts.** `negotiate.map.md` (`W_room`), `negotiate.brief.md` (`W_plan`), `negotiate.package.md` (200), `negotiate.move.md` (200), `negotiate.counter.md` (200), `negotiate.playbook.md` (`W_plan`).

**Report.** Role draw; interest map with supplied and inferred marked; BATNA and ZOPA map; packages; per-table transcript and outcome; playbook (opening, anchors, concession ladder, objection table, walk-away line, questions to ask); assumptions to confirm; skipped members and abandoned tables. `/export negotiate [id]` adds every map, the hidden briefs, and every turn with its table and round.

**TUI.** Banner `NEGOTIATE`; dividers `PREP`, `TABLE 1` to `TABLE 3`, `PLAYBOOK`. Tables render side by side when the terminal is wide enough and stacked otherwise. Counterpart messages carry a `counterpart` tag.

**Acceptance tests.**

- Six members with R of 3 make 29 calls and reserve 60 attempts; four members make 20.
- No negotiator or package prompt contains any counterpart brief text; no counterpart prompt contains our BATNA, limits or red lines, or another table's transcript.
- The role draw persists across restart and is never redrawn.
- A counterpart failure ends its table as `abandoned` with no deal recorded.
- `walk_away` ends a table as `no_deal` and releases the remaining turn reservations.
- With every table abandoned, the playbook still runs and the report says there is no rehearsal evidence.
- Three members are refused by the parser.

### Blow_it_up execution

```rust
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BlowItUpStatus {
    Preflight,
    WaitingForParticipants,
    Attacking,
    Challenging,
    Ruling,
    Reporting,
    Complete,
    Stopped,
    Failed,
}
```

- `BlowItUpSetup` — Frozen target, scope, participants and the private lens draw.
- `BlowItUpAttack` — One attacker's vectors and its diversity-slot finding.
- `BlowItUpPool` — The anonymous vector pool with private attacker provenance.
- `BlowItUpChallenge` — One member's verdict on every pooled vector it did not write, plus additions.
- `BlowItUpTally` — App-computed challenge counts and pre-ruling status per vector.
- `BlowItUpRuling` — Deduplicated findings with severity, likelihood, mitigation and rebuttal status.
- `BlowItUpReport` — The ranked findings and the killed appendix.
- `BlowItUpCaseMaterial` — Target, pool, tallies and challenge reasons.

`@all $blow_it_up` takes a plan, spec, prompt, config or diff as a `PlanSource` and attacks it. Use census or crosscheck to find what a plan forgot. Use this to find how it breaks or is exploited.

**Lenses.** The ordered lens list is: outside attacker; malicious insider or abusive user; operator error and operations failure; dependency and supply-chain failure; incentives, cost and fraud; legal, compliance and reputation. At admission the app draws a random participant-to-lens permutation, wrapping after six, and persists it. A lens binds only the attacker's diversity slot.

| State | Move and actors | Visibility, template and output | Completion and failure |
|---|---|---|---|
| `ATTACKING` | `fanout` to every participant, fresh context. | Target, scope and one assigned lens; `blow_it_up.attack.md`; up to 5 vectors `{localId, title, targetQuote, exploit, severity, likelihood, blastRadius}` and exactly one further vector marked `diversity: true` drawn from the assigned lens, described to the member as the finding it believes no other attacker will report. `severity` is `critical`, `high`, `medium` or `low`; `likelihood` is `likely`, `possible` or `unlikely`. | `targetQuote` must pass `verbatim` against the target; a vector that fails is invalid and goes to repair. Barrier after all slots resolve. Fewer than two accepted attack sets fails with `insufficient_attacks`. |
| Pooling | `tally`, `barrier`. | Mint opaque vector IDs, shuffle, strip lens, diversity flag and attacker. | Persist the pool and provenance together. |
| `CHALLENGING` | `fanout` to every participant, fresh context. | Target and the anonymous pool minus the member's own vectors; `blow_it_up.challenge.md`; exactly one `{vectorId, verdict, reason, duplicateOf, defeatingQuote}` per supplied vector and up to 2 added vectors. `verdict` is `holds`, `weak`, `false_positive` or `duplicate`. `false_positive` requires a `defeatingQuote` that passes `verbatim` against the target; `duplicate` requires `duplicateOf`; reasons are at most 20 words; 4,096 visible tokens. | Barrier locks ballots. A failed ballot contributes no verdict. |
| Tally | `tally`. | Per-vector counts and pre-ruling status. | Persist before ruling. |
| `RULING` | *Blind judge.* | Target, pool, app tallies and anonymous challenge reasons; `blow_it_up.rule.md`; findings `{findingId, title, sourceVectorIds, severity, likelihood, exploit, blastRadius, mitigation, rebuttalStatus, residual}` and notes on killed vectors. `rebuttalStatus` is `survived` or `weakened`. | Judge exhaustion fails with `judge_failed`; the pool and tallies stay exportable. |
| `REPORTING` | `tally`, then `report`. No model call. | Rank findings, attach finder counts, build the killed appendix. | Commit `blow_it_up.report`, enter `COMPLETE`. |

**Challenge tally.** For vector v let `n_v` be its accepted verdicts, `f_v` the number of `false_positive`, and `q_v` the number of `weak` or `false_positive`. The vector is `killed` when `n_v ≥ 2` and `f_v > n_v / 2`; otherwise `contested` when `n_v ≥ 2` and `q_v > n_v / 2`; otherwise `standing`. With `n_v < 2` it is `unchallenged` and the report says the false-positive filter did not run on it. Missing ballots are not votes. Peers kill false positives by majority and by quoting the text that defeats them, not by a judge's rubric.

**Ranking.** The judge sets each finding's final severity and likelihood. The app orders findings by `score = sev × lik`, with critical 4, high 3, medium 2, low 1 and likely 3, possible 2, unlikely 1; ties break by more distinct attackers among the finding's blind source vectors, then the judge's order. Finder count comes from the blind attack step only. Challenge-stage additions are kept, marked as additions, and carry no finder count.

**Lone findings.** A surviving finding whose only blind source is one attacker is highlighted as something the other attackers missed, as census highlights singletons.

**Judge limits.** The judge deduplicates, ranks, writes mitigations and sets rebuttal status. Every finding must reference at least one pooled vector that is not `killed`. The judge cannot reinstate a killed vector or invent a finding; it may add a one-line note to a killed vector saying the kill looks wrong, and that note is shown in the appendix. A `contested` source must yield `weakened` unless the judge states why the challenges fail.

**Calls.** `2m + 1`: m attacks, m challenges and one ruling. Six members cost 13. Reserved attempts are `4m + 3`.

| Step | Calls × attempts | Data bound |
|---|---|---|
| `blow_it_up.attack` | m × 2 | `J(target) + J(scope)` |
| `blow_it_up.challenge` | m × 2 | `B(target) + P(m · L(V4, 700))` |
| `blow_it_up.rule` | 1 × 3 | `B(target) + P(m · L(V4, 700)) + m · P(L(V4, W_plan))` |

**Prompts.** `blow_it_up.attack.md` (700 words), `blow_it_up.challenge.md` (`W_plan`), `blow_it_up.rule.md` (`W_plan`).

**Report.** Ranked findings with severity, likelihood, exploit, blast radius, mitigation, rebuttal status and residual risk; lone findings; additions; the killed appendix with tallies, defeating quotes and judge notes; unchallenged vectors; skipped members. `/export blow_it_up [id]` adds the target snapshot, the lens draw, every attack set, the pool with provenance, every ballot and the tallies.

**TUI.** Banner `BLOW IT UP`; dividers `ATTACKS`, `CHALLENGE`, `WHAT SURVIVED`. Findings are colour-coded by severity. The killed appendix is collapsed by default.

**Acceptance tests.**

- Six members make 13 calls and reserve 27 attempts.
- Every accepted attack set has exactly one diversity vector; a set without one is repaired once and then skipped.
- No challenge prompt contains its member's own vectors, a lens, a diversity flag, a finder count or a nick.
- Tally fixtures cover `killed`, `contested`, `standing` and `unchallenged`, including the exact-half case.
- A `false_positive` without a verbatim defeating quote is invalid.
- A judge finding that references only killed vectors, or no vector, is rejected.
- Ranking fixtures match `sev × lik` and each tie-break level.
- `@nick $blow_it_up` attacks that member's latest reply with the whole room.

### Readback execution

```rust
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReadbackStatus {
    Preflight,
    WaitingForParticipants,
    Interpreting,
    Diffing,
    AwaitingHuman,
    Rewriting,
    Reporting,
    Complete,
    Stopped,
    Failed,
}
```

- `ReadbackSetup` — Frozen spec version 1, participants and round count.
- `ReadbackInterpretation` — One member's blind reading of the spec.
- `ReadbackLedger` — The ambiguity ledger for one round, with private reader provenance.
- `ReadbackChoices` — The human's chosen readings for one round.
- `ReadbackRewrite` — A new spec version and its change log.
- `ReadbackReport` — Final spec, ledgers per round and the convergence table.
- `ReadbackCaseMaterial` — Step-scoped diff or rewrite inputs.

`@all $readback` takes a spec, ticket, prompt or contract clause as a `PlanSource`. Members each say what they would build from it. Where their readings differ in a way that changes behaviour, the human picks the intended reading and the judge rewrites the passage. `READBACK_ROUNDS` (R, default 2) bounds the loop.

| State | Move and actors | Visibility, template and output | Completion and failure |
|---|---|---|---|
| `INTERPRETING` | `fanout` to every participant, fresh context each round. | The current spec version only: no earlier version, ledger, choice or interpretation; `readback.interpret.md`; `{build, behaviours: [up to 8 {localId, situation, behaviour, specQuote}], edgeCases: [up to 6 {case, answer}], assumptions: [up to 6 {text}]}`. | Barrier. Fewer than two accepted interpretations fails with `insufficient_readings`. |
| `DIFFING` | *Blind judge.* | The spec version and the labelled interpretations; `readback.diff.md`; up to 10 entries `{entryId, passage, question, readings: [2 to 4 {readingId, text, sourceLabels}], behaviourChange}`. | `passage` must pass `verbatim` against the spec; every reading needs at least one source label; no two readings of an entry may have identical source sets and text. An empty ledger ends the loop with reason `converged`. Judge exhaustion fails the mode. |
| `AWAITING_HUMAN` | Human step. | Banner `READ IT BACK`. Each entry shows its passage, question and lettered readings with the number of members behind each. | See the answer rules below. |
| `REWRITING` | `judge`. | The spec version, the ledger and the human's choices; `readback.rewrite.md`; `{spec, changes: [{entryId, before, after}]}`. | The app checks that each `before` passes `verbatim` against the old version, that every unresolved entry's passage still passes `verbatim` against the new version, and that `changes` covers exactly the resolved entries. Judge exhaustion fails the mode and keeps the last good version. |
| `REPORTING` | `tally`, then `report`. No model call. | Final version, ledgers, choices, convergence table. | Commit `readback.report`, enter `COMPLETE`. |

**Ledger rule.** An entry exists only when the readings would lead to different behaviour or a different outcome, and `behaviourChange` states that difference. Differences of wording, emphasis or ordering are not entries. The cap is ten per round; the judge orders entries by impact and the app keeps the first ten, reporting the count cut.

**Human answers.** A choice-token message such as `1b 2a 4c` picks a lettered reading per entry. `/answer 3: <text>` supplies the human's own reading for entry 3. Several answer messages may be sent; a later answer for the same entry replaces the earlier one. The step completes when every entry is answered, or on `/continue`. An unanswered entry stays unresolved: its passage is left untouched and it is carried into the report as open. If nothing was answered, the readback completes with reason `no_readings_chosen` and no rewrite call. Member support counts are shown to the human only; they are never an instruction and no reading is preselected.

**Loop.** After a rewrite in round `r < R`, the next round interprets the new version in fresh contexts. After a rewrite in round R the readback completes with reason `rounds_complete`, and the report says the last rewrite was not read back.

**Convergence table.** Per round: spec version, accepted interpretations, ledger entries, entries resolved, entries left open. The count of entries is the convergence measure; the app computes it and the judge does not score convergence.

**Judge limits.** The judge reports divergence and rewrites to the human's choices. It does not choose a reading, does not resolve an entry the human left open, and changes no passage outside the resolved entries.

**Calls.** At most `R × (m + 2)`: per round m interpretations, one diff and one rewrite. Six members at the default R of 2 cost at most 16. Reserved attempts are `R × (2m + 6)`. Converging early releases the unused rounds.

| Step | Calls × attempts | Data bound |
|---|---|---|
| `readback.interpret` | Rm × 2 | `2 · J(spec)` |
| `readback.diff` | R × 3 | `2 · B(spec) + m · P(L(V2, W_room))` |
| `readback.rewrite` | R × 3 | `2 · J(spec) + A(V4) + 10 · L(V2, 100)` |

A rewritten spec is bounded at twice the frozen version 1 size; a rewrite beyond that is invalid.

**Prompts.** `readback.interpret.md` (`W_room`), `readback.diff.md` (`W_plan`), `readback.rewrite.md` (twice the spec's word count, at least `W_plan`).

**Report.** Final spec; the ambiguity ledger per round (passage, readings, who read it which way, chosen meaning, new wording); open entries; convergence table; skipped members. `/export readback [id]` adds every spec version, every interpretation and the human's raw answers.

**TUI.** Banner `READBACK`; dividers `ROUND n`, `READINGS`, `READ IT BACK`, `REWRITE`. The human step uses the shared human-step widget; the composer placeholder shows `1b 2a ... or /answer 3: your reading`. Rewrites render as before and after pairs.

**Acceptance tests.**

- Six members with R of 2 make at most 16 calls and reserve 36 attempts.
- An interpretation prompt in round 2 contains the new version only.
- A ledger passage that is not verbatim in the spec is rejected and repaired once.
- Choice tokens, `/answer n: text`, replacement of an earlier answer, an unknown entry and an unknown letter all behave as specified; an ordinary message during the step is handled normally and leaves the step pending.
- `/continue` with one of three entries answered rewrites that one passage and leaves the other two byte-identical.
- An empty ledger completes with `converged` and makes no rewrite call.
- `AWAITING_HUMAN` releases holds, keeps the claim, survives restart and never expires.
- A rewrite that alters an unresolved passage is rejected.

### Debug execution

Rust identifiers use the `Debugging` prefix and the module name `debugging`; the wire name is `debug`.

```rust
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DebuggingStatus {
    Preflight,
    WaitingForParticipants,
    Hypothesizing,
    Merging,
    ProposingTests,
    Selecting,
    AwaitingHuman,
    Eliminating,
    Reporting,
    Complete,
    Stopped,
    Failed,
}
```

- `DebuggingSetup` — Frozen brief, limits, participants and round count.
- `DebuggingHypotheses` — One member's blind hypothesis table.
- `DebuggingDifferential` — The merged differential with private provenance and each hypothesis's status.
- `DebuggingTestProposal` — One member's proposed tests with predicted outcomes.
- `DebuggingSelection` — The chosen test, its outcome labels and the merged predictions.
- `DebuggingResult` — The human's outcome label and pasted evidence.
- `DebuggingElimination` — App-computed status changes for one round.
- `DebuggingReport` — Final differential, evidence ledger, next test and nullable patch.
- `DebuggingCaseMaterial` — Step-scoped merge, selection or report inputs.

`@all $debug <symptom, evidence, what was tried>` runs a differential diagnosis that advances only on tests the human runs. Models are text-only and run nothing. Probe gives a decision tree in one pass; debug keeps a live differential and updates it with real results. `DEBUG_ROUNDS` (R, default 3) bounds the loop.

| State | Move and actors | Visibility, template and output | Completion and failure |
|---|---|---|---|
| `HYPOTHESIZING` | `fanout` to every participant, fresh context. | Brief and limits; `debug.hypothesize.md`; up to 4 `{localId, cause, supporting: [quote], contradicting: [quote], prior}` with `prior` `high`, `medium` or `low`. | Each quote is checked with `verbatim` against the brief; one that fails is kept and marked `unanchored`, and never counts as evidence. Barrier. Fewer than two accepted tables fails with `insufficient_hypotheses`. |
| `MERGING` | *Blind judge.* | Brief and labelled tables; `debug.merge.md`; up to 8 hypotheses `{hypothesisId, cause, sources, supporting, contradicting, priorSpread}`. | The judge merges duplicates and keeps disagreement: differing priors are kept as a spread and contradicting evidence is never dropped. It does not rank to one cause or drop a minority hypothesis. Judge exhaustion fails the mode. |
| `PROPOSING_TESTS`, each round | `fanout` to every participant, fresh context. | Brief, limits, the anonymous differential with statuses, and the evidence ledger of earlier results; `debug.test.md`; up to 2 `{localId, test, procedure, effort, safety, outcomes: [2 to 4 labels], predictions: [{hypothesisId, outcome, cannotPredictReason}]}` and one nullable `newHypothesis`. `effort` is `minutes`, `hour` or `day`; `safety` is `read_only`, `reversible` or `risky`. Every live hypothesis appears once per test, with exactly one of `outcome` and `cannotPredictReason`. | Barrier locks proposals and predictions. No accepted proposal ends the loop with reason `no_tests`. |
| `SELECTING`, each round | *Blind judge.* | Brief, limits, differential, evidence ledger and the anonymous proposals; `debug.select.md`; `{testRef, instruction, outcomes: [{label, description}], predictions: [{hypothesisId, outcome}], addedHypotheses, notSelected: [{testRef, why}]}`. `outcome` is an outcome label, `disputed` or `cannot_predict`. | `testRef` must name a proposed test whose safety is not `risky`. The merged predictions must put at least two live hypotheses on different outcome labels; otherwise the output is invalid. If no proposed test can do that, the judge returns `testRef: null` and the loop ends with reason `no_discriminating_test`. Judge exhaustion fails the mode. |
| `AWAITING_HUMAN`, each round | Human step. | Banner `RUN THE TEST`. Shows the instruction, effort, safety and outcome labels `O1` to `On`. | See the answer rules below. |
| `ELIMINATING`, each round | `tally`. | Status changes and the evidence ledger entry. | Persist together. Then loop, or go to reporting. |
| `REPORTING` | One judge-backed `report`. | Brief, final differential, evidence ledger and the app's `patchAllowed` flag; `debug.report.md`; `{summary, nextTest, patch, patchRationale, risks}` with `patch` nullable. | Validate, commit `debug.report`, enter `COMPLETE`. Judge exhaustion fails with `judge_failed`; the differential and ledger stay exportable. |

**Human answers.** `/answer O2 <what you saw>` records outcome `O2` with the pasted text as evidence. `/answer other <what you saw>` records a result that matches no listed outcome. `/answer unknown` and `/continue` record that the test was not run or was inconclusive. The pasted text is added to the evidence ledger and delivered to later steps.

**Elimination.** Elimination is application code over committed predictions and the human's label; no model decides it.

- With outcome `Ox` recorded, a live hypothesis becomes `eliminated` when its merged prediction for the selected test is an outcome label other than `Ox`. A hypothesis whose prediction is `Ox` gains one consistent result. `disputed` and `cannot_predict` change nothing.
- With `other` recorded, nothing is eliminated, the result is marked `unexpected`, and the next round's proposals may each add a hypothesis.
- With `unknown` recorded, nothing changes. An unrun test is never treated as confirmation or as elimination.

No status is called confirmed. A hypothesis is `live` or `eliminated`, and a live one carries its count of consistent results.

**Loop end.** After elimination the loop ends with `isolated` when exactly one hypothesis is live, `all_eliminated` when none is, and `rounds_complete` after round R. Otherwise the next round starts at `PROPOSING_TESTS`.

**Patch gate.** `patchAllowed` is true only when exactly one hypothesis is live and it has at least one consistent result from a test the human ran. When it is false, a non-null `patch` is invalid and the report says `No patch: the evidence does not yet isolate a cause.` When true, the patch must be minimal, must address the one live hypothesis, and is advice the human applies; nothing is run.

**Judge limits.** The judge merges, selects a test from those proposed and writes the final summary. It cannot propose its own test, eliminate a hypothesis, mark a test as run, or supply a result.

**Calls.** At most `(R + 1) × (m + 1) + 1`: m tables and one merge, then per round m proposals and one selection, then one report. Six members at the default R of 3 cost at most 29. Reserved attempts are `2m × (R + 1) + 3R + 6`. Ending early releases unused rounds.

| Step | Calls × attempts | Data bound |
|---|---|---|
| `debug.hypothesize` | m × 2 | `J(brief) + J(limits)` |
| `debug.merge` | 1 × 3 | `B(brief) + m · P(L(V2, W_room))` |
| `debug.test` | Rm × 2 | `J(brief) + J(limits) + A(V4) + R · L(V2, 400)` |
| `debug.select` | R × 3 | `B(brief) + J(limits) + A(V4) + R · L(V2, 400) + m · P(L(V2, W_room))` |
| `debug.report` | 1 × 3 | `B(brief) + J(limits) + A(V4) + R · L(V2, 400)` |

A pasted result is capped at 400 words inside prompts; the full text stays in the human transcript and the export.

**Prompts.** `debug.hypothesize.md` (`W_room`), `debug.merge.md` (`W_plan`), `debug.test.md` (`W_room`), `debug.select.md` (600), `debug.report.md` (`W_plan`).

**Report.** Current differential with status and consistent-result counts; evidence ledger (test, outcome label, pasted result per round); eliminated hypotheses with the prediction that failed; the next best test; risky tests that were proposed and not selected; the patch or the no-patch line; unanchored quotes; skipped members. `/export debug [id]` adds the brief, every hypothesis table, every proposal with predictions, each selection and the human's raw answers.

**TUI.** Banner `DEBUG`; dividers `DIFFERENTIAL`, `ROUND n`, `RUN THE TEST`, `RESULT`. The differential is a table that greys eliminated rows. The human step shows the instruction in a box with the outcome labels and the placeholder `/answer O1 <what you saw>`.

**Acceptance tests.**

- Six members with R of 3 make at most 29 calls and reserve 63 attempts.
- Hypothesis and test prompts contain no other member's table or proposal.
- A selection naming a `risky` test, an unproposed test, or a test that does not split two live hypotheses is rejected.
- Elimination fixtures: matching label, differing label, `disputed`, `cannot_predict`, `other` and `unknown`. `unknown` and `/continue` change no status.
- A report with a patch while two hypotheses are live, or while the one live hypothesis has no consistent result, is rejected.
- `isolated`, `all_eliminated`, `no_tests`, `no_discriminating_test` and `rounds_complete` each end the loop with the right reason and release unused reservations.
- No artifact or report text gives any hypothesis a confirmed status.
- `AWAITING_HUMAN` survives restart with the same instruction and outcome labels.

### Trace execution

```rust
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TraceStatus {
    Preflight,
    WaitingForParticipants,
    Extracting,
    Merging,
    Verifying,
    Assembling,
    Reporting,
    Complete,
    Stopped,
    Failed,
}
```

- `TraceSetup` — Frozen question, numbered sources, participants.
- `TraceExtraction` — One member's claims with quotes and counter-quotes.
- `TraceQuoteCheck` — App-computed verbatim result and span for every quote.
- `TraceClaims` — Canonical claims with merged verified quotes, private provenance and verifier assignments.
- `TraceVerification` — One verifier's entailment and scope verdicts.
- `TraceLedger` — App-computed status and independent-source count per claim.
- `TraceBrief` — The judge's brief, every paragraph tied to claim IDs.
- `TraceReport` — Brief plus the claim-to-source ledger.
- `TraceCaseMaterial` — Step-scoped merge or assembly inputs.

`@all $trace <question>` followed by `--`-separated sources turns a source packet into a brief a reader can audit. Sources are numbered `S1` to `Sn` in the order given, quoted messages after pasted segments; n is at most 12, and the packet must fit every participant's context or admission fails with `CONTEXT_TOO_LARGE`.

| State | Move and actors | Visibility, template and output | Completion and failure |
|---|---|---|---|
| `EXTRACTING` | `fanout` to every participant, fresh context. | Question and packet; `trace.extract.md`; up to 10 `{localId, claim, quotes: [1 to 3 {sourceId, quote}], counterQuotes: [0 to 2 {sourceId, quote}], scopeNote}`. Quotes are at most 60 words. | Barrier. Fewer than two accepted extractions fails with `insufficient_extractions`. |
| Quote check | `tally`. | Run `verbatim` on every quote against its cited source; store the result and span. | A quote that fails is removed from support and recorded as `quote_not_found`. It is not sent back for repair: a model asked to fix a missing quote tends to invent one. |
| `MERGING` | *Blind judge.* | Question, packet, labelled claims with only their verified quotes; `trace.merge.md`; up to 25 canonical claims `{claimId, text, sources: [{memberLabel, localId}], sharedOrigin: [[sourceId]]}`. | The app attaches the union of verified quotes from the referenced member claims. The judge cannot add a quote or a claim with no source reference. An empty canonical set skips to an empty-brief report and releases later reservations. |
| Assignment | `tally`. | Assign each canonical claim to 2 verifiers. | A member that contributed a source claim is ineligible for it when alternatives exist. Balance counts; break ties with the private seed. Persist before dispatch. |
| `VERIFYING` | `fanout` to every participant with assigned claims, fresh context. | Question, packet and only the assigned claims with their verified quotes, no authorship; `trace.verify.md`; one `{claimId, entailment, scope, qualifierQuote, note}` per assigned claim. `entailment` is `supported`, `partial` or `not_supported`; `scope` is `ok`, `overstated` or `missing_qualifier`; `missing_qualifier` requires a `qualifierQuote` that passes `verbatim`. | Barrier locks verdicts. A failed verifier leaves its verdicts missing. |
| Ledger | `tally`. | Claim status and independent-source count. | Persist before assembly. |
| `ASSEMBLING` | `judge`. | Question and the ledger with verified quotes; `trace.assemble.md`; `{sections: [{heading, paragraphs: [{text, claimIds}]}], disputed: [{claimId, text}], gaps: [text]}`. | Every paragraph needs at least one claim ID; body paragraphs may reference only `supported` claims; `disputed` entries only `disputed` claims. Judge exhaustion fails with `judge_failed`; the ledger stays exportable. |
| `REPORTING` | `report`. No model call. | Brief with claim markers, ledger, flagged claims. | Commit `trace.report`, enter `COMPLETE`. |

**Who checks what.** Whether a quote exists is decided by the app's matcher, which cannot be persuaded. Whether the quote supports the claim, and whether the claim drops a qualifier, needs reading, so two members who did not write the claim check that, blind to its author. An exact quote can still mislead; `scope` and `qualifierQuote` exist for that case.

**Claim status.** Let `V` be the accepted verdicts for a claim and `q` its number of verified quotes.

- `unsupported` when `q = 0`, or when `V` is non-empty and every verdict is `not_supported`.
- `unverified` when `q > 0` and `V` is empty.
- `supported` when `q > 0`, `V` is non-empty, and every verdict is `supported` with scope `ok`.
- `disputed` otherwise.

A claim's independent-source count is the number of distinct source IDs among its verified quotes after collapsing each `sharedOrigin` group to one. Several models agreeing is not several sources agreeing, and the report shows the two numbers separately.

**Judge limits.** The judge merges claims and writes the brief from the ledger. It cannot introduce a claim, quote or source, cannot upgrade a claim's status, and cannot place an unsupported or unverified claim in the body. The app prints those in their own section.

**Calls.** `2m + 2`: m extractions, one merge, m verification batches and one assembly. Six members cost 14. Reserved attempts are `4m + 6`.

| Step | Calls × attempts | Data bound |
|---|---|---|
| `trace.extract` | m × 2 | `J(question) + Σ J(source)` |
| `trace.merge` | 1 × 3 | `B(question) + Σ J(source) + m · P(L(V4, 900))` |
| `trace.verify` | m × 2 | `J(question) + Σ J(source) + P(A(V4))` |
| `trace.assemble` | 1 × 3 | `J(question) + P(A(V4)) + m · P(L(V2, W_room))` |

**Prompts.** `trace.extract.md` (900 words including quotes), `trace.merge.md` (`W_plan`), `trace.verify.md` (`W_room`), `trace.assemble.md` (`W_plan`).

**Report.** The brief with `[C3]`-style claim markers; claim-to-source ledger (claim, status, quotes with source number and span, independent-source count, verifier verdicts); disputed claims with the qualifier quotes; contradictions from counter-quotes; unsupported and unverified claims; quotes not found; evidence gaps; skipped members. `/export trace [id]` adds the numbered packet, every extraction, the quote-check table, assignments and every verdict.

**TUI.** Banner `TRACE`; dividers `EXTRACT`, `QUOTE CHECK`, `VERIFY`, `BRIEF`. Selecting a claim marker shows its quotes and source numbers. Flagged claims are listed under the brief, never inline with supported ones.

**Acceptance tests.**

- Six members make 14 calls and reserve 30 attempts.
- Matcher fixtures: curly quotes, dashes, line-wrapped whitespace, case difference, a stitched ellipsis and a too-short quote.
- A fabricated quote is recorded as `quote_not_found`, is absent from every later prompt, and triggers no repair.
- No verifier is assigned a claim it contributed to when another member is available; no verify prompt contains authorship.
- Status fixtures cover all four states, including one verifier missing.
- A brief paragraph with no claim ID, or one citing an unsupported claim, is rejected.
- Two sources in one `sharedOrigin` group count as one independent source.
- Thirteen sources, or no source, is refused by the parser; an oversized packet fails admission with `CONTEXT_TOO_LARGE`.

### Rank execution

```rust
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RankStatus {
    Preflight,
    WaitingForParticipants,
    Comparing,
    Aggregating,
    Explaining,
    Reporting,
    Complete,
    Stopped,
    Failed,
}
```

- `RankSetup` — Frozen criterion, items, participants, the pair design and its deal.
- `RankBatch` — One member's forced choices with reasons.
- `RankTally` — App-computed win matrix, strengths, order, gaps and cycles.
- `RankExplanation` — The judge's account of the top and of each reported cycle.
- `RankReport` — The assembled result.
- `RankCaseMaterial` — Items, tally and anonymous reasons.

`@all $rank <what to rank by>` followed by `--` and one item per line orders the human's own list: bugs, risks, features, candidates. The text before `--` is the criterion; there is no rubric step. n is the number of items, 3 to 40.

**Pair design.** The number of distinct pairs is `Q = min(n(n − 1) / 2, 3n)`. For `n ≤ 7` that is every pair. Otherwise the app shuffles the items with the private seed and takes the ring of adjacent pairs, which connects every item; it repeats with fresh shuffles, skipping pairs already taken, until it has Q pairs. Each pair is judged by `v = max(1, min(3, ⌊60m / Q⌋))` distinct members, so the total is `vQ` judgments. Judgments are dealt so that member loads differ by at most one, no member gets the same pair twice, and no member gets more than 60. Within each judgment the two items are shown as `A` and `B` in an order drawn at random per judgment. The design and deal are persisted before dispatch.

| State | Move and actors | Visibility, template and output | Completion and failure |
|---|---|---|---|
| `COMPARING` | `fanout` to every participant, fresh context. | Criterion and only the member's own pairs, items formatted identically; `rank.compare.md`; exactly one `{pairId, winner, reason}` per pair, `winner` is `A` or `B`, `reason` at most 12 words; 4,096 visible tokens. | Barrier locks batches. A failed batch leaves its judgments missing. Fewer than two accepted batches, or fewer than `n − 1` judged pairs, fails with `insufficient_comparisons`. |
| `AGGREGATING` | `tally`. | Strengths, order, gaps, split pairs, cycles, connectivity. | Persist inputs and unrounded results. |
| `EXPLAINING` | *Blind judge.* | Criterion, items, the app's order and cycles, and the anonymous reasons for the pairs involved; `rank.explain.md`; `{top: [{itemId, why}], cycles: [{cycleId, reading}], caveats}`. | Judge exhaustion fails with `judge_failed`; the order and cycles stay exportable and are still shown. |
| `REPORTING` | `report`. No model call. | Assemble from committed records. | Commit `rank.report`, enter `COMPLETE`. |

**Aggregation: Bradley-Terry (D86).** Let `w_ij` be the number of accepted judgments in which item i beat item j, and `n_ij = w_ij + w_ji`. Add a phantom item 0 with fixed strength `p_0 = 1` and `w_i0 = w_0i = 0.5` for every item, so `n_i0 = 1`. Let `W_i = Σ_j w_ij`, phantom included. Start every `p_i` at 1 and repeat, for all items together:

`p_i ← W_i / Σ_{j ≠ i} n_ij / (p_i + p_j)`,

with the sum over real items and the phantom, until the largest change in `ln p_i` is below 1e-9 or 1,000 iterations have run. Arithmetic is `f64`, and iteration order is item order, so the result is deterministic. The score is `s_i = ln p_i`. Items are ordered by descending `s`; scores within 1e-9 break by more wins, then input order.

Bradley-Terry is chosen over Copeland because the design is incomplete above seven items. Copeland counts pairwise wins, so an item dealt weak opponents outranks an item dealt strong ones; Bradley-Terry weighs each win by the opponent's strength and gives a gap between neighbours. The phantom item keeps strengths finite for an item that won or lost everything and fixes the scale without a separate normalisation. The report also shows each item's plain win and loss record so the fitted order can be checked by eye.

**Gaps.** For neighbours in the order, `gap = s_(k) − s_(k+1)`, and the implied chance the higher one wins is `1 / (1 + e^(−gap))`. A gap is `clear` when that chance is at least 0.70 and `close` otherwise.

**Cycle audit.** For each judged pair, the majority winner is the item with more wins; an even split gives no majority. A cycle is three items where i has the majority over j, j over k, and k over i, with all three pairs judged. The app finds every such triad, reports the total, and shows up to ten, those with the smallest combined vote margin first. A pair with `v > 1` whose judgments are not unanimous is listed as a split pair.

**Connectivity.** If missing batches leave the judged pairs disconnected, the report names the components and says that order between components rests on no comparison.

**Judge limits.** The judge explains the top five items, or all of them when n is below five, and reads each reported cycle as a likely clash inside the criterion or as noise. Its schema has no order, position or score field. It cannot reorder, break a cycle or drop an item.

**Calls.** `m + 1`: m comparison batches and one explanation. Six members cost 7. Reserved attempts are `2m + 3`.

| Step | Calls × attempts | Data bound |
|---|---|---|
| `rank.compare` | m × 2 | `J(criterion) + 120 · L(V2, 60)` |
| `rank.explain` | 1 × 3 | `B(criterion) + Σ J(item) + A(V4) + 40 · L(V2, 40)` |

An item is capped at 60 words; a longer item is refused at admission.

**Prompts.** `rank.compare.md` (800 words), `rank.explain.md` (`W_room`).

**Report.** The ordered list with score, win and loss record and gap label; the judge's account of the top; cycles with the judge's reading; split pairs; connectivity warnings; the design summary (`Q`, `v`, judgments per member); skipped members. `/export rank [id]` adds every judgment with its member and reason, the win matrix and the fitted strengths.

**TUI.** Banner `RANK`; dividers `COMPARING`, `ORDER`, `CYCLES`. The order is a numbered list with a gap marker between neighbours: a rule for `clear`, a dotted rule for `close`. Progress during comparing shows judgments returned out of `vQ`.

**Acceptance tests.**

- Six members make 7 calls and reserve 15 attempts.
- Design fixtures: n of 3, 7, 8 and 40 give the stated `Q` and `v`; the pair graph is connected; no member holds a pair twice or more than 60 judgments; A and B order varies.
- Bradley-Terry fixtures match hand-computed strengths to 1e-9 for a complete three-item set, an undefeated item, an item with no wins and a sparse design; two runs give identical bits.
- A planted three-cycle is found and reported; an even split yields no majority edge.
- A missing batch that disconnects the graph produces the component warning.
- A judge output that carries an order or omits a reported cycle is rejected.
- Two items, forty-one items, or an item over 60 words is refused.

### Stop, failures and recovery for the nine modes

`/stop` fences every outstanding member and judge work item of the active contest and enters `STOPPED` with reason `stopped`; accepted artifacts remain and export. Stopping during preflight cancels before any money is reserved. `/stop @nick` on a participant stops the containing contest, as in Contests.

A mode that cannot continue because the daily guard rejects its next call ends through the canonical daily-cap failure path with its accepted evidence preserved.

Quorum failures use the reasons named in each mode's table. Each one releases unused reservations and keeps accepted artifacts exportable. A report always names skipped members with the step they failed, and never claims a result that depended on their missing output.

On restart, in addition to the Contests list:

- Restore forge label maps, ballots, the Borda tally, fuser selection and candidate labels.
- Restore the forecast frame, locked first forecasts, delivered bundles and revisions.
- Restore decide shortlists, the frozen rubric, score sheets and option orders.
- Restore the negotiate role draw, hidden briefs, packages and each table's transcript and round.
- Restore blow_it_up lens draws, the pool and its provenance, ballots and tallies.
- Restore readback spec versions, ledgers, partial human answers and the round.
- Restore debug differentials, proposals, selections, recorded results and statuses.
- Restore trace quote checks, canonical claims, verifier assignments and verdicts.
- Restore rank pair designs, deals, A and B orders and accepted batches.
- Restore a pending human step without expiry and without a new model call.

Recovery never reruns an accepted step and never redraws labels, roles, lenses, pairs, assignments or samples.

### Call counts at a glance

| Mode | Normal model calls | Reserved attempts | Six members, defaults |
|---|---|---|---|
| forge | `2m + 3` | `4m + 7` | 15 |
| forecast | `2m + 2` | `4m + 6` | 14 |
| decide | `2m + 2 + g(m + 1)` | `4m + 6 + g(2m + 3)` | 14, or 21 generating options |
| negotiate | `2m − P + 2PR + 2`, `P = min(3, ⌊m/2⌋)` | `4m − 2P + 4PR + 6` | 29 |
| blow_it_up | `2m + 1` | `4m + 3` | 13 |
| readback | at most `R(m + 2)` | `R(2m + 6)` | 16 |
| debug | at most `(R + 1)(m + 1) + 1` | `2m(R + 1) + 3R + 6` | 29 |
| trace | `2m + 2` | `4m + 6` | 14 |
| rank | `m + 1` | `2m + 3` | 7 |

### Later support

Outside M8: outcome scoring for forecasts (a recorded resolution and Brier scores); the human playing our side in a negotiate rehearsal; an author-rebuttal round in blow_it_up; a tie-break comparison batch for rank cycles; quantile forecasts; MCP tools for the six modes without a human step.

### Part 2: Milestones

The task breakdown is in [M8 — New modes](milestones/M8-new-modes.md).
