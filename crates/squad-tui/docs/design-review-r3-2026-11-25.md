# Third design review fixes

Branch: `tui-polish-r3`, based on `65eee88`.

| Finding | Result | Evidence |
| --- | --- | --- |
| 1. F7 loses the verdict | F7 suspends follow and retains a message/header anchor through detail remeasure and resize. End resumes live from either focus; the footer exposes `↓ back to live · End`. | Composed tests assert visible winner/scores, retained resize anchor and End behavior at both sizes; expanded snapshots. |
| 2. Result hierarchy | One winner-coloured card contains the small VERDICT label, prepared winner-name FIGlet at wide widths, compact bold winner strip at narrow widths, paired weighted scores, round tally, judge, decision and takeaway. Details follow the decision summary. | Composed verdict/expanded snapshots; numeric reveal targets only score cells and winner geometry. |
| 3. Debate rhythm | Prepared opening/final-round banners at wide widths; contender-coloured single-row intermediate separators. All narrow phase boundaries remain one line. Entrances tint existing geometry without shifting argument text. | Boundary-height regression, narrow-boundary regression and effect glyph-preservation tests. |
| 4. Quiet chrome | Separate quiet-chrome/event-accent theme styles. Room heading and routine help are dim, focused input is bright, current phase and required actions use bold gold. Contender names/scores retain model hues. | ANSI tmux captures and static palette/effect tests. |
| 5. Score meaning | Explicit Weighted score and Round tally labels. Per-round numeric values are bold in member hues; cumulative metadata is dim. Tug endpoints include contender names and values. | Verdict and debate snapshots; protocol arithmetic remains authoritative. |
| 6. Narrow density | Completed debate turns omit the routine completion label and inter-turn blank rows; pending scoring reads Judge scoring…. Author headings, sticky matchup and phase separators remain. | 80×24 composed/debate snapshots and timed captures. |
| 7. Wide roster | Third muted row shows live rate/queue when active, otherwise usage. Missing values are n/a. The 16-column compact rail remains two rows per member. | Metric precedence/height regression, participant snapshots and composed captures. |

## Validation

The network-free FakeRoom demo was run in private tmux sessions at 140×42 and
80×24 for 90 seconds, with `env -u NO_COLOR TERM=xterm-256color COLORTERM=truecolor`.
Captures sample five-second intervals; the first accepted verdict triggers F7.
At 85 seconds each session changes to the other size while expanded; at 90
seconds End returns to live. Selected captures accompany this report; full plain
and ANSI samples are in `/tmp/squad-r3-captures` on dev2.

Gates: `cargo build --workspace`, `cargo test --workspace`,
`cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`.
The final workspace suite passed 693 tests with 3 pre-existing ignored runtime-binding tests. The demo was built and run with the release profile. Tests use fixtures/local
mocks and do not call live providers. No real `.env` was read.

The required `/data/wiki` is absent on this host, so wiki lookup/update was
unavailable. All repository changes are confined to `crates/squad-tui`.


## Selected tmux captures

`wide` starts at 140×42; `narrow` starts at 80×24. The resized samples swap sizes.

| Stage | Wide | Narrow |
| --- | --- | --- |
| Opening | [capture](captures/r3/wide-10s.txt) | [capture](captures/r3/narrow-10s.txt) |
| Live throughput / round 1 | [capture](captures/r3/wide-25s.txt) | [capture](captures/r3/narrow-25s.txt) |
| Final boundary | [capture](captures/r3/wide-45s.txt) | [capture](captures/r3/narrow-45s.txt) |
| Accepted result | [capture](captures/r3/wide-60s.txt) | [capture](captures/r3/narrow-60s.txt) |
| F7 expanded | [capture](captures/r3/wide-expanded.txt) | [capture](captures/r3/narrow-expanded.txt) |
| Expanded after resize | [capture](captures/r3/wide-resized.txt) | [capture](captures/r3/narrow-resized.txt) |
| End returns to live | [capture](captures/r3/wide-live.txt) | [capture](captures/r3/narrow-live.txt) |
