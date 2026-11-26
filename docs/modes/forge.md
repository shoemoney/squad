# Forge

`$forge` turns independent drafts into one finished deliverable: an email, launch post, design document or function. The authors of the two strongest drafts each write a fusion. A blind judge ranks those fusions alongside the best original. The original stands unless a fusion beats it.

```text
@all $forge Write a launch announcement for our offline notes app
@all $forge Write a short project update -- Under 150 words; include one next action
@claude @openai @gemini $forge Write a polite decline for a conference invitation
```

Choose at least three members. `@judge` cannot participate. Put constraints after `--`. The judge is included automatically.

1. **Drafts:** every participant independently writes the requested artifact, within `planWords` (default 1,200).
2. **Ranking:** members with accepted drafts rank every other accepted draft, under private shuffled labels. They name a steal-worthy element and an optional fatal flaw for each. Nobody votes for their own draft.
3. **Fusion:** the app averages normalized Borda scores, then breaks ties by first places, fewer flaw voters, and a persisted random seed. The two highest-ranked authors independently fuse the drafts. They receive anonymous drafts and deduplicated steals and flaws, with no scores or ranks. A fusion's body cannot exceed the longest accepted draft's word count.
4. **Verdict:** a blind judge ranks the best original and accepted fusions under fresh labels, without knowing which is which. The winning artifact is printed exactly as accepted; the judge cannot rewrite it.

Fewer than three valid drafts or two valid ballots ends the run with an explicit quorum failure. A failed member is skipped for that step. If both fusions fail, the best original wins without a judge call. Invalid member output gets one repair; transport failures get a plain retry. Judges have up to three attempts and at most one schema repair. Stop releases participant holds and unstarted reservations. Restart preserves committed work, labels, tally and fuser selection.

## Cost

With `m` members, the normal maximum is `2m + 3` calls, reserving `4m + 7` attempts. The default six-member roster makes at most 15 normal calls and reserves 31 attempts. Skipped later calls release their unused reservation. Preflight prices each step against its frozen models and full input bounds; normal estimated cost and the conservative reserved ceiling differ. The usual daily cap and confirmation policy apply. Actual cost depends on selected models and output lengths.

## Report and export

The report includes the gate outcome, winning artifact, human-readable provenance, Borda scores, steals, fatal flaws, handled flaws, judge reasons and seams, and skipped members. Drafts are collapsed in the Forge view.

```text
/export forge
/export forge <contest-id>
```

The Markdown export also includes all accepted drafts, ballots with label maps, fusions and the full judge output. Failed runs retain accepted evidence for export. These records are for the human; they do not enter members' ordinary chat histories. Forge awards no tournament trophies and changes no debate records.

## Example run

Not run: the shared outer judge dispatch currently rejects `ForgeJudge`. Live provider spend is $0; no observed wall time, actual cost or real report excerpt is available. Run the owner-approved six-model smoke after the integrator fixes that gate and the full-room test passes.

## Integration status (2026-11-26)

The mode-owned contracts, storage, policy, prompts, executor, report and view are implemented. The shared room runtime still blocks the judge phase. The Forge workflow suite therefore distinguishes direct execution of the owned runtime with real scoped readers and acknowledged router ingress from the pending full-room restart/stop test.

Required integrator change, outside the Forge worker's file ownership:

- `crates/squad-core/src/runtime/judge.rs`, `impl WorkExecutor for Executor`: extend the first structured-mode routing condition with `grant.work.kind.new_mode_judge()`. The inner `Runner` already dispatches new modes to `execute_new_mode`; the outer gate currently sends them to `squad_agents::judge::JudgeExecutor`, whose legacy allowlist rejects `ForgeJudge`. A six-seat full-room fixture completes all fourteen member calls, then leaves the judge work running without a judge HTTP request.

Two shared repair restrictions have mode-owned workarounds:

- `crates/squad-core/src/router/executor.rs`: the failure classifier should include `ErrorCode::MODE_INVALID_OUTPUT` alongside `JUDGE_INVALID_MODE_OUTPUT` in `FailureClass::InvalidOutput`. Forge currently submits the recognized structured-output code for members too, so their second attempt receives repair diagnostics.
- `crates/squad-core/src/prompts/render.rs`: permit the new mode prefixes in the `mode_step` encoder. Forge currently freezes the admitted step name as a literal into the repair suffix, so it does not pass a rejected `forge.*` value through that legacy encoder.

No shared source files were changed by this worker. Enablement and the live smoke require the outer judge dispatch fix, followed by the full-room acceptance test.
