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
