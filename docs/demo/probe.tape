# Probe walkthrough (M7.P2.L2.T7). Record with: vhs docs/demo/probe.tape
# Runs the PUBLISHED release binary (`squad` on PATH from the GitHub Release archive).
# The API key is loaded from the environment off-screen; it is never typed or shown.
# Default six-model roster; v0.1.2 release. Nothing is executed: the checks are proposals for you to run.
# Model waits are cut in the edited GIF and labelled on screen; the raw MP4 keeps them.


Output "/tmp/squad-demo/probe.mp4"
Output "/tmp/squad-demo/probe.txt"

Set Shell "bash"
Set Framerate 10
Set FontSize 14
Set Width 1400
Set Height 860
Set Padding 12
Set Theme "Catppuccin Mocha"
Set TypingSpeed 35ms
Set WindowBar Colorful

Hide
Type 'unset NO_COLOR; export SQUAD_DATA_DIR=/tmp/squad-demo/audit2-probe SQUAD_DAILY_MAX_USD=150 SQUAD_DEBATE_CONFIRM_USD=0 && source "$DEMO_ENV" && cd "$(mktemp -d)"; squad() { tmux -L audit-m7 -f /tmp/squad-demo/tmux.conf new-session -s probe "$(command -v squad)"; }; (until tmux -L audit-m7 capture-pane -p -t probe 2>/dev/null | grep -q "declared true"; do sleep 1; done; sleep 4; tmux -L audit-m7 send-keys -t probe F7; sleep 8; tmux -L audit-m7 send-keys -t probe F7) >/dev/null 2>&1 & clear'
Enter
Sleep 1s
Show

# 1. Open the room
Type "squad"
Sleep 600ms
Enter
Sleep 18s

# 2. Symptom brief, with the only resources the checks may use named in it
Type "@all $probe API p95 latency jumped from 120 ms to 2.1 s after Tuesday's deploy while CPU stayed flat. You may use only: application logs, the Postgres slow-query log, and load balancer metrics."
Sleep 500ms
Enter
Sleep 4s
# Cost preview: confirm
Type "y"
Sleep 330s

# 3. Decision tree is on screen. VHS has no F-keys, so the room runs in a tmux session and a hidden helper presses F7 (matrix on, then off).
#    Then export the probe
Sleep 6s
Sleep 14s
Type "/export probe"
Sleep 500ms
Enter
Sleep 2s
Enter
Sleep 3s

Ctrl+C
Sleep 1s
