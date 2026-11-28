# $tune raw recording (full length, uncut). Record with: docs/demo/new/record-all.sh <squad-binary>
# Runs the v0.2.1 release binary (`squad` on PATH). HOME, SQUAD_DATA_DIR and DEMO_WORK are
# private temp dirs made by record-all.sh; the API key comes from the environment and is never typed.
# Hidden cases, mutated prompts, held-out winner. One generation keeps it short. About 150 to 300 s live.

Output docs/media/full/tune.mp4

Set Shell "bash"
Set FontSize 14
Set Width 1400
Set Height 860
Set Padding 12
Set Theme "Catppuccin Mocha"
Set TypingSpeed 45ms
Set WindowBar Colorful

Hide
Type `unset NO_COLOR; export PS1='$ ' HOME="$DEMO_HOME" SQUAD_DAILY_MAX_USD=150 SQUAD_DEBATE_CONFIRM_USD=0 && cd "$DEMO_WORK" && clear`
Enter
Sleep 1s
Show

# 1. Open the room
Type "squad"
Sleep 600ms
Enter
Wait+Screen@180s /Ask the squad/
Sleep 2s

# 2. One generation; original prompt, standalone --, then the goal (Ctrl+J = newline, Enter sends)
Type '$TUNE_GENERATIONS=1 @all $tune Answer clearly and concisely.'
Ctrl+J
Type "--"
Ctrl+J
Type "Answer support questions accurately, state uncertainty, and avoid invented facts."
Sleep 800ms
Escape
Sleep 300ms
Enter
Wait+Screen@180s /Confirm/
Sleep 1s
# Preflight card: confirm
Type "y"
# Fixed wait: the report can scroll the status header off screen, so no text marker is reliable
Sleep 300s

# Done: leave the room. The room can stay on "awaiting ruling" after the contest is COMPLETE, so the
# clip continues OFFLINE with `squad export` on the same finished database (record-all.sh labels it).
Ctrl+C
Sleep 1s
Hide
Type `ID=$(python3 -I -c "import sqlite3,os;print(sqlite3.connect(os.path.expanduser('~/.local/share/squad/squad.db')).execute('select contest_id from contests').fetchone()[0])") && export PS1='$ ' && clear`
Enter
Sleep 1s
Show
Type "squad export tune $ID -o tune-report.md"
Sleep 500ms
Enter
Sleep 2s
Type "head -34 tune-report.md | cut -c1-150"
Sleep 500ms
Enter
Sleep 9s
