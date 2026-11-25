# Squad 🎙️

Six frontier models. One terminal. One room.

Squad is a terminal chat room where you talk to several LLMs at once through OpenRouter. You address them with `@nick` or `@all`, and a model only ever sees what was addressed to it. When two answers are worth a fight, `$debate` makes those two models argue it out round by round while a judge keeps score and then names a winner.

```text
@all Come up with a good use case for redis pub and sub that will go viral on github.
@grok @gemini $debate your solutions until $ROUNDS at which time @judge decides which is better
```

## Status

🚧 **Design phase, building in public.** There is no runnable code yet. The product requirements document lands here first, then the code, milestone by milestone.

## The plan

- One self-contained Rust binary. No server to run, no database to install.
- A [Ratatui](https://ratatui.rs) TUI: participants on the left, the room in the middle, your input at the bottom.
- Configuration is one `.env` file: your OpenRouter key, the debate length, and one line per model. See [`.env.example`](.env.example).
- History in a local SQLite file per room.

## Licence

[Apache License 2.0](LICENSE)


Census runs blind omission hunts and anonymous ratings over a plan: `@all $census <plan text>`, or `@nick $census` to use that member's latest eligible reply with the room's members. It requires at least three members. `SQUAD_CENSUS_PASSES` seeds `CENSUS_PASSES` (default 2, range 1–5) only when no persisted value exists; `/set CENSUS_PASSES=3` changes the room value. Each six-member pass uses 14 normal calls. Between passes, `/continue` retains the plan and `/continue "replacement plan"` creates a new plan version; larger inputs receive a revised preflight. Ordinary member holds are released while waiting, while the contest claim remains active. `/stop` releases that claim before a `$grill` handoff. `/export census <contest-id>` retains plan versions, evidence, missing participation, finder attribution, Chao1 provenance and the discovery curve. Estimate bounds remain unavailable until the range rule is resolved, and the engine cannot assert quiet from the point estimate alone.
