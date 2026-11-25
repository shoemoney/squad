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
