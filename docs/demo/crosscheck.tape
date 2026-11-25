# Crosscheck walkthrough (M7.P2.L2.T7). Record with: vhs docs/demo/crosscheck.tape
# Runs the PUBLISHED release binary (`squad` on PATH from the GitHub Release archive).
# The API key is loaded from the environment off-screen; it is never typed or shown.
# The goal is given after a `--` line, so no extra extraction call is made. Three members keep the model wait short.
# Model waits are cut in the edited GIF and labelled on screen; the raw MP4 keeps them.

Output docs/demo/crosscheck-raw.mp4
Output docs/demo/crosscheck-raw.gif

Set Shell "bash"
Set FontSize 14
Set Width 1400
Set Height 860
Set Padding 12
Set Theme "Catppuccin Mocha"
Set TypingSpeed 35ms
Set WindowBar Colorful

Hide
Type 'export SQUAD_DATA_DIR="$(mktemp -d)" SQUAD_DAILY_MAX_USD=5 SQUAD_DEBATE_CONFIRM_USD=0 && source "$DEMO_ENV" && cd "$(mktemp -d)" && clear'
Enter
Sleep 1s
Show

# 1. Open the room
Type "squad"
Sleep 600ms
Enter
Sleep 3s

# 2. Pasted plan before the --, goal on the line after it (Ctrl+J inserts a newline, Enter sends)
Type "@claude @openai @gemini $crosscheck Deploy plan for v2.4.0: 1) tag v2.4.0 on main, 2) run the database migration on the primary, 3) roll the API out in two batches, 4) turn on the new checkout flow for 10% of users, 5) watch the error rate for 30 minutes, 6) ramp checkout to 100%."
Sleep 500ms
Ctrl+J
Type "--"
Ctrl+J
Type "Goal: ship v2.4.0 with no customer-visible downtime and no data loss. Constraints: one on-call engineer, no staging environment, rollback within 15 minutes."
Sleep 500ms
Enter
Sleep 4s
# Cost preview: confirm
Type "y"
Sleep 180s

# 3. Coverage ledger and bounded-stop status are on screen; export the crosscheck
Sleep 6s
Type "/export crosscheck"
Sleep 500ms
Enter
Sleep 4s

Ctrl+C
Sleep 1s
