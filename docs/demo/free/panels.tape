Output "/tmp/squad-demo/free-panels.mp4"
Set Shell "bash"
Set FontSize 14
Set Width 1400
Set Height 860
Set Padding 10
Set Theme "Catppuccin Mocha"
Set WindowBar Colorful
Hide
Type "unset NO_COLOR; export TERM=xterm-256color; clear; (until tmux -L rec has-session -t r; do sleep 0.2; done; T='tmux -L rec send-keys -t r'; sleep 60; $T F7; sleep 6; $T F7; sleep 2; $T F8; sleep 5; $T Down; sleep 1; $T Down; sleep 3; $T F8) >/dev/null 2>&1 &"
Enter
Type "tmux -L rec -f /tmp/squad-demo/tmux.conf new-session -s r target/release/examples/demo"
Enter
Show
Sleep 82s
