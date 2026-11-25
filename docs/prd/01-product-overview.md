# Product overview

### Problem statement

Squad exists to produce the ultimate plan, one where everything has been thought of.

Comparing answers across several LLMs usually requires separate conversations, repeated prompts, and manual copying. Asking models to critique each other adds more copying and makes it difficult to establish which information each model received.

Squad provides one persistent local room with independent model conversations in a terminal. The human explicitly addresses messages, can quote selected material to other recipients, and can start a bounded debate, elimination tournament, or shared-floor grill. A summoned judge can evaluate a debate, rank tournament entries, or evaluate selected messages.

`$grill` hardens a plan; `$census` and `$crosscheck` find what it is missing and say when to stop; `$debate` and `$tournament` choose between ideas; `$survey` shows what the room agrees on; `$probe` handles a mystery.

One self-contained Rust binary runs the TUI, routing, model workers, contest orchestration, and persistence in one process. Live activity uses in-process channels. Durable state lives in a local SQLite file. No separate local service is required.

The human sees all room activity. Each model receives only its authorized context. Displaying messages together does not share them between models.

### Persona

**The individual builder:** one person exploring software ideas, comparing proposals, and testing assumptions before deciding what to build. They work in a terminal, want independent perspectives, and need to challenge selected proposals without manually assembling transcripts.

This person operates one local installation with an OpenRouter key. They use the TUI for interactive work and `squad send` for scripting. There are no additional human roles, shared accounts, administrators, or team permissions in v1.

### Goals

- Obtain independent answers from explicitly addressed members through parallel OpenRouter calls.
- Make recipients, deliberate quoting, and model visibility understandable before submission and throughout the conversation.
- Support private running conversations with each member.
- Turn existing proposals or a fresh topic into a bounded two-member debate with parallel turns, rebuttals, and closing statements.
- Provide optional debate judging, standalone judging, and follow-ups confined to a judging case.
- Preserve completed messages and recover conversation and contest state after restart.
- Run one TUI and all room processing in one self-contained Rust process.
- Make model identity, activity, failures, usage, spending, and contest records inspectable.
- Support reply regeneration and Markdown export.
- Prepare a fresh supported machine through `./setup.sh`.
- Support addressed submissions and results through `squad send`.
- Start a debate or grill from a question, collect independent answers, and pick participants without freezing the room.
- Run a blind-judged elimination tournament that produces one champion.
- Improve a selected plan through sequential grilling, plan revisions, and a question-and-answer appendix.
- Keep an always-visible daily spend bar and stop sending new requests when the daily max is reached.
- Find plan omissions through independent lens-assigned hunts, confirm them in a gap ledger, and estimate what remains unfound.
- Check a plan against an independently built coverage grid, including cross-lens pairs, and audit claimed closures.
- Map agreement claim by claim, including contested claims, lone insights, lone dissents, and rejected outliers.
- Turn a mystery into a decision tree of checks that distinguish competing explanations.

Local-first means room processing and durable conversation state remain on the local machine. Inference requires OpenRouter and sends each request’s authorized context outside that machine.

### Non-goals for v1

- Multiple human users, authentication, organization accounts, or hosted collaboration.
- Multiple rooms.
- Concurrent interactive sessions for the same room.
- Unaddressed conversational messages, unsolicited model participation, or model-controlled routing.
- Automatic sharing of independent answers outside an explicitly authorized contest.
- Debates with three or more debaters, continuous autonomous discussion, or multiple judge votes.
- `$review`, `$reveal`, `$vote`, or `$summarize`; these remain on the later roadmap.
- Runtime model changes or roster editing.
- Editing sent messages.
- Images, file uploads, direct provider integrations, offline inference, model tools, web search, code execution, or model file access.
- Treating judge scores as proof of correctness or automatically implementing a winning proposal.
- Persisting individual token chunks as durable transcript messages.
- A networked bus implementation.
- OpenRouter key-wide spending across other applications.

### Participants and product rules

The default members are `claude`, `fable`, `openai`, `gemini`, `grok`, and `muse`. The roster can include arbitrary OpenRouter slugs, including the same model under different nicks and personas. The canonical numbers table defines membership limits and nick restrictions.

Configuration comes from one `.env` file, with process environment variables taking precedence. **Foundations** owns `SquadConfig` in `crates/squad-protocol/src/config.rs`, environment variable validation, and file selection through `--env-file <path>`, `./.env`, then `<OS config dir>/squad/.env`. The shipped `.env.example` contains the six default members.

The roster consists of the `SQUAD_MODEL_<NICK>` variables. If none are set, the six defaults apply. The human appears as `me` in the TUI. `SQUAD_USER_NAME` defaults to `the user` and is supplied to models in their system prompt. `SQUAD_PERSONA_<NICK>` supplies an optional persona appended after the shared room system prompt. Personas are empty by default and never apply to the judge.

Roster additions, removals, renames, personas, and model slug changes take effect on restart. A concrete slug selects a fixed model; a `~...-latest` alias tracks releases through OpenRouter. Startup validates configured slugs and determines the concrete identities shown next to member nicks. Those identities are logged and remain fixed until restart. Every completion uses the resolved concrete slug. Each attempt records the response `model`; a mismatch logs a warning and is shown by `squad doctor`. **Agents and OpenRouter** owns model validation and identity resolution.

`SQUAD_ROUNDS` seeds `ROUNDS` when the room has no stored value. Its default is 10, minimum 1, maximum 20. Runtime assignments update the stored room variable, which takes precedence over the startup seed. Each admitted debate captures its round count.

`SQUAD_GRILL_ROUNDS` seeds the separate stored room variable `GRILL_ROUNDS`, with default 2, minimum 1, maximum 10. Each admitted grill captures its round count. **Command grammar and parser** owns assignments and reads of both variables.

Optional `SQUAD_CENSUS_PASSES` seeds the separate stored room variable `CENSUS_PASSES`, with default 2, minimum 1, maximum 5. It is not included in `.env.example`. **Command grammar and parser** owns its assignments and reads.

Every conversational submission requires explicit recipients. Control operations follow **Command grammar and parser** and do not become unaddressed model messages. `@all` addresses the configured roster, excluding the summoned judge. `@all -nick [-nick...]` removes named members from that expansion and stores the resolved explicit recipients.

Only the human routes ordinary messages and quotes. The orchestrator routes contest material. Mentions inside model output remain plain text and never trigger delivery, commands, assignments, or judge summons.

Each member has a private running history and mailbox. Its worker receives readers bound to that member’s context and never receives access to the human room feed or room transcript. Ordinary replies go to `me` and remain in their author’s history. Human-selected quotes deliberately deliver attributed text to the next message’s recipients. Debate material reaches only its participants and the judge when summoned. Tournament entrants receive only their own answers and results until the final placing list; the judge receives anonymised entries without entrant nicks or model identities. A grill deliberately shares its plan, questions, and answers among exactly its participants. Judge execution receives readers bound to its case context.

