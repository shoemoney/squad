# Offline continuation of the live survey recording, from its committed database.
# DEMO_DATA and DEMO_CONTEST identify that same run; this performs no provider calls.
Output "/tmp/squad-demo/survey-report.mp4"
Output "/tmp/squad-demo/survey-report.txt"
Set Shell "bash"
Set Framerate 10
Set FontSize 14
Set Width 1400
Set Height 860
Set Padding 12
Set Theme "Catppuccin Mocha"
Set TypingSpeed 15ms
Set WindowBar Colorful
Hide
Type 'export SQUAD_DATA_DIR="$DEMO_DATA"; source "$DEMO_ENV"; cd "$(mktemp -d)"; clear'
Enter
Sleep 1s
Show
Type "squad export survey $DEMO_CONTEST -o survey.md"
Enter
Sleep 3s
Hide
Type "clear"
Enter
Show
Type "sed -n '/^## Survey claim map/,+12p' survey.md | fold -sw 110"
Enter
Sleep 7s
Hide
Type "clear"
Enter
Show
Type "sed -n '/^### Contested/,/^### Lone insights/p' survey.md | fold -sw 110"
Enter
Sleep 8s
Hide
Type "clear"
Enter
Show
Type "grep '^Limitations:' survey.md | fold -sw 110"
Enter
Sleep 12s
Hide
Type "clear"
Enter
Show
