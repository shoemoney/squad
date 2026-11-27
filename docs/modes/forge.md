# Forge

Forge is enabled. Start it from the composer or with `squad send` as shown below.

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

One real OpenRouter run on dev2, 2026-11-26, used the default six-member roster and judge, release head `520d1a2`, a fresh private data directory, `--env-file /dev/null`, and `SQUAD_DAILY_MAX_USD=150`. The key was fetched directly into the process environment; no key or real environment file was saved.

```text
squad send '@all $forge Write a three-sentence launch announcement for PocketNotes, an offline notes app -- Under 75 words. Say notes stay on the device, search works offline, and invite readers to try the free beta. No invented features or claims.' --yes --json
```

The run exited zero and reached `COMPLETE` / `original_won` in **75.90 seconds**, including startup, provider work and shutdown. All six drafts and ballots, both fusions and the judge verdict were accepted. Both fusions used their one validation repair because they paraphrased a supplied flaw; the final prompt now explicitly requires copying that flaw's label and text exactly. The smoke's frozen prompts remain unchanged.

- Plan: 15 normal calls, 31 reserved attempts; estimated **$6.410698**, reserved ceiling **$24.723887**.
- Actual: 17 contest attempts cost **$0.372460**; four startup alias probes cost **$0.001526**; total recorded spend **$0.373986**.
- Final reservations: **$0**; unresolved calls: **0**. Stored provider usage totals **$0.373985**, differing by one microdollar from per-attempt ledger rounding.
- `squad export forge <contest-id> -o forge.md` succeeded. The report and export preserve the accepted winning draft byte for byte. No second live send was made.

Report excerpt:

> # FORGE
>
> The best draft by @openai stands. No fusion beat it.
>
> Meet PocketNotes, an offline notes app that keeps your notes on your device. Search your notes even when you’re offline. Try the free beta today.

Local evidence is retained under `/tmp/squad-forge-r2-smoke.nmUj0Z/`: SQLite ledger, stdout/stderr, wall time, summary and Markdown export. These temporary artifacts contain no saved credential.

## Integration notes (2026-11-26)

The resumed worker applied the owner-approved shared fixes: new-mode judges now enter the structured runner, Forge replaces the judge identity header with the existing blind prefix, and shared visibility fixtures use valid anonymous history metadata. Shared parser and completion expectations now follow Forge enablement. Full-room tests cover fifteen blind requests, the default six roster names, and stop/restart in every provider phase. Restart preserves committed labels, tally, accepted work and fuser selection.

Two mode-owned compatibility workarounds remain: Forge uses `JUDGE_INVALID_MODE_OUTPUT` for structured member repairs because the shared classifier does not yet recognize `MODE_INVALID_OUTPUT`, and it freezes its repair-step name as a literal because the shared `mode_step` encoder accepts only older mode prefixes.

## Validation evidence (2026-11-26)

Source head `1721e3a` passed the final gates on dev2:

- `CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=4 cargo test --workspace --no-fail-fast`: passed, 1,352 unique tests, zero failures. Cargo reports 1,353 passing executions because the WAL subprocess appears twice. The four existing ignores are three manual performance/memory gates and the PTY child entry point exercised by its parent.
- All 27 Forge checks passed across protocol, store, agents, core workflows/tallies/export and TUI, with no ignored Forge tests. The eleven workflows include the full shared room runtime and restart/stop in every provider phase.
- `CARGO_BUILD_JOBS=4 cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo fmt --all -- --check`: passed.
- The workspace gate also rebuilt and exercised the final release CLI. Its enabled-mode usage fixtures, shared visibility checks and tournament champion audit all passed. No shared integration changes remain outstanding for Forge.
