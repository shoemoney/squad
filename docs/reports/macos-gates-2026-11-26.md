# macOS release gates — 2026-11-26

**Result: FAIL; M4 exit remains blocked. All six assigned boxes remain open.**
The complete five-minute windows ran, but the spike missed key latency and the
real transcript missed draw p95 and key latency at every viewport. CPU and draw
p99 passed. The separate catalogue frame gate and all 54 release tests
passed. Clipboard delivery was repaired and verified with the CI-built demo.

## Run and evidence

- [Workflow run 45899337976](https://github.com/shoemoney/squad/actions/runs/45899337976):
  completed with failure; both timed-stream steps failed their unchanged gates.
  Compilation, release assertions, FX frame gate and artifact upload succeeded.
- Measured source: `a402d57fa2c8f1ba6a16edbd64337752fab2bc11`, branch `mac-gates`.
- [Downloaded measurement artifact](https://github.com/shoemoney/squad/actions/runs/45899337976/artifacts/15157022695):
  `mac-perf-a402d57fa2c8f1ba6a16edbd64337752fab2bc11`. It contains environment,
  Cargo.lock, raw logs, load samples and the fixed offline `squad-demo` executable.
- Local downloaded evidence is under `/tmp/mac-gates-evidence/run-45899337976/`
  on dev2. The tables below transcribe its raw measurements without excluded
  samples, threshold changes or substituted short runs.

The new workflow initially returned HTTP 404 because it was absent from the
default branch. The integrator registered it on main in `75013a1`; changes were pushed to `mac-gates`, without a main push or tag. Runs `45896877465` and `45897042130` were superseded to
include the clipboard fix and correct artifact lookup through Cargo metadata.
The integrator reported accidentally cancelling `45897506615`, then dispatched
`45899337976`. No completed measurements from those attempts are used here.

The current run queued behind release CI. The release CI's macOS
`m4b_pty_e2e_audit` and Python/PTY child were observed idle for over seven minutes;
the integrator cancelled that CI run and confirmed the hung processes were gone.
This performance workflow then ran normally. That separate CI hang is not a
performance result or a claimed fix in this branch.

## Hardware, versions and method

hueb (`192.168.1.3`, runner `squad-ci-macos`): Apple M3 Ultra, 28 reported CPUs,
103,079,215,104 bytes RAM (96 GiB), native arm64. macOS 27.2.1, build 26B131;
Darwin 27.2.0. Rust 1.99.1 (`58cf8f9c3`, LLVM 23.1.1), Cargo 1.99.1. The
committed lockfile records Ratatui 0.30.2, crossterm 0.29.0, TachyonFX 0.25.2
and Criterion 0.7.0. These were optimized Cargo bench/release builds.

`.github/workflows/mac-perf.yml` is workflow_dispatch only, selecting
`[self-hosted, macOS, ARM64, squad-ci]`. It compiles before timing and runs jobs
sequentially. No development checkout or build was created outside the CI runner
on either Mac. `OPENROUTER_API_KEY` is empty for CI; benchmarks need no network.

The existing `twelve_streams` is the throwaway effects-off spike, not a real
transcript widget. It ran 300 seconds each at 120×36 and 80×24. The existing
`composed_streams` is the actual App/feed reducer/Markdown/transcript/catalogue
rendering workload at full effects, 300 seconds each at 140×42, 80×24 and 200×60.
It uses its benchmark scheduler and TestBackend, rather than measuring a physical
terminal or invoking the production event loop. Both prepare 10,000 historical
messages and twelve 40-delta/s streams, with typing, scrolling and resizing.

This mirrors the Linux method in `docs/reports/perf-gates-2026-11-25/` and the
README, but is one macOS run rather than a three-run distribution. Drawing,
reducer-plus-draw and scheduled-key latency remain separate metrics. Key latency
includes scheduling delays until the frame containing the input completes; it
is not physical keyboard/display latency. Startup is fixture preparation and
first draw, excluding compilation and executable launch. CPU is the change in
process cumulative user+system `ps -o time=` divided by wall seconds, multiplied
by 100: 100% means one core, without dividing by the host's 28 CPUs.

The host was not isolated. Spike-start load was 3.12/2.52/2.44 (1/5/15 minutes).
Composed-start load was 1.86/2.05/2.21 and end load 2.37/2.09/2.11. The spike's
explicit end `uptime` was skipped when its command failed; the following
composed-start sample is retained separately. No automatic quiet-host wait or
causal explanation for the missed timing limits is claimed.

## Five-minute numbers versus limits

| Workload / viewport | Wall seconds | Frames | Draw p95 ms ≤8 | Draw p99 ms ≤16.7 | Key p95 ms ≤50 | CPU % of one core ≤25 | Result |
|---|---:|---:|---:|---:|---:|---:|---|
| Spike 120×36 | 300.048 | 4,413 | 0.517 | 0.572 | **107.662** | 0.927 | FAIL key |
| Spike 80×24 | 300.025 | 5,332 | 0.322 | 0.371 | **99.597** | 0.753 | FAIL key |
| Real transcript / full FX 140×42 | 300.017 | 4,699 | **12.295** | 13.041 | **122.909** | 17.402 | FAIL draw p95, key |
| Real transcript / full FX 80×24 | 300.065 | 4,751 | **11.738** | 12.634 | **125.049** | 16.533 | FAIL draw p95, key |
| Real transcript / full FX 200×60 | 300.045 | 4,025 | **13.042** | 13.906 | **129.535** | 15.851 | FAIL draw p95, key |

Each spike window processed 144,000 deltas, 3,614 keys, 150 scrolls and 60
resizes. The composed windows processed 144,012 / 144,036 / 144,024 deltas.
The low frame counts are retained; passing CPU does not compensate for missed
key latency. The benchmark catches up input/feed events and skips missed frame
deadlines. No frame-rate or timer diagnosis was established in this task.

Composed reducer-plus-draw p95/p99, which is not the draw-only gate:
140×42 **14.659/15.557 ms**; 80×24 **14.131/15.164 ms**; 200×60
**15.523/16.414 ms**.

| Workload / viewport | Fixture startup ms | End RSS KiB |
|---|---:|---:|
| Spike 120×36 | 3.346 | 15,712 |
| Spike 80×24 | 4.794 | 17,168 |
| Real transcript 140×42 | 994.588 | 241,504 |
| Real transcript 80×24 | 950.997 | 281,728 |
| Real transcript 200×60 | 948.603 | 287,184 |

`/usr/bin/time -l` recorded maximum resident set size of **121,749,504 bytes**
for the spike command, **299,515,904 bytes** for the composed command and
**121,995,264 bytes** for FX. These command-level resource counters include
Cargo/child-process resource use; they are not per-viewport benchmark peak RSS.
The per-viewport `ps` values above are current RSS snapshots. Startup and memory
have no numeric acceptance limit in these assigned boxes.

The workflow collects all composed windows with `COMPOSED_ENFORCE=0`, then
explicitly fails unless exactly three `gate=PASS` lines and no `gate=FAIL` lines
exist. All three reported FAIL here; measurement-only collection did not turn
them into passes. The spike also returned nonzero after both failed windows.

Existing Linux five-minute results remain separately qualified in the README:
three composed runs passed, with draw p95 3.408–4.132 ms, p99 4.254–4.992 ms,
key p95 17.310–18.000 ms and CPU 8.867–11.877%. They do not establish a macOS pass.

## FX, bounded work and coverage limit

The unchanged 200×60 `fx_frame` warmed 200-frame mean was **5.163428 ms**, passing
its strict **<6 ms** gate. Criterion estimated **5.1210 ms**, interval
**5.1168–5.1261 ms**, ten samples, one-second warmup and two-second measurement.
The earlier Linux warmed mean was 5.486 ms. These are buffer-painting results,
without terminal serialization or physical display latency.

All **54 release tests passed** across `clipboard`, `event_loop_shell`,
`fx_crosscheck`, `fx_governor`, `fx_invariants`, `fx_probe`, `fx_survey` and
`m4b_fx_integration_audit`. They verify the 256-cue queue, 48-active-effect cap,
governor degradation/recovery, sustained bounded feed draining, drawing during
bursts and idle timer disarming, alongside grid/bars/tree properties.

L20.T3 nevertheless remains open: `fx_frame` composes catalogue sample painters
with Paragraph streams/verdict, rather than a combined actual transcript
containing real 200-cell crosscheck, survey-bar and probe-tree widgets. The
catalogue timing and separate widget tests cannot establish that combined timed
workload. The `tests/fx_integration.rs` path named by L14.T3 is absent; existing
invariant, event-loop and M4b integration suites supplied the release assertions.
Benchmark/widget source changes were outside the original allowed scope; only
the subsequently requested clipboard source fix was added.

## Terminal matrix and clipboard repair

Terminals on hueb: Kitty **0.42.1**, Ghostty **1.4.0**, iTerm **3.5.14**; existing
tmux **3.8**. Homebrew Squad **0.1.2** was installed for the baseline checks.
Wick (`192.168.1.5`) reported macOS 27.2.1 build 26B131 and supplied the SSH hop.

Kitty used a temporary Unix remote-control socket and launch-only
`allow_remote_control=yes`. No permanent terminal config was changed. Bounded
raw-terminal probes sent DECRQM 2026, DECRQM 2027 and Kitty keyboard query/DA,
restoring termios afterward. `kitten @` drove input and inspected the emulator's
text buffer; there was no physical-display visual inspection or latency timing.

The baseline release used `/dev/null` as env file, separate temporary data
directories, dummy credentials and a loopback provider fixture. The patched
check used the artifact's FakeRoom demo from the measured source, with temporary
`SQUAD_DATA_DIR` and `SQUAD_FX=full`. Both paths required **$0 live-provider spend**;
the demo's displayed accounting is synthetic fixture data.

| Route | Sync 2026 | Kitty keyboard | Mode 2027 | Clipboard / viewport evidence |
|---|---|---|---|---|
| Kitty direct, brew 0.1.2 | probe `ESC[?2026;2$y`; no app sync begin | `ESC[?0u`, app pushed flags 3 | `ESC[?2027;0$y`: unsupported | 120×36 and 80×24 input/reply worked; Ctrl+Y notice but empty clipboard, no OSC 52 emitted |
| Kitty direct, fixed CI demo | app still emitted no sync begin | flags 3 in capture | same terminal capability | exact `pbpaste` match at 120×36 (251 bytes) and 80×24 (264 bytes); one OSC 52 each, exit 0 and exact termios restoration |
| Kitty + isolated tmux, default config | probe `ESC[?2026;2$y` | no Kitty response; DA `ESC[?1;2;4c`, ordinary fallback | `ESC[?2027;0$y`: unsupported | fixed demo emitted OSC 52 but `set-clipboard external` left the clipboard sentinel unchanged |
| Kitty + isolated tmux, `set-clipboard on` | no app sync begin | fallback | unsupported | exact delivery at 120×36 (264 bytes, two emitted requests including the blocked default) and 80×24 (220 bytes, one request); exit 0 and exact termios restoration; no DCS passthrough required |
| Ghostty | unverified | unverified | unverified | app launch produced no command/probe output |
| iTerm | unverified | unverified | unverified | AppleScript window request timed out after 10 seconds |
| wick → hueb SSH | unverified in terminal | unverified | unverified | agent-forwarded shell hop succeeded; GUI-originated hueb→wick→hueb route failed “No route to host” |

System Events automation hung and was stopped. Screen capture reported “could
not create image from display”. These limitations and the successful shell SSH
hop were not counted as a completed physical-terminal or terminal-over-SSH pass.
The original tmux run also retained its reply across 120×36 → 80×24 resizing.

### Clipboard causes, fix and user configuration

The original failure had two independent causes:

1. Squad created `App::pending_copy` but never consumed it in the event loop.
   Commit `10939fb` adds write-and-flush delivery before waiting for the next
   input/feed/frame event. The normal entry point uses stdout; `run_with_output`
   supplies an explicit writer for the regression test. Write/flush failures
   propagate through the existing shutdown path. Feedback remains “delivery not
   confirmed”, because OSC 52 has no portable delivery acknowledgement.
2. Tmux defaults to `set-clipboard external`, which ignores clipboard requests
   from applications. The temporary server had `allow-passthrough off`, a valid
   `Ms` terminfo entry and the client's `clipboard` feature. With the default,
   a manual OSC 52 request and the fixed demo's Ctrl+Y were blocked. With
   `set-clipboard on`, both reached Kitty and `pbpaste` exactly.

Kitty's loaded `clipboard_control` already included `write-clipboard` and
`write-primary` (reads require confirmation). A standalone ordinary OSC 52
transport test also passed directly in Kitty. No Kitty permission change was
needed, and tmux DCS wrapping is not needed for the normal `Ms` forwarding route.

User setting, documented in the README:

```sh
tmux set-option -s set-clipboard on
```

For persistence, the user may add `set -s set-clipboard on` to `~/.tmux.conf`.
Testing changed only the isolated temporary server. The branch fix does not
update Homebrew's installed 0.1.2 binary; tmux configuration alone cannot repair
that release's missing write.

The regression test `event_loop_delivers_copy_once_and_flushes_without_a_frame`
asserts exact OSC 52 bytes, one flush, request consumption and zero required
redraws with ready quit input. Removing the delivery block made it fail with
empty output; restoring the fix passed. Local clipboard/scheduler tests,
targeted Clippy `-D warnings`, formatting and the existing PTY input/resize/
restoration test passed. The macOS release clipboard regression also passed.

## Boxes and cleanup

| Milestone | State | Evidence / remaining limit |
|---|---|---|
| M4.P1.L1.T3 | open | spike key p95 107.662/99.597 ms exceeds 50 ms |
| M4.P4.L3.T2 | open | clipboard repaired, but app sync, Ghostty/iTerm and terminal-SSH coverage incomplete |
| M4.P4.L5.T3 | open | real draw p95 11.738–13.042 ms exceeds 8; key p95 122.909–129.535 ms exceeds 50 |
| M4.P4.L5.T4 | open | final evidence recorded, but both-platform passing gate and M4.P5 dependency not complete |
| M4.P5.L14.T3 | open | all 54 release tests pass; real-widget drawing/key release gate fails |
| M4.P5.L20.T3 | open | catalogue mean 5.163428 ms passes <6, but combined real grid/bars/tree timing is not established |

No thresholds were relaxed. All six boxes stay open; no passing Linux result,
unit assertion or catalogue-only timing substitutes for a failed macOS gate.
No main push or tag was made. The Homebrew installation remains as requested.
Owned windows/processes, isolated tmux server and temporary Mac files are cleaned
up after the follow-up checks; clipboard contents are restored when still
holding the test's known content. No task files were retained on wick.

Downloaded CI data and local raw terminal captures/results remain under
`/tmp/mac-gates-evidence/` on dev2. No credential values are included in this
report. Repository changes are the original four allowed paths plus the
explicitly requested `src/event_loop.rs` and `tests/clipboard.rs` clipboard fix.
