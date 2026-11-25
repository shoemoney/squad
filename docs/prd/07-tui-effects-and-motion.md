# TUI effects and motion

**Integration rulings.** These override the design text below wherever they differ.

1. **Banners are inline and permanent.** A round banner is a block in the transcript (owned by the TUI section) and stays in scrollback as static coloured FIGlet art. The effects layer animates its entrance in place (dim band, horizon line, unfold, glint, filter drift during the hold) and then lets it settle. There is no overlay over other content and no collapse on exit. The one-line rule is only the narrow-terminal fallback. If tokens for the round arrive early the banner settles immediately.
2. **No ties in a final verdict.** The judge always picks a winner, so there is no tie reveal and the record stays W-L. Equal scores can occur within a single round and get no lead-change effect.
3. **Verdict card criteria** are the four canonical rubric criteria owned by the Contests section (`fit`, `feasibility`, `originality`, `survivedAttacks`) with router-computed weighted totals and the round tally. The criteria names in storyboard (b) are illustrative.
4. **Configuration** is `SQUAD_FX` (`full`, `lite`, `off`) plus `NO_COLOR` and `SQUAD_BANNERS`. There is no `SQUAD_MOTION`. `SQUAD_FX_DEBUG=1` is a developer aid, listed in the Foundations variable table.
5. **Launch splash** runs at most 1.2 s, is skipped by any key, and never runs for one-shot commands. Skipping an animation never consumes the key; normal input handling still runs.
6. **Paths** below are relative to `crates/`.
7. **Ownership.** Effects owns the palette and animation. TUI owns the static widgets, banner placement, input handling and the one event loop in `crates/squad-tui/src/event_loop.rs`. `squad-core` owns banner text rendering, embedded fonts and Markdown export. Effects consumes their output and never owns contest state or generation admission.
8. **Daily spend.** The daily max replaces the room budget. Effects reads the authoritative daily-spend state defined by Protocol and Foundations; it never estimates actual billed cost or stops in-flight work.
9. **Shared engine.** All seven modes are scripts over the same engine's `collect`, `pick`, `fanout`, `turns`, `judge`, `tally`, `barrier` and `report` moves. Effects consumes committed script state and artifacts through the TUI reducer. It does not execute moves, release barriers, calculate results or deliver payloads to members. The Contests section owns the scripts and D71 build order: debate, grill, census, crosscheck, tournament, survey, probe. Animation never delays the next move.

Verified on docs.rs, 2026-11-24:

| Crate | Version | Decision |
|---|---|---|
| `ratatui` | 0.30.2 | Base. |
| `tachyonfx` | 0.25.2 (released 2026-09-06) | **Use** for buffer post-processing. It builds on `ratatui-core ^0.1.2`, lives in the ratatui org, and ships `EffectManager` (effects replaceable by id), `Interpolation`, `CellFilter`, `pattern` and `SimpleRng`. |
| `figlet-rs` | 1.0.0 | **Use** for banners in `squad-core`. Built-in `FIGlet::standard()`, `FIGlet::slant()`, `Toilet::smblock()`, and it loads `.tlf` fonts. |
| `tui-big-text` | 0.8.10 | **Reject.** It renders `font8x8` glyphs, not FIGlet fonts. |
| `throbber-widgets-tui` | 0.11.1 | **Reject.** A spinner is ten lines, and we need it on our own clock. |

Unverified: the exact `tachyonfx::fx` constructor names and signatures, whether `figlet-rs` can load a `.tlf` from embedded bytes, and tmux's synchronized-output behaviour. No maintained ratatui gradient crate turned up (`tui-gradient` is a 404 on docs.rs), so gradients are in-house.

### 1. Design language

**Identity: "broadcast night".** Squad looks like a fight-night broadcast graphics package running in a terminal: a dark stage, one saturated colour per contender, chrome that lights up when something happens and goes quiet when nothing does.

**Palette.**
- Each model owns one signature hue: claude `#E0825A`, fable `#A78BFA`, openai `#19C39A`, gemini `#5B9BFF`, grok `#E6E9EF`, muse `#FF4FA3`. Slots 7 to 12 are picked from an OKLCH wheel at fixed lightness 0.74 and chroma 0.15, choosing the hue furthest from those already taken.
- The human's chrome and all neutral chrome use gold `#FFD23F`. Human message body text uses the terminal's default foreground.
- A model's hue appears in exactly four places: its nick, its bubble's left rail, its participants row, and its portion of any contest graphic. Message body text is always the terminal's default foreground.
- A gradient only ever interpolates between two owned hues, or one hue and its darkened self, in OKLCH. Rainbow is reserved for the launch logo and contest opening or final celebrations.
- Colour tiers are chosen once at startup: truecolor, 256 (nearest cube entry), 16 (each model mapped to a distinct ANSI colour, gradients become two-step bold/normal), and `NO_COLOR` (bold, dim, reverse and underline only).

The 16-colour tier uses this assignment; the TUI section references the same mapping. Other configured members receive stable distinct remaining ANSI colours.

