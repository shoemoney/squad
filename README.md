# Squad 🎙️

Six frontier models. One terminal. One room.

Squad is a terminal chat room where you talk to several LLMs at once through OpenRouter. You address them with `@nick` or `@all`, and a model only ever sees what was addressed to it. When two answers are worth a fight, `$debate` makes those two models argue it out round by round while a judge keeps score and then names a winner.

```text
@all Come up with a good use case for redis pub and sub that will go viral on github.
@grok @gemini $debate your solutions until $ROUNDS at which time @judge decides which is better
```

## Status

The Rust implementation is in progress. The live room runtime, debate and crosscheck are available; additional contest modes follow the milestones.

## The plan

- One self-contained Rust binary. No server to run, no database to install.
- A [Ratatui](https://ratatui.rs) TUI: participants on the left, the room in the middle, your input at the bottom.
- Configuration is one `.env` file: your OpenRouter key, the debate length, and one line per model. See [`.env.example`](.env.example).
- History in a local SQLite file per room.

## Crosscheck

Build an independent coverage grid for a pasted plan:

```text
@all $crosscheck Deploy one region, verify it, then expand.
--
Keep service available; use the existing staff.
```

A selected quote or `@nick $crosscheck <goal and constraints>` can supply the plan instead. Without a goal-and-constraints brief, the judge extracts it before any inventory work. At six members, preflight shows 16 calls with a brief or 17 with extraction. Cost confirmation and `--yes` authorize those calls; they do not accept residual risks.

Inventory writers cannot see the plan. Reviewers receive only their assigned cells and cannot review their own inventory contributions. Two fresh auditors inspect persisted closure samples. Reports retain omissions, failures and open gaps; COMPLETE means the script finished, not that its bounded stop rule passed.

Use `/accept <cellId…> -- <reason>` on a COMPLETE run to record explicit, version-bound human acceptance. F5 toggles cell details in the TUI. Export the committed ledger with `/export crosscheck <contestId>` or `squad export crosscheck <contestId>`; export does not call models or change coverage. See [implementation and verification notes](docs/crosscheck-implementation.md).

## Licence

[Apache License 2.0](LICENSE)
