# Squad canonical demo (M7.P2.L2.T1). Record with: vhs docs/demo/squad.tape
# Runs the PUBLISHED release binary (`squad` on PATH from the GitHub Release archive).
# The API key is loaded from the environment off-screen; it is never typed or shown.
# Long model waits are cut in the edited GIF and labelled on screen; the raw MP4 keeps them.

Output docs/demo/squad-raw.mp4
Output docs/demo/squad-raw.gif

Set Shell "bash"
Set FontSize 14
Set Width 1400
Set Height 860
Set Padding 12
Set Theme "Catppuccin Mocha"
Set TypingSpeed 35ms
Set WindowBar Colorful

Hide
Type 'cd "$(mktemp -d)" && export SQUAD_DATA_DIR="$PWD/data" SQUAD_DAILY_MAX_USD=5 SQUAD_DEBATE_CONFIRM_USD=0 && source "$DEMO_ENV" && clear'
Enter
Sleep 1s
Show

# 1. Open the room
Type "squad"
Sleep 600ms
Enter
Sleep 3s

# 2. Six models, one question, all streaming at once
Type "@all In one sentence: what's the most underrated Unix command, and why?"
Sleep 500ms
Enter
Sleep 45s

# 3. Short debate between two of them, judged
Type "/set ROUNDS 1"
Enter
Sleep 1s
Type "@grok @gemini $debate Which of your two picks would save a new developer more time? @judge"
Sleep 500ms
Enter
Sleep 4s
# Cost preview: confirm
Type "y"
Sleep 120s

# 4. Verdict card is on screen; show the spend bar, then export
Sleep 6s
Type "/export debate"
Enter
Sleep 2s
# accept the pre-filled export path
Enter
Sleep 3s

Ctrl+C
Sleep 1s
