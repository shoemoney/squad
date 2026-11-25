# Decision log

## Owner interview (owner interview, 2026-11-24)

All 34 confirmed by the owner. Where a decision conflicts with text elsewhere in this PRD, the decision wins.

| ID | Decision |
|---|---|
| D1 | `fable` is a normal room member AND `judge` is a separate fresh Fable instance with empty context, summoned only. If `fable` is a debater, the judge swaps to another model (claude's, else openai's) and the user is told. |
| D2 | Each model keeps a private running history: everything addressed to it (or `@all`) plus its own replies. Follow-ups like `@grok expand on that` work. |
| D3 | Models cannot route. An `@nick` inside a model's reply is plain text and is never delivered. Only the human routes, plus the orchestrator inside a debate. |
| D4 | "Your solutions" = each debater's most recent reply to `me`; topic = the user message those replies answered. UI override: select specific messages to debate instead. |
| D5 | Debaters write simultaneously within a round. |
| D6 | Debate turns and verdict are visible only to `me`, the two debaters and the judge. The other four models never learn of it. |
| D7 | Debaters keep the debate turns and the verdict in their private history afterwards. |
| D8 | v1 is one persistent room that survives restarts. `/clear` wipes it. Multi-room is later; keys stay namespaced per room. |
| D9 | All-TypeScript stack: Node server, Vue 3 web, Ink TUI, shared `packages/protocol`. |
| D10 | A message with no recipient is refused (client and server) with a hint. One keystroke re-inserts the recipients of the previous message. |
| D11 | v1 runs on the laptop, bound to localhost, no login. |
| D12 | Models are plain chat in v1: no web search, code execution or file access. |
| D13 | After `@all`, six separate bubbles in ONE timeline, each streaming in place. Web adds a toggle to a side-by-side compare view. TUI is timeline only. |
| D14 | Every model's system prompt names the full roster and states it cannot see what the others say. |
| D15 | `$ROUNDS=5` anywhere in a message sets it and is sticky for the room. Default 3, min 1, max 10. `/set ROUNDS 5` is equivalent. Bare `$ROUNDS` reads the current value. |
| D16 | A message to a model that is generating or mid-debate is queued per model and answered next in order. It never interrupts. Chat with other models continues during a debate. |
| D17 | `Esc` or `/stop` stops everything generating. `/stop @nick` stops one model. A stopped debate keeps its transcript and skips the judge. `/judge` summons the judge on a stopped (or finished) debate's existing transcript. |
| D18 | One model failing on `@all` does not affect the others. One automatic retry, then an error bubble with a retry button. |
| D19 | The judge must pick a winner: no ties. It scores both sides 1 to 10 on a fixed default rubric: fit to the original ask, feasibility, originality, survived the attacks. It lists what is worth stealing from the loser. The free text of the user's `$debate` message is passed to the judge as the tiebreaker criterion. |
| D20 | Debaters are told to concede individual points when the opponent is right but to keep defending their overall idea to the end. The judge rewards honest concessions. |
| D21 | Default reply caps, adjustable per room: about 500 words per chat reply, about 400 words per debate turn. |
| D22 | The room shows a running dollar total. A debate shows an estimated cost up front and asks for confirmation only when the estimate exceeds a threshold (default $2). A hard room budget (default $20) stops all generation when reached. |
| D23 | Every model runs at medium reasoning effort by default, configurable per model. The judge runs at high. Reasoning text is never shown, only the answer. |
| D24 | Web and TUI can be open at once on the same room and stay in sync live. |
| D25 | `@judge` may be summoned WITHOUT a debate: the user selects messages and writes e.g. `@judge which of these is best?`. The judge starts with empty context and sees only the selected messages plus the question. So `@judge` is legal (a) inside `$debate` and (b) as the sole recipient of a message that has at least one selected/quoted message attached. It is still never part of `@all` and never in the member list. |
| D26 | v1 has a "quote" action: select one or more messages and they are attached to the user's next message; every recipient of that message sees the quoted text (with original author nick) as part of it. This is the one deliberate way to cross the wall between models, and the human is still the only router. `$reveal` stays on the later roadmap. |
| D27 | A debate renders inline in the main timeline as a collapsible block: round markers, the two sides, then the verdict card. Web lays the two sides out in columns inside the block. No separate screen. |
| D28 | `$debate` requires exactly two debaters in v1 and refuses otherwise with a clear message. 3+ is later roadmap. |
| D29 | The user can regenerate a model reply (the new reply replaces the old one in that model's private history). Sent messages cannot be edited. |
| D30 | Any debate, and the whole room, can be exported as a markdown file (web download, TUI/CLI `squad export`). |
| D31 | "Latest" is resolved once at server start, shown next to each nick in the participants list (e.g. `grok · 4.7`), and fixed until restart. A version can be pinned in config. |
| D32 | Roster changes (add, remove, rename) are config-file only and take effect on restart. |
| D33 | Squad uses its own Redis via `docker compose up`. The OpenRouter key comes from `OPENROUTER_API_KEY`. An optional helper script may fetch the key from a local vault. |
| D34 | Built public-ready (no secrets in repo, clean README, MIT licence) but kept in a private repo. Publishing is an explicit owner decision at M8, never automatic. |
| D35 | The graphical client is an Electron desktop app for macOS and Linux. Vue 3 is the renderer and runs ONLY inside Electron: there is no user-facing browser version, and the server serves no web pages. `apps/web` becomes the renderer of `apps/desktop` (Electron main + preload + Vue renderer). UI end-to-end tests drive the Electron app (Playwright's Electron support), not a browser tab. |
| D36 | Opening the desktop app starts the Squad server inside the Electron main process (or a child process it owns), so no separate `squad serve` step is needed. If a Squad server is already listening on the configured port, the app attaches to it instead of starting a second. The TUI attaches to the same server. Quitting the app stops a server it started, never one it merely attached to. |
| D37 | v1 desktop expects Redis to be reachable (default `redis://127.0.0.1:6379`). If it is not, the app shows a setup screen with the exact command to start it and a Retry button. Bundling a Redis binary in the app is later roadmap. |
| D38 | Packaging via electron-builder: `.dmg` for macOS arm64, `.AppImage` and `.deb` for Linux x64. Unsigned in v1. macOS signing and notarisation need the owner's Apple Developer ID and wait for the publish decision (D34). Intel Mac and Linux arm64 are optional extra targets. |

### Structural consequences of D35 to D38

- Milestone M4 is renamed **Desktop UI (Vue renderer)**. All "web", "browser", "responsive/mobile layout" requirements are rewritten for a resizable desktop window (minimum window size instead of mobile breakpoints).
- A new milestone **M4b Electron shell and packaging** is added after M4: main process, preload with a minimal typed IPC bridge (contextIsolation on, nodeIntegration off, sandboxed renderer), server lifecycle (D36), Redis setup screen (D37), native menus and shortcuts, window state persistence, markdown export via native save dialog, electron-builder config and CI build matrix for macOS arm64 and Linux x64 (D38).
- The OpenRouter key never reaches the renderer. It lives in the main/server process only.
- Electron security baseline: no remote content loaded, strict Content-Security-Policy, external links open in the system browser, navigation and new-window creation blocked.

### Round 4 decisions

| ID | Decision |
|---|---|
| D39 | `$debate` works on a fresh topic with no prior solutions (e.g. `@grok @claude $debate tabs vs spaces`). The orchestrator then runs an opening round 0 in which each debater states its position (each picks its own side unless the user's text assigns sides); those positions become the "solutions". Round 0 does not count toward `$ROUNDS`. If only one debater has a prior solution on the topic, round 0 is run for both. |
| D40 | Each verdict is a sealed case. The user can ask the judge follow-ups about a case (reply to the verdict card, or `@judge` with the verdict quoted); the judge then sees that case's material, its own verdict and the prior follow-ups for that case only. Any new summon starts blank. |
| D41 | Each participant may have an optional `persona` string in config, appended after the shared room system prompt. Empty by default. Config only. Not applied to the judge. |
| D42 | The roster is config-driven: any OpenRouter slug under any unique nick, including the same model twice with different personas. Max 12 members. The six are defaults. |
| D43 | `@all -nick` (one or more exclusions) addresses everyone except the named members. The message is stored with the resolved explicit recipient list. `-nick` is only meaningful directly after `@all`. Excluding every member is an error. |
| D44 | Per-member debate win/loss record is kept in Redis for the room and shown in the participants list (e.g. `7-2`). Updated when a verdict is committed. Cleared by `/clear`. |
| D45 | v1 is text only. Pasting long text and code is supported. No images or file uploads. |
| D46 | Config `user.name` (default "the user") is given to models in the system prompt. The human still appears as `me` in their own clients. |
| D47 | Desktop: native notification when a debate finishes while the window is unfocused; in-chat find (Cmd/Ctrl+F). No auto-update and no telemetry in v1. Nothing leaves the machine except OpenRouter calls. |
| D48 | After the PRD is final, building starts with M0 and M1 in a new private repo. Coding is split across two worker models, one review pass per milestone, owner check-in at each milestone boundary. |

### Pivot (owner, 2026-11-24, after round 4)

| ID | Decision |
|---|---|
| D49 | Squad is a TUI only. There is no Electron app, no Vue app and no browser client. D35, D36, D37, D38 and D47 are VOID. The web-only compare view in D13 is void (single timeline only). D24 becomes: several TUI instances may attach to the same room and stay in sync. Milestone M4 (desktop/web UI) and M9 (Electron) are removed. |
| D50 | The TUI framework is chosen from current research into the most performant options, not assumed to be Ink. |
| D51 | (Partly VOID by D54: there is no separate server, so `squad serve`, `squad tui`, attaching and the gateway are gone, and `squad debug bus` became `squad --bus-log` and the F12 debug pane.) The whole product is Rust. One `squad` binary contains the server (router, agents, orchestrator, gateway) and the TUI. The TUI is built on Ratatui + crossterm + tokio. `squad` starts the room and opens the TUI (attaching to a running server if one is listening); `squad serve` runs headless; `squad tui` attaches only; `squad send`, `squad export`, `squad debug bus` are one-shot. D9 (TypeScript stack) is VOID. Shared protocol lives in a Rust crate (serde), not zod. |
| D52 | Redis stays as the backbone: pub/sub for live traffic, Streams for durable history, exactly as designed. |
| D53 | The repo ships a `./setup.sh` that installs every dependency on macOS and Linux, so a fresh machine goes from clone to a running `squad` with one script. |
| D54 | NO Redis. Squad is one self-contained binary and one process: in-process channels for live traffic and a local SQLite file for durable history. Each model still has its own private mailbox and private history. The bus sits behind a trait so a networked backend can be added after v1. v1 is one front end per room (no second TUI attaching). D24, D33 and D52 are VOID; D53's setup script installs no Redis. |
| D55 | `ROUNDS` defaults to 10 (min 1, max 20). Supersedes the default of 3 and max of 10 in D15. |
| D56 | All configuration is one `.env` file: the OpenRouter key, the rounds, and the model slugs, shipped as `.env.example` pre-populated with the six default members. There is no `squad.toml`. The roster is whatever `SQUAD_MODEL_<NICK>` lines exist. Supersedes the config-file wording in D32, D41, D42 and D46 (those settings become optional env vars). |
| D57 | The judge scores every debate round. After each round's two turns are committed, the judge (the same case context for the whole debate, still with no room memory) scores both debaters 1 to 10 for that round with a one-line reason each. The user sees a live running scorecard. Debaters do NOT see round scores until the final verdict. The next round starts without waiting for the round score. The final verdict remains the judge's overall decision; if its winner differs from the round-tally leader it must say why. |
| D58 | Between rounds the TUI shows an ASCII-art banner (FIGlet style, like `echo ROUND 1 \| toilet`): `ROUND n`, `FINAL ROUND`, `VERDICT`. Rendered in-process from a font embedded in the binary, never by shelling out. Falls back to a one-line divider when the terminal is too narrow. Included in markdown export as a code block. |
| D59 | The TUI goes heavy on visual effects and flair: animated banners, reveals, gradients, a verdict sequence, per-model colour identity. Effects must never cost smoothness or readability, must degrade on limited terminals, and can be turned down or off (`SQUAD_FX`). |
| D60 | Squad is released under the Apache License 2.0 (supersedes MIT in D34). The repo ships `LICENSE` and `NOTICE`; every crate sets `license = "Apache-2.0"`; embedded fonts and other vendored assets must be Apache-2.0 compatible and are credited in `NOTICE`. Publishing remains an explicit owner decision. |
| D61 | Squad is built in public from day one. The repo is `github.com/shoemoney/squad`, public, created 2026-11-24. Supersedes the "private until launch" part of D34 and the private-repo part of D48. |
| D62 | The owner's local `.env` takes its OpenRouter key from his own key vault. This is local setup only; the repo and `setup.sh` know nothing about any vault. |
| D63 | Build approach: most coding units go to the GPT Sol 6.1 worker on its fast tier, dispatched asynchronously with the widest fan-out each milestone allows (one git worktree per worker, merged by the orchestrator); overflow goes to the other coding workers; one review pass per milestone. Refines D48. |
| D64 | A daily spend bar is always visible (`$spent ▰▰▱▱ $max`). The max is `SQUAD_DAILY_MAX_USD` in `.env` (default 20). Spend is the real cost OpenRouter reports per call. At the max Squad stops sending new requests until the next local day or until the max is raised. This replaces the separate room budget in D22 (the debate confirmation threshold stays). |
| D65 | `$tournament`: every named entrant (or `@all`) answers; the judge ranks all answers with scores and critiques; the lowest-ranked is eliminated; survivors are told their own result and present again; repeat until one champion remains. Entrants see only their own results. Judging is blind (anonymised, shuffled labels), so Fable stays the judge even when it competes. One elimination per round. Minimum three entrants. |
| D66 | `$grill`: the user picks one model's plan (`@leader $grill`); every other model grills the leader one at a time on a shared floor that all grill participants can see; the leader answers and updates the plan after each griller; at the end of each round the leader rebuilds the full plan; repeat for `GRILL_ROUNDS` (default 2, `SQUAD_GRILL_ROUNDS` in `.env`). Up to 3 questions per turn; grillers may pass; if all pass the grill ends early. Output is the final plan plus a question-and-answer appendix. No judge. |
| D67 | Debate, tournament and grill share one contest engine; one contest runs at a time per room. Build order: debate, then tournament, then grill. |
| D68 | Modes can start from the question: `@all $debate <question>` → everyone answers → banner `CHOOSE YOUR DEBATE BROS` → the user picks two by typing `@a @b` → the debate runs. Same for `$grill` with `CHOOSE WHO GETS GRILLED` and one pick. The direct forms (`@a @b $debate`, `@leader $grill`) remain as shortcuts. The room stays usable while a pick is pending; Esc cancels and the answers remain ordinary replies. |
| D69 | `$survey` and `$probe` are v1 modes. `$survey` maps where the room agrees claim by claim; `$probe` turns a mystery into a decision tree of checks. Both were proposed by the room's own models in a blind test run. |
| D70 | `$census` (Fable's design: lens-assigned blind gap hunt, merged ledger, everyone rates each gap, and a capture-recapture estimate of gaps still unfound) and `$crosscheck` (OpenAI's design: a coverage grid built blind to the plan, checked cell by cell including every cross-lens pair) are v1 modes. Squad's stated purpose is to produce the ultimate plan, one where everything has been thought of. |
| D71 | All seven modes are scripts over one engine's shared moves. Build order: debate, grill, census, crosscheck, tournament, survey, probe. Supersedes the order in D67. |
