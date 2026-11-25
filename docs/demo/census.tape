# Census walkthrough (M7.P2.L2.T7). Record with: vhs docs/demo/census.tape
# Runs the PUBLISHED release binary (`squad` on PATH from the GitHub Release archive).
# The API key is loaded from the environment off-screen; it is never typed or shown.
# Smallest settings: one pass. Model waits are cut in the edited GIF and labelled on screen; the raw MP4 keeps them.


Output "/tmp/squad-demo/census.mp4"
Output "/tmp/squad-demo/census.txt"

Set Shell "bash"
Set FontSize 14
Set Width 1400
Set Height 860
Set Padding 12
Set Theme "Catppuccin Mocha"
Set TypingSpeed 35ms
Set WindowBar Colorful

Hide
Type 'unset NO_COLOR; export SQUAD_DATA_DIR=/tmp/squad-demo/data-census SQUAD_DAILY_MAX_USD=1.50 SQUAD_DEBATE_CONFIRM_USD=0 && source "$DEMO_ENV" && source "$DEMO_MODELS" && cd "$(mktemp -d)" && clear'
Enter
Sleep 1s
Show

# 1. Open the room
Type "squad"
Sleep 600ms
Enter
Sleep 3s

# 2. One pass only (stored value; the room default is 2)
Type "/set CENSUS_PASSES 1"
Sleep 500ms
Enter
Sleep 1s

# 3. Paste a short deploy plan straight into the census command
Type "@luna @grok @mini $census Deploy plan for v2.4.0: 1) tag v2.4.0 on main, 2) run the database migration on the primary, 3) roll the API out in two batches, 4) turn on the new checkout flow for 10% of users, 5) watch the error rate for 30 minutes, 6) ramp checkout to 100%."
Sleep 500ms
Enter
Sleep 4s
# Cost preview: confirm
Type "y"
Sleep 150s

# 4. Hunt, anonymous ratings and the estimate are on screen; export the census
Sleep 6s
Type "/export census"
Sleep 500ms
Enter
Sleep 2s
Enter
Sleep 3s

Ctrl+C
Sleep 1s
