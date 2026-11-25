# Probe walkthrough (M7.P2.L2.T7). Record with: vhs docs/demo/probe.tape
# Runs the PUBLISHED release binary (`squad` on PATH from the GitHub Release archive).
# The API key is loaded from the environment off-screen; it is never typed or shown.
# Four members is the minimum for a probe, so this is the smallest run. Nothing is executed: the checks are proposals for you to run.
# Model waits are cut in the edited GIF and labelled on screen; the raw MP4 keeps them.


Output "/tmp/squad-demo/probe.mp4"
Output "/tmp/squad-demo/probe.txt"

Set Shell "bash"
Set FontSize 14
Set Width 1400
Set Height 860
Set Padding 12
Set Theme "Catppuccin Mocha"
Set TypingSpeed 35ms
Set WindowBar Colorful

Hide
Type 'unset NO_COLOR; export SQUAD_DATA_DIR=/tmp/squad-demo/data-probe SQUAD_DAILY_MAX_USD=3 SQUAD_DEBATE_CONFIRM_USD=0 && source "$DEMO_ENV" && source "$DEMO_MODELS" && cd "$(mktemp -d)"; squad() { tmux -L rec -f /tmp/squad-demo/tmux.conf new-session -s r "$(command -v squad)"; }; (until tmux -L rec capture-pane -p -t r 2>/dev/null | grep -q "declared true"; do sleep 1; done; sleep 4; tmux -L rec send-keys -t r F7; sleep 8; tmux -L rec send-keys -t r F7) >/dev/null 2>&1 & clear'
Enter
Sleep 1s
Show

# 1. Open the room
Type "squad"
Sleep 600ms
Enter
Sleep 3s

# 2. Symptom brief, with the only resources the checks may use named in it
Type "@luna @nano2 @grok @luna2 $probe API p95 latency jumped from 120 ms to 2.1 s after Tuesday's deploy while CPU stayed flat. You may use only: application logs, the Postgres slow-query log, and load balancer metrics."
Sleep 500ms
Enter
Sleep 4s
# Cost preview: confirm
Type "y"
Sleep 150s

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
