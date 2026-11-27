# $trace raw recording (full length, uncut). Record with: docs/demo/new/record-all.sh <squad-binary>
# Runs the v0.2.0 release binary (`squad` on PATH). HOME, SQUAD_DATA_DIR and DEMO_WORK are
# private temp dirs made by record-all.sh; the API key comes from the environment and is never typed.
# Numbered sources S1.., checked claims, a brief whose paragraphs cite quoted spans. About 100 s live (unmeasured).

Output docs/media/full/trace.gif
Output docs/media/full/trace.mp4

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

# 2. Question, then each source after a standalone -- line
Type "@all $trace What retention obligations do these sources establish?"
Ctrl+J
Type "--"
Ctrl+J
Type "The service retains records for thirty days. Backups expire after seven days."
Ctrl+J
Type "--"
Ctrl+J
Type "The backup policy applies only to production accounts."
Sleep 800ms
Escape
Sleep 300ms
Enter
Wait+Screen@180s /Confirm/
Sleep 1s
# Preflight card: confirm
Type "y"
# Fixed wait: the report can scroll the status header off screen, so no text marker is reliable
Sleep 120s
# 3. Hold on the audited brief and its source spans
Sleep 10s

# Done: leave the room
Ctrl+C
Sleep 1s
