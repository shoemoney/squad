# Survey walkthrough (M7.P2.L2.T7). Record with: vhs docs/demo/survey.tape
# Runs the PUBLISHED release binary (`squad` on PATH from the GitHub Release archive).
# The API key is loaded from the environment off-screen; it is never typed or shown.
# Four members keep the model wait short. Model waits are cut in the edited GIF and labelled on screen; the raw MP4 keeps them.

Output docs/demo/survey-raw.mp4
Output docs/demo/survey-raw.gif

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

# 2. One product question to four members; they answer, claims are extracted, then voted on
Type "@claude @openai @gemini @grok $survey Should the first release of our offline-first notes app ship with sync conflict resolution, or ship single-device and add sync in v2?"
Sleep 500ms
Enter
Sleep 4s
# Cost preview: confirm
Type "y"
Sleep 180s

# 3. Claim map by band is on screen; export the survey
Sleep 6s
Type "/export survey"
Sleep 500ms
Enter
Sleep 4s

Ctrl+C
Sleep 1s
