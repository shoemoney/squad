# Squad canonical hero demo (README). Record with: vhs docs/demo/squad.tape
# Runs the PUBLISHED v0.1.2 release binary (`squad` on PATH from the GitHub Release archive).
# The API key is loaded from the environment off-screen; it is never typed or shown.
# Long model waits are cut in the edited GIF and labelled on screen; the raw MP4 keeps them.
# Script: instant startup checklist, Tab starter menu, @ + Tab picks @all, six stream at
# once, $ + Tab shows the modes, a judged two-model debate with preflight, verdict, TIP line.

Output docs/demo/squad-raw.mp4

Set Shell "bash"
Set FontSize 14
Set Width 1400
Set Height 860
Set Padding 12
Set Theme "Catppuccin Mocha"
Set TypingSpeed 45ms
Set WindowBar Colorful

Hide
Type 'cd "$(mktemp -d)" && export SQUAD_DATA_DIR="$PWD/data" SQUAD_DAILY_MAX_USD=150 SQUAD_DEBATE_CONFIRM_USD=0 && source "$DEMO_ENV" && clear'
Enter
Sleep 1s
Show

# 1. Open the room: the startup checklist ticks in
Type "squad"
Sleep 600ms
Enter
Sleep 5s

# 2. Tab on an empty line: the typeahead starter menu
Tab
Sleep 3s
Escape
Sleep 500ms

# 3. @ + Tab: the member list; pick @all
Type "@"
Sleep 1s
Tab
Sleep 2s
Up
Sleep 1500ms
Enter
Sleep 500ms

# 4. Short punchy question, six models stream side by side
Type "In one sentence: what is the most underrated Unix command?"
Sleep 800ms
Enter
Sleep 35s

# 5. $ + Tab: the contest modes
Type "$"
Sleep 800ms
Tab
Sleep 3s
Escape
Sleep 300ms
Ctrl+A
Sleep 300ms
Backspace
Sleep 500ms

# 6. Judged debate, one round
Type "/set ROUNDS 1"
Sleep 500ms
Enter
Sleep 1500ms
Type "@grok @gemini $debate Which pick would save a new developer more time? @judge"
Sleep 800ms
Escape
Sleep 300ms
Enter
Sleep 5s
# Preflight card: confirm
Type "y"
Sleep 130s

# 7. Verdict card, then the idle TIP line
Sleep 8s
Escape
Sleep 12s

Ctrl+C
Sleep 1s
