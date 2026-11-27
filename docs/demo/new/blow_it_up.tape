# $blow_it_up raw recording (full length, uncut). Record with: docs/demo/new/record-all.sh <squad-binary>
# Runs the v0.2.0 release binary (`squad` on PATH). HOME, SQUAD_DATA_DIR and DEMO_WORK are
# private temp dirs made by record-all.sh; the API key comes from the environment and is never typed.
# Attack vectors, blind challenge, severity-ranked findings, then F7 opens the killed appendix.
# VHS has no function keys, so the room runs inside tmux and a hidden watcher sends F7 after COMPLETE.
# About 170 s live.

Output docs/media/full/blow_it_up.gif
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
Type 'unset NO_COLOR; export TERM=xterm-256color HOME="$DEMO_HOME" SQUAD_DAILY_MAX_USD=150 SQUAD_DEBATE_CONFIRM_USD=0 && cd "$DEMO_WORK" && clear'
Enter
Type '(until tmux -L demo capture-pane -p -t r 2>/dev/null | grep -q COMPLETE; do sleep 1; done; sleep 9; tmux -L demo send-keys -t r F7) >/dev/null 2>&1 &'
Enter
Type "tmux -L demo -f /dev/null new-session -s r squad ';' set status off"
Enter
Sleep 4s
Show

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
Wait+Screen@1200s /Complete ·/

# 2. Ranked findings on screen; the watcher presses F7 after 9 s to expand the killed appendix
Sleep 20s

Ctrl+C
Sleep 1s
