# 🎬 Squad demo script

This file is the timed script for the canonical Squad demonstration, the walkthrough section for each new contest mode, and the rules for editing and checking the recordings. It covers M7.P2.L2.T1, T2 and T7 in `docs/prd/milestones/M7-launch.md`.

| Recording | Tape | GIF in `docs/media/` | Complete raw MP4 (outside git) |
|---|---|---|---|
| Room, streaming | `docs/demo/free/room.tape` | `room-streaming.gif` | `~/squad-demo-raw/free-room.mp4` (103 s) |
| Debate rounds | `docs/demo/free/room.tape` | `debate-rounds.gif` | same recording |
| Verdict card | `docs/demo/free/room.tape` | `verdict.gif` | same recording |
| Panels (F7, F8) | `docs/demo/free/panels.tape` | `panels-f7-f8.gif` | `~/squad-demo-raw/free-panels.mp4` (80 s) |
| Narrow 80x24 | `docs/demo/free/narrow.tape` | `narrow-80x24.gif` | `~/squad-demo-raw/free-narrow.mp4` (66 s) |
| Effects reel | `docs/demo/free/effects.tape` | `effects-reel.gif` | `~/squad-demo-raw/free-effects.mp4` (29 s) |
| Tournament (live) | `docs/demo/tournament.tape` | `tournament.gif` | `~/squad-demo-raw/live-tournament.mp4` (164 s) |
| Grill (live) | `docs/demo/grill.tape` | `grill.gif` | `~/squad-demo-raw/live-grill.mp4` (169 s) |
| Census (live) | `docs/demo/census.tape` | `census.gif` | `~/squad-demo-raw/live-census.mp4` (161 s) |
| Crosscheck (live) | `docs/demo/crosscheck.tape` | `crosscheck.gif` | `~/squad-demo-raw/live-crosscheck.mp4` (206 s) |
| Survey (live v0.1.2) | `docs/demo/survey.tape` + `survey-report.tape` | [survey.gif](media/survey.gif) | `~/squad-demo-raw/audit-survey-v0.1.2.mp4` + report continuation |
| Probe (live v0.1.2) | `docs/demo/probe.tape` + `probe-report.tape` | [probe.gif](media/probe.gif) | `~/squad-demo-raw/audit-probe-v0.1.2.mp4` + report continuation |

## 🏠 Hero demo (README)

The README hero is `docs/media/squad-demo.gif`, recorded live on 2026-11-26 from the **v0.1.2 release binary** with the default six-model roster and `SQUAD_DAILY_MAX_USD=150`. Tape: `docs/demo/squad.tape`. Complete uncut recording: `docs/demo/squad-raw.mp4` (210 s, in git). Live spend for the final take: about $0.25 (the spend bar reads $0.24); three takes in all, about $0.6.

Script: the instant startup checklist ticks in, Tab on an empty line opens the starter menu, `@` + Tab lists the members and `@all` is picked, a one-sentence question streams from all six, `$` + Tab shows the contest modes, `/set ROUNDS 1`, then `@grok @gemini $debate ... @judge`, the preflight card is confirmed with `y`, and the verdict card ends the clip with the idle TIP line showing.

The GIF is 46 s at 1400 px. It is built by `docs/demo/tools/edit_hero.py`, and every speed-up carries an on-screen label:

| Raw time | GIF treatment | Label |
|---|---|---|
| 0:00 to 0:03.5 | real time (checklist) | none |
| 0:03.5 to 0:06.5 | x3 | `typing sped up x3` |
| 0:06.5 to 0:09.6 | real time (Tab starter menu) | none |
| 0:09.6 to 0:18.5 | x2 (`@` menu, `@all`, question typed) | `typing sped up x2` |
| 0:18.5 to 0:29 | real time (six models stream) | none |
| 0:29 to 0:53 | x12 | `TIME CUT - 24 s of model wait sped up x12` |
| 0:53 to 1:06 | x2.5 (`$` menu, `/set`, debate typed) | `typing sped up x2.5` |
| 1:06 to 1:09.5 | real time (preflight card) | none |
| 1:09.5 to 2:19 | x16 (debate and judging) | `TIME CUT - 70 s of model wait sped up x16` |
| 2:19 to 2:28 | real time (verdict card and TIP line) | none |

Rebuild: `python3 docs/demo/tools/edit_hero.py docs/demo/squad-raw.mp4 docs/media/squad-demo.gif 1400 10 0.3:3.5:1 3.5:6.5:3 6.5:9.6:1 9.6:18.5:2 18.5:29:1 29:53:12 53:66:2.5 66:69.5:1 69.5:139:16 139:148:1`.

