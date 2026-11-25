# Open questions

**Agents and OpenRouter**

- When the catalog omits a usable context limit, which shared error and startup behavior should apply? The previous requirement for an explicit context-limit override has no equivalent among the permitted environment variables.

**Foundations, Room API and CLI**

- How does a waiting `squad send` accept the later pick for a question-first debate or grill? The supplied contracts define the pick request and persistent pending state, but do not choose stdin interaction, returning at `AWAITING_PICK`, or requiring `--no-wait` for these submissions.

**TUI effects and motion**

1. **Font licensing.** `slant` and `smblock` come with `figlet-rs`. A heavier block font for banners (ANSI Shadow style, as sketched in storyboard a) would have to be embedded as a `.tlf`, and its licence needs checking before it ships in the binary. `squad-core` owns the embedded asset; it must be Apache-2.0 compatible and credited in `NOTICE` under D60.
2. **Sync output inside tmux.** If the DECRQM probe is unreliable under tmux, do we trust `TMUX` plus a version check, or default tmux to `lite`? This needs a measurement on real tmux.

**Contests: debate, tournament, grill, census, crosscheck, survey, probe**

- How should one `@judge` message quoting verdicts from multiple cases select its case?
- How should multiple tournament forfeits in the same round be ordered to choose the single eliminated entrant?

**Hardening and launch**

- When crosscheck has fewer than five closed cells, what sampling rule should each auditor use? FEATURES2.md requires five sampled closures per auditor but does not define this case.
