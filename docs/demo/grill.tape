# Grill walkthrough. Record with: DEMO_ENV=<env file> DEMO_MODELS=<model env file> vhs docs/demo/grill.tape
# Runs the PUBLISHED release binary (`squad` on PATH from the GitHub Release archive).
# The API key is loaded from the environment off-screen; it is never typed or shown.
# Smallest settings: one round, three members (one leader, two grillers). The complete recording is /tmp/squad-demo/grill.mp4.

Output "/tmp/squad-demo/grill.mp4"
Output "/tmp/squad-demo/grill.txt"

Set Shell "bash"
Set FontSize 14
Set Width 1400
Set Height 860
Set Padding 12
Set Theme "Catppuccin Mocha"
Set TypingSpeed 35ms
Set WindowBar Colorful

Hide
Type 'unset NO_COLOR; export SQUAD_DATA_DIR=/tmp/squad-demo/data-grill SQUAD_DAILY_MAX_USD=1.50 SQUAD_DEBATE_CONFIRM_USD=0 && source "$DEMO_ENV" && source "$DEMO_MODELS" && cd "$(mktemp -d)" && clear'
Enter
Sleep 1s
Show

# 1. Open the room
Type "squad"
Sleep 600ms
Enter
Sleep 3s

# 2. One round only (stored value; the room default is 2)
Type "/set GRILL_ROUNDS 1"
Sleep 500ms
Enter
Sleep 1s

# 3. Named members plus text: everyone answers first, then you pick the leader
Type "@luna @nano @mini $grill Plan a zero-downtime database migration for a small web app, in five short steps."
Sleep 500ms
Enter
Sleep 25s

# 4. Pick the leader by sending only the nick, then confirm the cost preview
Type "@luna"
Sleep 500ms
Enter
Sleep 4s
Type "y"
Sleep 120s

# 5. Final plan and Q&A appendix are on screen; export the grill
Sleep 6s
Type "/export grill"
Sleep 500ms
Enter
Sleep 2s
Enter
Sleep 3s

Ctrl+C
Sleep 1s
