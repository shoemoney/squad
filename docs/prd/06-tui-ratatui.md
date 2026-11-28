# TUI (Ratatui)

### Part 1: Specification

#### Scope and ownership

Squad ships as one self-contained Rust binary. `crates/squad-tui` implements the terminal interface with Ratatui 0.30.x, crossterm, and tokio. One process opens the persistent room `main`, runs its core tasks, and presents one TUI.

The TUI uses `RoomHandle` from `crates/squad-core` for requests, live events, history pages, authoritative state, export, and shutdown. Live traffic uses the in-process bus. SQLite holds durable state. The TUI does not access database tables, construct model context, or call OpenRouter.

The workspace contains six crates: `squad-protocol`, `squad-store`, `squad-agents`, `squad-core`, `squad-tui`, and `squad-cli`. Executors, the OpenRouter client, and the model registry live in `squad-agents`; the TUI accesses their results through `RoomHandle`.

Shared contracts retain their owners:

| Contract | Owning section |
|---|---|
| Envelopes, streaming identities, `Work`, `Cursor`, `ErrorCode`, `Request`, `RequestResult`, and `DebatePreflight` | Protocol, store and bus |
| Grammar, AST, contextual validation, and exact validation strings | Command grammar and parser |
| `SquadConfig`, `.env` loading, environment variables, commands, and distribution | Foundations |
| `RoomHandle`, request lifecycle, Markdown export, and shared banner text and fonts in `squad-core` | Room API and CLI |
| Ordinary generation, queueing, retries, context assembly, and model resolution | Agents and OpenRouter |
| Contest engine, admission, preflight semantics, states, debate setup, round scoring, cases, verdict schema, tournament rankings, and grill plans | Contests |
| Prompt content, limits, and placeholders | Prompt pack |
| Palette, effects design, effects manager, and banner animation | TUI effects and motion |

The same Contests owner defines the shared engine moves and scripts for all seven modes, including census ledgers and estimates, crosscheck cells and audits, survey tallies and bands, and probe prediction matrices and decision trees. The TUI presents those authoritative artifacts and does not execute scripts or calculate substitute results.

The TUI consumes these contracts without defining alternative event names, error codes, request shapes, cursor formats, or verdict schemas. Command definitions belong to Foundations; this section specifies their terminal presentation and interaction.

Configuration comes from the single `.env` configuration contract in Foundations. Process environment takes precedence. The roster follows `SQUAD_MODEL_<NICK>` order, personas use `SQUAD_PERSONA_<NICK>`, and the human display label remains `me` regardless of `SQUAD_USER_NAME`. Roster changes, personas, and concrete model selection take effect on restart. There is no runtime roster editor.

#### Why Ratatui

Ratatui uses immediate-mode drawing into a buffer, compares it with the previous buffer, and writes changed cells. The independent synthetic benchmark supports its suitability for efficient terminal drawing, but establishes no result for 12 concurrent streams. The owner selected Ratatui; Squad must validate its actual transcript and editing workload before implementation proceeds. The research also leaves textarea soft wrapping unresolved, so Squad will implement its own input widget and validate that choice in an input spike.

