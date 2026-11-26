# Offline continuation of the live probe recording, from its committed database.
# DEMO_DATA and DEMO_CONTEST identify that same run; this performs no provider calls.
Output "/tmp/squad-demo/probe-report.mp4"
Output "/tmp/squad-demo/probe-report.txt"
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
Type "squad export probe $DEMO_CONTEST -o probe.md"
Enter
Sleep 3s
Hide
Type "clear"
Enter
Show
Type "sed -n '/^## Prediction matrix/,+18p' probe.md | fold -sw 110"
Enter
Sleep 8s
Hide
Type "clear"
Enter
Show
Type "sed -n '/^## Node n1/,+12p' probe.md | fold -sw 110"
Enter
Sleep 8s
Hide
Type "clear"
Enter
Show
Type "tail -34 probe.md | fold -sw 110"
Enter
Sleep 12s
Hide
Type "clear"
Enter
Show
