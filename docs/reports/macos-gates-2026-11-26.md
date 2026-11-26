# macOS release gates — 2026-11-26

**Result: incomplete; M4 exit remains blocked.** No macOS performance measurements
were produced. All six assigned milestone boxes remain open. Terminal checks found
an actual clipboard failure and incomplete emulator/SSH coverage. No limit was
relaxed and no Linux measurement was substituted for a macOS result.

## Workflow and dispatch blocker

The new, dispatch-only `.github/workflows/mac-perf.yml` was pushed on `mac-gates`
in commit `416717a`. It selects `[self-hosted, macOS, ARM64, squad-ci]` and runs:

1. The unchanged `twelve_streams` feasibility spike, 300 seconds each at 120×36
   and 80×24, effects off by design.
2. The unchanged `composed_streams` actual App/reducer/Markdown/transcript/full-FX
   workload, 300 seconds each at 140×42, 80×24 and 200×60. This is the existing
   real-widget counterpart to the throwaway spike; setting `SQUAD_FX=full` does
   not turn `twelve_streams` into a real-widget benchmark.
3. `fx_frame` at 200×60, ten Criterion samples, one-second warmup and two-second
   measurement, preserving its hard warmed-mean <6 ms assertion.
4. Release tests `fx_invariants`, `fx_governor`, `event_loop_shell`,
   `m4b_fx_integration_audit`, `fx_crosscheck`, `fx_survey` and `fx_probe` for
   queue/cap/governor, sustained feed, idle disarming and grid/bars/tree coverage.

Compilation precedes timing; jobs run sequentially. As in the Linux evidence in
`docs/reports/perf-gates-2026-11-25/`, steady drawing and reducer-plus-draw timing
remain distinct. CPU is process user+system time from `ps -o time=`, divided by
elapsed time, as a percentage of one core. Per-viewport startup and RSS come from
the harness; `/usr/bin/time -l` additionally records process-tree resource use
and maximum resident size (macOS reports bytes). This peak is not a per-viewport
allocation profile. Start/end `uptime`, hardware, OS, Rust/Cargo and Cargo.lock
are retained. There is no automatic quiet-host isolation or repeated-run claim.

The workflow collects every composed viewport with `COMPOSED_ENFORCE=0`, then
explicitly fails unless exactly three `gate=PASS` results and no `gate=FAIL`
exist. It does not silently accept a failing measurement. Evidence and an offline
FakeRoom demo executable upload even after a gate failure under artifact name
`mac-perf-<source SHA>` with 30-day retention.

Dispatch attempted:

```text
gh workflow run mac-perf.yml --ref mac-gates
HTTP 404: Not Found
```

A direct dispatch API request also returned 404. The repository's registered
workflow list contained only `.github/workflows/ci.yml` and `release.yml` on the
final check. The new workflow needs default-branch registration before dispatch;
no main push, tag, or modification of an existing workflow was made. The owner
was asked whether the integrator could merge just the new workflow. There is
**no mac-perf run URL and no downloadable CI measurement artifact**. The unrelated
push-triggered CI run is not performance evidence. The runner was online and
busy, with the expected labels.

After registration:

```sh
gh workflow run mac-perf.yml --ref mac-gates
# Read the dispatched run's status, then download its mac-perf-<SHA> artifact.
```

## Numbers versus limits

| Gate | Limit | macOS measurement | Disposition |
|---|---:|---|---|
| Spike draw p95, both sizes | ≤8 ms | unavailable | open |
| Spike draw p99, both sizes | ≤16.7 ms | unavailable | open |
| Spike scheduled key p95 | ≤50 ms | unavailable | open |
| Spike CPU, one logical core | ≤25% | unavailable | open |
| Real transcript draw p95, all sizes | ≤8 ms | unavailable | open |
| Real transcript draw p99, all sizes | ≤16.7 ms | unavailable | open |
| Real transcript scheduled key p95 | ≤50 ms | unavailable | open |
| Real transcript CPU, one logical core | ≤25% | unavailable | open |
| Full catalogue warmed frame mean | <6 ms | unavailable | open |
| Cue queue / active effects | 256 / 48 maximum | tests scheduled, not executed by this workflow | open |
| Governor / sustained feed / idle disarming | degradation / bounded drain / no idle timer | tests scheduled, not executed by this workflow | open |
| Startup / current and peak RSS | record measurements; no numeric limit in these boxes | unavailable | open |

Existing Linux composed measurements remain in the README: all three five-minute
runs passed; p95 drawing across viewports/runs was 3.408–4.132 ms, p99
4.254–4.992 ms, key p95 17.310–18.000 ms and CPU 8.867–11.877%. The earlier Linux
catalogue warmed mean was 5.486 ms (<6 ms). These are historical Linux results,
not newly run checks or proof of the macOS gates.

There is also a harness coverage limitation: `fx_frame.rs` paints catalogue
samples over Paragraph streams/verdict, rather than a complete actual transcript
containing real 200-cell crosscheck, survey-bar and probe-tree widgets. Separate
widget tests and the composed benchmark do not establish a timed combined
real-widget workload for all three. `tests/fx_integration.rs` named by L14.T3
is absent; the workflow uses the existing invariant, event-loop and M4b integration
suites. Source edits to those harnesses were outside this task's allowed paths.
Even a future green workflow must not automatically close L20.T3 on that basis.

## Machines and terminal versions

- hueb, 192.168.1.3: Apple M3 Ultra, 103,079,215,104 bytes RAM (96 GiB), arm64;
  macOS 27.2.1, build 26B131. GitHub runner `squad-ci-macos`.