Census hunters do not receive one another’s lists; raters receive an anonymised ledger without finder identities or counts. Crosscheck inventory writers receive the goal, constraints, and assigned lens without the plan or other inventories; reviewers receive the plan and only their assigned cells; auditors use fresh context without reviewer identities. Survey members receive unattributed claims and anonymous reasons, without others’ whole answers, authorship, or vote counts. Probe explainers and checkers work independently; explainers later receive the checks without other explanations or predictions. The human sees all artifacts, including attribution and counts withheld from members.

The router is the sole database writer. **Protocol, store and bus** owns envelopes, work records, ordering, visibility, durable delivery, replay primitives, `Request`, `RequestResult`, and the canonical `DebatePreflight` payload. **Agents and OpenRouter** owns ordinary execution, queueing, context assembly, retries, and model resolution. **Contests** owns the shared contest engine, admission, preflight estimates, round barriers, states, judging cases, verdicts, and recovery. **Prompt pack** owns prompt text and placeholders. **Room API and CLI** owns `RoomHandle`, command wiring, and export. **Foundations** owns configuration and setup.

The Rust workspace separates these responsibilities into `crates/squad-protocol`, `crates/squad-store`, `crates/squad-agents`, `crates/squad-core`, `crates/squad-tui`, and `crates/squad-cli`. Executors, the OpenRouter client, and the model registry live in `squad-agents`, which depends on `squad-protocol` and reader/submit traits, never on `squad-store` or `rusqlite`. `squad-core` constructs the context-bound readers and contains the bus, router, orchestrator, prompt renderer, export, banner text and embedded fonts, and `RoomHandle`. Compile-fail isolation tests live in `squad-agents`. Default prompt templates live in `prompts/`. One shipped binary, `squad`, contains the complete product.

Squad has one persistent room, `main`. `/clear` wipes its conversation state, member contexts, contests, judging cases, and scoreboard through the authoritative router operation. It is not a local display reset. It does not reset daily spending.

`squad` opens the room and its Ratatui TUI, using crossterm and tokio. The room uses an exclusive advisory lock. A second process opening the same room exits with `Room main is already open in another squad process (pid N).` Read-only `squad export` remains available while the room is open.

Live requests, mailbox wake-ups, and human-visible activity use an in-process `Bus` implementation. SQLite stores durable state, using bundled SQLite, WAL mode, and a dedicated writer thread. Interactive startup restores committed state and re-wakes unfinished work under the owning execution and contest contracts. `RoomHandle::open` takes `OpenMode { Interactive, OneShot }`; `OneShot` never re-wakes work it did not create. `squad models` and `squad doctor` do not open the room. The TUI loads persisted events and pages older events on scroll. Token deltas remain ephemeral.

Debate, grill, census, crosscheck, tournament, survey, and probe are scripts over one contest engine. Only one contest may be active per room, including answer collection and a pending pick. Participant queues drain before contest execution; ordinary messages accepted while participants are held enter their private histories only after the hold is released. **Contests** owns the detailed admission, hold, stop, and recovery contracts.

The shared moves are `collect` for mutually invisible parallel answers, `pick` for human selection, `fanout` for parallel calls with a separate payload per member, `turns` for sequential calls, `judge` for schema-validated judging, `tally` for local computation, `barrier` for atomic commit and authorized delivery, and `report` for the final artifact and Markdown export. Each mode’s script declares its actors, permitted context, prompts, output schemas, word limits, and failure rules. Preflight derives call counts and estimates from the script. Restating debate, tournament, and grill over these moves preserves their existing behaviour.

Census, crosscheck, survey, and probe support preflight against the remaining daily amount, `/stop`, restart recovery, and Markdown export. A member whose step fails after retry is skipped and named in the report. Their judge calls use the judge system prompt, a mode-specific task prompt, strict JSON schemas, medium reasoning effort, and a visible output ceiling of 4,096 tokens.

The daily max replaces the former room spending limit. `SQUAD_DAILY_MAX_USD` defaults to 20. The always-visible spend bar shows actual OpenRouter-reported costs for the local calendar day, pending reservations, and the max. All Squad rooms on the machine share the daily total under **Protocol, store and bus**. At the max, Squad stops sending new requests until the next local day or until the max is raised; in-flight calls finish. Contest preflight must fit within the remaining daily amount. `SQUAD_DEBATE_CONFIRM_USD` remains the confirmation threshold, with default 2.

Credentials are loaded from `.env` or the process environment. `setup.sh` creates `.env` with mode 0600 and never overwrites an existing file. `.env` is excluded from version control. The OpenRouter key is never logged, printed by `squad doctor`, or stored in the database. The owner’s local key vault is local setup only; the repository and `setup.sh` know nothing about any vault.

Repository public (D61); release publication requires the owner's M7 decision. The repository is `github.com/shoemoney/squad`, with clean documentation and no committed secrets. Squad uses Apache License 2.0, ships `LICENSE` and `NOTICE`, and sets `license = "Apache-2.0"` in every crate. Embedded fonts and other vendored assets must be Apache-2.0 compatible and credited in `NOTICE`. Distribution uses prebuilt binaries for macOS arm64 and Linux x64, a Homebrew formula, `cargo install`, and `./setup.sh` for building from a clone.

After the PRD is final, implementation begins with M0 Foundations and M1 Protocol, store and bus in the existing public repository. Most coding units go to the GPT Sol 6.1 worker on its fast tier, dispatched asynchronously with the widest fan-out each milestone allows, using one git worktree per worker and merges by the orchestrator. Overflow goes to the other coding workers. There is one review pass per milestone and an owner check-in at each milestone boundary. M5 builds debate, then grill, census, crosscheck, tournament, survey, and probe.

