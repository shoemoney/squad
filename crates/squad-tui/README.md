# squad-tui

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
