# $blow_it_up raw recording (full length, uncut). Record with: docs/demo/new/record-all.sh <squad-binary>
# Runs the v0.2.1 release binary (`squad` on PATH). HOME, SQUAD_DATA_DIR and DEMO_WORK are
# private temp dirs made by record-all.sh; the API key comes from the environment and is never typed.
# Attack vectors, blind challenge, severity-ranked findings, then the offline export shows the ranked findings.
# About 250 to 400 s live.

Output docs/media/full/blow_it_up.mp4

Set Shell "bash"
Set FontSize 14
Set Width 1400
Set Height 860
Set Padding 12
Set Theme "Catppuccin Mocha"
Set TypingSpeed 45ms
Set WindowBar Colorful

Hide
Type `unset NO_COLOR; export PS1='$ ' TERM=xterm-256color HOME="$DEMO_HOME" SQUAD_DAILY_MAX_USD=150 SQUAD_DEBATE_CONFIRM_USD=0 && cd "$DEMO_WORK" && clear`
Enter
Sleep 1s
Show

# Open the room
Type "squad"
Sleep 600ms
Enter
Wait+Screen@180s /Ask the squad/
Sleep 2s

# 1. Red-team a one-line spec (target in a fenced block)
Type '@all $blow_it_up ```All requests are accepted without authentication.```'
Sleep 800ms
Escape
Sleep 300ms
Enter
Wait+Screen@180s /Confirm/
Sleep 1s
# Preflight card: confirm
Type "y"
# Fixed wait: the report can scroll the status header off screen, so no text marker is reliable
Sleep 420s

# Done: leave the room. The room can stay on "awaiting ruling" after the contest is COMPLETE, so the
# clip continues OFFLINE with `squad export` on the same finished database (record-all.sh labels it).
Ctrl+C
Sleep 1s
Hide
Type `ID=$(python3 -I -c "import sqlite3,os;print(sqlite3.connect(os.path.expanduser('~/.local/share/squad/squad.db')).execute('select contest_id from contests').fetchone()[0])") && export PS1='$ ' && clear`
Enter
Sleep 1s
Show
Type "squad export blow_it_up $ID -o blow_it_up-report.md"
Sleep 500ms
Enter
Sleep 2s
Type "head -26 blow_it_up-report.md | cut -c1-150"
Sleep 500ms
Enter
Sleep 9s
