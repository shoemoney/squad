# $debug raw recording (full length, uncut). Record with: docs/demo/new/record-all.sh <squad-binary>
# Runs the v0.2.0 release binary (`squad` on PATH). HOME, SQUAD_DATA_DIR and DEMO_WORK are
# private temp dirs made by record-all.sh; the API key comes from the environment and is never typed.
# Differential of hypotheses, a safe discriminating test, you run it and /answer. Human step. One round. About 150 s to the test.

Output docs/media/full/debug.gif
Output docs/media/full/debug.mp4

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

# 2. One round; symptom, standalone --, then constraints (Ctrl+J = newline, Enter sends)
Type "/set DEBUG_ROUNDS 1"
Sleep 500ms
Enter
Sleep 1500ms
Type "@all $debug Requests time out after a deploy. Health checks pass. Rolling back fixes it."
Ctrl+J
Type "--"
Ctrl+J
Type "Read-only checks only; production restarts are unavailable."
Sleep 800ms
Escape
Sleep 300ms
Enter
Wait+Screen@180s /Confirm/
Sleep 1s
# Preflight card: confirm
Type "y"
# 3. Human step: the room shows the test and labels O1, O2...; report an outcome.
#    Pick a label that exists in the first raw take (O2 assumes at least two outcomes).
Wait+Screen@1200s /RUN THE TEST/
Sleep 8s
Type "/answer O2 The lookup returned the old address; the resolver still points at the retired host."
Sleep 800ms
Enter
Sleep 2s
Type "/continue"
Sleep 500ms
Enter
Wait+Screen@1200s /Complete ·/
Sleep 10s

# Done: leave the room
Ctrl+C
Sleep 1s
