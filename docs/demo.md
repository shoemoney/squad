# 🎬 Squad demo script

This file is the timed script for the canonical Squad demonstration, the walkthrough section for each new contest mode, and the rules for editing and checking the recordings. It covers M7.P2.L2.T1, T2 and T7 in `docs/prd/milestones/M7-launch.md`.

| Recording | Tape | Raw (complete) | Edited GIF |
|---|---|---|---|
| Canonical demo | `docs/demo/squad.tape` | `docs/demo/squad-raw.mp4` | `docs/demo/squad-raw.gif` |
| Census | `docs/demo/census.tape` | `docs/demo/census-raw.mp4` | `docs/demo/census-raw.gif` |
| Crosscheck | `docs/demo/crosscheck.tape` | `docs/demo/crosscheck-raw.mp4` | `docs/demo/crosscheck-raw.gif` |
| Survey | `docs/demo/survey.tape` | `docs/demo/survey-raw.mp4` | `docs/demo/survey-raw.gif` |
| Probe | `docs/demo/probe.tape` | `docs/demo/probe-raw.mp4` | `docs/demo/probe-raw.gif` |

<!-- RECORDING: canonical demo, edited GIF with time cuts (docs/demo/squad-raw.gif) -->
<!-- RECORDING: canonical demo, complete raw MP4 (docs/demo/squad-raw.mp4) -->

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

<!-- RECORDING: canonical demo, title card at 1:08 (confirm) -->

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

<!-- RECORDING: squad-raw.mp4 (complete, uncut, retained) -->

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

<!-- RECORDING: credential inspection log (see docs/release-checklist.md) -->

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

<!-- RECORDING: census raw MP4 (docs/demo/census-raw.mp4) -->
<!-- RECORDING: census edited GIF (docs/demo/census-raw.gif) -->

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

<!-- RECORDING: crosscheck raw MP4 (docs/demo/crosscheck-raw.mp4) -->
<!-- RECORDING: crosscheck edited GIF (docs/demo/crosscheck-raw.gif) -->

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

<!-- RECORDING: survey raw MP4 (docs/demo/survey-raw.mp4) -->
<!-- RECORDING: survey edited GIF (docs/demo/survey-raw.gif) -->

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

<!-- RECORDING: probe raw MP4 (docs/demo/probe-raw.mp4) -->
<!-- RECORDING: probe edited GIF (docs/demo/probe-raw.gif) -->

---

## 📤 Exports from every walkthrough

Each walkthrough ends with `/export <scope>`, which writes Markdown to the temporary directory. The README lists the scopes as `room`, `debate`, `grill`, `tournament`, `census`, `crosscheck`, `survey` and `probe`.

Two things to confirm when the first recording runs. The README's export table says the contest ID is needed for these scopes, while the command form shows the ID in brackets. If `/export census` (or the others) asks for an ID, or refuses without one, use the contest ID shown in that run's report header, and note the change in this file.

---

## 🧷 Publication checklist

- [ ] Raw MP4 kept, complete and uncut.
- [ ] Edited GIF has a label on every cut, and each cut is disclosed next to its link.
- [ ] Credential inspection passed for each recording (counts only, no matches printed).
- [ ] `squad.tape` fixes applied (`/set ROUNDS 1`, export `Enter`, export directory).
- [ ] Links from `README.md` replace the `RECORDING` placeholders above.