- wick, 192.168.1.5: macOS 27.2.1, build 26B131; used for SSH connectivity checks.
- Homebrew `squad` 0.1.2 installed for this check; existing tmux 3.8 used.
- Kitty 0.42.1; Ghostty 1.4.0; iTerm 3.5.14.
- Benchmarks would use pinned Rust/Cargo 1.99.1 and the committed lockfile
  (Ratatui 0.30.2, crossterm 0.29.0); no macOS benchmark dependency/version
  capture was produced because dispatch never started.

## Terminal method and matrix

Kitty was launched with a temporary Unix remote-control socket and
`-o allow_remote_control=yes`, explicit cell geometry and no permanent config
edit. `kitten @ send-text`, `send-key`, `get-text` and window resizing drove the
installed release. Checks inspected the emulator's rendered text buffer; there
was no physical-display visual inspection or display/compositor latency timing.

A temporary loopback HTTP fixture on hueb served the models catalogue and a
fixed streamed reply. `OPENROUTER_BASE_URL` pointed only at 127.0.0.1 and the
credential was a dummy fixture value. Two fixture roster models and the judge
resolved to `fixture/plain-v1`. `/dev/null` was the explicit env file and each
run had a separate temporary `SQUAD_DATA_DIR`. No live provider calls or spend
were required ($0). The reply was:

```text
macOS clipboard fixture: exact source copied through Ctrl+Y.
```

A temporary Python raw-terminal probe sent DECRQM for 2026 and 2027 plus the
Kitty keyboard query and primary DA. It restored termios afterward. These raw
replies establish emulator capability, separately from application behavior.
The `doctor` command was offline; its stdout was redirected, so its
“unsuitable for TUI” stdout classification is not a failed interactive launch.
The subsequent interactive TUI did open and stream successfully.

| Terminal / route | Sync 2026 | Kitty enhancement | Mode 2027 | Ctrl+Y → pbpaste | Viewports / interaction |
|---|---|---|---|---|---|
| hueb Kitty direct | `ESC[?2026;2$y`: supported, initially reset; app capture had no sync-begin sequence | `ESC[?0u`; app emitted push `ESC[>3u` | `ESC[?2027;0$y`: unsupported | FAIL: “Copy requested” but empty clipboard; no OSC 52 in session capture | 120×36 and 80×24: startup, request and streamed fixed reply; narrow reply wraps |
| hueb Kitty → tmux 3.8, default isolated config | `ESC[?2026;2$y`: supported | no Kitty response within 1 s; DA `ESC[?1;2;4c`, fallback | `ESC[?2027;0$y`: unsupported | FAIL: request notice, fixture text not in clipboard | started 120×35 with status line; disabled status for 120×36, then resized to 80×24; reply and notice retained |
| hueb Ghostty | not established | not established | not established | not established | app launched, but command/probe produced no output; GUI automation unavailable |
| hueb iTerm | not established | not established | not established | not established | AppleScript window request timed out after 10 s; no successful input automation |
| wick → hueb SSH | not established in terminal | not established | not established | not established | shell hop succeeded with agent forwarding; GUI-originated SSH route failed “No route to host” |

The direct 120×36 run used macOS `script` to capture terminal bytes. The 80×24
run used the release directly to avoid confusing a script-wrapper resize with
application resize. The tmux server used a unique socket and `-f /dev/null`;
its normal keyboard fallback was not treated as Kitty enhancement support.

Ctrl+Y was sent after F6 and Up to focus a message. At 120×36 `pbpaste` returned
zero bytes after the copy-request notice. The raw capture contained neither an
OSC 52 sequence nor sync-begin, but did contain the keyboard push. Current
branch source has `App::pending_copy`/`take_copy()` but no event-loop consumer;
that is corroborating source evidence, not a patched release or a proof about
all terminal configurations. The release's failed delivery remains the result.

`System Events` automation hung; the owned process was stopped. iTerm's separate
AppleScript request also timed out. Ghostty's launch did not generate its probe
file. `screencapture` reported “could not create image from display”, so no
visual claim is made. These checks did not change accessibility, screen-recording
or local-network permissions. An initial ordinary wick→hueb SSH attempt was
refused; forwarding the existing agent and disabling the configured proxy for
the direct LAN hop made a shell `hostname` check succeed. A Kitty-originated
hueb→wick→hueb route failed with “No route to host”, so it did not exercise the
application over SSH. Shell connectivity alone does not close that matrix cell.

## Scope, cleanup and remaining boxes

No development checkout or build was created on either Mac. The Homebrew package
installation remains; it is the requested installed release. No permanent
terminal or tmux config was changed. The fixture server, isolated tmux server,
owned Kitty/Ghostty/iTerm windows/processes and temporary Mac files were removed.
No files were created on wick. Local raw terminal evidence was copied to
`/tmp/mac-gates-evidence/` on dev2 before Mac cleanup; the observations needed to
interpret it are recorded above. No credential values are included in this report.

| Milestone | State | Missing or failing evidence |
|---|---|---|
| M4.P1.L1.T3 | open | macOS five-minute spike and startup/memory results unavailable |
| M4.P4.L3.T2 | open | OSC 52 failed; Ghostty/iTerm and terminal SSH coverage incomplete; 2027 unsupported in tested Kitty |
| M4.P4.L5.T3 | open | macOS real-widget/full-FX five-minute rerun unavailable |
| M4.P4.L5.T4 | open | both-platform final passing evidence and complete M4.P5 dependency not established |
| M4.P5.L14.T3 | open | macOS real-widget rerun and release invariant results unavailable |
| M4.P5.L20.T3 | open | macOS <6 ms measurement unavailable; combined real grid/bars/tree timing not established |

Validation: new workflow YAML parsed; dispatch-only trigger and exact runner
labels checked; `git diff --check` passed. No Rust production source changed.