| Member | 16-colour tier |
|---|---|
| `claude` | Yellow |
| `fable` | Magenta |
| `openai` | Green |
| `gemini` | Cyan |
| `grok` | Blue |
| `muse` | Bright magenta |
| `me` body | Default foreground |
| Human and neutral chrome | Yellow |

**Motion.**
- Three durations: 120 ms (acknowledge), 280 ms (arrive), 900 ms and up (hero moments).
- Entrances ease out (`CubicOut`, `BackOut` for banners), exits ease in (`QuadIn`). Nothing loops except status glyphs, the stream cursor, the daily-spend warning and pending-pick highlights.
- Things that never move: message body text once laid out, the input line and caret, scroll position, panel geometry. Effects change colour and decorate edges; they never shift text you are reading.
- No strobing: no cell changes luminance by more than 40% more than three times a second.

**Levels (`SQUAD_FX=full|lite|off`, default `full`).**

| Level | Keeps |
|---|---|
| `full` | Everything: particles, full-buffer passes, 60 fps during hero moments. |
| `lite` | Colour identity, static gradients, spinners, banners with a 150 ms fade, count-ups. No particles, no full-buffer passes, no ambience, 30 fps cap. Auto-selected over SSH (`SSH_TTY` set) and when synchronized output is absent, only when `SQUAD_FX` is unset. |
| `off` | No animation. Every effect renders its final frame immediately. Colour stays. Status glyphs are static letters. Selected by `SQUAD_FX=off`. |

`NO_COLOR` is orthogonal to the level and removes colour at any level.

### 2. Effects catalogue

Abbreviations: F = full, L = lite. `off` always shows the end state. "16/NC" is the 16-colour and `NO_COLOR` fallback. "Narrow" means under 100 columns or under 30 rows.