The tape needs one typeahead workaround: `Ctrl+A` then Backspace clears a draft (Ctrl+U does nothing). Enter now sends a fully typed command even with the suggestion list open, so the older Escape-before-Enter step is no longer required.

---

## Survey and probe follow-up (2026-11-26, v0.1.2)

Both runs use the published **v0.1.2 Linux x64 archive**, verified against its release SHA-256 file, and the **default six-model roster** with unchanged member reasoning settings:

| Nick | Resolved model |
|---|---|
| claude | `anthropic/claude-opus-5.5` |
| fable | `anthropic/claude-fable-5.2` |
| openai | `openai/gpt-6.2-sol` |
| gemini | `google/gemini-4-neon` |
| grok | `x-ai/grok-4.7` |
| muse | `meta/muse-wonder` |
| judge (separate case) | `anthropic/claude-fable-5.2` |

The source key is loaded off-screen from a private mode-0600 file. No real `.env` is read into a recording or committed. A $150 daily admission ceiling permits the default roster's conservative reservations; it is **not actual spend** and is not a provider-side hard cap. The owner authorized these small live calls.

| Run | Mode calls | Recorded actual spend | Final outstanding reservation | Outcome |
|---|---:|---:|---:|---|
| Survey | 20 | $1.391816 | $0 | COMPLETE; all required participation accepted |
| Probe | 10 | $0.591611 | $0 | Complete; decision tree and prediction matrix committed |

Call counts exclude four startup alias probes per run; the spend figures include those probes.

Final published runs total **$1.983427**. Including two discarded captures, the recorded total is **$3.902313**; the discarded probe also retains an unresolved started call/reservation, so its unknown charge is not presented as zero. The first captures failed when VHS frame storage hit the `/tmp` quota. The successful captures use disk-backed `TMPDIR` and request 10 fps. Complete final raw MP4s are retained outside git. The failed frame captures were discarded; their room databases remain private.

**What is shown:** the survey collects six independent answers, extracts claims, accepts first ballots, presents anonymous reasons, revotes selected claims, and commits the claim map. The probe splits six members into three explainers and three checkers, collects blind proposals, predicts outcomes, and commits a tree and matrix. These checks are proposals for a human to execute; no checks were executed and no explanation was declared true. Survey agreement and vote changes are not proof, and shared model blind spots remain.

**UI limitation retained:** v0.1.2 leaves the survey TUI on “awaiting ruling” even after the report is durably complete; the spend bar can retain a stale reservation. The probe's expanded transcript also exceeds one viewport. Each GIF therefore appends an explicitly labelled **offline export continuation from the same completed database**. This shows the real report, limitations and successful export without another provider call. It does not imply that the live survey card refreshed. The exported survey includes three unresolved classifications, which remain visible in its limitations.

`survey-report.tape` and `probe-report.tape` take `DEMO_DATA` and `DEMO_CONTEST` from the corresponding recorded room, in addition to the private `DEMO_ENV`. They use `squad export <mode> <contest-id> -o <file>` and display excerpts from that actual export.

Time cuts use raw-video positions; no wait is silently deleted. `docs/demo/tools/edit_followup.py` keeps the first 35 seconds and last 14 seconds in real time, speeds the intervening model wait up **20×** with a permanent on-screen label, then appends the offline continuation in real time with its own label. Full raw MP4s remain unchanged.
| GIF | Main raw duration | Sped-up raw interval | Offline continuation | Encoded length | Size |
|---|---:|---|---:|---:|---:|
| `survey.gif` | 356.04 s | 35.00–342.04 s at 20× | 46.64 s | 111.010 s | 5,481,468 bytes |
| `probe.gif` | 336.36 s | 35.00–322.36 s at 20× | 24.04 s | 87.380 s | 1,727,498 bytes |

**Credential inspection:** 1,125 repository/capture/raw-media/database files were compared in-process with the literal credential: zero matches. OCR of 66 GIF frames sampled every three seconds found zero literal-key, key-prefix, `sk-or` or `Bearer` matches. Raw and export contact sheets were visually inspected; setup is hidden, reports match the committed exports, and the GIFs label both the time cut and export continuation. Counts only are retained in [credential-inspection.json](reports/audit-foundations-2026-11-26/credential-inspection.json); no credential or OCR text is committed.

![Live default-roster survey with its offline claim-map export and caveats.](media/survey.gif)

