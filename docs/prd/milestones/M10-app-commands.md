# M10 — App commands

**Goal:** Give the TUI a small set of app-level commands, typed with a leading dot, that act on the terminal session itself (leave, dump, save) and never reach the shared `/` command parser or the room.

**Why dot:** `/` commands are room commands. They are parsed by `squad-protocol`, mapped to requests, accepted by `squad send`, and some of them change stored state. App commands only touch the local terminal and the local filesystem, so they get their own prefix. A `.` also cannot start a `@`, `$` or `/` token, so it cannot collide with grammar. A message that merely begins with a dot (`.5 is a number`) is still an ordinary message.

**Exit criteria:**

- `.quit`, `.exit`, `.dump` and `.save` work from the composer in the real binary.
- None of them sends a request, calls a model, or writes outside the current directory.
- Output files are named `output-<YYYYMMDD-HHMMSS>.txt` and `saved-<YYYYMMDD-HHMMSS>.md` and are never overwritten.
- The composer completes the dot commands, and the README documents all four.
- `cargo test -p squad-tui` passes and a real-binary check (tmux) proves each command end to end.

## Phase M10.P1 — Command set

- **M10.P1.L1 — Recognition**
  - [x] M10.P1.L1.T1 — A `SystemCommand` parser in `crates/squad-tui/src/app.rs` accepts exactly `.quit`, `.exit`, `.dump` and `.save` (surrounding whitespace allowed).
    - **Done when:** `/quit`, `/exit`, `quit`, `.quit now`, `.5 is a number` and `.dumpster` all parse to `None`. Evidence: `dot_commands_parse_and_slash_or_prose_do_not`.
  - [x] M10.P1.L1.T2 — `App::submit` runs a recognised command before validation, so it never builds a `Request`.
    - **Done when:** after submitting each command `app.pending` is `None` and nothing was sent to the room. Evidence: `quit_and_exit_leave_the_room_without_sending_a_request`.

- **M10.P1.L2 — Commands**
  - [x] M10.P1.L2.T1 — `.quit` and `.exit` set `app.quit`, the same exit path as Ctrl+C.
    - **Done when:** both leave the room in the unit test and in a real tmux session (the session ends).
  - [x] M10.P1.L2.T2 — `.dump` writes every loaded transcript message (author, status, source, widget rows) in order to `output-<YYYYMMDD-HHMMSS>.txt` in the current directory.
    - **Done when:** the file holds all loaded messages in order, a second dump in the same second gets a `-1` suffix instead of overwriting, the draft is cleared and a notice names the file. Evidence: `dump_writes_the_whole_buffer_to_a_timestamped_file_and_never_overwrites`, `dot_dump_via_the_composer_clears_the_draft_and_reports_the_file`.
  - [x] M10.P1.L2.T3 — `.save` writes the newest member reply (not the human's own message, not an empty card) to `saved-<YYYYMMDD-HHMMSS>.md`.
    - **Done when:** with `me`, `claude`, `grok`, `me` in the transcript the file contains exactly grok's text; with no agent output it fails with a notice and writes nothing. Evidence: `save_writes_only_the_last_agent_reply`.

## Phase M10.P2 — Discoverability

- **M10.P2.L1 — Composer**
  - [x] M10.P2.L1.T1 — Typing `.` at the start of the composer lists `.dump`, `.exit`, `.quit` and `.save` with a one-line description each, and Tab completes the selection.
    - **Done when:** a completion test types `.`, sees the four commands, narrows with `.s` to `.save`, and Tab replaces the draft with `.save `.
  - [x] M10.P2.L1.T2 — The completion list never offers a dot command anywhere but at the start of the draft.
    - **Done when:** `@all .` and `hello .` offer no dot commands.

## Phase M10.P3 — Verification and docs

- **M10.P3.L1 — Verification**
  - [x] M10.P3.L1.T1 — Prove `.dump`, `.save` and `.quit` in the real `squad` binary under tmux.
    - **Done when:** `.dump` creates one `output-*.txt` in the working directory, `.save` with no reply reports that nothing was saved and creates nothing, and `.quit` ends the session. Evidence (2026-11-27, debug build, tmux): `.save` printed `Save failed: no agent output to save yet` and wrote 0 files; `.dump` printed `Dumped the buffer to ./output-20261127-215617.txt` and wrote 1 file; `.quit` ended the session. With the completion list open, Enter accepts the highlighted item, so the smoke test presses Escape before Enter, the same as for `/` commands.
  - [x] M10.P3.L1.T2 — The whole `squad-tui` suite, `cargo clippy -p squad-tui --all-targets` and `cargo fmt --check` pass.
    - **Done when:** all three exit 0. Evidence: `cargo test -p squad-tui` exit 0 with no failures, clippy exit 0 with no warnings, `cargo fmt --check` exit 0.

- **M10.P3.L2 — Documentation**
  - [x] M10.P3.L2.T1 — Document the four commands in the README command table and `CHANGELOG.md` under Unreleased.
    - **Done when:** both files name all four commands, their file names and the dot-versus-slash rule.
  - [x] M10.P3.L2.T2 — Update `docs/prd/06-tui-ratatui.md` with the app-command contract (prefix, no request, no overwrite, file names).
    - **Done when:** the PRD section exists and its examples match the tests.