Canonical values are defined in [Protocol’s canonical numbers and configuration](02-message-protocol-store-and-bus.md#canonical-numbers-and-configuration).

For models whose catalog entry lists reasoning support, requests send `max_tokens = visible ceiling + reasoning budget` and an explicit `reasoning.max_tokens` equal to that budget. Models without reasoning support do not receive reasoning parameters; only an explicitly configured unsupported effort raises `MODEL_PARAMETER_UNSUPPORTED`. **Agents and OpenRouter** owns parameter validation and request construction.

Timeouts, attempt counts, queue depth, visible output token ceilings, reasoning budgets, preflight expiry, lease timings, channel capacities, and message size limits are compile-time constants in `squad-protocol` in v1.

### User stories and acceptance criteria

1. **Address one member.** As the builder, I can maintain a private running conversation with one model.

   - **Acceptance:** `@grok Explain this tradeoff` enters the human transcript and Grok’s private history. Other members receive neither the question nor the reply. A subsequent `@grok expand on that` uses Grok’s authorized running history.

2. **Request independent answers.** As the builder, I can address several members or the whole roster.

   - **Acceptance:** With the default roster, `@all` dispatches six independent requests without waiting for another member’s answer. Multiple named recipients follow the same isolation rule. One recipient’s failure does not stop the others.

3. **Exclude members from a broadcast.** As the builder, I can use `@all -nick [-nick...]`.

   - **Acceptance:** Exclusions are meaningful only directly after `@all`. Excluded members receive no message or work. The stored message contains the resolved explicit recipient list. Excluding every member is rejected.

4. **Catch addressing mistakes.** As the builder, I receive immediate feedback when a conversational message lacks recipients.

   - **Acceptance:** The TUI and router use the validation strings owned by **Command grammar and parser**. Rejection preserves the draft and creates no conversation message or model call. One keystroke reinserts the previous message’s recipients.

5. **Follow concurrent activity.** As the builder, I can distinguish authors and unfinished responses.

   - **Acceptance:** Independent answers stream in separate stable entries in one timeline. Each retains its nick and textual status. Incomplete output is distinguishable from a committed final. The judge never appears as a member.

6. **Inspect configured identities.** As the builder, I can see which concrete model and persona each nick represents.

   - **Acceptance:** Startup validation and concrete identity resolution are inspectable and logged. Model slugs come from the environment configuration. Roster and persona changes require restart. Every member’s system prompt names the configured roster and explains its visibility limits. Every completion uses the resolved concrete slug; response-model mismatches are recorded, warned, and inspectable through `squad doctor`.

7. **Set debate length.** As the builder, I can choose a persistent round count.

   - **Acceptance:** `SQUAD_ROUNDS` seeds an unset room variable with default 10, within 1 to 20. `$ROUNDS=5` anywhere in a submission and `/set ROUNDS 5` update the same room variable under **Command grammar and parser**. Bare `$ROUNDS` reads the current value. Invalid assignments preserve the previous value. An admitted debate retains its captured round count.

8. **Debate existing proposals.** As the builder, I can select exactly two members to challenge their solutions.

   - **Acceptance:** “Your solutions” selects each debater’s most recent eligible reply to `me`, unless explicit solution selection overrides it. Selected replies must satisfy the authorship and shared-topic checks in **Contests**. The user’s free-text instruction is retained. A direct `$debate` submission creates no ordinary reply work.

9. **Debate a fresh topic.** As the builder, I can submit `@grok @claude $debate tabs vs spaces` without prior solutions.

   - **Acceptance:** When either debater lacks a prior reply on the topic, both generate opening positions in round 0. Each chooses its position unless the instruction assigns sides. Those positions become the solutions. Round 0 does not count toward `ROUNDS`.

10. **Follow bounded rounds.** As the builder, I can inspect criticism, rebuttals, and closing statements.

    - **Acceptance:** Debaters generate simultaneously within each round. Neither receives the opponent’s current-round result before generating its own. Both results enter their histories through the round barrier. Debate-turn attempts reuse the round’s frozen boundary. The final counted round includes closing statements. Debaters concede valid individual points while continuing to defend their overall proposals.

11. **Continue other conversations.** As the builder, I can chat while a contest is pending or running.

    - **Acceptance:** Only one contest is active per room. A second contest is rejected with `A {kind} is already running. Stop it before starting another.` A debate waits in `WAITING_FOR_DEBATERS` while existing ordinary work drains. Ordinary messages sent to participating members during the debate queue until its terminal state; their insertion into private history is deferred until the hold is released. Other members continue normally. While a pick is pending, ordinary conversations continue and the pick stays pending.

12. **Review spending before a debate.** As the builder, I can see its estimate and control expensive starts.

    - **Acceptance:** Every debate emits preflight information showing its model-call count, estimate, debaters, captured round count, whether an opening round is required, and effective judge when requested, using the canonical `DebatePreflight` contract in **Protocol, store and bus**. For a judged debate, the estimate includes one round scoring call per counted round in addition to the final verdict. An estimate at or below the configured threshold auto-confirms; one above it waits for confirmation or cancellation. `squad send --yes` pre-confirms. Admission validates context capacity and the remaining daily amount before provider calls, including round 0 and every permitted attempt. Question-first debates run this preflight after the pick; the preceding ordinary answers are already billed and are excluded from that estimate.

13. **Receive an optional scored verdict.** As the builder, I can include `@judge` in a direct debate command.

    - **Acceptance:** Question-first debates request the judge by default. After the final round and after all round scoring work has completed or been given up, the judge receives the prescribed topic, solutions, transcript, instruction, rubric, available round scores, and identification of unscored rounds. The accepted verdict names one debater as winner, shows criterion scores and locally computed totals, explains the decision, and lists useful elements from the loser. It includes a locally computed `roundTally` and a `tallyOverrideReason` that must explain a winner who is not the round-tally leader; a missing required reason triggers schema repair. Equal totals still require a winner, using the retained instruction as tiebreaker. Both debaters receive the verdict, its `roundTally`, and the per-round scores in the verdict announcement, which is the first time they see the round scores. A direct debate without `@judge` ends in `COMPLETE_UNJUDGED` and has no round scoring.

14. **Keep judging separate.** As the builder, I can distinguish a judge from room member `fable`.

    - **Acceptance:** Each new case begins without member or previous-case history. Debate judging normally uses Fable’s model. Conflict is determined by resolved concrete model identity, including when another nick uses the same model. Fallback selection skips debaters, models matching a debater’s model, and unconfigured members; only explicitly configured fallback lists require validation. The warning is `Judge model changed to {judge_model_slug} because @{nick} is debating.` Judge execution uses only its router-written case context. **Agents and OpenRouter** and **Contests** own the conflict and fallback contracts, which also apply to standalone judging. Tournament judging is blind and never swaps the judge model.

15. **Stop generation and judge later.** As the builder, I can interrupt work without losing completed content.

    - **Acceptance:** `/stop` stops all generation; `/stop @nick` targets one member. Escape closes the topmost transient element first. With nothing open, Escape requests a global stop; during a contest, a second Escape within 2 s confirms it. Animation skipping does not consume Escape or input keys. Stopping a debate preserves its transcript and prevents further automatic rounds or judging. `/stop` cancels outstanding round scoring work. `/judge [debateId]` can judge a stopped or finished debate’s existing transcript and whatever round scores exist, without scoring rounds retroactively. Manual judging uses the judge-only preflight and state transitions in **Contests**; cancellation or plan expiry restores the stored prior status.

16. **Understand failures and retry.** As the builder, I can recover an ordinary failed answer and inspect failed debate turns.

    - **Acceptance:** Ordinary execution follows the canonical attempt limit, then exposes an error and retry action. Debate retries belong to the orchestrator; exhausted turns become recorded forfeits and remain part of the judging evidence. Failed attempts are never concatenated into a successful answer.

17. **Quote selected messages.** As the builder, I can deliberately share material across member boundaries.

    - **Acceptance:** Selecting messages includes author-labelled text in the next addressed message. Every recipient receives that quoted text as part of the user message. No unselected material is shared. Ordinary quoting is distinct from the solution-selection references reserved for `$debate`.

18. **Summon a standalone judge.** As the builder, I can select messages and ask `@judge which of these is best?`.

    - **Acceptance:** `@judge` is the sole recipient and at least one quote is included. This opens a fresh case whose evidence is exactly the quoted messages and the user’s question. The ruling is free-form text, not a scored debate verdict. Invalid summons use the parser-owned validation message.

19. **Ask judge follow-ups.** As the builder, I can question a ruling within its case.

    - **Acceptance:** A reply action on a verdict includes it as a quote, or I explicitly quote it in a message addressed only to `@judge`. The judge receives that case’s material, its verdict, and prior case follow-ups. Other cases and room-member histories remain excluded. A new summon starts blank.

20. **Regenerate a reply.** As the builder, I can request a replacement model answer.

    - **Acceptance:** `/regen [messageId]` uses the regenerate contract in **Protocol, store and bus** and **Room API and CLI**. The old reply is superseded and excluded from that member’s subsequent context; new reply work is queued. Sent user messages remain immutable.

21. **Export completed work.** As the builder, I can save the room or a contest as Markdown.

    - **Acceptance:** `/export room|debate|tournament|grill|census|crosscheck|survey|probe [id]` and `squad export room|debate|tournament|grill|census|crosscheck|survey|probe [id] [-o file]` use the export operation owned by **Room API and CLI**. The result preserves authorship and conversation or contest structure and excludes credentials. Read-only CLI export is allowed while the room is open elsewhere. Markdown export and banner text rendering live in `squad-core`; the API is `RoomHandle::export(scope, id) -> String`, with `export_offline(data_dir, room, scope, id)` for offline export.

22. **Track debate results.** As the builder, I can see each member’s room win/loss record.

    - **Acceptance:** The participants list shows the record. Committing a debate verdict updates it without duplicate increments on replay or delivery retries. It survives restarts in SQLite and is cleared by `/clear`.

23. **Resume the persistent room.** As the builder, I can reopen Squad without losing completed work.

    - **Acceptance:** Interactive startup recovers persisted finals and contest state without duplicate presentation or visibility expansion, including a pending pick. Older committed events load on scroll. Missed token chunks are not represented as recovered durable history. Recovery follows the owning execution and contest sections. One-shot opens never re-wake work they did not create. A second process opening the same room receives `Room main is already open in another squad process (pid N).`

24. **Inspect daily spending.** As the builder, I can understand reported spending and remaining capacity.

    - **Acceptance:** An always-visible bar such as `$3.42 ▰▰▰▱▱▱▱▱▱▱ $20.00` shows today’s actual spend and `SQUAD_DAILY_MAX_USD`, default 20, in the participants panel footer or status line when narrow. A lighter pending segment shows reserved estimates until actual costs arrive. Spend uses OpenRouter-reported per-call cost, including billed failures, cancellations, retries, and judging, committed with the completing event. Missing cost is recorded as `cost unknown`, never zero; the bar gains a `~` prefix and `squad doctor` reports the count. The local calendar day determines today’s shared machine-wide Squad total. Bar colour changes at 75% and 90%. At the max, new generation requests receive `DAILY_MAX_REACHED`; with the default max the message is `Daily max of $20.00 reached. Raise SQUAD_DAILY_MAX_USD or wait until tomorrow.` In-flight calls finish. A contest whose preflight estimate exceeds the remaining daily amount is refused before execution with the same code. **Hardening** owns the cost guard.

25. **Work in the terminal.** As the builder, I can operate Squad through its TUI.

    - **Acceptance:** The TUI provides conversation, selection, quoting, debate, judging, regeneration, export, recovery, and in-transcript search. It accepts long pasted text and code. Debates appear inline as collapsible blocks with round markers, both sides, and the verdict. Tournament rankings, grill plan versions, and pending picks also appear inline under the **TUI** contract. Census, crosscheck, survey, and probe have inline collapsible report blocks for the gap ledger, coverage grid summary, five-band claim map, and decision tree. The TUI uses Ratatui and crossterm within the same process as room execution.

26. **Reset the persistent room.** As the builder, I can explicitly discard the room’s accumulated state.

    - **Acceptance:** Restarts preserve the room. `/clear` wipes it through the authoritative router operation and clears its scoreboard, including tournament trophies. Daily spending remains accounted for. The TUI reflects the committed reset. `/who` and `/help` expose current participants and supported commands.

27. **Prepare a fresh machine.** As the builder, I can use `./setup.sh` to prepare a supported machine and reach a running Squad session.

    - **Acceptance:** On macOS arm64 or supported Debian/Ubuntu and Fedora family Linux x64 systems, the script installs missing build dependencies and the pinned Rust toolchain, creates `.env` from `.env.example` if absent, builds the release binary, runs `squad doctor`, and prints `./target/release/squad` as the next command. If `OPENROUTER_API_KEY` is missing, it explains where to set it without printing a key. After supplying the key and following the printed command, the builder reaches the running TUI. Re-running the script preserves existing configuration and checks before installing. Setup behavior and flags follow **Foundations**.

28. **Script an addressed request.** As the builder, I can use `squad send` from a shell script.

    - **Acceptance:** `squad send "<message>" [--quote <seq>...] [--yes] [--json] [--no-wait]` opens the room headless in `OpenMode::OneShot` and submits through `RoomHandle` using the shared parser and routing contracts. By default it waits until every addressed model, or the contest, has finished, prints replies as text or JSON lines, and exits. Quotes follow the ordinary quote contract. `--yes` pre-confirms spending under the shared preflight contract. `--no-wait` exits after commit and prints the accepted cursor; accepted work runs at the next interactive `squad` open. The command never re-wakes unrelated work. Opening a room already in use fails with the room lock message. **Room API and CLI** owns the command’s behavior.

29. **Follow round scoring.** As the builder, I can see how a judged debate develops without influencing the debaters with interim scores.

    - **Acceptance:** After each counted round’s barrier commit, the judge scores only that round’s two turns for argument quality, honesty of concessions, and responses to the opponent’s last attack. Round 0 is not scored. Each debater receives an integer score from 1 to 10 and a one-sentence reason of at most 200 characters; equal round scores are allowed. Scoring runs without blocking the next round. Each scorecard identifies its round, shows both scores and the locally computed running tally, and expands to show reasons. A pending marker remains until the result arrives, including when it arrives after later turns. Two failed scoring attempts, including schema repair, record the round as unscored and allow the debate to continue. Round scores are committed for `me` and the judge case context only and never enter debater contexts before the verdict announcement. The participants panel continues to derive wins and losses only from final verdicts. **Contests**, **Protocol, store and bus**, **Prompt pack**, and **TUI** own the detailed contracts.

30. **Recognize debate boundaries.** As the builder, I can follow openings, counted rounds, the final round, and the verdict through ASCII banners.

    - **Acceptance:** The TUI derives `OPENING`, `ROUND n`, `FINAL ROUND`, and `VERDICT` banners from persisted debate state transitions, so history replay reproduces the boundaries without separate banner events. `VERDICT` appears on entering `JUDGING`; the verdict reveal plays when the verdict arrives. Embedded FIGlet fonts render in-process, using the default font when it fits, then the compact font, then a one-line divider. Banners are centred, use the theme accent, and are plain under `NO_COLOR`. `SQUAD_BANNERS` supports `on` by default, `compact`, and `off`. Markdown export renders each banner as a fenced code block using the default font. **Room API and CLI** owns shared banner text rendering and embedded fonts in `squad-core`; **TUI** owns layout, width fallbacks, and transcript row accounting.

31. **Experience debate effects.** As the builder, I want a debate to feel like an event and can turn effects down or off.

    - **Acceptance:** `SQUAD_FX` supports `full` by default, `lite`, and `off`. Static rendering remains complete and correct with effects off. **TUI effects and motion** owns effects design, including banner animation.

32. **Pick a debate from independent answers.** As the builder, I can start with `@all $debate <question>` and choose the two answers worth debating.

    - **Acceptance:** The addressed members answer in parallel as ordinary, mutually invisible replies in `COLLECTING_ANSWERS`. Once every addressed member has answered, errored, or been stopped, eligible answers appear under `CHOOSE YOUR DEBATE BROS` in `AWAITING_PICK`. Sending only two eligible mentions, such as `@grok @gemini`, submits `ContestPick { contestId, nicks }` under the protocol-owned request contract. Those exact answers become the solutions, the original question becomes the topic, and judging is requested by default. Preflight and confirmation follow the pick. The collected replies count toward daily spending and are excluded from the later contest estimate. Other conversations remain usable; autocomplete offers eligible nicks first, and invalid picks use parser-owned validation without ending the pick. Fewer than two eligible answers ends the contest as `FAILED` with `Not enough answers to start. {n} of {m} members answered.` A pending pick occupies the active-contest slot, does not expire, and survives restart. `/stop` or the confirmed Escape stop cancels it as `STOPPED` with reason `pick_cancelled`; collected answers remain ordinary replies in the transcript and their authors’ private histories. Exactly two named members without `@all` use the direct shortcut instead; **Command grammar and parser** owns the full decision table.

33. **Choose a tournament champion.** As the builder, I can submit `@all $tournament <prompt>` or name at least three entrants.

    - **Acceptance:** `@all -nick` works and judging is implied. Fewer than three entrants is refused with `$tournament needs at least three entrants.` Every surviving entrant answers in parallel under ordinary output limits. The judge receives shuffled anonymous labels each round and returns a strict ranking with integer scores from 1 to 10 and a one-line critique per entrant. The label map never enters the judge context, and the judge model is not swapped when Fable competes. Each entrant receives only its own answer and per-round rank, score, critique, and survival result; survivors improve their answers in the next round. Exactly one entrant is eliminated each round, producing one champion after k−1 rounds for k entrants. Exhausted entrant attempts become forfeits ranked last under the contest contract. Ranking validation, three judging attempts, and the one permitted round replay follow **Contests**. The human sees all entries and rankings. At completion, every entrant receives only the final nicks and placements, without others’ answers or critiques. The champion’s trophy count increments once, survives restart, and appears beside the debate record. Preflight shows the call count and estimate for k + (k−1) + ... + 2 answers and k−1 judging calls, including permitted attempt and replay allowances. Tournament banners and ranking reveals follow **TUI** and **TUI effects and motion**.

34. **Grill and rebuild a plan.** As the builder, I can select one member’s plan and have other members challenge it on a shared floor.

    - **Acceptance:** `@leader $grill [free text]` uses the leader’s most recent reply to `me`, or a selected or quoted reply by that leader; every other member grills by default. The no-text direct form `@grok @claude @gemini $grill` restricts grillers to Claude and Gemini. A missing plan is refused with `@{nick} has no plan to grill yet. Ask it something first.` Alternatively, `@all $grill <question>` collects ordinary independent answers and shows `CHOOSE WHO GETS GRILLED`; one eligible mention selects the leader, and every other member that answered becomes a griller. At least one griller is required. Question-first and direct-form disambiguation follows **Command grammar and parser**. Preflight occurs after the pick and shows `rounds × (grillers × 2 + 1)` planned calls and the estimate, including permitted attempts.
    - **Acceptance:** Grillers act sequentially in roster order rotated by one each round. Each asks up to three questions under an assigned rotating angle or returns exact `PASS`; the leader answers and states concrete “Plan changes” before the next griller. Every participant receives the plan, questions, and answers as committed; non-participants receive nothing. At each round’s end, the leader rebuilds a complete standalone plan. Working context uses the latest plan, the current round’s questions and answers, and a compact list of earlier questions to prevent repeats. All-pass early completion uses reason `survived`, with the rebuild skipped if no questions were asked at all. Exhausted griller attempts skip that turn; exhausted leader attempts fail the grill while preserving the last good plan. There is no judge or winner. The output is the final plan plus an appendix containing every question, questioner, answer, and plan change; all plan versions remain viewable and exportable. **Contests** owns the detailed execution and context contracts.

35. **Set grill length.** As the builder, I can choose a persistent round count independently of debate length.

    - **Acceptance:** `SQUAD_GRILL_ROUNDS` seeds an unset `GRILL_ROUNDS` with default 2, within 1 to 10. `$GRILL_ROUNDS=n` and `/set GRILL_ROUNDS n` update the same stored room variable under **Command grammar and parser**. Invalid assignments preserve the previous value. Each admitted grill retains its captured count.

36. **Find what a plan omits.** As the builder, I can run a census of missing concerns and see whether further searching is worthwhile.

    - **Acceptance:** `@all $census` uses a quoted plan, including a selected message or a grill’s latest plan version; `@all $census <pasted plan>` supplies it directly. `@nick $census` can select that member’s latest reply as the plan. A census requires at least three members. `CENSUS_PASSES` defaults to 2, within 1 to 5. Each pass assigns one lens per member for a blind hunt, merges duplicate gaps, obtains independent ratings against an anonymised ledger, and finalises confirmed gaps and covered items with citations. Members never receive finder identities or counts.
    - **Acceptance:** The human receives a ranked ledger with finder counts, lenses, consequences, highlighted singletons, covered-item citations, a discovery curve, and `Found {n}; estimated {lo} to {hi} still unfound.` The app computes the estimate from hunt finder counts and presents a range with the caveat that shared model blind spots make it a floor. The human may supply a patched plan between passes. A pass adding no new confirmed gap with an estimate below 2 reports `Census is quiet. Good place to stop.` The report includes a hint to send singletons to `$grill`.

37. **Check coverage independently of the plan.** As the builder, I can test whether a plan handles an independently built inventory and the interactions between its items.

    - **Acceptance:** `@all $crosscheck` accepts a quoted or pasted plan as for `$census`, plus goal and constraints. If those are absent, the judge extracts them in an extra call. Members build lens-specific inventories without seeing the plan or other inventories. The app builds every single-item cell and every cross-lens pair from the normalised inventory, capped at 200 by the judge’s priority order. Reviewers receive only their assigned cells and the plan, and never review a cell built from their own inventory item.
    - **Acceptance:** Every cell records a plan citation, an inapplicability reason, or an open gap with a proposed fix; “probably covered” counts as a gap. Two randomly chosen members audit sampled closures in fresh context without reviewer identities and each names a case the grid may have missed. The judge rules on challenges and additions without dropping cells or accepting risks for the human. The report identifies residual risks requiring explicit human acceptance. The bounded stop rule requires every cell closed or explicitly accepted and an audit finding no false closure; it does not prove completeness.

38. **Map agreement claim by claim.** As the builder, I can see which claims the room supports and which need more evidence.

    - **Acceptance:** `@all $survey <question>` collects independent answers; `@all $survey` can reuse the room’s most recent set of parallel answers to one question. The judge extracts up to 25 neutral, deduplicated, unattributed atomic claims and privately records their sources. Members vote AGREE / DISAGREE / UNSURE with a one-line reason and confidence from 1 to 5. The app selects up to five most evenly split claims for another vote using only anonymous reasons from both sides; members state what evidence would change their minds. They never receive others’ whole answers, authorship, or vote counts.
    - **Acceptance:** The report maps five bands: consensus, contested claims, lone insights, lone dissents, and rejected outliers. It shows the strongest opposing reasons, vote changes, and a question that would settle each contested claim. Rejected outliers identify claims raised by one or two members and rejected by the rest as likely errors. Agreement remains an account of the room’s responses rather than proof of truth.

39. **Investigate a mystery.** As the builder, I can obtain practical checks that distinguish plausible explanations.

    - **Acceptance:** `@all $probe <observation, what is ruled out, what can be inspected>` needs an even active count of at least four members. With an odd count, the last member sits out and is told so in the transcript. The app randomly splits the active members into explainers and checkers and shows the human the assignments. Each explainer independently proposes one explanation and key assumption; each checker independently proposes one cheap, non-destructive check with exact measurements and outcomes.
    - **Acceptance:** Each explainer then receives all checks and commits to predicted results under its own explanation, marking results it cannot predict, without seeing other explanations or predictions. The judge removes checks that do not distinguish explanations, orders the useful checks by effort and usefulness, and identifies explanations the checks cannot separate. The report contains a runnable decision tree, prediction matrix, and an unexpected-result branch. It does not declare an explanation true or execute checks.

### Canonical walkthrough

Use the default roster, an empty room, and the default round count of 10. Launch `squad` to open the room and TUI.

1. Submit:

   ```text
   @all $debate Come up with a good use case for redis pub and sub that will go viral on github.
   ```

   Squad enters `COLLECTING_ANSWERS` and dispatches six independent ordinary requests. The human sees six separately labelled answers streaming in one timeline. Each member receives the question and its own answer, without any other member’s answer. These calls count toward today’s spending.

   Once every addressed member has answered, errored, or been stopped, Squad enters `AWAITING_PICK` and displays `CHOOSE YOUR DEBATE BROS` with the eligible nicks. With at least two eligible answers, the pick remains pending without expiry. Other conversations remain usable.

2. Pick Grok and Gemini by submitting only:

   ```text
   @grok @gemini
   ```

   Their exact replies from step 1 become the original solutions, and the step 1 question supplies the topic. Judging is requested by default. This input is a contest pick, not an ordinary conversational message.

   Preflight displays the model-call count, estimate, captured count of 10 rounds, and effective judge. The estimate includes 10 round scoring calls and the final verdict, including every permitted attempt. The ordinary answers from step 1 are excluded from this estimate. An estimate at or below the confirmation threshold auto-confirms. An estimate above it requires confirmation before admission. An estimate exceeding the remaining daily amount is refused. Cancellation or rejection starts no debate generation.

   The debate runs 10 counted rounds with parallel turns and a barrier between rounds. No round 0 is needed because both original solutions exist. A fresh judge case accumulates the prescribed evidence and scores each counted round without blocking the next round. The human sees round scorecards; neither debater receives those scores yet. The final round includes closing statements. Once all round scoring work has completed or been given up, the judge commits a scored verdict with the round tally, delivers the verdict and per-round scores to both debaters, and updates the member records.

The walkthrough uses two inputs, plus confirmation when required by the estimate.

| Participant | What they receive in step 1 | What they receive in step 2 |
|---|---|---|
| `me` | Question and all six streamed answers; eligible nicks and pick banner | Pick, preflight, setup, both streams, committed turns, state changes, round scores, and verdict |
| `claude` | Question and own answer | No debate material |
| `fable` | Question and own answer | No debate material; member history remains separate from the judge |
| `openai` | Question and own answer | No debate material |
| `gemini` | Question and own answer | Setup containing both originals and the instruction; both sides’ results after each round barrier; verdict with round tally and per-round scores |
| `grok` | Question and own answer | Setup containing both originals and the instruction; both sides’ results after each round barrier; verdict with round tally and per-round scores |
| `muse` | Question and own answer | No debate material |
| `judge` | Not summoned | The prescribed case evidence, each round’s turns and its own round scores, then its own verdict |

Grok and Gemini retain the committed debate turns and verdict, including its round tally and per-round scores, in their private histories. The judge retains case material for explicit follow-ups under **Contests**. Other members receive none of it unless the human later quotes selected material to them.

The direct shortcut remains available after ordinary independent answers:

```text
@grok @gemini $debate your solutions until $ROUNDS at which time @judge decides which is better
```

It skips answer collection and the pick, retains the free-text instruction, and uses the same preflight and debate execution contracts. With the TUI closed, ordinary answer collection followed by this direct shortcut also works through sequential `squad send` invocations using the shared parser and `RoomHandle`. Both paths preserve the same committed outcomes in SQLite.

### Daily spend walkthrough

Launch `squad` with `SQUAD_DAILY_MAX_USD=20`. The spend bar remains visible while ordinary replies and contests run. Each in-flight call contributes a lighter reserved segment; its reported actual cost replaces that reservation when the completing event commits. Billed failed and cancelled calls remain included.

If any call returns no cost, the bar shows `~` and `squad doctor` reports the unresolved count. At the daily max, new generation requests are refused with `DAILY_MAX_REACHED`; in-flight calls finish. A contest estimate that does not fit the remaining daily amount is refused before execution. The next local day or a raised max permits new requests. Restarting or clearing the room does not erase the day’s spending.

### Tournament walkthrough

With no active contest, submit:

```text
@all $tournament Come up with a good use case for redis pub and sub that will go viral on github.
```

There is no pick step. Preflight shows the call count and estimate against the remaining daily amount. With the default six entrants, the planned successful path contains 20 entry calls and five judging calls; the estimate also includes permitted retries, repairs, and the allowed round replay.

After confirmation when required, all six entrants answer in parallel. The human sees every answer. The judge receives shuffled anonymous labels and returns a strict ranking with scores and critiques. The human sees the ranking revealed from last place upward; each entrant receives only its own result. The lowest-ranked entrant is eliminated.

The five survivors answer again using their own previous rank, score, and critique. The process repeats with four, three, and then two entrants. The TUI shows `TOURNAMENT`, `ROUND r · N REMAIN`, `FINAL`, and `CHAMPION` at the appropriate boundaries. One champion remains after five elimination rounds, and its trophy count increments once.

Every entrant receives the final placing list containing nicks and placements only. No entrant receives another entrant’s answer or critique. Export the completed contest with `/export tournament [id]` or `squad export tournament [id]`.

### Grill walkthrough

With no active contest, submit:

```text
@all $grill Propose a reliable launch plan for this product.
```

Squad collects six independent ordinary answers. Once collection finishes, it shows `CHOOSE WHO GETS GRILLED`. Pick the leader by submitting only:

```text
@grok
```

Grok’s collected answer becomes the initial plan. Every other member that answered becomes a griller. Preflight then shows the captured `GRILL_ROUNDS`, call count, and estimate against the remaining daily amount. With all six answers and the default two rounds, the planned successful path contains `2 × (5 × 2 + 1)` calls, plus permitted retry allowances. The preceding ordinary answers are excluded from this estimate.

After confirmation when required, the grill opens a shared floor for Grok and the five grillers. The first griller asks up to three questions or returns `PASS`. Grok answers the questions and states concrete plan changes before the next griller acts. Every participant receives the committed questions and answers; non-participants receive none of this material.

At the end of the round, Grok rebuilds the full plan. The next round uses that version and rotates the griller order by one. Earlier questions remain represented in the working context as a compact no-repeat list. An all-pass round ends the grill early with reason `survived` under the rebuild rules.

The TUI shows `GRILL`, `GRILL ROUND r`, and `FINAL PLAN`, question and pass counts, and collapsible plan versions with the newest expanded. The result contains the final plan and the full question-and-answer appendix. There is no judge or winner. Export it with `/export grill [id]` or `squad export grill [id]`.

For an existing plan, `@grok $grill` skips collection and the pick. The no-text shortcut `@grok @claude @gemini $grill` makes Grok the leader and restricts grilling to Claude and Gemini.

### Census walkthrough

Select the latest plan from a completed grill, attach it as a quote, and submit:

```text
@all $census
```

Preflight shows the pass count, call count, and estimate against the remaining daily amount. With six members, one successful pass contains six hunts, one merge, six ratings, and one finalisation: 14 calls. The default two passes contain 28 planned calls, with permitted attempts included in the estimate.

After confirmation when required, the TUI shows `CENSUS` and step dividers. Each member receives the plan and one lens: stakeholders; failure modes; dependencies and sequencing; hidden assumptions; external and legal factors; or the day after success. Each independently lists up to eight omissions and their consequences. Lenses repeat with more than six members; with fewer, the first lenses are used and rotate each pass.

The judge merges duplicate finds. Every member then rates the anonymised ledger as `real`, `covered` with a plan citation, or `trivial`, and may add missing gaps. The ledger supplied to members contains no finder names or counts. The human can inspect the member-by-gap table.

The final ledger drops gaps a majority rate covered or trivial, retains citations, and merges additions. The app computes the unseen-gap estimate from the original hunt finder counts. The report displays `Found {n}; estimated {lo} to {hi} still unfound.`, explains that shared model blind spots make the estimate a floor, highlights singletons, and shows the discovery curve across passes.

Between passes, the human may quote a patched plan so the next pass uses the current version. A pass adding no new confirmed gap with an estimate below 2 displays `Census is quiet. Good place to stop.` The report also offers the hint to send singletons to `$grill`. Export the ledger with `/export census [id]` or `squad export census [id]`.

### Crosscheck walkthrough

Attach the current plan as a quote and submit its goal and constraints:

```text
@all $crosscheck Goal: launch the product reliably. Constraints: one operator, a fixed launch date, and no additional paid services.
```

Preflight shows the planned calls and estimate against the remaining daily amount. With six members, the successful path contains 16 calls: six inventories, one normalisation, six reviews, two audits, and one ruling. Omitting the goal and constraints requires an additional judge call to extract them from the plan.

After confirmation when required, the TUI shows `CROSSCHECK` and step dividers. Each inventory writer receives only the goal, constraints, and one lens: stakeholders, phases, dependencies, failure states, operating conditions, or decision rights. Each lists three concrete items without seeing the plan or other inventories.

The judge normalises the inventory, and the app builds every single-item cell and every cross-lens pair. At the defaults, 18 items produce 153 cells. Larger grids are capped at 200 using the judge’s priority order. Cells are split evenly among reviewers, avoiding any cell built from the reviewer’s own inventory item.

Each reviewer receives the plan and its assigned cells. A cell must have a citation showing coverage, a justified exclusion, or an open gap with a proposed fix. “Probably covered” remains a gap.

Two randomly chosen auditors use fresh context. Each checks five sampled closures and proposes one case the grid may have missed. They receive the plan, grid, and sampled evidence without reviewer identities. The judge rules on challenged closures and additions without dropping a cell.

The inline coverage report retains every cell and identifies residual risks for explicit human acceptance. It shows the bounded stop rule: every cell closed or accepted and an audit finding no false closure. Export the ledger with `/export crosscheck [id]` or `squad export crosscheck [id]`.

### Survey walkthrough

With no active contest, submit:

```text
@all $survey What should this product's first release include, and what should wait?
```

Preflight shows the call count and estimate against the remaining daily amount. With six members, the successful path contains 20 calls: six answers, one extraction, six votes, six second votes, and one report. To reuse the room’s most recent parallel answers to one question, submit `@all $survey`; that path contains 14 planned calls. Estimates include permitted attempts.

After confirmation when required, the TUI shows `SURVEY` and step dividers. Members answer independently when collection is needed. The judge extracts up to 25 neutral, unattributed atomic claims, privately recording which answers contained each claim.

Each member votes AGREE, DISAGREE, or UNSURE on every claim, with a one-line reason and confidence from 1 to 5. The app tallies votes and selects up to five most evenly split claims. Members see only anonymous reasons from both sides of those claims and vote again, stating what evidence would change their minds. They receive no other member’s whole answer, authorship, or vote counts.

The report appears as a five-band map: consensus, contested claims, lone insights, lone dissents, and rejected outliers. With six members, consensus includes 6–0 or 5–1 agreement. Contested claims show the split, the best reason on each side, who switched, and a question that would settle the issue. Lone insights identify claims raised by one member and endorsed by the rest; rejected outliers identify claims raised by one or two and rejected by the rest as likely errors.

Export the map with `/export survey [id]` or `squad export survey [id]`.

### Probe walkthrough

With no active contest, submit:

```text
@all $probe Observation: requests slow down after several hours. Ruled out: increased traffic. Can inspect: application logs, queue depth, and memory measurements.
```

Preflight shows the call count and estimate against the remaining daily amount. With six members, the successful path contains 10 calls: six initial contributions, three predictions, and one report. Estimates include permitted attempts. The app randomly assigns three explainers and three checkers and shows the human the assignments. An odd addressed count leaves the last member out, with a transcript notice; at least four active members are required.

After confirmation when required, the TUI shows `PROBE` and step dividers. Each explainer independently proposes one explanation and its key assumption. Each checker independently proposes one cheap, non-destructive check, specifying exactly what to measure and which outcomes to record. These contributions are mutually invisible.

Each explainer then receives all checks and commits to the results it expects under its own explanation, marking anything it cannot predict. It receives neither other explanations nor their predictions.

The judge compares the locked prediction matrix, removes checks that do not distinguish explanations, and orders useful checks by effort and usefulness. The report draws a decision tree with a next step for each result, identifies explanations the available checks cannot separate, and includes an unexpected-result branch. Squad proposes the checks; the human performs them outside Squad.

Export the decision tree and prediction matrix with `/export probe [id]` or `squad export probe [id]`.

### Recommended planning pipeline

Use `$grill` → `$census` → patch → `$crosscheck` → `$census` until quiet.

Start with a grill to challenge the written plan and rebuild it. Run a census to find omissions, then patch the confirmed gaps. Run a crosscheck on the revised plan to test individual obligations and cross-lens interactions, audit closures, and identify residual risks requiring explicit acceptance. Run another census on the current plan and continue patching and checking until a pass adds no new confirmed gap and its estimate is below 2.

Each contest runs separately under the one-active-contest rule. The human supplies revised plan versions and decides whether to accept residual risks or continue. A quiet census and a closed, audited coverage grid provide bounded stopping signals; shared model blind spots can still leave omissions.

### Success metrics

These are release acceptance measures, not claims of observed performance:

- **Isolation:** zero unauthorized deliveries or prompt inclusions across ordinary routing, quotes, contests, judging cases, regeneration, replay, and restart scenarios.
- **Addressing:** the TUI and router reject every recipient-free conversational submission using shared validation.
- **Parallel dispatch:** eligible addressed members start without waiting for another member’s response. Provider latency is measured separately.
- **Walkthrough completion:** controlled provider responses produce the canonical question-first answers, pick, parallel debate, and valid verdict through the TUI, and the direct shortcut through the CLI, including both confirmation paths.
- **Case isolation:** standalone rulings and follow-ups include only their authorized case material.
- **Recovery:** completed messages and contest progress recover from SQLite without duplicate presentation, repeated committed results, or expanded visibility.
- **Exclusive room use:** a second process opening the same room receives the prescribed lock error; read-only export remains available.
- **Control and resilience:** stop prevents further automatic work; exhausted debate attempts produce recorded forfeits; unrelated conversations continue.
- **Accounting:** every provider attempt has an accounting record. Replay does not double-count spending or wins and losses. Unknown charges remain visibly unresolved.
- **Export and reset:** Markdown exports preserve the selected scope, exclude credentials, and remain usable before `/clear` removes room state.
- **Setup:** supported fresh-machine checks complete the documented setup path, preserve an existing `.env` on re-run, and reach a running TUI after key configuration and the printed launch command.
- **Scripting:** controlled provider responses verify default waiting, text output, JSON lines, quoted evidence, and debate pre-confirmation under the **Room API and CLI** contract.
- **Configuration:** `.env` and process environment precedence produce the expected roster and initial round counts; stored runtime `ROUNDS`, `GRILL_ROUNDS`, and `CENSUS_PASSES` survive restart and take precedence over their startup seeds.
- **Daily max:** actual reported costs, pending reservations, unknown costs, billed failures, local-day rollover, and refusal at the max follow the shared accounting contract without resetting on `/clear`.
- **Pending picks:** invalid picks preserve the pending contest, unrelated messages continue normally, cancellation retains collected answers, and restart restores the eligible answers and pending pick.
- **Tournament:** isolation checks exclude other entrants’ answers and results from entrant contexts and exclude entrant nicks and model names from judge context; ranking validation, forfeits, and k−1 elimination rounds produce one champion without duplicate trophies.
- **Grill:** shared-floor delivery reaches exactly the participants; rotation, `PASS`, early completion, the no-repeat list, and leader failure preserve the specified final plan and appendix.
- **One-shot isolation:** one-shot commands never resume unrelated work; `--no-wait` commits and prints its cursor without starting accepted generation.
- **Census:** blind hunts, anonymous ratings, finder-count accounting, covered-item citations, singleton highlighting, estimate ranges, and the quiet stop condition produce the specified report across passes and plan revisions.
- **Crosscheck:** inventories exclude the plan, cell assignments exclude the reviewer’s own inventory items, fresh audits exclude reviewer identities, and every retained cell has evidence, a justified exclusion, or an open gap; risk acceptance remains the human’s decision.
- **Survey:** extraction, independent voting, anonymous reasons, second votes, and the five report bands preserve the specified visibility limits and distinguish agreement from verified truth.
- **Probe:** random role assignment, blind contributions, locked predictions, and the decision tree preserve competing explanations and include an unexpected-result branch without declaring one explanation true.
- **Shared scripts:** all seven modes use the shared engine moves, retain their prescribed behaviour, and enforce one active contest, script-derived preflight, stop, recovery, and export contracts.

### Glossary

| Term | Meaning |
|---|---|
| Room | The single persistent human-visible conversation, `main`. |
| Nick | A configured member identifier. |
| Recipient | A participant explicitly addressed by the human or authorized contest routing. |
| Private history | A model’s durable authorized conversational context. |
| Mailbox | A private in-process wake-up path for one agent context; durable work is read from SQLite. |
| Feed | Human-visible live activity; never an agent context source. |
| Bus | The trait for requests, feed subscriptions, and mailbox wake-ups, implemented in-process in v1. |
| SQLite store | The local database holding committed room events, private histories, work, and room state. |
| RoomHandle | The in-process API used by the TUI and one-shot commands. |
| Quote | Human-selected, attributed message text delivered with a new addressed message. |
| Solution | An eligible selected reply, or an opening position generated in round 0. |
| Topic | The originating user question or the fresh topic supplied for a contest. |
| Round | In a debate, one parallel turn from each debater, followed by a history commit barrier. |
| Round 0 | Opening positions generated before the counted debate rounds when needed. |
| Forfeit | A recorded contest participant failure after permitted attempts are exhausted, handled under that contest kind’s rules. |
| Judge | A summoned evaluator separate from all room members. |
| Case | Isolated judging material, its ruling, and explicit follow-ups. |
| Verdict | A validated scored debate decision and supporting assessment. |
| Standalone ruling | A free-form assessment of selected quoted messages. |
| Regenerate | Supersede a model reply and queue replacement reply work. |
| Replay | Recovery of committed durable events from SQLite after restart. |
| `.env` | The single configuration file, overridden by process environment variables. |
| Contest | A debate, grill, census, crosscheck, tournament, survey, or probe managed by the shared orchestrator; at most one is active per room. |
| Pick | A mentions-only control input selecting eligible collected answers for a pending debate or grill. |
| Tournament | A blind-judged elimination contest that removes one entrant per round until one champion remains. |
| Grill | A shared-floor contest in which grillers sequentially question a leader’s plan and the leader answers and rebuilds it. |
| Plan version | A preserved standalone plan produced initially or by a grill rebuild. |
| Daily spend | Squad’s actual OpenRouter-reported costs for the local calendar day, with reservations and unknown costs tracked separately. |
| Census | A lens-assigned blind gap hunt followed by anonymous ratings, a confirmed gap ledger, and an estimate of gaps still unfound. |
| Crosscheck | A coverage check against an inventory built without the plan, including cross-lens pairs and a fresh-context audit of closures. |
| Survey | A claim-by-claim map of independent agreement, disagreement, uncertainty, and changes after anonymous reasons. |
| Probe | A diagnostic contest that turns independent explanations, checks, and committed predictions into a decision tree. |
| Shared move | A reusable engine operation composed into a mode script: `collect`, `pick`, `fanout`, `turns`, `judge`, `tally`, `barrier`, or `report`. |
| Singleton | A census gap found by exactly one member during the blind hunt. |
| Quiet census | A pass adding no new confirmed gap with an unseen-gap estimate below 2. |

