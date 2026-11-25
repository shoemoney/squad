# Demo design review repairs — 2026-11-25

All seven review findings are implemented inside `crates/squad-tui`.

| Finding | Result | Main files |
| --- | --- | --- |
| 1. Verdict hierarchy and motion | Winner-coloured bounded card, paired weighted totals, compact tally/judge, reasoning and takeaway. Totals count up, then the winner receives one reverse accent. Successful completion retains the queued reveal instead of cancelling it before drawing. | `widgets/verdict.rs`, `ui_fx.rs`, `event_loop.rs`, `composed.rs` |
| 2. Narrow roster | Two rows per member, six members visible at 80×24; activity and W–L record stay in the roster. F8 opens focused model/accounting details, Up/Down selects a member. | `widgets/participants.rs`, `app.rs`, `ui.rs` |
| 3. Debate density | One combined type/number/matchup/phase heading, compact topic summary, one opposing accent entrance, single-row round dividers. Embedded widgets omit duplicate outer headings. Permanent FIGlet is reserved for a completed result. | `widgets/debate.rs`, `widgets/transcript.rs`, `ui_fx.rs`, `composed.rs` |
| 4. Scores | One paired row per scored round, cumulative totals, short labelled two-colour bar. The sticky active matchup shows the latest committed tally. | `widgets/debate.rs`, `ui.rs` |
| 5. Turn styling | Coloured rails, aligned name/body inset, secondary status and one blank row between speakers. | `widgets/debate.rs` |
| 6. Soft-wrap whitespace | Prose consumes delimiter whitespace at soft boundaries, including across styled spans. Explicit indentation, code and editor source whitespace remain preserved. | `markdown.rs`, `text.rs` |
| 7. Resolved identities | `RoomSnapshot.models` carries the core startup model projection. The roster shows a short version; details show the concrete slug. FakeRoom supplies six identities before replies and uses them in completed messages. | `session.rs`, `room_adapter.rs`, `ui.rs`, `widgets/participants.rs` |

## Verification

Updated composed snapshots at 80×24 and 140×42, plus participant, Markdown,
debate and verdict widget snapshots. Added regressions for prose/code wrapping,
six-member visibility, startup model identity and F8 navigation, score-only
animation, and successful completion preserving the queued verdict reveal.

Two release-demo tmux passes captured both 80×24 and 140×42 for 90 seconds each
with `env -u NO_COLOR COLORTERM=truecolor SQUAD_FX=full`. The final pass retained
179 ANSI captures per size in `/tmp/squad-design-final` (local session evidence).
The first pass exposed completion clearing the verdict effect; the final pass
verified the repair. At 80×24, frames at 68.13, 68.63 and 69.14 seconds show
`5.38 : 4.71`, `7.98 : 6.98`, then `8.00 : 7.00`. At 140×42, the 70.15-second
frame shows `7.25 : 6.35` before the accepted totals settle. Body text stays fixed.
Both final captures show all six roster members, compact paired scores and the
bounded result card. This is presentation verification, not a performance gate.

Workspace gates: build passed; tests passed (691 passed, 3 ignored); Clippy with
warnings denied and format check passed. Logs are in `/tmp/squad-design-final`.

No milestone checkbox IDs were assigned by this review, so none were changed.
No findings deferred. Nothing needed outside the territory. No open questions.
