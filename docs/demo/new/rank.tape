# $rank raw recording (full length, uncut). Record with: docs/demo/new/record-all.sh <squad-binary>
# Runs the v0.2.0 release binary (`squad` on PATH). HOME, SQUAD_DATA_DIR and DEMO_WORK are
# private temp dirs made by record-all.sh; the API key comes from the environment and is never typed.
# Pair comparisons, Bradley-Terry order, cycles and split pairs. Shortest run, about 40 s live.

Output docs/media/full/rank.gif
Output docs/media/full/rank.mp4

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
Sleep 4s

# 2. Criterion, standalone --, then one item per line
Type "@all $rank prioritize by customer impact relative to effort"
Ctrl+J
Type "--"
Ctrl+J
Type "Fix intermittent data loss"
Ctrl+J
Type "Make search tolerate spelling mistakes"
Ctrl+J
Type "Refresh the settings icons"
Ctrl+J
Type "Add dark mode"
Ctrl+J
Type "Speed up the first page load"
Sleep 800ms
Escape
Sleep 300ms
Enter
Sleep 5s
# Preflight card: confirm
Type "y"
Wait+Screen@240s /COMPLETE/
# 3. Hold on the ordered list, gaps and cycles
Sleep 10s

# Done: leave the room
Ctrl+C
Sleep 1s