Canonical values are defined in [Protocol’s canonical numbers and configuration](02-message-protocol-store-and-bus.md#canonical-numbers-and-configuration).

Every output ceiling limits visible output. For models whose catalog entry lists reasoning support, the Agents and OpenRouter contract sends `max_tokens = visible ceiling + reasoning budget` and an explicit `reasoning.max_tokens` equal to that budget. Unsupported models receive no reasoning parameters unless an explicit unsupported setting must be rejected. The TUI displays accepted configuration and errors without constructing provider requests.

`SQUAD_ROUNDS` seeds `ROUNDS` only when the room has no stored value. The stored room value wins thereafter. `SQUAD_GRILL_ROUNDS` seeds `GRILL_ROUNDS` by the same rule. Other configuration mappings and compile-time constants remain defined by Foundations and `squad-protocol`.

Display `CENSUS_PASSES` and its optional `SQUAD_CENSUS_PASSES` seed through the Foundations configuration contract. The optional environment variable is not added to `.env.example`.

#### Application architecture

A single tokio application loop in `crates/squad-tui/src/event_loop.rs` owns mutable UI state. It uses `tokio::select!` over:

- Terminal events from crossterm.
- Client events from the local `RoomHandle` subscription, request acknowledgements, and history-page results.
- An armed frame timer using `FxManager::next_wake()`, capped at 60 Hz and disarmed when idle.

Input is always handled first. Drain feed events in bounded batches of at most 256 items per iteration so a burst cannot starve drawing. Pending dirty-state and token-batch deadlines also arm the timer when needed; effects off must not suppress ordinary updates. There is no second effects event loop.

Request handling and history loading must not block terminal input. Expensive parsing and highlighting run outside the drawing path. Results return as UI events associated with the relevant message revision.

The application state is organized as an `App` struct:

```rust
struct App {
    room: RoomState,
    transcript: TranscriptState,
    input: InputState,
    participants: ParticipantsState,
    focus: Focus,
    completion: CompletionState,
    selection: SelectionState,
    search: SearchState,
    pending: PendingSubmissions,
    preflight: PreflightView,
    contest: ContestView,
    pick: PickView,
    spend: DailySpendView,
    terminal: TerminalCapabilities,
    viewport: Rect,
    debug: DebugPaneState,
    dirty: bool,
}
```

`RoomState` is the shared authoritative type. The other fields contain local interaction state or views of shared values; they do not redefine shared contracts. `ContestView`, `PickView`, and `DailySpendView` hold presentation state for the authoritative active contest, eligible pick candidates, and daily accounting.

The TUI attaches an internal origin tag when applying live or replayed feed items:

```rust
enum FeedOrigin {
    Live,
    Replay,
}
```

This tag is not a field added to the protocol's `FeedItem`. Replayed items restore static presentation without replaying entrance effects.

Event handlers update state and schedule effects. Render functions receive immutable application state and write to a Ratatui buffer. They perform no requests, database access, state transitions, Markdown parsing, or syntax highlighting. Widget measurement and cache updates occur before drawing.

The render loop exposes a post-render effects pass and an effects manager hook. Static rendering is complete and correct with effects off. The TUI effects and motion section owns effects behavior, `FxManager::next_wake()`, and banner animation; this section owns event-loop scheduling.

Enable raw mode, the alternate screen, bracketed paste, and mouse capture for the session. Restore terminal modes, keyboard enhancements, cursor visibility, and synchronized-output state on normal exit, initialization failure, drawing failure, and panic.

Ctrl+C, empty-draft Ctrl+D, and `q` outside the input invoke the shared shutdown lifecycle before leaving the process. Shutdown and unfinished-work recovery remain owned by Room API and CLI and Protocol, store and bus.

Logs use the Foundations file logging configuration. Logs must not write to the terminal while the TUI is active.

#### Layout and participants

The three primary regions are participants, transcript, and input. A room-status and daily-accounting header spans the top. Keyboard help occupies the last row.

| Viewport | Participants | Divider | Transcript |
|---|---|---|---|
| 120×36 | 20 cells | 1 cell | 99 cells |
| 80×24 | 16 cells | 1 cell | 63 cells |

The minimum viewport is 80×24. Input starts at four rows and expands to eight, taking space from the transcript. The header uses two rows so today's spending, pending reservations, daily max, and remaining daily amount fit at both widths. Keep the daily spend bar in the participants footer when it fits; otherwise use the status line. It remains visible while the participants list scrolls.

The following ASCII mocks show 36 and 24 rows respectively. Trailing blank cells are omitted; the table defines allocated widths. Bracketed text denotes content placeholders.

120×36:

```text
Squad | room main | [room status]
$3.42 ▰▰▱▱▱▱▱▱▱▱ $20.00 | Pending [cost] | Remaining today [cost]
Participants        |Transcript
grok 4.7            |me
streaming q0        |  [user message]
tokens unavailable  |grok 4.7 streaming
cost unavailable    |  [streamed Markdown]
W-L 7-2 trophies 1  |
                    |claude [version]
claude [version]    |  [reply text]
idle q0             |
tokens unavailable  |Debate [id] expanded [state]
cost unavailable    |  Topic: [topic]
W-L 0-0 trophies 0  |  [OPENING banner and opening positions]
                    |  [ROUND 1 banner and scorecard]
openai [version]    |  grok: [turn text]
idle q0             |  claude: [turn text]
tokens unavailable  |  [remaining rounds and closing statements]
cost unavailable    |  [VERDICT banner and actual judge model]
W-L 0-0 trophies 0  |  Winner: [debater]
                    |  [criterion scores, weighted totals and round tally]
gemini [version]    |  What to steal from the loser
idle q0             |  [accepted verdict content]
tokens unavailable  |[older history remains accessible]
idle q0             |
                    |
muse [version]      |
idle q0             |
                    |
                    |[new activity count when reading older content]
Input | recipients: [resolved members] | quotes: 0
> @grok @claude $debate tabs vs spaces
  [continuation row]
  [continuation row]
F6 Focus | Tab Complete | Ctrl+J Newline | Ctrl+R Recipients | Ctrl+F Find | F12 Debug
```

80×24:

```text
Squad | room main | [room status]
$3.42 ▰▰▱▱▱▱▱▱▱▱ $20.00 | Pending [cost] | Remaining today [cost]
Participants    |Transcript
grok 4.7        |me
streaming q0    |  [user message]
tokens          |grok 4.7 streaming
unavailable     |  [streamed Markdown]
cost            |Debate [id] expanded
unavailable     |  [state and topic]
W-L 7-2 trophy 1|  [original solutions]
claude [version]|  [OPENING banner]
idle q0         |  [opening positions]
tokens          |  [ROUND 1 banner and scorecard]
unavailable     |  [two independently streaming turns]
cost            |  [VERDICT banner and collapsed summary]
unavailable     |  [winner and judge model]
W-L 0-0 trophy 0|
[more members]  |[older history remains accessible]
                |[new activity count]
Input | recipients: [members] | quotes: 0
> @grok @claude $debate tabs vs spaces
  [continuation row]
  [continuation row]
F6 Focus | Tab Complete | Ctrl+J Newline | Ctrl+F Find | F12 Debug
```

Participant entries wrap within their region and scroll independently. Full nick lengths and a 12-member roster must remain navigable without dropping identity, status, or accounting fields.

Each entry shows:

- Nick with resolved model version immediately beside it, such as `grok · 4.7`.
- Textual presence: `idle`, `thinking`, `streaming`, `error`, or `offline`.
- Queue depth and contest participation when applicable.
- The authoritative room win/loss record, such as `7-2`, with tournament trophy count beside it.
- Participant token usage and cost, with unavailable values labeled.

Participant detail and `/who` expose full concrete model slugs. Versions remain fixed until process restart. The judge appears inside its contest or case, never as a room member or an `@all` completion target.

Round scorecards do not change the participants panel. Win/loss records still come only from verdicts. Tournament eliminations mark the entrant's row `eliminated` for the rest of that tournament. Trophy counts come from authoritative tournament results. Grill completion changes neither record.

Census, crosscheck, survey, and probe show current participation and work status without changing win/loss records or trophies. Show the probe member who sits out as excluded from that run, without marking it offline or failed.

#### Startup, restoration, and failure screens

Startup obtains the room state and newest durable history through `RoomHandle`, then pages older events on demand. Subscribe and reconcile using canonical identities so events arriving during initialization are not lost or duplicated.

Disable submissions and mutations until initialization or state reconciliation completes. Keep available history readable and preserve the draft, quote selections, and reading position during reconciliation.

If the feed reports lag, restore authoritative state and missing durable events through Room API and CLI recovery semantics, re-reading durable events after the last seen cursor. Discard unrecoverable provisional text and restore current work state. Pending requests retain their original identities; do not create replacement submissions merely because an acknowledgement is delayed.

Restore the active contest, daily accounting, trophies, tournament elimination markers, grill plan versions, and any pending pick from authoritative state and durable history. A pending pick has no expiry and remains usable after restart.

Restore each new mode's current step, committed artifacts, source plan or question, census pass and plan version, crosscheck acceptance records, survey voting stages, probe role split, and final or partial report from authoritative state. Replay restores static content without rerunning a mode, reassigning roles, resampling audits, or replaying entrance effects.

If the API reports an expired or invalid cursor, show its shared error and available earliest cursor. Offer the API's explicit history-reset behavior without claiming unavailable history was recovered.

Provide these screens:

- **Missing key:** identify missing `OPENROUTER_API_KEY`, show the selected `.env` location and Foundations setup guidance, and provide a visible exit action. Never display a key or request it through the transcript.
- **Room locked:** display `Room main is already open in another squad process (pid N).` using the reported PID, restore the terminal, and exit according to the Foundations command contract. Do not bypass the lock.
- **Terminal too small:** show the current dimensions and required 80×24 minimum. Preserve the draft, cursor, selections, transcript anchor, and room activity. Restore the normal layout when space becomes sufficient. Keep the daily spend bar visible in the compact status display.

#### Virtualised transcript and scrolling

Implement a custom virtualised transcript widget. Do not construct or render the whole room as one text widget.

Maintain message-level wrapped-height caches keyed by canonical identity, content revision, available width, and presentation state. Presentation state includes contest expansion, scorecard expansion, tournament ranking details, grill plan and appendix expansion, pick candidates, banner mode and font fallback, and Markdown styling. Banner height counts in the row cache. Maintain cumulative heights for locating the visible range. Draw only visible rows and bounded overscan.

Include census ledger rows and pass details, crosscheck grid summaries and cell details, survey bands and voting details, and probe tree branches and prediction matrices in presentation-state cache keys. Expanding a report or applying a committed artifact preserves the same transcript anchor rules as other contests.

Keep a bounded rendered window while retaining access to older durable history. Reaching the oldest loaded content requests another `RoomHandle::page_events` page. A `Cursor` is the shared integer event sequence; the TUI does not invent another cursor format.

Preserve the top message ID and source grapheme offset across streaming, reflow, Markdown tail changes, and contest expansion. If an anchor disappears, use the nearest surviving message. `/clear` invalidates affected selections and anchors after the authoritative reset.

Follow new content only while at the bottom. Scrolling upward freezes the reading position and shows a new-activity count. Streaming below the viewport must not move it.

Page Up and Page Down move by transcript height minus two rows. Mouse wheel input scrolls the region under the pointer: transcript or participants. Transcript wheel scrolling suspends live following until the user returns to the bottom.

The alternate screen requires Squad to provide its own history navigation, selection, search, and copy.

#### Text measurement and streaming Markdown

Use `unicode-segmentation` for extended grapheme clusters and a pinned `unicode-width` implementation for display cells. Never use byte length or Unicode scalar count as terminal width.

Ordinary characters occupy one cell, wide CJK characters and emoji occupy two, combining marks remain attached, and East Asian ambiguous characters occupy one. Probe terminal mode 2027 where available and record capability results. Unsupported or unanswered probes use the pinned fallback and must not delay startup indefinitely.

Transcript bodies have two cells of padding on each side. Wrap complete grapheme clusters greedily without inserting hyphens. Preserve spaces and explicit newlines. Code uses the same wrapping rule. Expand tabs to four-cell stops relative to the content origin and normalize CRLF to LF.

Use `pulldown-cmark` to parse Markdown into an internal block representation:

- Cache finalized top-level blocks and their styled content.
- Re-parse and re-render only the live tail during append-only streaming.
- Re-evaluate the tail boundary before finalizing it. A paragraph that becomes a table or code fence must remain revisable.
- Cache wrapping separately by width, so resizing reflows cached blocks without reparsing finalized source.
- On attempt replacement, invalidate that attempt's provisional blocks and tail.
- On durable completion, finalize the remaining tail and reconcile the message once.

Use `syntect` for fenced-code highlighting. Preserve fence language labels, keep incremental highlighting state for an open fence, and use plain code styling for unknown languages. Highlighting must not block input or drawing. Code remains grapheme-wrapped and copyable independently of its visual wrapping.

Strip terminal escape sequences and unsupported control characters from all untrusted display fields before producing terminal spans. Markdown cannot emit terminal control sequences. Model text cannot execute commands, route messages, set variables, or trigger completion.

#### Streaming and drawing

Reserve one stable reply card per addressed generation in configured participant order. Concurrent token arrival updates cards in place and cannot reorder the response group.

Use protocol identities to correlate requests, work, attempts, messages, and chunks. A higher attempt replaces the previous attempt's provisional text. Never concatenate attempts or invent a reset event.

Buffer deltas outside the drawing path. Flush token-only card changes no more often than once per 50 ms. The event loop draws dirty UI state within its fixed budget, capped at 60 Hz, without drawing once per token. Disarm the frame timer when idle. Effects scheduling uses the hook owned by TUI effects and motion.

Final, failure, cancellation, and room-lifecycle events bypass the token batching delay and render immediately. Coalesce resize events to the latest dimensions.

Use Ratatui cell diffing. Do not deliberately clear the full screen between updates. Individual effects are specified in TUI effects and motion.

Wrap each draw in synchronized output using DEC private mode 2026 when supported. Always close synchronized output, including on errors. Unsupported or unanswered capability queries fall back to ordinary diffed drawing. Verify behavior through tmux and SSH rather than assuming terminal support.

A durable final replaces its provisional card exactly once. Repeated durable events reconcile by canonical identity. Reasoning text is never rendered.

Failed or canceled partial output remains visibly incomplete and does not become durable conversation content. An exhausted ordinary reply exposes the shared retry action. Retrying one failed recipient does not restart successful siblings. Contest retries remain orchestrator-controlled.

Regeneration uses the shared `regen` request. Render `message.superseded` and replacement work. Keep the old reply inspectable as superseded while excluding it from current reply selection. The core controls its exclusion from future member context. Sent human messages cannot be edited.

#### Multi-line input and completion

Write a Squad input widget rather than adopting a textarea dependency. A spike must compare a minimal custom implementation with Ratatui 0.30.x-compatible textarea candidates and record soft-wrap, cursor, selection, paste, and resize results. The custom widget must pass these requirements before integration proceeds.

Reserve two cells for the prompt or continuation indentation, then use the shared grapheme wrapping algorithm. Soft wrapping does not change submitted text; explicit newlines do.

Cursor movement, selection, and deletion operate on grapheme boundaries. Terminal coordinates are never interpreted as string offsets. When input exceeds its visible rows, follow the cursor and show `more above` or `more below`.

Maintain session input history of accepted human submissions. History navigation preserves an unfinished draft and its cursor so returning from history restores it. Recalled text is editable and requires explicit submission.

Enable bracketed paste. Pasting long text or code inserts draft text without submitting it or interpreting pasted key sequences as actions. Enforce the canonical UTF-8 byte limit through shared validation. Images and file uploads are unavailable.

Query and enable Kitty keyboard protocol progressive enhancement when available. Distinguish Shift+Enter from Enter only when the terminal reports that distinction. Keep Ctrl+J and Alt+Enter as newline fallbacks. Restore prior keyboard mode on exit. Handle key press, repeat, and release events without duplicate submission.

Completion opens above input, shows at most six candidates, scrolls its selection, and keeps the selected row visible. Match case-sensitive prefixes at the caret. Acceptance replaces only the current token and adds a separating space.

Completion includes:

- Configured member mentions and `@all`.
- `-nick` exclusions directly after `@all`, omitting already selected exclusions.
- `@judge` in valid debate or quote-supported judging contexts.
- `$debate`, `$ROUNDS`, and `$ROUNDS=`.
- `$tournament`, `$grill`, `$GRILL_ROUNDS`, and `$GRILL_ROUNDS=`.
- The complete supported slash-command set from Foundations.
- `$census`, `$crosscheck`, `$survey`, `$probe`, `$CENSUS_PASSES`, and `$CENSUS_PASSES=`.

While a pick is pending, mention completion offers eligible nicks first while retaining ordinary completion targets for other messages.

Use the shared parser for validation, recipient previews, protected code, escapes, variable handling, and command recognition. The recipient preview shows the resolved explicit list for `@all -nick [-nick...]`. Parser spans use UTF-8 byte offsets and parsing returns a plain Rust `Result`.

`$ROUNDS=5`, `/set ROUNDS 5`, and bare `$ROUNDS` retain their shared assignment and lookup semantics. Show changes as pending until authoritative confirmation. Apply the same presentation to `$GRILL_ROUNDS=n`, `/set GRILL_ROUNDS n`, and bare `$GRILL_ROUNDS`.

Apply the same pending and confirmed presentation to `$CENSUS_PASSES=n`, `/set CENSUS_PASSES n`, and bare `$CENSUS_PASSES`. The shared parser owns the four new verbs' argument rules and exact validation strings; display those strings verbatim.

Display parser errors verbatim, including:

- `Add at least one recipient: @nick or @all.`
- `$debate requires exactly two distinct debaters.`
- `$tournament needs at least three entrants.`
- `@{nick} has no plan to grill yet. Ask it something first.`
- `Add a question after $debate.`

Direct and question-first forms follow the decision table owned by Command grammar and parser. The TUI does not infer a different mode from the same input.

Preserve the draft and quote attachments after validation failure or rejection. A rejected submission creates no local durable message.

Ctrl+R reinserts the previous accepted message's recipients at the caret.

#### Search, selection, and copy

Provide in-transcript literal search through Ctrl+F. Show the query, current match, match count, and whether older history remains unsearched. Search available durable history through the existing history-page API without treating unloaded content as absent.

Search navigation reveals collapsed content containing a match and scrolls to it. Leaving search restores normal navigation without discarding the draft or message selection.

Transcript focus supports message navigation, detail inspection, selection, and labeled actions. Space toggles selection of the focused durable message.

Provide distinct actions for Quote, Regenerate, Debate solutions, Grill plan, Copy message, and Copy code block. Eligibility comes from the owning contracts. Selected-message actions submit explicit canonical IDs.

OSC 52 copy is user initiated. Copy sanitized message source or fenced-code contents without visual wrapping or presentation labels. Model output must never initiate clipboard access. If OSC 52 is unavailable or blocked, report that copying was not confirmed and offer the Foundations export workflow.

Every action has visible text and keyboard help. Focus, selection, presence, failure, and pending actions remain understandable without color.

#### Quote selection and judge cases

A Quote action adds selected durable messages to the next user message. Show author, text preview, attachment count, and a removal action before submission. Enforce the maximum of 12.

Submit IDs through the shared quote contract. The router resolves content and author labels. Every recipient receives those quoted words as part of the human message. Selection or inspection alone shares nothing with models.

Quote selection is separate from debate solution selection. Use `replyTo` only for `$debate` solution references, never for ordinary quoting or judge follow-ups.

The user can select messages and submit `@judge which of these is best?`. The judge must be the sole recipient. A new standalone case starts with exactly the selected material and the question. Render its ruling as attributed free-form text without scores or a winner card.

A Follow up action on a verdict quotes that verdict and addresses only `@judge`. Manually quoting a verdict has the same effect. The core reopens that case with its material, verdict, and prior case follow-ups. Display the case identity and its relationship to the original verdict. Unrelated cases remain separate.

Invalid judge requests display `Quote at least one message for @judge to rule on.` Case context, persistence, and supplied evidence remain owned by Contests.

#### Inline debates and preflight

Debates are collapsible blocks within the single transcript. Expanding one preserves surrounding content and the reading anchor. There is no separate debate screen.

Users can select eligible completed replies from different members as explicit solutions. Preview both authors, selected texts, and topic before submission. Typed requests follow Contests source-selection rules. The TUI does not choose substitute sources.

When required by the core, show opening round 0 separately from configured rounds. Its positions become the solutions and do not count toward `ROUNDS`. Round 0 has no scorecard.

Every `debate.preflight` displays the authoritative estimate, number of model calls, reservation ceiling, remaining daily amount, debaters, round count, effective judge when requested, and warnings. Consume the Protocol section's `DebatePreflight`, including `debaters`, `rounds`, `judgeRequested`, and `openingRoundRequired`, with an optional judge model. The estimate includes one round-scoring judge call per configured round when judging is requested, in addition to final judging. Do not calculate a competing estimate.

At or below the configured threshold, show that the core auto-confirmed. Above it, provide explicit Confirm and Cancel controls using `debate.confirm` and `debate.cancel` through `RoomHandle`. Bind actions to the authoritative plan identity. Expired or changed plans require new preflight. Plans expire after 10 min. Disable repeated actions while acknowledgement is pending.

The typed direct debate request remains pending until admission. It becomes a durable submitted message only after admission is confirmed. Canceled direct preflight creates neither a local completed debate nor ordinary replies. Answers already committed by a question-first contest remain ordinary replies if its later preflight is canceled.

Display `WAITING_FOR_DEBATERS` while earlier work drains. Ordinary messages addressed to debaters during a debate appear queued. Other members remain usable. A second contest request displays `A {kind} is already running. Stop it before starting another.` using the active contest's kind. Held-message history delivery and frozen round boundaries remain owned by the contest engine.

Every `$verb` mode displays its authoritative model-call count and cost estimate before starting its contest work. Tournament and grill preflight use the shared contest engine's plan and confirmation contracts, including admission against the remaining daily amount. A pending pick occupies the room's one active contest slot.

Expanded blocks show debate ID, debaters, topic, original solutions, retained instruction, effective rounds, state, and round markers. Turns stack vertically in stable participant order. Show parallel streams independently, including retries, forfeits, truncation or filtering indicators, and closing statements.

For judged debates, each configured round shows a pending score marker until its committed `debate.round_score` arrives. Render the scorecard inside that round, for example `Round 3   grok 7  ·  gemini 8   (tally 20 to 22)`. A labeled Expand scorecard action reveals both reasons; Collapse scorecard hides them. Wrap the scorecard within the available width without dropping identities, scores, or tally.

Consume the shared event's `debateId`, `caseId`, `round`, `scores`, and `tally`. Scores contain each debater's integer score and reason. The tally is the authoritative running sum supplied by the core; do not calculate a competing total or add repeated events again. Place late scores by their round identity even when later turns are already visible. Preserve the reading anchor when a scorecard arrives or expands.

If the shared event has `scores: null` and `reason: "unscored"`, show `unscored` rather than a pending marker or fabricated zero. Round-scoring failures do not stop subsequent turns. Show outstanding scoring work while the core waits to begin final judging; the final judge runs only after all round-scoring work has completed or been given up.

Round scores are addressed to `me` only during the debate. Displaying or expanding them does not send them to either debater. The core delivers existing round scores and the final round tally to both debaters with the verdict announcement.

Escape closes the topmost transient element first. With nothing transient open, it requests a global stop; during an active contest, show `Press Esc again within 2 s to stop the active contest.` and require that confirming second Escape before sending the stop request. `/stop` and `/stop @nick` use the shared stop operations. Preserve the transcript and display stopping state until acknowledgement. Do not infer successful cancellation locally. `/stop` also cancels outstanding `judge.round` work; reflect authoritative cancellation without leaving canceled score work marked as pending. Animation skipping must not consume Escape or input keys.

Without `@judge`, completed direct debates display `COMPLETE_UNJUDGED` and have no round scoring or pending score markers. Expose the shared judging action for eligible completed or stopped debates. `/judge` on a stopped debate produces a final verdict using the transcript and existing scores without retroactive round scoring.

Manual judging follows the contest engine's judge-only preflight transitions from `STOPPED`, `COMPLETE_UNJUDGED`, or `FAILED` through `PREPARING`, optional `AWAITING_CONFIRMATION`, and `JUDGING`. Canceling or expiring that plan restores the stored prior status. Keep the associated cases inspectable by their authoritative identities.

#### Question-first contests and picking

The canonical flow is `@all $debate <question>` → ordinary answers → `CHOOSE YOUR DEBATE BROS` → `@grok @gemini` → preflight and confirmation → rounds → verdict. The direct `@a @b $debate [text]` form remains a shortcut.

Consume `contest.collecting`, `contest.pick_requested`, and `contest.picked` through the shared event contract. During `COLLECTING_ANSWERS`, render the addressed members' ordinary replies as stable, independently streaming cards. When all addressed members have answered, errored, or been stopped, render the authoritative eligible candidate list in `AWAITING_PICK`.

Use these inline FIGlet banners, hints, and input placeholders:

| Kind | Banner | Hint and input placeholder |
|---|---|---|
| Debate | `CHOOSE YOUR DEBATE BROS` | `Type two names, e.g. @grok @gemini · Esc to cancel` |
| Grill | `CHOOSE WHO GETS GRILLED` | `Type one name, e.g. @grok · Esc to cancel` |

Show `pick pending` in the status line. The hint stays directly under the banner and wraps when necessary. With `SQUAD_BANNERS=off`, retain the ordinary pick label, candidates, and hint.

A pick message consists only of the required number of eligible `@nick` mentions. Submit the parser's accepted pick through the Protocol-owned `ContestPick { contestId, nicks }` request using the pending contest's identity. Do not send it as an ordinary human message or create duplicate work while its acknowledgement is pending.

Display shared pick-validation strings verbatim, substituting the eligible list:

- `Pick exactly two of: @a, @b, @c.`
- `Pick exactly one of: @a, @b, @c.`
- `@{nick} did not answer, so it cannot be picked.`

Wrong picks preserve the pending pick and draft. Any input classified by the shared parser as an ordinary message or command follows its normal path, and the pick remains pending. For example, `@claude explain your third point` remains available. The room is not frozen.

After a debate pick, the picked answers become the two solutions, the original question becomes the topic, and judging is requested by default. After a grill pick, the picked answer becomes the leader's plan and every other member that answered becomes a griller. Preflight and confirmation happen after the pick. Tournament has no pick step.

Show `Not enough answers to start. {n} of {m} members answered.` when the core ends collection as `FAILED`: fewer than two eligible answers for a debate, or fewer than two participants overall for a grill.

Escape follows the transient-first and second-Escape confirmation rule above. A confirmed Escape stop or `/stop` while a pick is pending cancels the contest as `STOPPED` with reason `pick_cancelled`. The ordinary answers remain in the transcript and in their members' private histories. Do not expire a pending pick locally.

Initial answers use ordinary `reply` work and count toward today's spend. They are excluded from the later contest preflight estimate. Candidate-row highlights and banner entrances belong to TUI effects and motion; with effects off, candidates remain identifiable through text and ordinary selection styling.

#### Inline tournaments

Render each tournament as a collapsible block in the single transcript. Show its ID, prompt, entrants, current survivors, round, state, actual judge model, and authoritative preflight. For k entrants, the preflight displays k + (k−1) + ... + 2 answer calls plus k−1 judge calls and their total estimate. Consume the core's count and estimate.

Render `tournament.setup`, `tournament.state`, `tournament.entry`, `tournament.ranking`, `tournament.result`, and `tournament.champion` using their Protocol-owned payloads and identities. Display `PREFLIGHT`, `WAITING_FOR_ENTRANTS`, `ANSWERING`, `JUDGING`, `ELIMINATING`, `COMPLETE`, `STOPPED`, and `FAILED` as authoritative states.

Within each round, surviving entrants' answers stream in parallel in stable participant order. The human can inspect all answers, ranks, integer scores, and critiques. Show forfeit and retry states explicitly. The TUI does not rank entrants or resolve ties.

The accepted ranking is a strict best-first order. The effects layer reveals it from last place up; the completed static view preserves the accepted order and shows the eliminated entrant clearly. Mark that entrant's participants row `eliminated` for the rest of the tournament. Keep previous rounds inspectable. There is exactly one elimination per round and k−1 rounds for k entrants.

Display the champion and final placing list when committed. Use the authoritative trophy count beside the debate win/loss record without incrementing a local counter on repeated events. Champion celebration belongs to TUI effects and motion and reuses the verdict celebration.

The judge remains the configured judge even when its model competes because judging is blind. Do not apply the debate substitution warning to tournaments. Inspection, expansion, ranking reveals, and copying do not send the human-visible answers or other entrants' results to any entrant. Blind labels, label shuffling, entrant-only results, and the final placement announcement remain controlled by the core.

If judging exhausts its three attempts, show the core's single round replay and its replacement work. If that replay also fails, show `FAILED` with the shared error and retained transcript. Do not initiate an independent replay or expose invalid rankings as accepted results.

#### Inline grills

Render each grill as a collapsible block showing its ID, leader, grillers, focus, `GRILL_ROUNDS`, current round, current turn, current plan version, state, and authoritative preflight. The displayed call count is rounds × (grillers × 2 + 1), with the core's estimate.

Direct `@leader $grill [free text]` uses the leader's latest eligible reply, or an explicitly selected or quoted leader message, under the contest engine's source-selection contract. Preview the selected plan and leader. Default grillers are the other room members; an explicit direct participant list retains first-mention leadership and the requested griller subset. Question-first grill selection follows the pick contract above.

Render `grill.setup`, `grill.state`, `grill.question`, `grill.answer`, `grill.pass`, `grill.plan`, and `grill.complete` using their Protocol-owned payloads. Display `PREFLIGHT`, `WAITING_FOR_PARTICIPANTS`, `QUESTIONING`, `ANSWERING`, `REBUILDING`, `COMPLETE`, `STOPPED`, and `FAILED` as authoritative states.

Turns appear sequentially in the core's roster order, rotated by one each round. Each turn has a heading such as `claude ▸ grills ▸ grok`, its primary angle, up to three questions, and the leader's answers followed by `Plan changes`. Wrap headings without losing either identity. Show `PASS` and skipped turns explicitly. Display the authoritative counter in the form `questions asked 14 · passed 2`.

Keep all plan versions viewable as collapsible blocks, with the newest expanded. At the end of a round, show rebuilding state until the complete replacement plan is committed. Preserve the last good plan on leader failure. The final view contains the final plan and an appendix of every question, its author, the answer, and the plan change.

A grill has a shared floor: the leader and grillers receive its committed plan, questions, and answers. Non-participants receive nothing. Explain the participant scope in the grill block before confirmation; opening a block does not change delivery. Context compaction and the earlier-question list remain owned by the contest engine.

Show early completion with reason `survived` when all grillers pass under the shared completion rules. A griller that fails after retry is shown as skipped for that turn. A leader that fails after retry leaves the grill `FAILED` with the last good plan inspectable. There is no judge, winner, verdict card, trophy, or win/loss update.

#### Shared presentation for the four new modes

Census, crosscheck, survey, and probe use the same inline contest container, authoritative preflight, stop controls, request acknowledgements, history paging, and recovery path as the existing modes. All seven modes occupy the same single active contest slot. D71's implementation order is debate, grill, census, crosscheck, tournament, survey, probe; existing TUI task IDs remain stable.

The engine's `collect`, `pick`, `fanout`, `turns`, `judge`, `tally`, `barrier`, and `report` moves do not create separate screens or client-side orchestration. Render the script's user-facing step labels. Parallel member work has stable actor cards, sequential work follows committed order, judging shows pending status until accepted structured output arrives, and a barrier exposes the committed batch together. Tallies and reports display stored core results. No model-call spinner is shown for an application-only tally.

For each new mode, consume its Protocol-owned `<mode>.setup`, `<mode>.state`, committed-artifact events, and `<mode>.report`. Show `PREFLIGHT`, `WAITING_FOR_PARTICIPANTS`, the authoritative state for each script step, and `COMPLETE`, `STOPPED`, or `FAILED`. Keep step completion distinct from contest completion. Invalid or provisional structured output cannot appear as an accepted artifact.

Before work starts, show the selected participants, source material, authoritative model-call count, cost estimate, reservation, remaining daily amount, and confirmation status. The core owns count formulas, retries, admission, and confirmation requirements. The six-member reference counts are 14 calls per census pass, 16 for crosscheck with explicit goal and constraints, 20 for question-first survey or 14 with existing answers, and 10 for probe. Crosscheck goal-and-constraint extraction adds one call when needed. Show that extra call explicitly when the core includes it.

Every mode starts with its named banner and retains ordinary step dividers with banners disabled. The step labels are:

| Mode | Step dividers |
|---|---|
| Census | `PASS p · HUNT`, `PASS p · MERGE`, `PASS p · RATE`, `PASS p · FINALISE AND ESTIMATE` |
| Crosscheck | `EXTRACT GOAL AND CONSTRAINTS` when needed, `BLIND INVENTORY`, `NORMALISE AND BUILD GRID`, `CHECK CELLS`, `AUDIT`, `RULE AND REPORT` |
| Survey | `COLLECT ANSWERS` when needed, `EXTRACT CLAIMS`, `BLIND VOTE`, `TALLY`, `RE-VOTE`, `MAP` |
| Probe | `BLIND EXPLANATIONS AND CHECKS`, `LOCK PREDICTIONS`, `COMPARE AND REPORT` |

These are display labels for script progress, not new protocol states or additional model calls. Restore them from authoritative state and committed artifacts.

The human may inspect all material delivered to the TUI, including provenance withheld from members. Show the visibility scope beside each step. Expanding, searching, selecting, or copying an artifact does not deliver it to members. Quoting or sending material remains an explicit human routing action through the shared contracts. The TUI never reconstructs member payloads from the human-visible transcript.

When a member fails a step after retry, show the member, affected step, and authoritative skipped status. Include skipped members in the report and retain incomplete work as incomplete. Do not fabricate missing ratings, cells, votes, or predictions. On `/stop`, preserve committed artifacts and wait for the acknowledged stopped state. A stopped or failed run must not display a completion or stop-rule success message.

Each final report is its own collapsible block within the contest. Its collapsed summary retains mode, contest ID, state, source reference, and principal result. F2 and labeled report-detail actions provide keyboard access. Tables become stacked labeled records where necessary; no field disappears at 80×24. Search reveals matching details, and copying uses source content without display wrapping.

Expose presentation hooks for the Effects section's mode banner entrances, census estimate count-down, crosscheck cell filling, survey bar splitting, and probe branch drawing. The complete static report remains available with effects off. Animation never changes accepted values, implies a new artifact, or delays stop controls.

#### Inline census

Preview the plan supplied by quote, pasted text, a grill's latest plan version, or the latest eligible reply selected by the shared `@nick $census` contract. Show the resolved participating members and `CENSUS_PASSES`; source-member selection does not replace the core's participant validation.

The `CENSUS` banner introduces the run. Each pass shows its number, configured pass count, plan reference or version, assigned member lenses, and the four step dividers. During the blind hunt, the human can inspect each member's list while the visibility label states that members cannot see one another's findings. The merge step produces the ledger; the rating step shows `real`, `covered`, or `trivial` judgments, cited plan locations, and additions as they commit.

Render the report as a ranked ledger table with gap ID, gap, finder count, lens, consequence, and disposition. Mark every singleton with the text `singleton` as well as the effects-owned emphasis. Expand a gap to inspect its source findings, ratings, additions, and covered-item citations. Keep covered and trivial items inspectable separately from confirmed gaps. The member-by-gap table is a human-only drilldown; finder identities and counts do not become member-visible through inspection.

The summary uses the authoritative line exactly:

`Found {n}; estimated {lo} to {hi} still unfound.`

Keep the range and the caveat visible together: models share blind spots, so the true number still unfound is at least this estimate. Never replace the range with a single number, derive bounds in the TUI, or count rating-stage additions as hunt finder observations. The core owns the estimate and its inputs.

Show the discovery curve across passes with aligned pass labels and authoritative counts, with a textual detail view. Identify which plan version each pass examined. Between passes, show the core's continuation state and the option to quote a patched plan or send the ledger to `$grill`; the TUI must not silently advance using an assumed replacement plan.

Keep the stop rule visible: no new confirmed gap in a pass and an estimate below 2. Show `Census is quiet. Good place to stop.` only when supplied by the core for that pass. Exhausting the configured passes does not by itself satisfy that rule.

Include the hint `Send singletons to $grill.` A labeled action may prepare the appropriate quote and draft through existing routing controls; sending requires the user's submission.

#### Inline crosscheck

Preview the resolved plan, goal, constraints, and participants. If the core must extract goal and constraints, show that preliminary step and its additional call before the blind inventory. The `CROSSCHECK` banner introduces the run.

During `BLIND INVENTORY`, show each member's lens and committed inventory under a clear plan-hidden visibility label. The human's plan preview remains separate from the inventory step's member payload. Inventory writers do not receive the plan by opening either view.

Render grid construction and cell review as an inline coverage grid summary with textual counts for pending review, cited coverage, justified exclusion, open gap, and explicit human risk acceptance. Use stable cell IDs and distinct textual statuses; color is supplementary. Show the actual retained cell count, the 200-cell cap, and any priority trimming reported by the core. Do not imply that trimmed candidates were checked.

An expanded cell shows its single inventory item or cross-lens pair, the plan citation or exclusion reason, or the gap and proposed fix. Pending and skipped reviews remain distinguishable from closures. `Probably covered` remains an open gap. The TUI does not close a cell based on prose or change an accepted ruling.

Show both auditors, their fresh-context status, sampled closures, evidence, challenged closures, and proposed missing cases in expandable audit details. Display the judge's accepted ruling and resulting cell dispositions. No cell disappears when an audit challenges it; preserve the original cell and its history.

The coverage ledger report retains every cell and its evidence or outstanding work. Residual risks requiring human acceptance appear in a separate labeled list. Provide the core-owned explicit acceptance action for an eligible risk, identifying the exact risk and scope. Opening the report, dismissing a notice, or accepting one risk cannot accept other risks. Show acceptance only after authoritative acknowledgement.

Keep the stop rule visible: every cell closed or explicitly accepted, and an audit finding no false closure. Surface material missing cases and unresolved audit findings. State that this is a bounded stop rule, not proof of completeness. Display the core's evaluation rather than inferring success from the grid's filled appearance.

#### Inline survey

Preview either the new question or the room's most recent eligible parallel-answer set selected by the core. The `SURVEY` banner introduces the run. Question-first answers remain stable parallel cards; the existing-answer form starts at claim extraction without presenting those answers as newly generated work.

The claim view shows up to 25 neutral, deduplicated atomic claims with stable IDs. During blind voting, show per-member progress and accepted AGREE / DISAGREE / UNSURE votes with one-line reasons and confidence from 1 to 5 in human detail views. The visibility label states that members receive claims without whole answers, authorship, or vote counts.

The tally step identifies the core's selected set of up to five most evenly split claims. Re-vote details show anonymous reasons from both sides, each member's accepted second vote, and the evidence that would change its mind. The TUI does not choose the split claims or send its human-visible tallies to members.

Render the final report as a five-band map:

| Band | Presentation |
|---|---|
| Consensus | Claim and authoritative agreement pattern, including 6–0 or 5–1 at six members |
| Contested | Split, best reason from each side, who switched, and the question that would settle the claim |
| Lone insights | Claims raised by one member and endorsed by the rest |
| Lone dissents | The dissenting position and its reason |
| Rejected outliers | Claims raised by one or two and rejected by the rest, labeled as likely errors rather than proven falsehoods |

Show labeled AGREE, DISAGREE, and UNSURE bars with counts and an expandable first-vote/re-vote comparison. Preserve absent votes and skipped members separately from UNSURE. Origins, counts, and switcher identities shown to the human remain outside member payloads. Band classification, thresholds for other roster sizes, and switch detection come from the core.

Keep every band labeled when empty. Each claim remains traceable by ID across extraction, voting, re-voting, and report views. Survey produces no winner, participant ranking, trophy, or win/loss change.

#### Inline probe

Preview the observation, ruled-out possibilities, and available inspections. The `PROBE` banner introduces the run. Show the core's random explainer/checker split to the human. With an odd addressed count, name the last member who sits out in the transcript and show the actual participating count and authoritative preflight.

During the first step, place explanations and checks in separate labeled groups, each with stable actor cards. Explanations show the proposed explanation and key assumption. Checks show exactly what to measure, outcomes to record, and effort. The visibility label states that the first step is blind and checkers do not see explanations.

During prediction locking, show each explainer's progress and accepted prediction for every check, including explicit cannot-predict entries. Explain that explainers receive all checks but no other explanations or predictions. Keep uncommitted predictions provisional.

Render the final report as a runnable decision tree drawn with box characters. Each branch names a check, the observation to record, the result condition, and the explanations that remain or the next check. Wrap branch labels within the transcript width and preserve the branch structure. Every tree includes a visible `Unexpected result` branch.

Provide an expandable prediction matrix aligned by explanation and check, with key assumptions, cannot-predict entries, and skipped work preserved. Keep dropped non-distinguishing checks inspectable with their reasons. Show explanations the available checks cannot separate explicitly.

The ordering, branch conditions, and remaining-explanation sets come from the accepted core report. The TUI does not declare an explanation true or execute a check. The user performs the cheap, non-destructive inspections described by the report.

#### ASCII round banners

Banners render contest boundaries derived from authoritative state transitions and pick requests. They are not additional events. Reconstruct the same boundaries from durable history without duplicating banners during reconciliation.

Use `OPENING` for round 0, `ROUND n` for configured rounds before the last, `FINAL ROUND` for the last configured round, and `VERDICT` for final judging. A one-round debate uses `FINAL ROUND` for its only configured round. Place `VERDICT` on entering `JUDGING`; the verdict reveal begins when the accepted verdict arrives.

Additional boundaries are:

| Contest | Banner |
|---|---|
| Tournament start | `TOURNAMENT` |
| Tournament round with more than two survivors | `ROUND r · N REMAIN` |
| Tournament round with two survivors | `FINAL` |
| Tournament champion committed | `CHAMPION` |
| Grill start | `GRILL` |
| Grill round | `GRILL ROUND r` |
| Grill completed with its final plan | `FINAL PLAN` |
| Debate awaiting pick | `CHOOSE YOUR DEBATE BROS` |
| Grill awaiting pick | `CHOOSE WHO GETS GRILLED` |
| Census start | `CENSUS` |
| Crosscheck start | `CROSSCHECK` |
| Survey start | `SURVEY` |
| Probe start | `PROBE` |

Use the in-process banner renderer and embedded default and compact fonts owned by `squad-core/banner/`. Fonts and their licences are vendored there, not in `squad-tui`. Never shell out to `toilet` or `figlet`; `setup.sh` installs neither.

Consume `SQUAD_BANNERS` through the Foundations `.env` contract:

| Value | Rendering |
|---|---|
| `on` | Default. Use the default font if it fits the available transcript width, then the compact font, then a one-line divider |
| `compact` | Start with the compact font; use a one-line divider if it does not fit |
| `off` | Omit FIGlet banners; retain ordinary round, verdict, tournament, grill, champion, final-plan, and pick labels |

With `SQUAD_BANNERS=off`, also retain the ordinary census, crosscheck, survey, and probe labels and all step dividers.

The one-line fallback uses the boundary text, for example `──── ROUND 3 ────`. Centre banners within the available transcript width and use the theme accent. Under `NO_COLOR`, render them plainly. Include their measured heights in the virtualised transcript's row cache and remeasure on width changes.

Banner animation belongs to TUI effects and motion. Banners settle inline into their permanent static form. Banner text, fonts, and width fallbacks remain correct with effects off. Versus graphics also settle inline without introducing a pinned transcript region.

#### Verdicts, scoreboard, and daily spend

Show the actual judge model and substitution warning before judging. Preserve the exact warning:

`Judge model changed to {judge_model_slug} because @{nick} is debating.`

The core detects conflict using resolved concrete models, including members under other nicks. Standalone judging uses the core's corresponding conflict-resolution contract. Tournaments remain blind and never swap.

Render accepted debate verdicts using the schema owned by Contests. Show the winner, criterion scores and scale, authoritative weighted totals, reasoning, and `What to steal from the loser`. A winner remains required when totals are equal.

Also show the authoritative `roundTally` for both debaters and `tallyOverrideReason` when non-null. `roundTally` is computed by the core, not produced by the model. When the winner is not the round-tally leader, the accepted verdict must contain the judge's override reason. Render that reason without deriving a substitute explanation. Keep unscored rounds identifiable alongside the available per-round scores.

Use an aligned score table where it fits and stacked criterion blocks otherwise. Malformed or exhausted judge output shows explicit failure and permitted recovery actions. Never fabricate missing fields.

Apply authoritative scoreboard snapshots and updates. Repeated verdict events must not increment a local score counter. The confirmed `/clear` reset clears the displayed record and invalidated local state. Restore tournament trophies from the authoritative room state. Clearing room history must not make already billed daily spending disappear.

Always show today's actual spend, a bar, and the configured daily max, for example `$3.42 ▰▰▰▱▱▱▱▱▱▱ $20.00`. The filled fraction reflects the authoritative spend-to-max ratio. Keep the bar in the participants footer or, when narrow, the status line. Show pending reservations as a lighter segment and an explicit pending amount; replace each reservation with its reported actual cost when the authoritative accounting update arrives. Retain textual distinctions under `NO_COLOR`.

Today means the local calendar day. Daily totals cover Squad's own calls across all rooms on the machine. Consume Foundations' daily-spend state and the Protocol section's `spend_daily` accounting contract. The TUI does not query OpenRouter key-wide usage, sum local transcript costs, or write accounting rows.

Reported spend includes billed retries, failed or canceled calls, and judge work. Accounting is committed by the router with the call-completion event. Repeated events do not add charges again. Judge usage remains separate from room-member `fable`.

If a response has no reported cost, show `cost unknown` for that call and prefix the spend display with `~`. Preserve the authoritative unknown-call count; `squad doctor` reports that count. Missing usage or cost is unavailable, never zero. Reservations remain estimates and do not imply a guaranteed provider invoice cap.

At 75% and 90%, change the bar colour through the daily-spend heat treatment owned by TUI effects and motion. Keep spend and max readable without colour.

At 100%, show `DAILY_MAX_REACHED` and the shared message, formatted with the effective max:

`Daily max of $20.00 reached. Raise SQUAD_DAILY_MAX_USD or wait until tomorrow.`

New generation requests are refused until the next local day or until the effective max is raised. In-flight calls finish; do not present the limit as a global cancellation. A contest preflight that exceeds the remaining daily amount shows the same error before starting. Retain completed content, the draft, and readable history.

#### Commands, export, debug, and distribution

Use the complete command set, optional targets, flags, and validation owned by Foundations and Command grammar and parser. `/help` presents that authoritative syntax with terminal shortcuts. `/who` presents the authoritative roster and room state.

`/clear` applies the confirmed persistent room reset. It is not a local display-clear action. Its effects wipe is colour-only before the authoritative reset cuts the cleared content.

The TUI export action requests a destination path, obtains Markdown through `RoomHandle::export(scope, id) -> String`, writes that returned text to the destination, and reports success only after the write succeeds. Markdown generation lives in `squad-core/export/`; offline CLI export uses the core's `export_offline(data_dir, room, scope, id)`. Preserve available messages, attribution, contest rounds, and accepted rulings while excluding credentials and reasoning text.

Markdown export includes available round scores, their reasons, unscored markers, authoritative tallies, and any accepted tally override reason. Render each contest boundary banner as a fenced code block using the core's embedded default font, independent of terminal width or `SQUAD_BANNERS`.

Support the shared room and debate scopes plus `/export tournament [id]` and `/export grill [id]`. Tournament export retains answers, accepted rankings, critiques, eliminations, and the champion. Grill export retains plan versions and the complete question-and-answer appendix with authors and plan changes.

Support the shared census, crosscheck, survey, and probe export scopes, including `/export census`. Present the parser-owned optional targeting syntax in help and actions. Census export retains pass and plan references, ledgers, provenance, covered citations, estimate ranges and caveats, and the discovery curve. Crosscheck export retains every cell, evidence, exclusions, gaps and fixes, audit findings, rulings, and explicit risk acceptances. Survey export retains claims, votes, reasons, confidence, re-votes, switches, five bands, and settling questions. Probe export retains the role split, explanations, checks, predictions, decision tree, inseparable explanations, and unexpected-result branch. Obtain all Markdown from the core and preserve stopped, failed, and skipped-work status.

F12 opens a TUI debug pane using the shared diagnostic contract. Preserve body redaction unless explicitly enabled through Foundations configuration.

`$review`, `$reveal`, `$vote`, and `$summarize` remain unavailable and belong on the later roadmap.

Startup, noninteractive behavior, one-shot output, exit codes, `./setup.sh`, and release packaging remain owned by Foundations. Integrate `crates/squad-tui` into the single binary without introducing another executable. Repository public (D61); release publication requires the owner's M7 decision.

#### Color and terminal compatibility

TUI effects and motion owns the palette, including truecolor member hues and the gold chrome accent. Within that palette, the 16-colour tier maps author labels as follows: `claude` yellow, `fable` magenta, `openai` green, `gemini` cyan, `grok` blue, and `muse` bright magenta. Assign other configured members stable palette colors through that palette's slot rules. `me` and all message body text use the default foreground; gold is for chrome only.

The Foundations `NO_COLOR` environment variable and a dumb terminal disable color and styling. Retain textual status, focus, selection, and action labels.

Automatic lite effects over SSH or without synchronized output apply only when `SQUAD_FX` is unset. An explicit configured level follows Foundations and TUI effects and motion. The roster-resolve effect starts from the resolved state at first draw; it does not require a new protocol event. `SQUAD_FX_DEBUG=1` shows the effects-grade tag under the Foundations configuration contract.

On resize, recompute layout from the latest dimensions, invalidate width-dependent measurements, and reflow from original text. Preserve the composer cursor's grapheme offset and transcript anchor. Do not shrink text or discard content to fit.

Verify terminal behavior directly on the release platforms, including supported and unsupported keyboard enhancement, synchronized output, mode 2027, OSC 52, tmux, and SSH.

#### Keybindings

| Key | Action |
|---|---|
| Enter | Accept active completion; otherwise submit valid input when input has focus |
| Ctrl+J, Alt+Enter | Insert an explicit newline |
| Shift+Enter | Insert a newline when the terminal distinguishes it |
| Left, Right | Move the input cursor by grapheme |
| Up, Down | Move by visual input row; navigate completion when open |
| Home, End | Move to start or end of the visual input row |
| Alt+Up, Alt+Down | Navigate accepted input history and restore the saved draft |
| Page Up, Page Down | Page transcript by its height minus two rows |
| Ctrl+Home, Ctrl+End | Move to oldest loaded transcript position or live tail |
| Tab | Open completion or accept its highlighted item |
| Ctrl+R | Reinsert previous accepted recipients at the caret |
| Escape | Close the topmost transient element first; otherwise request global stop, requiring a second Escape within 2 s during a contest |
| F6 | Cycle focus through input, participants, and transcript |
| F2 | Expand or collapse focused inline contest |
| Ctrl+F | Open transcript search |
| F3, Shift+F3 | Move to next or previous search match |
| Up, Down with transcript focus | Navigate messages |
| Space with transcript focus | Toggle focused-message selection |
| Enter with participant focus | Insert the participant mention |
| Enter with transcript focus | Open labeled message actions |
| Enter with action focus | Activate the labeled action |
| F12 | Toggle the debug pane |
| Ctrl+C | Shut down through the shared lifecycle and exit |
| Ctrl+D | Exit through the shared lifecycle when the draft is empty |
| q outside input | Shut down through the shared lifecycle and exit |
| Mouse wheel | Scroll transcript or participants under the pointer |

The labeled action menu provides keyboard access to quote removal, copying, regeneration, debate solution selection, grill plan selection, judging, follow-up, scorecard expansion or collapse, tournament ranking details, grill plan versions and appendix, and preflight confirmation or cancellation.

Extend labeled actions to census pass and gap details, the member-by-gap table, crosscheck cell and audit details, explicit risk acceptance, survey band and vote details, probe branches and prediction matrices, and each new report's expansion and export. Actions that prepare quoted material preserve the draft and require normal submission.

#### Verification requirements

Use Ratatui `TestBackend` with `insta` snapshots for deterministic layout and state presentation. Cover both specified viewports, eight-row input, long nicks, 12 members, no-color output, startup failures, preflight, expanded debates, verdicts, and undersized terminals.

Test grapheme editing and wrapping with CJK, emoji, combining sequences, tabs, explicit newlines, and CRLF. Verify source-to-cell mappings, cursor preservation, selections, search offsets, and anchors after resize.

Test the Markdown pipeline with partial fences, paragraph-to-table transitions, attempt replacement, completion, unknown languages, and hostile terminal escapes. Instrument cache behavior so token arrival does not reparse finalized blocks or draw offscreen rows.

Snapshot `OPENING`, `ROUND n`, `FINAL ROUND`, and `VERDICT` banners at available widths of 120, 80, and 40 columns, including `NO_COLOR`. Exercise both embedded fonts, one-line fallbacks, `SQUAD_BANNERS` modes, history replay, height-cache invalidation, and anchored reflow. The 40-column cases test the banner widget independently of the minimum application viewport.

Verify pending, late, repeated, equal, failed, and canceled round-score presentation. Check scorecard expansion, authoritative tally display, unchanged participant win/loss records, final round tallies and override reasons, and the absence of scoring for round 0 and unjudged debates. Integration checks must preserve M5's rule that final judging waits for round-scoring completion or failure and that judging a stopped debate does not score rounds retroactively. Context isolation, orchestrator scoring behavior, and tally arithmetic remain owned by Contests and Hardening.

Verify complete static rendering with effects off and the post-render effects manager hook. Effects behavior and its verification remain owned by TUI effects and motion and Hardening. The initial 12-stream spike uses a throwaway renderer with effects off. After M4.P5, repeat the gate with the real transcript widget and `SQUAD_FX=full` as an M4 exit criterion.

Verify the single event loop prioritizes input, drains at most 256 feed items per iteration, draws during sustained feed bursts, respects pending token deadlines, and disarms its frame timer when idle. Verify transient-first Escape, the 2 s contest-stop confirmation window, animation input preservation, and `q` outside input.

Verify daily-spend presentation for actual and reserved cost, reservation replacement, billed failures and cancellations, unknown cost and the `~` prefix, repeated events, local-day rollover, a raised effective max, 75% and 90% thresholds, and refusal at 100% while in-flight calls finish. Verify room clearing does not erase today's billed spend.

Snapshot tournament answers, rankings, eliminated rows, trophies, champion, replayed rounds, and failure states. Snapshot grill turn headings, questions, answers, passes, counters, skipped turns, plan versions, final appendix, survived completion, and leader failure. Include every new banner at 120, 80, and 40 available columns, effects off, and `NO_COLOR`. Verify authoritative preflight call counts and estimates for both modes.

Verify question-first debate and grill collection, candidate lists, exact pick hints, changed input placeholders, `pick pending`, eligible-first completion, valid and invalid picks, ordinary chat during a pending pick, insufficient answers, cancellation, restart restoration without expiry, and preflight after picking. Initial answers remain ordinary replies after cancellation and are excluded from the later estimate. Tournament must never enter a pick view.

Snapshot census ledgers, singletons, covered citations, member-by-gap details, estimate ranges and caveats, discovery curves, patched-plan references, and quiet versus pass-limit completion. Snapshot crosscheck grid summaries, every cell disposition, audit samples and challenges, retained cells, proposed fixes, explicit risk acceptances, and bounded stop guidance. Snapshot survey's five bands, empty bands, confidence, UNSURE and missing votes, first-vote/re-vote comparison, switchers, and settling questions. Snapshot probe roles, odd-member sit-out, explanations, checks, prediction matrix, cannot-predict entries, box-character decision tree, inseparable explanations, and unexpected-result branch.

Verify all four mode banners and step dividers at 120, 80, and 40 available columns, all banner modes, effects off, and `NO_COLOR`. Verify collapsed and expanded report layouts at both application viewports, keyboard actions, search reveal, copy, anchored reflow, and large-report virtualization. Repeated events and replay must not duplicate artifacts, banners, tallies, role assignments, or acceptance records.

Verify new-mode preflight counts and estimates from M5 fixtures, including crosscheck's extra extraction call, survey's existing-answer path, census pass count, and probe's actual participating count. Verify rejection preserves the draft and quotes, stopped and failed reports remain incomplete, skipped members are named, daily-max refusal leaves completed content readable, and export success follows the destination write.

Verify inspection and expansion send no member requests or payloads. Context isolation, blind-step enforcement, estimate arithmetic, survey band formulas, random selection, and script failure rules remain owned by Contests and Hardening; TUI checks consume their accepted fixtures.

Run one TUI PTY-driven end-to-end test against the M2 fake provider and temporary SQLite data. Exercise real terminal input, paste, streaming, resize, search, copy initiation, inline debate confirmation, and clean terminal restoration. Room API and Hardening own their separately scheduled PTY checks.

Use the workspace checks defined by Foundations: `cargo nextest`, `insta`, `clippy -D warnings`, and `rustfmt`. M4 exits after M5 integration checks pass.

### Part 2: Milestones


Census continuation uses Parser’s `/continue ["<plan>"]`, preserving a single quoted-plan attachment when supplied and mapping to `Request::CensusContinue`. Crosscheck residual-risk acceptance uses `/accept <cellId…> -- <reason>` on a COMPLETE crosscheck and maps to `Request::CrosscheckAccept` with the displayed plan, grid and state version. These controls retain Parser’s exact errors and become executable with their modes in M5.

## App commands (M10)

The composer accepts four app commands, typed with a leading dot. They are handled by `App::submit` before validation: they never reach the shared `/` parser, never build a `Request`, never call a model, and never write outside the current directory.

| Command | Effect |
|---|---|
| `.quit`, `.exit` | Leave the room, same path as Ctrl+C. |
| `.dump` | Write every loaded transcript message (author, status, source, widget rows) in order to `output-<YYYYMMDD-HHMMSS>.txt`. |
| `.save` | Write the newest member reply to `saved-<YYYYMMDD-HHMMSS>.md`. Reports a notice and writes nothing when there is no agent output. |

Matching is exact after trimming, so `.quit now`, `/quit` and `.5 is a number` stay ordinary input. Files are created with `create_new`; a name collision in the same second gets a `-1`, `-2` suffix and nothing is overwritten. A leading `.` in the composer opens the completion list with these four commands; they are offered only at the start of the draft. Tests: `crates/squad-tui/tests/system_commands.rs`. Plan: `docs/prd/milestones/M10-app-commands.md`.
