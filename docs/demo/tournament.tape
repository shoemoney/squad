# Tournament walkthrough. Record with:  vhs docs/demo/tournament.tape
# Runs the PUBLISHED release binary (`squad` on PATH from the GitHub Release archive).
# The API key is loaded from the environment off-screen; it is never typed or shown.
# Three entrants (the minimum) keep the model wait short. The complete recording is /tmp/squad-demo/tournament.mp4.


Output "/tmp/squad-demo/tournament.mp4"
Output "/tmp/squad-demo/tournament.txt"

Set Shell "bash"
Set FontSize 14
Set Width 1400
Set Height 860
Set Padding 12
Set Theme "Catppuccin Mocha"
Set TypingSpeed 35ms
Set WindowBar Colorful

Hide
Type 'unset NO_COLOR; export SQUAD_DATA_DIR=/tmp/squad-demo/data-tournament SQUAD_DAILY_MAX_USD=1.50 SQUAD_DEBATE_CONFIRM_USD=0 && source "$DEMO_ENV" && source "$DEMO_MODELS" && cd "$(mktemp -d)" && clear'
Enter
Sleep 1s
Show

# 1. Open the room
Type "squad"
Sleep 600ms
Enter
Sleep 3s

# 2. Three entrants, one prompt
Type "@luna @nano @mini $tournament Design a useful CLI in two sentences."
Sleep 500ms
Enter
Sleep 4s
# Cost preview: confirm
Type "y"
Sleep 150s

# 3. Champion and placings are on screen; export
Sleep 6s
Type "/export tournament"
Sleep 500ms
Enter
Sleep 2s
Enter
Sleep 3s

Ctrl+C
Sleep 1s
