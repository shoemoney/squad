# squad-tui


## Integrated product renderer (2026-11-24)

The demo now uses the prepared Markdown transcript, participant roster, graphical
spend meter, inline debate/opening/round cards, accepted round scores, tug bar,
verdict and case widgets. Question-first picks are inline. Generic tournament and
grill states use their real widgets; typed exchanges/rankings remain an M5 input
boundary rather than a private TUI JSON schema. The network-free script takes
60–90 seconds (checked with Tokio's virtual clock), then leaves a grill pick open.
Page Up/Down and the mouse wheel scroll history; End resumes
following the room. Mouse scrolling over the roster scrolls that panel separately.

### Decision presentation (2026-11-25)

The accepted verdict is a bounded winner-coloured card with paired weighted
scores, compact tally/judge metadata, reasoning and takeaway. Live totals count
up before the winner accent lands; successful debate completion preserves this
queued reveal. Effects off and history replay show the accepted final values.

The roster uses two rows per member at 80×24 and adds a muted metric row at
wide sizes: live throughput/queue first, otherwise usage with explicit n/a.
It shows the resolved model's short version from startup, activity and W–L record. F8 opens
the focused member's full model/accounting details; Up/Down changes the member.
The demo supplies six representative model identities before any replies arrive.

Debates have one combined heading and a compact topic summary. The opposing
accent entrance runs once. Wide opening/final boundaries use prepared banner art;
intermediate rounds and narrow boundaries use single-row dividers. Winner FIGlet
art belongs inside the verdict card, with a compact winner strip at narrow sizes.
F7 anchors to that card through expansion and resize; End returns to live.
Each score distinguishes per-round values from cumulative tally, with named tug
bar endpoints. The sticky matchup retains the latest committed tally. Turns use
coloured rails and a shared body column; narrow finished turns omit redundant
completion labels and blank rows. Prose soft wraps
consume delimiter whitespace; code and editor whitespace remain preserved.

`benches/composed_streams.rs` exercises the actual `App`, feed reducer, Markdown,
widgets and catalogue painters at `SQUAD_FX=full`, with 10,000 prepared historical
messages, twelve streams at 40 deltas/s each, editor mutations, scrolling,
resizing, and repeated verdict reveals. It verifies typed input in the completed
frame. Draw timing starts immediately before `Terminal::draw` and includes
preparation inside `ui::render`, layout, painting, diff and TestBackend flush.
Reducer-plus-draw timing additionally includes the current frame's feed/input
work. Key latency is wall time from scheduled input to its visible frame; CPU
uses process user+system ticks as in the spike below. These are separate metrics.

Quiet-host gate command:

```sh
cargo bench -p squad-tui --bench composed_streams
```

The default is 300 seconds per viewport with gate assertions enabled. For a
measurement-only smoke, `COMPOSED_SECS=10 COMPOSED_ENFORCE=0` retains and prints
failures without terminating before the remaining viewports. It does not convert
a failed gate into a pass.

### Quiet-host release rerun (2026-11-25)

Dev3 (AMD Ryzen 9 8945HX, 32 logical CPUs, Linux x86_64, Rust 1.99.1),
optimized Cargo bench profile, headless TestBackend, truecolor and full effects.
Three complete runs used the default 300 seconds **per viewport**, sequentially,
with gate assertions enabled. All cells are **min / median / max** across runs.

| Viewport | Draw p95 ms (≤8) | Draw p99 ms (≤16.7) | Key p95 ms (≤50) | CPU % of one core (≤25) | Result |
|---|---:|---:|---:|---:|---|
| 140x42 | 3.656 / 3.798 / 3.947 | 4.540 / 4.641 / 4.734 | 17.310 / 17.598 / 17.735 | 9.443 / 10.337 / 10.757 | PASS 3/3 |
| 80x24 | 3.408 / 3.420 / 3.671 | 4.254 / 4.318 / 4.428 | 17.337 / 17.425 / 17.486 | 8.867 / 9.050 / 9.890 | PASS 3/3 |
| 200x60 | 4.043 / 4.124 / 4.132 | 4.768 / 4.928 / 4.992 | 17.635 / 17.822 / 18.000 | 10.983 / 11.750 / 11.877 | PASS 3/3 |

The runner waited for one-minute load below 2 after compilation and a separate
workspace build. The first TUI invocation started at 0.61/3.04/4.02 (1/5/15-minute
load); the benchmark's first steady window started at 0.64/3.00/4.00. Per-window
start/end loads, 30-second samples, memory and raw output are retained in
[the M6 performance evidence](../../docs/performance.md). This is shared-host
Linux headless evidence, not a physical-terminal or macOS result, a 30-minute
sustained test, or the separate 200x60 all-catalogue 6-ms effects gate.

Three original attempts failed before timing because the benchmark checked the
editor title's cell. The harness now derives the input cell from current layout,
checks every typed character, and resizes compact width between 80/81 instead of
entering the unsupported 79-column screen. A six-second resize smoke passed at
all three sizes before these full runs. Thresholds and production code are
unchanged; failed attempt logs are retained alongside the successful runs.

### Loaded-host measurements; not release acceptance

Dev1 was running several other workers' Cargo builds. The final measurement-only
smoke used optimized code, headless TestBackend, truecolor and full effects.
The server restart interrupted the 200x60 window after startup; no result is
claimed for that viewport. Load values below are the sampled **1/5/15-minute**
Linux load averages at each window's start and end.

| Viewport | Seconds / frames | Inside draw p95 / p99 ms | Reducer + draw p95 / p99 ms | Wall key p95 ms | CPU, one core | Load start → end | Observed gate |
|---|---:|---:|---:|---:|---:|---|---|
| 140x42 | 10.002 / 600 | 3.939 / 5.154 | 4.168 / 5.419 | 18.653 | 12.298% | 196.18/178.19/138.64 → 167.02/172.53/137.23 | PASS smoke |
| 80x24 | 10.003 / 600 | 7.053 / 10.684 | 7.366 / 11.931 | 18.920 | 16.596% | 167.02/172.53/137.23 → 142.27/167.05/135.82 | PASS smoke |
| 200x60 | interrupted | unavailable | unavailable | unavailable | unavailable | unavailable | pending |

Each completed smoke window processed 4,812 deltas. Startup was 1,192.285 ms and
1,213.207 ms respectively, including the virtual-clock demo fixture, 10,000-message
cache fill and first draw. These short loaded-host observations do **not** replace
the five-minute quiet-host exit gate, the 200x60 6 ms effects gate, a real terminal
measurement, or macOS verification. The quiet-host release follow-up is recorded
above; these older observations remain historical evidence.

Retained failed development smokes: before artifact-cache optimization, the
140x42 10.022 s / 331-frame run measured **reducer-plus-draw** p95/p99
65.695/231.705 ms, wall key p95 421.713 ms and 19.457% CPU. An optimized but still
heavily descheduled 16.652 s / 103-frame run measured reducer-plus-draw
151.882/508.836 ms, wall key p95 10,035.409 ms and 4.624% CPU. Those older runs
labelled the combined interval “draw”; they did not collect time inside draw
separately. Load was sampled at 123.38/159.42/124.16 and later
217.63/176.32/131.36 around this work, and 209.37/181.38/135.88 immediately after
the latter failure. They cannot establish a quiet-host pass or regression.


## M4.P1.L1: throwaway 12-stream spike

This standalone benchmark uses Ratatui directly, with effects off. It imports no
Squad transcript, Markdown pipeline, application loop, or effects implementation.
This measures the feasibility baseline; the real-widget, full-effects gate must
be repeated after M4.P5. **macOS arm64 is pending, so the two-platform L1 gate
remains blocked.** Linux headless results do not establish terminal-emulator or
PTY input latency.

Run from the workspace root:

```sh
# Release profile; five minutes PER viewport (ten minutes total).
cargo bench -p squad-tui --bench twelve_streams
# Short CI check; 40 seconds total, exercises resize and scroll.
SPIKE_SECS=20 cargo bench -p squad-tui --bench twelve_streams
# Optional real terminal; requires stdout TTY of at least 120x36.
SPIKE_TERMINAL=1 cargo bench -p squad-tui --bench twelve_streams
```

A failed gate returns a nonzero exit status. `SPIKE_SECS` must be positive.
The real-terminal path enters and restores the alternate screen on normal return
or returned error; it uses simulated typing rather than reading physical keys.

### Workload and measurement method

Each viewport (120x36 and 80x24) starts with 10,000 distinct mixed-prose/fenced-Rust
messages. Twelve independent stream buffers receive one synthetic token delta
every 25 ms (480 deltas/s total), in deterministic synchronized batches. Each
stream completes a message every 240 ticks and starts another; history grows.
The fixture is ASCII, with a small custom wrapping/cache path, not a Markdown
parser or syntax highlighter. Finalized history rows are cached; live rows are
wrapped before rendering, and only viewport rows are drawn. Fences/code receive
simple color styling.

The single loop draws at at most 60 Hz, catches up all scheduled stream and input
events after a delay, and skips missed frame deadlines. Simulated typing occurs
every 83 ms, clearing the draft at 60 characters. Every two seconds, scrolling
cycles through recent history, midpoint, oldest history, and live tail. Every
five seconds the viewport width alternates between its nominal width and one
column narrower, reflowing all cached history while preserving the row anchor.
These are throwaway scheduling and row-anchor choices, not production behavior.

Draw duration uses `Instant` from live-row preparation through `Terminal::draw`
completion, including rendering, buffer diff and backend flush. Resize/reflow
work occurs before that timer; it is included in CPU and scheduled-key latency.
Input latency starts at the intended event deadline (including time waiting for
a frame), and ends after the draw containing the input completes. The harness
asserts every draft character is present in the completed frame buffer. It
reports nearest-rank p95/p99 across all draws and p95 across all key events,
without a warmup exclusion. Initial fixture creation/cache fill and first draw
are measured separately as startup, excluding executable launch and dependency
compilation. Startup is not included in the steady-state timing/CPU window.

On Linux, CPU is the delta of `/proc/self/stat` user plus system ticks, converted
with `getconf CLK_TCK`, divided by actual wall seconds, multiplied by 100.
100% means one fully occupied logical core; it is not divided by the 32 host
threads. At 100 ticks/s, five-minute quantization is about 0.0033 percentage
points. The macOS fallback reads cumulative user+system time via `ps -o time=`;
its coarser resolution is unsuitable for interpreting very short CPU samples.
Linux memory reports current resident size (`VmRSS`) and process-lifetime peak
resident size (`VmHWM`) from `/proc/self/status` at each viewport's end. Peak is
cumulative across both viewports. macOS reports RSS through `ps` (peak pending).

### Linux x64 measurement (2026-11-24)

- Host: dev1, AMD Ryzen 9 8945HX, 16 cores / 32 logical CPUs, 32 GB installed
  DDR5 RAM (29,257 MiB reported usable). No CPU affinity or isolation.
- OS: Ubuntu 26.04.1 LTS, kernel 7.0.0-44-generic, x86_64.
- Terminal/backend: headless Ratatui `TestBackend`, fixed viewport. No TTY,
  terminal emulator, escape-sequence serialization, or display compositor.
- Toolchain: rustc 1.99.1 (58cf8f9c3, 2026-10-13), Cargo 1.99.1;
  default optimized Cargo bench/release profile.
- Dependencies: Ratatui 0.30.2, ratatui-core 0.1.2, ratatui-widgets 0.3.2,
  ratatui-crossterm 0.1.2, crossterm 0.29.0. No added dev-dependencies.
- Shared host, not a dedicated benchmark machine. Initial load averages were
  0.59 / 0.52 / 0.48; workspace compilation/checks finished before the full run.

Required gates per viewport: draw p95 ≤ 8 ms, draw p99 ≤ 16.7 ms,
key-to-visible p95 ≤ 50 ms, and average process CPU ≤ 25% of one logical core.

| Viewport | Wall seconds | Frames | Draw p95 ms | Draw p99 ms | Key p95 ms | CPU, one core | Result |
|---|---:|---:|---:|---:|---:|---:|---|
| 120x36 | 300.004 | 17,994 | 0.341 | 0.521 | 15.890 | 1.097% | PASS |
| 80x24 | 300.036 | 17,889 | 0.370 | 0.532 | 16.136 | 1.090% | PASS |

Each window delivered exactly 144,000 deltas, 3,614 keys, 150 scroll operations,
and 60 resize/reflow operations. Startup was 4.130 ms at 120x36 and 2.282 ms at
80x24. The kernel reported RSS / VmHWM of 13,984 / 13,984 KiB after 120x36 and
13,976 / 13,976 KiB after 80x24. These are the reported counters, not an
independent allocation profiler. The shared host's sampled load during the
first window reached 13.20 / 7.36 / 3.12; a late second-window sample reached
39.43 / 14.78 / 7.04, and the immediately post-run sample was
82.16 / 29.44 / 12.36. This was not an isolated idle-machine run. Both windows
passed without rerunning or excluding samples. Fewer than 18,000 frames reflects
skipped frame deadlines, not skipped stream or input events.

Additional validation: a six-second-per-viewport release smoke passed, exercising
scroll and resize; zero-second and non-TTY terminal-mode requests were rejected.
A one-second-per-viewport `script` pseudo-terminal smoke passed the optional
Crossterm path, with alternate-screen enter/leave sequences both present. This
is code-path validation, not a physical terminal performance measurement.

### macOS arm64 measurement

Pending: the same five-minute release workload at both viewports, with hardware,
OS/terminal/dependency versions, startup, RSS/peak memory, drawing, scheduled-key
latency, and CPU measurements. Do not infer a macOS pass from Linux or a short
run. M4.P1.L1.T3 and the overall cross-platform gate remain incomplete.

## Integrated effects catalogue (2026-11-24)

All 18 effect families now live in `src/fx/`, use the shared `FxLevel`, and
share startup `ColorTier` and palette helpers. `verdict` and `tournament` use
`ParticlePool` for celebrations. `fx/mod.rs` documents the app-shell contract:
queue live committed cues, call `pre`, draw authoritative static widgets, bind
accepted cues with `attach_pass` and `Pass::catalogue`, then call `post` and arm
`next_wake`. The same runner clips both in-house and TachyonFX painters against
protected text and terminal colour capabilities. Stop/resize settle everything;
collapse/failure/pick completion can settle one contest. Reopening draws the
committed static report, including partial results.

The additional four-mode cues reference committed passes/artifacts, while their
replacement keys identify the contest's graphic slot. This lets a new artifact
replace its predecessor; separate bounded artifact deduplication suppresses a
second entrance even if the artifact arrives under a new event identity.
`FxCue::Catalogue` covers local chrome, status, dispatch, command and other UI
notices without adding protocol events. Candidate row keys include contest and
member, and `FxContext::pending_pick` removes them when the pick ends.

### Full-catalogue frame gate

```sh
cargo bench -p squad-tui --bench fx_frame -- --sample-size 10 --measurement-time 2 --warm-up-time 1
```

On dev1/Linux x64 with the pinned Rust 1.99.1 toolchain, the release workload
renders 12 streams, a verdict, and all 18 actual catalogue families through the
manager at 200x60 (31 concurrent entries). The verdict is sampled during its
pool-based confetti phase; a separate particle burst is also active. Protected
body rectangles cover all twelve streams. Every family is deliberately painted
over the same viewport to stress composition rather than model one app layout.

The 200-frame warmed mean was **5.486 ms**, passing the hard **6 ms** gate.
Criterion estimated **5.541 ms/frame**, interval **5.522–5.557 ms** (10 samples,
1 s warm-up, 2 s sampling). Timing covers fresh static rendering, manager hooks,
protected scratch copying and all catalogue painters. Setup and teardown are
excluded; repeated samples use the same deterministic active timeline instant.
This is a buffer-rendering measurement without terminal serialization, backend
flush, input latency, or a macOS claim. The application-shell performance gate
still needs its real event loop and backend.

### Snapshot changes from shared palette integration

Glyphs/timings remain unchanged except confetti positions, which now follow the
shared seeded pool. Colour snapshots changed where local helpers used RGB/OKLab
mixing, clipped out-of-gamut colours, or used a different 256-colour quantizer.
They now use shared OKLCH interpolation, chroma gamut mapping and nearest cube
entries. Updated snapshots (all under `tests/snapshots/`):

- `fx_banner__fx_banner_{Round(3),FinalRound,Opening}_{120,80}_{TrueColor,Indexed,Ansi}.snap`
- `fx_banner__fx_banner_mode_{Census,Crosscheck,Survey,Probe}_{120,80}.snap`
- `fx_bubble__seal_{0,37,75,150,167}.snap`
- `fx_command__wipe_140.snap`, `fx_filter__filter_storyboards.snap`
- `fx_grill__storyboard.snap`, `fx_crosscheck__storyboard_single_and_200_cell_audit.snap`,
  `fx_probe__storyboard_root_first_unexpected_and_inseparable.snap`
- `fx_pick__fx_pick_{Debate,Grill}_{120,80,40}.snap` (shared tier names in snapshots)
- `fx_tournament__tournament_champion_Full_false_{3100,3650}.snap`
- `fx_verdict__verdict_{full,narrow}_{3100,4200}.snap`

### Reel and verification

`cargo run -p squad-tui --example fx_reel` plays the actual logo/roster, launch
streak/spoke burst, versus card, horizon-unfold OPENING/ROUND 2/FINAL ROUND
banners, per-model streaming bubbles/comets/seals, score slams and tug bars,
VERDICT entrance/reveal/confetti, tournament ranking and survey bar split.
The verdict totals are computed from its displayed criteria: **34 : 31**.
`q`/Escape exits, `r` restarts, a key skips the launch/verdict without consuming
normal input handling.

A 31-second 140x40 PTY smoke reached the survey, exited on `q` with status 0,
emitted alternate-screen enter/leave, and restored the exact original termios
settings. Deterministic scene checks also render every phase at 80x24, 140x40
and 200x60. The expanded invariant suite covers all families' protected glyphs,
final/off equivalence, replay of 1,000 events, replacement, duplicate artifacts,
dropped cues, viewport-bounded allocation, and partial-report settlement.

## Offline milestone audit (2026-11-26)

From the repository root, `./setup.sh` installs the Foundations dependencies,
preserves an existing `.env`, builds the release binary and prints the next
command, `./target/release/squad`. For an offline rehearsal without credentials,
use `cargo run -p squad-tui --example demo`. The built-binary startup, streamed
reply, Ctrl+C/SIGINT, hangup recovery and mode restoration checks live in
`crates/squad-cli/tests/pty.rs`; installer preservation is covered by
`crates/squad-cli/tests/m6_setup.rs` and `scripts/test-setup.sh`.

`tests/m4b_conversation_audit.rs` opens temporary SQLite rooms with the M5 loopback
catalogue fixture and checks actual census, both crosscheck extraction paths,
new-answer/existing-answer survey, and adjusted probe preflight values at both
viewports. `tests/m4b_tournament_runtime_audit.rs` and `tests/grill_runtime.rs` run the
production room against canned provider responses, replay its committed output
into the static TUI, and check exports. These tests never read local credentials
or contact OpenRouter. Survey collection is a boolean in M5 settings; grill
rounds and the selected plan live under `settings.setup`.

`tests/m4b_pty_e2e_audit.rs` uses Python 3's standard-library PTY driver on Unix. It runs the
real terminal guard and crossterm decoder against the FakeRoom script, checking
literal bracketed paste, Ctrl+J/Alt+Enter, streaming, Ctrl+F, Ctrl+Y initiation,
preflight confirmation, 120×40 → 80×24 resize, alternate-screen/paste/cursor
cleanup and exact termios restoration. The ignored `pty_child` test is its
subprocess entry point and is exercised by the ordinary parent test.

`tests/m4b_fx_input_audit.rs` checks input and Escape during all 18 effect families;
`tests/m4b_fx_integration_audit.rs` checks that the four new effects do not gate shared moves
or barriers, submit delivery requests, or alter exported Markdown. Destination
writes and flush failures for every export scope are checked in `tests/m4b_export_audit.rs`.

`tests/m4b_debate_audit.rs` checks exact threshold boundaries, expiry/cancellation,
equal and late authoritative scores, F7 reasons and retained anchors/records.
`tests/m4b_recovery_audit.rs` and `tests/m4b_session_audit.rs` check draft/quote
preservation, acknowledged resets, duplicate scoreboards, restored probe roles,
crosscheck audit samples and explicit risk acceptances.

The audit does not close the physical-terminal capability matrix or the macOS
arm64 full-effects performance gate. Linux headless measurements above retain
their original qualification. Report source search/copy and complete large-report
navigation still depend on the earlier M4.P2.L8 implementation; the combined
seven-mode completed-result integration gate also remains open. M4 must not exit
while its two-platform gates remain open.

Final-source validation on dev3/Linux x64 with Rust 1.99.1 and four Cargo build
jobs passed `cargo test --workspace --no-fail-fast` with `RUST_TEST_THREADS=1`:
1,254 passed, zero failed, two ignored. The ignored entries are the existing
manual release performance gate and the PTY child entry point invoked by its
passing parent test. All nine `m4b_*_audit.rs` integration binaries ran. Formatting
passed `cargo fmt --check`; all-target Clippy passed
`cargo clippy --workspace --all-targets -- -D warnings`.