![Live default-roster probe with its prediction matrix, decision tree and limitations.](media/probe.gif)

---

## 🎞️ Recorded media (2026-11-25)

### How the GIFs were made

- **Free GIFs** come from `target/release/examples/demo` (the scripted room) and `fx_reel`. VHS has no function keys, so the F7 and F8 shots run the room inside `tmux` and a hidden helper sends the keys. The 80x24 shot uses a 800x450 pixel terminal.
- **Live GIFs** come from the **v0.1.0 release binary** (`squad-cli-x86_64-unknown-linux-gnu.tar.xz` from the GitHub Release). Each tape uses a fresh data directory, the API key is `source`d from a 0600 file inside the tape's `Hide` block, and the room's own tool output never shows it.
- **Editing:** `docs/demo/tools/edit_live.py` and `mkgif.sh` trim each raw MP4, speed up the waits, burn the label onto the sped-up part, and encode with `ffmpeg` and `gifsicle -O3 --lossy=60 --colors 128`.
- **Size:** all ten GIFs together are 11 MB; the largest is `grill.gif` (2.2 MB).

### Time cuts (every cut is sped up, not removed, and labelled on screen)

| GIF | Real time kept | Sped up (raw seconds) | On-screen label | GIF length |
|---|---|---|---|---|
| `tournament.gif` | 0:00 to 0:16, 1:06 to 1:20 | 0:16 to 1:06 (50 s) at x8 | `TIME CUT - 50 s of model wait sped up x8` | 36 s |
| `grill.gif` | 0:00 to 0:06, 0:32 to 0:46, 1:24 to 1:36 | 0:06 to 0:32 (26 s) at x3; 0:46 to 1:24 (38 s) at x6 | `TIME CUT - 26 s ... x3` and `TIME CUT - 38 s ... x6` | 47 s |
| `census.gif` | 0:00 to 0:18, 2:02 to 2:14 | 0:18 to 2:02 (104 s) at x10 | `TIME CUT - 104 s of model wait sped up x10` | 40 s |
| `crosscheck.gif` | 0:00 to 0:30, 3:10 to 3:24 | 0:30 to 3:10 (160 s) at x16 | `TIME CUT - 160 s of model wait sped up x16` | 54 s |
| the six free GIFs | no cuts: real time | none | none | 14 to 26 s |

The sped-up parts are model waiting (streaming replies, judging, ledger work). The results (champion and placings, final plan, report) are shown in real time.

### Live spend (budget: $4.00 for all live recordings)

Every figure is from `squad doctor` ("actual") and the room's `spend_daily` table (micro-dollars). The reservation estimates (the worst-case "needs up to") are not spend.

| Recording | Final run, actual | Doctor line |
|---|---|---|
| Tournament | $0.0091 | `actual $0.01` |
| Grill | $0.0078 | `actual $0.01` |
| Census | $0.0446 | `actual $0.04` |
| Crosscheck | $0.0789 | `actual $0.08`, 1 unknown-cost call |
| Survey (failed, 348 s) | $0.1130 | `actual $0.11` |
| Probe (failed, 189 s) | $0.0425 | `actual $0.04` |

**Total across every attempt, including all the discarded trial and failed runs: about $0.72 recorded in the kept data directories, plus roughly $0.10 for one survey run (420 s) whose recording was lost to a full disk and whose data directory was deleted. Well under $4.00.** The roster used the cheapest models that finished the work (below), so the cost is not what the default roster would cost.

### Models used (not the defaults)

The default roster reserves far more than the $1.50 cap allows: a census with three members on the defaults needs up to **$9.97** reserved, so it is refused. The live tapes set concrete low-cost models with `SQUAD_MODEL_<NICK>` and a low reasoning effort.