| Moment | Effect | Trigger | Looks like | Lvl | Fallback |
|---|---|---|---|---|---|
| Launch | Logo reveal | Process start | `SQUAD` in `slant`, centred. Cells resolve from random glyphs (`░▒▓#/\`) left to right over 700 ms `CubicOut`, a rainbow OKLCH sweep passes once (400 ms) and settles to gold. Any key skips it. | F; L fades 150 ms | 16/NC: bold, no sweep. Narrow: one-line `SQUAD`. |
| Launch | Roster resolve | First draw of the resolved roster | Participants rows start as dim `·······`; the resolved nick types in at 20 ms per char in its hue with a 120 ms reverse-video flash. A failed row turns red with `×`. This uses the state returned by open, with no provider-handshake event. | F, L | NC: bold type-in. |
| Idle | Border breath | No activity | A dim gold highlight three cells long travels the transcript border, one lap per 8 s, at 4 fps. Stops after 20 s and redraw halts. | F | Absent elsewhere. |
| Send | Launch streak | Enter | Input border flashes gold to dim over 120 ms; a `▸` streak runs up the left gutter to the new human bubble in 180 ms. | F; L flash only | NC: none. |
| Fan-out | Spoke burst | `@all` or multi-mention dispatched | One `━` line per target runs from the human bubble's rail across the panel divider to that target's row, staggered 40 ms, each recoloured from gold to the target hue as it travels, 280 ms each. Each row pulses on hit. | F; L pulses rows only | 16: gold only. Narrow: row pulses only. |
| Thinking | Orbit glyph | Status = thinking | Braille spinner `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏` at 10 fps in the model hue at 60% lightness. | F, L | NC: `...`; off: `T`. |
| Streaming | Throughput bar | Status = streaming | Row glyph becomes `▁▂▃▅▇`, height mapped to that model's real tokens per second over the last 500 ms. Hue at full lightness. | F, L | 16: `>>` bold. |
| Arrival | Rail drop | First token of a bubble | The left rail `▌` draws top-down in the hue over 280 ms; the header nick sweeps from white to the hue. Body text is never animated. | F; L instant rail | NC: `|` rail. |
| Streaming | Comet cursor | While tokens arrive | A `█` in the hue at the insertion point. The last four cells behind it carry a foreground fade from hue to default (per frame, colour only, glyphs unchanged). | F; L block only | NC: `_`. |
| Complete | Seal | Stream end | Cursor collapses `█▓▒░` over 120 ms; the rail dims to 50%; the footer (`412 tok · $0.0031 · 3.2s`) fades in over 150 ms. | F, L | NC: footer appears. |
| Error | Rail fracture | Stream error | Rail turns red and alternates `▌`/`╎` twice (240 ms total, one cell wide, no row shake). Row shows `×`. | F; L static red | NC: `[ERR]` reverse. |
| Retry | Backoff ring | Retry scheduled | `◔◑◕●` fills over the real backoff duration next to the row. | F, L | NC: `retry 3s`. |
| Queue | Pip stack | Queue depth changes | Depth shown as `▪▪▪` (max 5, then `▪▪▪▪▪+3`). A new pip slides in from the right over 120 ms; a consumed pip dissolves. | F; L instant | NC: `q3`. |
| Cost | Odometer | Daily spend changes | Digits roll (each changed digit cycles through three intermediate digits in 150 ms). The daily spend bar `▰▰▰▱▱` fills in gold; reserved estimates form a lighter pending segment, replaced by actual cost when committed. The `~` prefix remains visible while cost is unknown. | F; L instant digits | NC: text with explicit pending and unknown labels. |
| Daily max | Heat warning | 75%, 90%, 100% of daily max | Bar gradient shifts gold to orange to red. At 90% the bar pulses once per 2 s (two luminance steps). At 100% one red sweep crosses the participants panel, then a static reverse `DAILY MAX`. | F; L static colours | 16: yellow/red. NC: `!`, `!!`, `DAILY MAX`. |
| Quote | Lasso | Quote selection moves | Selected bubble's rail becomes `┃` in gold with a 120 ms sweep down; other bubbles dim to 70%. | F; L no dim | NC: reverse rail. |
| Debate | Versus card | `$debate` accepted | Full-width card: left half in A's hue slides in from the left, right half in B's from the right, 280 ms `BackOut`. They meet at a centre `VS` in `slant` that flashes white for 120 ms and throws 24 spark particles (`*·+`) along the seam. Shows records (`W4 L2`) and topic. Holds 1,200 ms, then settles inline as a static card in the transcript. | F; L fade, no sparks | Narrow: three-line text card. NC: box, bold names. |
| Rounds | Round banner | Round boundary | Showpiece (a). | F; L 150 ms fade | See storyboard. |
| Score | Score slam | Judge scores a round | Two score chips drop one row into place (120 ms), digits count from 0 to value over 400 ms `QuadOut`. The higher chip gets a one-time white flash. | F; L count-up only | NC: bold higher score. |
| Tally | Tug bar | Tally updates | A 30-cell bar split by cumulative score share, A's hue on the left, B's on the right. The split point animates over 400 ms. | F, L | 16: two ANSI colours. NC: `####|...`. |
| Lead | Lead flip | Leader changes | The tug bar's leading half sweeps a white highlight across; `LEAD CHANGE` in `smblock` shows for 800 ms above the bar in the new leader's hue; that participants row pulses. | F; L text line | Narrow: single-line text. |
| Final | Final round | Last round begins | Banner uses the metal filter with a red-gold duotone, the transcript border turns red for the round, hold is 300 ms longer. | F; L colour only | 16: red bold. |
| Verdict | Verdict banner | Debate enters `JUDGING` | The TUI places the inline `VERDICT` banner; Effects animates its entrance as in (a). | F; L 150 ms fade | See storyboard. |
| Verdict | Verdict reveal | Judge verdict arrives | Showpiece (b), using the `VERDICT` banner already placed on entering `JUDGING`. | F; L card fades in with count-ups | See storyboard. |
| Scoreboard | Record tick | Debate ends | Winner's `W` digit rolls up with a gold flash; loser's `L` digit rolls with no flash. | F, L | NC: instant. |
| `/stop` | Brake | `/stop` | Every active stream cursor turns to `■` at once; rails dim; a `━━ stopped ━━` line draws centre-out over 180 ms. | F; L instant | NC: text line. |
| `/clear` | Wipe | `/clear` | Transcript colours fade top to bottom in 280 ms along a diagonal front, then the cleared transcript is shown with a hard cut. The pass never changes message-body glyphs. | F; L instant | Instant. |
| Resize | Hard cut | Terminal resize | All running effects snap to their end state, layout recomputes, and the border gets one 120 ms flash. No effect survives a resize. | F, L | Same, no flash. |
| Quit | Power down | Ctrl+C, Ctrl+D, or `q` outside the input | Screen collapses vertically to the centre row in 180 ms, then that row to a point (CRT off). Hard cap 250 ms; a second Ctrl-C skips it. | F | L, 16, NC: instant. |
| Tournament | Ranking reveal | Accepted `tournament.ranking` | Ranking rows reveal from last place upward, with score count-ups in each entrant's hue. Rows keep their final positions throughout. | F; L fade and count-up | 16: mapped member colours. NC: ranks and scores as text. |
| Tournament | Eliminated stamp | Committed elimination | `ELIMINATED` appears once beside the lowest-ranked entrant; that participants row retains a static eliminated marker for the rest of the tournament. | F; L 150 ms fade | NC/off: static `ELIMINATED`. |
| Tournament | Champion | `tournament.champion` | Inline `CHAMPION` banner and champion name reuse the verdict celebration and confetti; the trophy count ticks once. | F; L fade and count-up, no confetti | Narrow: one-line champion label. NC: bold reverse champion. |
| Grill | Turn handoff | Committed turn change | A 280 ms colour sweep moves between the active participants' row accents; the static turn heading remains readable throughout. | F; L row pulse | NC/off: static active-turn label. |
| Grill | Plan version bump | `grill.plan` | The new version badge gets a 120 ms gold flash; the new plan block settles inline with its body text unchanged. | F; L fade | NC/off: static version badge. |
| Grill | Survived | `grill.complete` with reason `survived` | `SURVIVED` settles beside the inline `FINAL PLAN` banner with a single gold sweep. No winner, score or record tick. | F; L 150 ms fade | NC/off: static `SURVIVED`. |
| Pick | Pick banner | Live `contest.pick_requested` | Inline `CHOOSE YOUR DEBATE BROS` or `CHOOSE WHO GETS GRILLED` uses the round-banner entrance and settles permanently above the pick hint. | F; L 150 ms fade | Narrow: wrapped plain banner text. NC: bold; off: static. |
| Pick | Candidate highlight | Pick remains pending | Each eligible candidate's participants row gets a slow colour-only highlight, one cycle per 2 s, until the pick is accepted or the contest ends. | F; L static highlight | NC/off: static eligible marker. |
| Census | Census banner | TUI places `CENSUS` from accepted live script state | Inline gold banner uses the horizon unfold and glint from (a), then settles permanently. Step dividers get a single 120 ms gold accent. | F; L 150 ms fade | 16: yellow bold. NC: bold. Narrow: one-line banner. Off: static. |
| Census | Estimate count-down | Committed pass estimate | A separate estimate graphic tweens each endpoint from the previous pass's value to the new value over 400 ms `QuadOut`; falling values count down. The exact current headline and caveat remain static and visible. | F; L 150 ms tween | NC: same labelled endpoints without colour. Narrow/off: static range and headline. |
| Crosscheck | Crosscheck banner | TUI places `CROSSCHECK` from accepted live script state | Inline gold banner uses the horizon unfold and glint from (a), then settles permanently. Step dividers get a single 120 ms gold accent. | F; L 150 ms fade | 16: yellow bold. NC: bold. Narrow: one-line banner. Off: static. |
| Crosscheck | Grid fill | Committed cell-result batch or audit ruling | Cells receive a 120 ms gold accent one by one in the TUI's fixed grid order, staggered across a batch lasting at most 900 ms. Each cell's committed status and evidence remain readable throughout. | F; L one 150 ms batch fade | 16/NC: status labels remain explicit. Narrow: pulse the coverage summary once. Off: static grid. |
| Survey | Survey banner | TUI places `SURVEY` from accepted live script state | Inline gold banner uses the horizon unfold and glint from (a), then settles permanently. Step dividers get a single 120 ms gold accent. | F; L 150 ms fade | 16: yellow bold. NC: bold. Narrow: one-line banner. Off: static. |
| Survey | Bars splitting | Committed initial or revised vote tally | Each claim's labelled AGREE / DISAGREE / UNSURE bar splits to the authoritative proportions over 400 ms `QuadOut`; a re-vote moves boundaries from the preceding tally. Claim rows stay fixed. | F; L 150 ms tween | 16/NC: labelled segments with distinct fill patterns. Narrow/off: static labelled counts. |
| Probe | Probe banner | TUI places `PROBE` from accepted live script state | Inline gold banner uses the horizon unfold and glint from (a), then settles permanently. Step dividers get a single 120 ms gold accent. | F; L 150 ms fade | 16: yellow bold. NC: bold. Narrow: one-line banner. Off: static. |
| Probe | Branch drawing | Accepted `probe.report` | A gold trace follows the decision tree's box-character connectors branch by branch, root first, over at most 900 ms. Node labels, outcomes and the prediction matrix are present from the first frame. | F; L one 150 ms connector fade | 16: yellow connectors. NC: bold connector trace. Narrow: trace only visible wrapped branches. Off: static tree. |

**Daily-spend presentation.** The always-visible bar is the TUI's `$3.42 ▰▰▰▱▱▱▱▱▱▱ $20.00`, in the participants panel footer or the status line when narrow. Actual spend, pending reservations and unknown-cost indicators come from the authoritative state; animated digits never replace that state. The heat thresholds use actual daily spend divided by the current daily max. Pending estimates do not announce that the max has been reached. A local-day rollover or a raised max recomputes the heat state and removes an obsolete warning. In-flight calls finish at the max; rejecting new generation belongs to the router.

**Tournament presentation.** The TUI owns `TOURNAMENT`, `ROUND r · N REMAIN`, `FINAL` when two remain, and `CHAMPION` banner placement. Effects uses the accepted ranking in its authoritative order and reveals it bottom-up; it never sorts scores or breaks ties. Invalid judge output and repair attempts produce no ranking or elimination effects. The static ranking contains every accepted score and critique even while its entrance is running. Eliminated markers survive animation completion and resize, and clear when the tournament ends. Champion celebration updates the tournament trophy display, not debate W-L records. These effects are local presentation for the human and do not add any entrant-visible messages.

**Grill presentation.** The TUI owns `GRILL`, `GRILL ROUND r`, `FINAL PLAN`, the turn heading, `questions asked 14 · passed 2`, and collapsible plan versions with the newest expanded. Effects decorates the active row and version badge without moving question, answer or plan text. A `PASS` advances the handoff and updates the authoritative counter without inventing a leader answer. `SURVIVED` appears only for the engine's `survived` completion reason. A normal completion shows the final plan without that label; a leader failure retains the last good plan without a success celebration.

**Pick presentation.** The TUI owns the eligible list, input placeholder, `pick pending` status and hint below the banner: `Type two names, e.g. @grok @gemini · Esc to cancel` for debate, or `Type one name, e.g. @grok · Esc to cancel` for grill. Effects animates only the banner and eligible row accents. Other messages remain usable while the pick is pending. A rejected pick leaves the highlight active; an accepted pick, cancellation or failure removes it immediately. Restoring `AWAITING_PICK` restores the static banner, hint and candidate markers from state without replaying the entrance. `$tournament` has no pick effect.

**Four-mode presentation contract.** The TUI owns each mode's banner, step dividers, preflight call count and estimate against the remaining daily amount, and inline collapsible report. Effects decorates those widgets in place. `SQUAD_BANNERS` keeps its existing meaning; Effects never inserts replacement art when banners are disabled. The four banner entrances use gold and its darkened self, with no debate score subline. Early output settles the entrance immediately. No new mode uses winner, champion, W-L or trophy effects. Preflight, waiting, retry and repair do not produce result effects. All effects are local to the human's terminal and add nothing to member histories, judge payloads or exports.

**Census presentation.** The TUI owns the ranked ledger, singleton markers, covered-item citations, member-by-gap table, discovery curve and the hint to send singletons to `$grill`. Effects reads the engine's stored estimate range and never computes Chao1, finder counts or the range. The exact headline `Found {n}; estimated {lo} to {hi} still unfound.` and the caveat that models share blind spots and the true figure is at least this remain readable throughout the count-down. The first pass shows its range immediately because there is no previous estimate. Later endpoints may rise as well as fall; the tween follows the committed values without forcing a decline. An unavailable estimate stays unavailable and never animates from or to zero. Singleton markers remain visible without animation. `Census is quiet. Good place to stop.` appears only when supplied by the authoritative report; a low animated endpoint never triggers it.

**Crosscheck presentation.** The TUI owns the coverage grid summary and every cell's citation, justified exclusion, open gap, proposed fix and explicit human risk acceptance. Grid fill means that a result has been committed, not that the cell is closed. Open gaps and challenged closures receive the same arrival treatment as other results and retain their explicit status. Pending or skipped reviews never acquire a closed appearance. Audit rulings update cells in place without removing them or changing the grid order. The bounded-stop statement comes from the report and remains qualified; Effects never infers completeness from a filled grid.

**Survey presentation.** The TUI owns the five-band map: consensus, contested, lone insights, lone dissents and rejected outliers. Bars use neutral styling with explicit AGREE / DISAGREE / UNSURE labels and counts; colour alone never encodes a vote. Initial tallies split from an unsplit neutral track; revised tallies move from the previous committed proportions. Reasons, confidence, switch information and the question that would settle a contested claim remain static. Missing votes are labelled as missing rather than counted as UNSURE. The human may see the report's attribution and switch details; animation does not add authorship or vote counts to the anonymous member payloads. Bar motion never ranks members or announces a claim as true.

**Probe presentation.** The TUI owns the role split and any sit-out notice, decision tree, prediction matrix, inseparable explanations and unexpected-result branch. Effects traces only connectors in the accepted tree's stored order. It never invents branches, changes predictions, selects an explanation or executes a check. The unexpected-result branch and unresolved alternatives are visible from the first frame and remain after the trace settles. Narrow rendering wraps the TUI's static tree without shrinking or moving labels during the effect.

### 3. The two showpieces

#### (a) Round banner transition

Total 1,900 ms. The banner animates inside its inline transcript block and never blocks input or streaming.

**Colour filters.** Both are pure functions of cell position within the banner and time.
- **Gay:** hue = `(x * 9 + y * 14 + t * 120) mod 360` degrees at OKLCH L 0.78, C 0.16, so diagonal rainbow bands drift.
- **Metal:** a vertical ramp by row, `#E8F1FF` to `#7FA6D9` to `#27406B`, with a hard white horizon line at 45% height and a specular glint column that sweeps across once.
- Rounds 1 to N-1 use a duotone of the two debaters' hues with the metal ramp's luminance. `OPENING` and `VERDICT` use gay. `FINAL ROUND` uses metal in red and gold.

```
t=0 ms      banner band dims to 60% (120 ms)
            ──────────────────────────────────────────────
t=120       a horizon line draws centre-out in white (160 ms)
            ················━━━━━━━━━━━━━━················
t=280       glyph rows unfold from the horizon up and down, BackOut, 320 ms.
            Cells appear as ▓ first, then resolve to the real glyph.
              ▓▓▓▓▓▓  ▓▓▓▓▓  ▓▓  ▓▓ ▓▓  ▓▓ ▓▓ ▓▓▓▓▓       ▓▓▓▓
              ██████╗  ██████╗ ██╗   ██╗███╗   ██╗██████╗     ██████╗
t=600       fully formed. The glint column sweeps left to right (300 ms).
              ██╔══██╗██╔═══██╗██║   ██║████╗  ██║██╔══██╗    ╚════██╗
              ██████╔╝██║   ██║██║   ██║██╔██╗ ██║██║  ██║     █████╔╝
              ██╔══██╗██║   ██║██║   ██║██║╚██╗██║██║  ██║     ╚═══██╗
              ██║  ██║╚██████╔╝╚██████╔╝██║ ╚████║██████╔╝    ██████╔╝
            A's hue enters from the left edge and B's from the right,
            meeting in the middle of the word.
t=900       hold 700 ms. Filter bands drift at 4 cells per second.
            subline fades in:     claude 21  ━━━━━━━━━╋━━━━━━  19 gemini
t=1600      settle, 300 ms: filter drift stops, the glint clears and the
            banner band undims. Glyph rows stay in place.
t=1900      static coloured FIGlet art remains in the transcript as the
            permanent, copyable record.
```

Fallbacks:
- `lite`: fades in over 150 ms, holds 700 ms, then settles as static duotone.
- 16-colour: the left half takes A's ANSI colour and the right half B's, in bold.
- `NO_COLOR`: bold glyphs only.
- Narrow: the `smblock` font (3 rows); under 60 columns, only the one-line rule.
- If tokens for the round start arriving before settling, the banner settles immediately from its current frame. Streaming always wins.

#### (b) Verdict reveal

Total about 6.5 s. Any key skips to the final card without consuming the key or bypassing normal input handling. Escape follows the TUI's transient-element and stop-confirmation contract.

```
t=0         The VERDICT banner placed on entering JUDGING is static.
            If its entrance is still running, it settles immediately.
            The card reveal begins with a 600 ms accent sweep.
t=600       Blackout: the transcript dims to 35%. A card frame draws from
            its centre outward (280 ms), double-line border in gold.
            ╔══════════════════ VERDICT ══════════════════╗
            ║   claude                          gemini    ║
t=900       Criteria rows appear one every 350 ms. In each row both bars
            grow from the centre outward while the digits count up from 0
            (400 ms QuadOut). Bars are each debater's hue.
            ║   logic       8 ▰▰▰▰▰▰▰▰╋▰▰▰▰▰▰▰ 7         ║
            ║   evidence    7  ▰▰▰▰▰▰▰╋▰▰▰▰▰▰▰▰▰ 9       ║
            ║   rebuttal    9 ▰▰▰▰▰▰▰▰▰╋▰▰▰▰▰▰ 6         ║
            ║   style       8 ▰▰▰▰▰▰▰▰╋▰▰▰▰▰▰▰▰ 8        ║
t=2300      Totals row: both numbers count up together and stop at 80% of
            the lower total. Hold 500 ms. Both name plates pulse
            alternately (two pulses each). This is the only suspense beat.
            ║   TOTAL      ??                    ??       ║
t=2800      Totals finish counting (300 ms). The winner's name plate
            flashes white for 120 ms.
t=3100      Winner reveal: the winner's nick renders in slant, in its hue
            with the gay filter sweeping through once, and a crown row
            (♛) above it. The card border recolours to the winner's hue.
            ║           ♛                                 ║
            ║     ___ _      _   _   _ ___  ___           ║
            ║    / __| |    /_\ | | | |   \| __|          ║
            ║   | (__| |__ / _ \| |_| | |) | _|           ║
            ║    \___|____/_/ \_\\___/|___/|___|   32–30  ║
t=3100      Confetti: 160 particles from the card's two top corners, glyphs
            ▪ ● ◆ ▴ * in both debaters' hues plus gold, gravity 30 cells/s²,
            random horizontal drift, each living 1.8 to 2.6 s. Particles
            draw only into blank cells or the dimmed background and never
            over card text. Pool is fixed, no allocation after startup.
t=4200      Loser treatment: the loser's plate stays in full colour (not
            greyed), gets a ✦ and the label "respect", and the section
            "what to steal from <loser>" types in under the card in the
            loser's hue, 12 ms per char. The loser's best criterion
            ("evidence 9") gets a single highlight sweep.
t=5600      Confetti has landed. Background undims over 400 ms. The
            scoreboard record tick fires in the participants panel.
t=6500      The card remains as static, copyable text in the transcript.
```

Fallbacks:
- `lite`: card fades in, count-ups run, no blackout, no confetti, no suspense hold.
- 16-colour and `NO_COLOR`: the same card with bars as `#` and `-`, the winner in bold reverse.
- Narrow: single-column card, criteria as `logic 8 : 7`, winner in `smblock`.

### 4. Architecture

All animation lives in `crates/squad-tui/src/fx/`. Banner text rendering and embedded fonts live in `crates/squad-core/src/banner/`; Markdown export lives in `crates/squad-core/src/export/`. Core app state knows nothing about animation.

**Separation.**
- `App` (state) is rendered by widgets into the frame buffer.
- `FxManager` owns all animation state and is called twice per frame: `fx.pre(&mut ctx)` for widget-level parameters and `fx.post(buf, area, now)` for buffer passes.
- Deleting the two calls yields a correct, static UI. That is the test for "cannot hurt the core".

**Two kinds of effect.**
- **Widget-level tweens:** scalar values that widgets read through a read-only `FxView` (score digits, bar split, rail draw height, spinner frame). These are `Tween { start, dur, from, to, ease }` sampled at `now`. A missing tween means the final value.
- **Post passes:** functions `fn(elapsed: Duration, area: Rect, buf: &mut Buffer) -> Done` that only mutate style or blank cells inside a registered `Rect`. Covers sweeps, dims, dissolves, banner filters, particles and the comet tail.
  - Standard passes wrap `tachyonfx::Effect` in a `tachyonfx::EffectManager` keyed by an `FxKey` enum, so retriggering replaces rather than stacks.
  - Particles and the gay/metal filters are in-house passes.
  - Message-body rects are registered as protected. The pass runner clips every pass against them, except passes on a short allowlist (dim, comet tail, colour-only clear wipe) that are colour-only.

**Triggering.**
- The feed reducer emits `FxCue` values (`BubbleOpened { id, model }`, `RoundStarted { n, final }`, `ScorePosted`, `LeadChanged`, `VerdictReady`, ...) into a bounded `VecDeque<FxCue>` (capacity 256, drop oldest).
- The manager drains the queue at the start of each frame. Cues are plain data and state never waits on an effect.

The following additions to `FxCue` are internal TUI types, not protocol events. `EventId` and `MemberNick` refer to Protocol's canonical identifier types. Numeric values and displayed text are read from the reduced authoritative state.

| Cue | Payload | Trigger |
|---|---|---|
| `DailySpendChanged` | None | Daily spend, reservations, unknown-call count, day or max changes |
| `TournamentRankingReady` | `contest_id: EventId`, `round: u32` | Accepted live `tournament.ranking` |
| `TournamentEliminated` | `contest_id: EventId`, `member: MemberNick` | Committed elimination |
| `TournamentChampion` | `contest_id: EventId`, `member: MemberNick` | Live `tournament.champion` |
| `GrillTurnChanged` | `contest_id: EventId`, `member: MemberNick` | Committed next active speaker |
| `GrillPlanReady` | `contest_id: EventId`, `version: u32` | Live `grill.plan` |
| `GrillSurvived` | `contest_id: EventId` | Live `grill.complete` with reason `survived` |
| `PickRequested` | `contest_id: EventId` | Live `contest.pick_requested` |
| `ModeBannerReady` | `contest_id: EventId`, `mode: census \| crosscheck \| survey \| probe` | TUI places the mode banner after accepted live setup or state |
| `CensusEstimateReady` | `contest_id: EventId`, `pass: u32` | Committed live pass estimate from the script's tally |
| `CrosscheckCellsReady` | `contest_id: EventId`, `artifact_id: EventId` | Committed live cell-result batch or audit ruling |
| `SurveyTallyReady` | `contest_id: EventId`, `artifact_id: EventId` | Committed live initial or revised vote tally |
| `ProbeTreeReady` | `contest_id: EventId` | Accepted live `probe.report` |

Persistent presentation, including candidate and eliminated markers, comes from reduced state rather than cue delivery. Dropping a cue can omit an entrance but cannot lose a marker or leave it active after the state changes. Candidate-highlight animation is keyed by contest and member; its lifetime follows `AWAITING_PICK`, and completion or cancellation removes all matching effects.

**Shared-move effects.** A move starting is not evidence that its output exists. The reducer emits artifact cues only after the corresponding commit, including a `barrier` commit where the script uses one. Judge token streams, rejected schemas and repair attempts cannot trigger estimate, grid, vote or tree effects. Cues reference committed artifacts rather than copying private payloads. Effects never broadcasts a human-visible report back to members.

New effect keys include the contest and the relevant pass, artifact, cell or claim identifier. Duplicate delivery of the same committed artifact produces no second entrance. A newer artifact replaces a running tween from its current frame toward the newest stored result. A large grid batch uses one bounded effect with a staggered schedule rather than one queued effect per cell. The queue and concurrent-effect caps stay unchanged.

Report bodies, citations, reasons, matrix entries and tree node labels are protected text. Grid fill and branch drawing change connector or cell styles only. Census endpoint tweens and survey bar boundaries occupy dedicated graphic rects outside protected text. Collapsing a report settles its active effects; reopening it shows the static committed artifact without replaying the entrance. Resize, `/stop`, `STOPPED` or `FAILED` settles the effects to the last committed state, retaining partial results and skipped-member notices without suggesting that unfinished steps completed.

**Replay.**
- The TUI internally tags feed delivery as `Live | Replay`; this is not a field on Protocol's `FeedItem`. The reducer only emits cues for `Live`.
- As a second guard, a cue carries its event timestamp. The manager discards any cue older than 2 s and caps concurrent effects at 48 (oldest non-hero evicted).
- Loading a thousand-message history produces zero effects.

**Clock.**
- `trait Clock { fn now(&self) -> Duration }` with `SystemClock` and `ManualClock`.
- Nothing in `fx/` calls `Instant::now()`. Randomness is `SimpleRng` seeded from the cue id.

**Frame-budget governor.** It measures draw plus flush time per frame as an EWMA (alpha 0.2) and bytes written per second.

| Grade | Enter when | Sheds |
|---|---|---|
| G0 | default | Nothing. 60 fps during hero moments, 30 fps otherwise. |
| G1 | EWMA over 8 ms for 15 frames, or over 120 KB/s out | Ambience, comet tail, confetti count halved, 30 fps cap. |
| G2 | EWMA over 14 ms for 15 frames, or over 250 KB/s | All particles, all full-buffer passes (dims, sweeps, drifting filters become static), banner becomes fade-only. |
| G3 | EWMA over 22 ms for 10 frames, or any input event waited over 50 ms | All effects snap to end state, 15 fps, spinners only. |

- Recovery steps up one grade after 3 s under the lower threshold.
- The effective level is `min(SQUAD_FX, grade)`.
- A small `fx:G2` tag appears in the status line when shedding, visible only with `SQUAD_FX_DEBUG=1`.

**Idle.** The manager reports `next_wake() -> Option<Duration>`.
- Active effects or streams: the frame interval.
- Only spinners, the daily-spend pulse or pending-pick highlights: 100 ms.
- Only ambience: 250 ms, ending 20 s after the last cue.
- Otherwise `None`, and the loop blocks on input and feed events with no timer at all.

**Input safety.** Use the TUI section's single event-loop contract in `crates/squad-tui/src/event_loop.rs`: input is always handled first, feed is drained in bounded batches of up to 256 items per iteration, and the frame timer is armed from `FxManager::next_wake()`, capped at 60 Hz and disarmed when idle. Effects never run on the input path. Animation skip leaves every key available to normal input handling. Escape closes the topmost transient element first; global stop during a contest requires the TUI's confirming second Escape within 2 s. Pick cancellation follows the TUI and Contests contracts.

**Capabilities.** `fx/caps.rs` resolves, once:
- colour tier from `NO_COLOR`, `COLORTERM` and `TERM`;
- synchronized output via a DECRQM query for mode 2026 with a 100 ms timeout;
- `SSH_TTY`, `TMUX` and terminal size.

Without synchronized output, full-buffer passes are disabled regardless of level because they tear.

**Testing.**
- Every effect has an `insta` snapshot test on a `TestBackend` with `ManualClock` at t = 0, 25%, 50%, 100% and 100% plus one frame. The last must equal the no-effect render.
- A property test asserts that for any cue sequence, the buffer after all effects finish equals the `SQUAD_FX=off` buffer, glyph for glyph.
- A second property test asserts protected rects never have glyphs changed.
- Governor tests feed synthetic frame times and assert grade transitions.
- A `criterion` bench renders 12 streams plus the verdict at 200x60 and fails CI over 6 ms per frame.
- Daily-spend tests cover 75/90/100% heat transitions, pending-to-actual replacement, unknown-cost `~`, local-day rollover and a raised max, in all effect levels.
- Tournament tests cover bottom-up reveal without changing the accepted order, one eliminated stamp per committed elimination, persistent markers through resize, and champion celebration with one trophy tick and no debate record tick.
- Grill tests cover sequential turn handoff, `PASS`, plan version bumps, `survived` completion and leader failure retaining the last good plan without celebration.
- Pick tests cover both banner strings and hints, eligible-only highlighting, rejected picks retaining the highlight, removal on accepted pick or cancellation, and static restoration after restart without an entrance.
- Input tests assert that skipping launch, verdict or champion effects preserves typed input and Escape handling. Integration tests use the TUI-owned event loop to verify drawing during sustained feed bursts and idle timer disarming.
- Four-mode banner tests cover `CENSUS`, `CROSSCHECK`, `SURVEY` and `PROBE`, step accents, disabled banners, early output and permanent inline settlement at wide and narrow sizes in every effect level and colour tier.
- Census tests cover first-pass rendering, falling and rising range endpoints, unavailable estimates, permanent caveats and singleton markers, and the quiet message appearing only from authoritative state.
- Crosscheck tests cover a 200-cell batch, gaps and exclusions arriving without appearing closed, challenged closures, explicit risk acceptance, skipped reviews and audit updates without cell removal or reordering.
- Survey tests cover initial bar splits, re-vote boundary changes, UNSURE and missing votes remaining distinct, all five report bands, unchanged reasons and switch details, and no winner or record effects.
- Probe tests cover branch order, unchanged node labels and matrix entries, the unexpected-result branch, inseparable explanations and narrow wrapping without an explanation being declared true.
- Shared-move tests cover commit-only cues, duplicate artifacts, rejected judge output, repair, dropped cues, partial failures, stop, report collapse, resize and restart replay. End-state and protected-glyph properties apply to all four modes; no effect changes member deliveries or exported Markdown.

