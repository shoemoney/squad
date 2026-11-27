# $tune raw recording (full length, uncut). Record with: docs/demo/new/record-all.sh <squad-binary>
# Runs the v0.2.0 release binary (`squad` on PATH). HOME, SQUAD_DATA_DIR and DEMO_WORK are
# private temp dirs made by record-all.sh; the API key comes from the environment and is never typed.
# Hidden cases, mutated prompts, held-out winner. One generation keeps it short. About 150 s live.

Output docs/media/full/tune.gif
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
Type 'unset NO_COLOR; export HOME="$DEMO_HOME" SQUAD_DAILY_MAX_USD=150 SQUAD_DEBATE_CONFIRM_USD=0 && cd "$DEMO_WORK" && clear'
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
Sleep 200s
# 3. Hold on the winning prompt and its held-out scores
Sleep 10s

# Done: leave the room
Ctrl+C
Sleep 1s