| Recording | Members | Judge |
|---|---|---|
| Tournament, Grill | `luna`=`openai/gpt-6-luna`, `nano`=`openai/gpt-5-nano`, `mini`=`openai/gpt-5-mini` (tournament's third entrant `mini`; grill leader `luna`) | `openai/gpt-5-mini` |
| Census | `luna`=`openai/gpt-6-luna`, `grok`=`x-ai/grok-4.20`, `mini`=`openai/gpt-5-mini` | `openai/gpt-6-luna` |
| Crosscheck | `luna`, `grok`, `mini` as above | `x-ai/grok-4.20` |

Nicks are the model names on screen, not the default nicks. Daily caps used: **$1.50** for tournament, grill, census and crosscheck (as asked). The survey tape needed **$4** and the probe tape **$3**, because their worst-case reservations (survey $3.51, probe $1.85, on the cheap roster with a `grok` judge) are above $1.50. Real spend stayed in cents. This is the one place the $1.50 instruction was exceeded.

### ❌ Modes that failed live (v0.1.0, release binary)

These are not faked and have no GIF.

| Mode | What happened | Evidence |
|---|---|---|
| **Survey** | Stuck at "Judging, @judge awaiting ruling" for the whole 330 s wait (9 calls completed, no report). With a `deepseek/deepseek-v4-flash` judge it ended with `Survey output failed validation: invalid JSON object`; with a `openai/gpt-5-mini` judge, `PROVIDER_ERROR (HTTP 400)` | `~/squad-demo-raw/live-survey.mp4` |
| **Probe** | Ends **Failed**, with `Skipped work: <id>` and the explainers' proposals shown but no decision tree or matrix. Tried four rosters (Anthropic and Gemini members, then OpenAI nano and mini members, then `grok` and `gpt-5.6-luna` checkers) with the same result. With `gpt-5-mini` or `gpt-4o-mini` in the roster the calls came back `unknown` or `HTTP 400` | `~/squad-demo-raw/live-probe.mp4` |
| **Census (earlier rosters)** | `PROVIDER_ERROR (HTTP 400)` on Anthropic (`claude-haiku-5.5`, `claude-haiku-4.5`) and Gemini (`gemini-3.1-flash-lite`, `gemini-3.5-flash-lite`) member calls, so the pass ended `Failed` with `invalid_step`. The recorded census uses OpenAI and xAI members and finished, but still shows `Estimate unavailable` and one `Invalid census response` | trial runs kept under `/tmp/squad-demo/failed/` |
| **Crosscheck (earlier rosters)** | `PROVIDER_ERROR (HTTP 400)` or `Crosscheck response failed validation` with `gpt-6-luna` as judge, `nano` members or a Haiku judge. Finished with a `grok-4.20` judge | trial runs kept under `/tmp/squad-demo/failed/` |

Fixing the model roster or the judge changed the result, so these look like structured-output incompatibilities with some models rather than one bad prompt. Not investigated further. The grill and tournament runs had no failed attempts.

Two other things seen on camera: the first Enter after `/export` opens a path prompt that needs a second Enter (the tapes now send it), and with the default roster the room takes more than 3 s to start, so the first typed command can be lost.

### 🔎 Credential inspection (2026-11-25)

- **Text captures:** every `Output x.txt` from every live run (the 6 final runs and every discarded trial run: **36 files**), the OCR of 119 frames sampled every 3 s from all ten GIFs, and every tracked text file in the repository (972 files). The 10 GIFs were also searched byte-for-byte for the key.
- **Searches:** `sk-or` and `Bearer` (case-insensitive) found **0** matches in the captures and OCR, and only the existing grep recipe and test fixtures in the repository (no real key). The literal key value, compared in-process, found **0** matches everywhere, including inside the GIF files, and so did its first 8 characters in the OCR text. Counts only; no match text was printed.
- **Frames:** contact sheets of every live recording and sample frames from each GIF were looked at. The room shows no key and no environment; the hidden setup line is never on screen. Once the room exits, the shell prompt returns (the end of the raw MP4s). That part is not in any GIF. One earlier tmux tape showed a job number after the room exited, in a raw MP4 only.
- **Result: pass.** Published: the GIFs and tapes. Not published: the raw MP4s and text captures, which stay outside git.


---

## ⚙️ Recording rules

Every tape follows the same conventions as `squad.tape`.

- **Release binary only.** The tapes run `squad` from the published release archive on `PATH`. They never run a development build.
- **Key from the environment, never typed.** The first hidden line runs `source "$DEMO_ENV"`. The OpenRouter key is never typed, echoed or shown.
- **Temporary data directory.** `SQUAD_DATA_DIR="$(mktemp -d)"` keeps each recording's room, ledger and logs out of your own data directory.
- **Daily cap.** `SQUAD_DAILY_MAX_USD=5` caps spending for a whole recording. The README says the cap is read at start, so set it before `squad` opens.
- **Confirmation threshold 0.** `SQUAD_DEBATE_CONFIRM_USD=0` makes every contest with a non-zero estimate wait for `y` or `n`. The walkthroughs rely on this.
- **Export directory.** The walkthrough tapes `cd` into a temporary directory in the hidden setup line, so `squad-export.md` does not land in the repository.
- **Hide and Show.** Setup is hidden. Only the room is on screen.
- **Sleeps are budgets.** A `Sleep` is the time allowed for the model work. If a contest is still running when the tape moves on, the raw recording shows it, and the sleep must be lengthened before re-recording.

### 🔧 Fix before the next recording

`squad.tape` has two lines that do not match the README. The walkthrough tapes already use the README form.

| In `squad.tape` | README form | Why it matters |
|---|---|---|
| `Type "/set ROUNDS=1"` | `/set ROUNDS 1` (the README's `/set NAME value`) | The README lists `=`-free syntax only. Use the README form so the stored value really changes. |
| `Type "/export debate"` then `Enter` then `Sleep 4s` | The path prompt is pre-filled with `squad-export.md` and needs accepting | Without a second `Enter`, the export prompt stays open and the file is never written. |

Also set `cd "$(mktemp -d)"` in the hidden setup line, as the walkthroughs do, so the export does not land in the repository.

---

## 🎥 Canonical timed script (`squad.tape`)

Approximate times are for the raw recording. Typing time is 35 ms per character, so the figures are rough.

| Step | Time (approx.) | What is typed | Expected screen | Notes |
|---|---|---|---|---|
| 0 · setup | hidden | `export SQUAD_DATA_DIR=… SQUAD_DAILY_MAX_USD=5 SQUAD_DEBATE_CONFIRM_USD=0 && source "$DEMO_ENV" && clear` | Nothing. The shell is clear when `Show` starts. | Key comes from `$DEMO_ENV`. Never typed. |
| 1 · open room | 0:00 to 0:06 | `squad`, then Enter | The room opens with the roster and the spend bar at zero. | Waits 3 s for the room to settle. |
| 2 · six at once | 0:06 to 0:55 | `@all In one sentence: what's the most underrated Unix command, and why?`, then Enter | Six nicks stream replies at the same time. Each reply is labelled with its nick. | 45 s budget. Cut in the GIF if the streams finish early. |
| 3a · set rounds | 0:55 to 1:00 | `/set ROUNDS 1` (fix from `ROUNDS=1`) | Stored value changes. | See the fix above. |
| 3b · direct debate | 1:00 to 1:08 | `@grok @gemini $debate Which of your two picks would save a new developer more time? @judge`, then Enter | Cost preview appears: `Y Confirm · N Cancel`. | Direct debate: exactly two members. `@judge` asks for a verdict. |
| 3c · confirm | 1:08 | `y` | The debate starts. Round by round, each debater takes a turn. Judge scores appear per round. | Conditional. See below. |
| 3d · debate runs | 1:08 to 3:08 | nothing | Turns stream, then the judge's verdict card. | 120 s budget. Time-cut in the GIF. |
| 4a · spend bar | 3:08 to 3:14 | nothing | The spend bar reads `spent / cap · pending · remaining` against $5. | Shown for 6 s. |
| 4b · export | 3:14 to 3:20 | `/export debate`, then Enter | The export path prompt, pre-filled with `squad-export.md`. | Accept with Enter (see fix). |
| 5 · close | ends | `Ctrl+C` | The shell returns. | The room exits cleanly. |


### ❓ Conditional confirmation steps

The cost preview appears in step 3b when the debate's estimate is above `SQUAD_DEBATE_CONFIRM_USD`. With the tape's value of `0`, almost every contest waits for you.

| What appears | What to send | What happens next |
|---|---|---|
| `Y Confirm · N Cancel` | `y` | The debate starts. The reservation is held until each call finishes. |
| `Y Confirm · N Cancel` | `n` | The debate is cancelled. Nothing is admitted or generated. Do not export a debate that never ran. Go to `Ctrl+C`. |
| No card appears (estimate is zero, or the threshold is higher than the estimate) | Send nothing | The debate starts at once. Delete the `y` line from the tape before re-recording, or the stray `y` is typed into the input box. |
| A stray `y` already went into the input box | `Ctrl+A` to select the input, then type over it or delete it | The input is cleared. README: `Ctrl+A` selects all input. |
| `DAILY_MAX_REACHED` (`Daily max of $X reached…`) | Nothing | The preflight is refused. Raise `SQUAD_DAILY_MAX_USD`, restart `squad`, and re-record. |
| `Debate plan expired` | Nothing | The card lasted more than 10 minutes. Re-type the command. |

`SQUAD_DEBATE_CONFIRM_USD` decides which case you get.

- **Set to `0` (tape default):** every non-zero debate waits for `y`.
- **Set to `2` (README default):** cheap debates auto-confirm. Remove the `y` line from the tape, or the `y` is typed into the room.

The same rules hold for every contest walkthrough below. Their cards always wait for `y` under the tapes' `0` threshold.

---

## ✂️ Time cuts and the raw recording

Model waits are long. The edited GIF is for reading. The raw MP4 is the complete record.

1. **Keep the raw.** The VHS output `*-raw.mp4` is the complete, uncut recording, with every wait. It is never edited, trimmed or replaced. Keep it in private storage even after publishing the GIF.
2. **Make the GIF from the raw.** Cut the waits in a copy, not in the raw file. Use `ffmpeg` with trim filters on a copy, then encode the result to `*-raw.gif` or a separate `*.gif` name. The published GIF is the only edited output.
3. **Label every cut on screen.** Each cut gets a title card of about 2 seconds, burned into the GIF at the join. Use this wording:

   ```text
   ⏩ TIME CUT: 2 min 00 s of model wait removed
   ```

   The card states the real length removed. Do not shorten the label, and do not cut without one.
4. **Disclose the cuts next to the link.** Wherever the GIF is linked, add a line saying that the wait was cut and that the complete recording is the MP4.
5. **No cut inside a step's result.** Do not cut through a verdict, report, export or confirm card. Cut only through the waiting.

The complete, uncut recordings are kept in `~/squad-demo-raw/` (outside git). See "Recorded media" above for each file and its length.

---

## 🔎 Credential inspection

Run this before anything is published. The check covers the GIF, the complete MP4, any plain-text or `.cast` capture and any exported Markdown. It is run on the machine that holds the recording, and nothing from `$DEMO_ENV` is printed.

### 1. Before recording

Load the key into a shell variable without printing it. The value is never written to the terminal or to a file.

```bash
KEY="$(sed -n 's/^OPENROUTER_API_KEY=//p' "$DEMO_ENV")"
test -n "$KEY" && echo "key loaded (length ${#KEY})"
```

Only the length is printed.

### 2. After recording: collect every text source

Use a new empty working directory. Do not reuse one from an earlier run.

```bash
WORK="$(mktemp -d)"
mkdir "$WORK/frames"
ffmpeg -loglevel error -i docs/demo/squad-raw.mp4 -vf fps=2 "$WORK/frames/%05d.png"
```

Repeat for each raw MP4, and for each text capture if the run produced one (`.txt`, `.ascii`, or a `.cast`). Copy the exported Markdown into `$WORK` too.

### 3. Turn frames into text

Use OCR on each frame. OCR can miss characters, so this check is necessary but not sufficient.

```bash
for f in "$WORK"/frames/*.png; do tesseract "$f" stdout 2>/dev/null; done > "$WORK/ocr.txt"
```

### 4. Search for credentials

Search for the patterns, and for the literal key value. Print counts and file names only. Do not print matches, or the match leaks the key.

```bash
grep -c -i -E 'sk-or|Bearer |OPENROUTER_API_KEY' "$WORK/ocr.txt"
grep -q -F -- "$KEY" "$WORK/ocr.txt" && echo "LITERAL KEY FOUND" || echo "literal key not found"
```

Run the same two checks on every text capture and on the exported Markdown.

### 5. Decide

| Result | Action |
|---|---|
| Every count is `0` and the literal key is not found | Pass. The GIF may be published with its disclosure line. |
| Any hit, in any file | Fail. Do not publish the GIF or the MP4. Delete the working copy, rotate the key, check the room data directory is removed, and re-record. |

Record the check in the release checklist (`docs/release-checklist.md`, M7.P2.L2.T7): the date, which files were checked, and the counts. Never record the key or any match text.

The inspection log for the recorded media is in "Credential inspection (2026-11-25)" above.

---

## 🧪 Walkthrough: Census (`census.tape`)

**What it shows:** a blind hunt for omissions in a pasted deploy plan. Members hunt, the judge merges, members rate anonymously, and the report estimates what is still unfound. One pass only, so no patch step.

**Run:** `vhs docs/demo/census.tape`

| Step | Time (approx.) | What is typed | Expected screen |
|---|---|---|---|
| 1 · open | 0:00 to 0:08 | `squad`, Enter | Room opens. |
| 2 · one pass | 0:08 to 0:12 | `/set CENSUS_PASSES 1` | Stored value changes. README form is `/set NAME value` with a space. |
| 3 · plan and command | 0:12 to 0:25 | `@all $census Deploy plan for v2.4.0: …`, Enter, then `y` at the card | Preflight card: `Y Confirm · N Cancel`. After `y`, the hunt starts. |
| 4 · model wait | 0:25 to 2:55 | nothing | Hunts, ledger, ratings, judge finalise. Time-cut in the GIF (150 s budget). |
| 5 · report | 2:55 to 3:01 | nothing | Census report: pass, ledger, ratings, dispositions, finder counts, estimate, caveat. |
| 6 · export | 3:01 to 3:10 | `/export census`, Enter (accepts `squad-export.md`) | Export written to the temporary directory. |

Expected report lines (README wording): `Found {n}; estimated {lo} to {hi} still unfound.` and the caveat `Models share blind spots; the true number still unfound is at least this estimate.` The quiet line `Census is quiet. Good place to stop.` appears only when its conditions hold, so do not expect it from one pass.

**Conditional:** if the card is not shown, send nothing and delete the `y` line before re-recording. A refused start with `DAILY_MAX_REACHED` means the cap is too small for six members at this plan length.

**Recorded:** [`docs/media/census.gif`](media/census.gif), complete raw `~/squad-demo-raw/live-census.mp4` (161 s). One pass, three members on low-cost models. The recorded report shows `Estimate unavailable` and `Invalid census response`, so the estimate line was not shown. The 104 s of waiting is sped up x10 and labelled.

---

## ✅ Walkthrough: Crosscheck (`crosscheck.tape`)

**What it shows:** a plan-blind inventory, cell-by-cell reviews with no self-review, two fresh auditors, and a judge ruling. The status line says whether the run is bounded, which is the point of the caveat.

**Run:** `vhs docs/demo/crosscheck.tape`

The goal goes after a `--` line, so there is no extraction call. Newlines are inserted with `Ctrl+J`, and `Enter` sends.

| Step | Time (approx.) | What is typed | Expected screen |
|---|---|---|---|
| 1 · open | 0:00 to 0:08 | `squad`, Enter | Room opens. |
| 2 · plan | 0:08 to 0:20 | `@claude @openai @gemini $crosscheck Deploy plan for v2.4.0: …`, `Ctrl+J`, `--`, `Ctrl+J`, goal text, Enter | The plan is read as the text before `--`; the goal is the text after. |
| 3 · confirm | 0:20 to 0:25 | `y` at the card | Preflight card: `Y Confirm · N Cancel`. After `y`, inventories start. |
| 4 · model wait | 0:25 to 3:25 | nothing | Inventories, normalise, grid, reviews, two audits, ruling. Time-cut in the GIF (180 s budget). |
| 5 · report | 3:25 to 3:31 | nothing | Coverage ledger and bounded-stop status. |
| 6 · export | 3:31 to 3:40 | `/export crosscheck`, Enter | Export written to the temporary directory. |

**Stop caveats to read on screen.** `COMPLETE` means the script finished. It is not proof of completeness. Cells past the 200-cell cap are not reviewed, and `bounded_stop` stays false unless every condition holds. Say so in the voiceover or caption: a COMPLETE, unbounded crosscheck is not a clean bill of health.

**Human risk acceptance** (`/accept <cellId…> -- <reason>`) is only possible once the crosscheck is COMPLETE. It is not in this recording, because it changes the ledger.

**Recorded:** [`docs/media/crosscheck.gif`](media/crosscheck.gif), complete raw `~/squad-demo-raw/live-crosscheck.mp4` (206 s). 160 s of waiting sped up x16 and labelled. The report ends in a COMPLETE, unbounded ledger, which is the caveat this walkthrough is about.

---

## 📊 Walkthrough: Survey (`survey.tape`)

**What it shows:** members answer a product question, the judge extracts claims, members vote blind, and the report sorts the claims into bands.

**Run:** `vhs docs/demo/survey.tape`

| Step | Time (approx.) | What is typed | Expected screen |
|---|---|---|---|
| 1 · open | 0:00 to 0:08 | `squad`, Enter | Room opens. |
| 2 · question | 0:08 to 0:18 | `@claude @openai @gemini @grok $survey Should the first release of our offline-first notes app ship with sync conflict resolution, or ship single-device and add sync in v2?`, Enter | Four answers arrive in parallel. |
| 3 · confirm | 0:18 to 0:25 | `y` at the card | Preflight card: `Y Confirm · N Cancel`. After `y`, claims are extracted and voted on. |
| 4 · model wait | 0:25 to 3:25 | nothing | Extract, vote, select five contested claims, revote, report. Time-cut in the GIF (180 s budget). |
| 5 · report | 3:25 to 3:31 | nothing | Five bands: Consensus, Contested, Lone insights, Lone dissents, Rejected outliers. Participation summary reads `COMPLETE` or `DEGRADED`. |
| 6 · export | 3:31 to 3:40 | `/export survey`, Enter | Export written to the temporary directory. |

**Uncertainty to show.** Every vote has a confidence from 1 to 5. A contested claim has a settling question. The limitations note is at the bottom of the report. If the summary reads `DEGRADED`, say so, and read the bands with care.

**Not recorded successfully.** The live survey did not finish (stuck judging, or `invalid JSON object`); see "Modes that failed live". Raw: `~/squad-demo-raw/live-survey.mp4`. No GIF is published.

---

## 🔬 Walkthrough: Probe (`probe.tape`)

**What it shows:** a symptom becomes a short list of cheap checks. Explainers commit predictions, and the judge builds a decision tree from the locked matrix. Nothing is executed. The checks are for you to run.

**Run:** `vhs docs/demo/probe.tape`

| Step | Time (approx.) | What is typed | Expected screen |
|---|---|---|---|
| 1 · open | 0:00 to 0:08 | `squad`, Enter | Room opens. |
| 2 · brief | 0:08 to 0:20 | `@claude @fable @openai @gemini $probe API p95 latency jumped from 120 ms to 2.1 s after Tuesday's deploy while CPU stayed flat. You may use only: application logs, the Postgres slow-query log, and load balancer metrics.`, Enter | The role split is shown: two explainers and two checkers. |
| 3 · confirm | 0:20 to 0:25 | `y` at the card | Preflight card: `Y Confirm · N Cancel`. After `y`, proposals start. |
| 4 · model wait | 0:25 to 2:55 | nothing | Proposals, committed predictions, judge report. Time-cut in the GIF (150 s budget). |
| 5 · tree and matrix | 2:55 to 3:07 | `F7`, wait 6 s, `F7` | Decision tree on screen. `F7` shows the prediction matrix, then hides it. |
| 6 · export | 3:07 to 3:16 | `/export probe`, Enter | Export written to the temporary directory. |

**Say on screen:** the report states that no explanation is declared true and no check has been executed. Those words are the walkthrough's point, so do not cut them.

**Not recorded successfully.** The live probe ended `Failed` with a skipped work item and no decision tree; see "Modes that failed live". Raw: `~/squad-demo-raw/live-probe.mp4`. No GIF is published.

---

## 🏆 Walkthrough: Tournament (`tournament.tape`)

**Recorded:** [`docs/media/tournament.gif`](media/tournament.gif), complete raw `~/squad-demo-raw/live-tournament.mp4` (164 s). Three entrants (the minimum), one prompt, two rounds. The preflight card shows the worst-case reservation; blind judging eliminates one entrant per round; the report shows the champion and the final placings. 50 s of waiting sped up x8 and labelled.

---

## 🧊 Walkthrough: Grill (`grill.tape`)

**Recorded:** [`docs/media/grill.gif`](media/grill.gif), complete raw `~/squad-demo-raw/live-grill.mp4` (169 s). `/set GRILL_ROUNDS 1`, three named members and text, then the leader pick (`@luna`), then the cost card. The grill ends with a final plan and a question-and-answer appendix. Waiting sped up x3 and x6 and labelled.

Note: for the named-members form there is **no** cost card before the leader pick (the answers are collected first), so the tape sends `y` only after the pick.

---

## 📤 Exports from every walkthrough

Each walkthrough ends with `/export <scope>`, which writes Markdown to the temporary directory. The README lists the scopes as `room`, `debate`, `grill`, `tournament`, `census`, `crosscheck`, `survey` and `probe`.

Two things to confirm when the first recording runs. The README's export table says the contest ID is needed for these scopes, while the command form shows the ID in brackets. If `/export census` (or the others) asks for an ID, or refuses without one, use the contest ID shown in that run's report header, and note the change in this file.

---

## 🧷 Publication checklist

- [x] Raw MP4 kept, complete and uncut (`~/squad-demo-raw/`, outside git).
- [x] Edited GIF has a label on every cut, and each cut is disclosed next to its link (README captions).
- [x] Credential inspection passed (44 files, counts only, 0 hits).
- [ ] `squad.tape` fixes applied (`/set ROUNDS 1`, export `Enter`, export directory): the walkthrough tapes have them; the canonical live `squad.tape` was not re-recorded.
- [x] Links from `README.md` replace the `RECORDING` placeholders above.
- [ ] Survey and probe walkthroughs recorded successfully (blocked: both fail live on v0.1.0).
