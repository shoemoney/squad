# Integration wave 6 operational gate (dev3, 2026-11-25)

Base: `43c39b455171422bb1b019150f67cbcdf4a20c46` (includes `348ea93`).
All Cargo commands use `CARGO_BUILD_JOBS=8`, loopback provider fixtures, and no real `.env`.

## Root cause

The old shutdown fix does not cure the operational stall. Initial attempts 1–3
passed (23.781 seconds including compilation, 1.869 seconds, 1.674 seconds).
Attempt 4 timed out at 300.034 seconds (exit 124), deadlocked with three tests unfinished: `exhausted_member_does_not_block_other_member_or_persist_provisional_text`,
`provider_retries_replace_partial_output_and_reconcile_each_paid_attempt`, and
`targeted_stop_preserves_other_inflight_work_and_blocks_late_result`.

The native thread dump proves a SQLite mutex lock cycle. Thread 1916380
(targeted-stop, opening a post-shutdown scalar connection) owns the global VFS
mutex at `staticMutexes.122+432` and waits for the inode mutex at
`0x7fed90013468` inside `findReusableFd`. Thread 1916392 (asynchronous writer
close) owns that inode mutex and waits for the global mutex in
`unixIsSharingShmNode`, called by `sqlite3WalClose`. Other tests block behind
the global mutex or their store job replies. The application-level async
timeouts cannot interrupt these synchronous native mutex waits.

`rusqlite 0.38.0` bundles SQLite 3.51.1 through `libsqlite3-sys 0.36.0`.
The [SQLite 3.51.2 release notes](https://sqlite.org/releaselog/3_51_2.html)
identify the upstream deadlock fix; the [upstream report](https://sqlite.org/forum/info/7497f8f6052ed70a763672edffb414ec6c6834037cf2d7d4e5b486284fcc6b7a)
also documents a reachable open/close cycle. Upgrade the three rusqlite consumers
to 0.39.0, selecting `libsqlite3-sys 0.37.0` and SQLite 3.51.3.

The regression test checks the fixed engine version and runs 1,600 concurrent
WAL opens/reads/closes across eight threads. It runs in a child process with a
20-second failure deadline so a native deadlock cannot strand the test runner.
The old dependency fails the version check. With that guard temporarily disabled,
the old engine also deadlocks in the concurrency probe and is killed at 20.01 seconds.
The upgraded engine passes in 0.08 seconds.
No test was newly ignored and no existing timeout was increased.

## Validation

Ten consecutive executions of `timeout 300 cargo test -p squad-core --test m6_operational` all passed after the dependency fix. Every run reports 7 passed, 0 failed, 1 pre-existing ignored reservation blocker. Durations below include Cargo startup.

| Run | Result | Seconds |
|---|---|---|
| 1 | PASS | 2.448 |
| 2 | PASS | 1.916 |
| 3 | PASS | 1.524 |
| 4 | PASS | 1.391 |
| 5 | PASS | 2.135 |
| 6 | PASS | 2.315 |
| 7 | PASS | 2.414 |
| 8 | PASS | 2.039 |
| 9 | PASS | 2.671 |
| 10 | PASS | 2.743 |


Full gate: `CARGO_BUILD_JOBS=8 cargo test --workspace --no-fail-fast` passed
(exit 0, 155 seconds): **912 passed, 0 failed, 2 pre-existing ignored**.
The raw log also contains the nested WAL subprocess success line; it is not
an additional Cargo test. `cargo fmt --all --check` and `git diff --check` passed.

`CARGO_BUILD_JOBS=8 cargo clippy --workspace --all-targets -- -D warnings` passed (exit 0).

## Color and effects refinement

Two design review rounds followed the operational gates. The first identified
raw RGB logo-sweep samples leaking into indexed-color terminals, equally
saturated idle/live roster labels, and an unaccented focused input divider.
The changes map startup sweep signatures to terminal capability, dim idle and
offline status, emphasize active/error status, and use the established gold
accent on the focused input divider. Prose retains the terminal foreground.
No animation passes or scheduling work were added. The second review checked
fresh full-color local demo captures and signed off without further findings.
A 900-ms indexed-logo frame regression verifies no RGB cells leak through.
The focused chrome, participant and composition checks passed (12 tests).

Final gates after the design refinements: workspace tests **913 passed,
0 failed, 2 pre-existing ignored** (exit 0); all-target Clippy with warnings
denied, formatting and diff whitespace checks all passed. The additional test
is the indexed-logo frame regression. The initial compile produced unused-import
warnings; those imports were removed and final Clippy verifies the warning-free
source.

Local demo terminal captures: [startup](startup.png), [six-member streaming](streaming.png),
and [opening with the focused gold input divider](opening-focused.png).
These use `FakeRoom`, not live provider traffic.
