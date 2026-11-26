#!/usr/bin/env bash
# Run from the repository root through typeahead.tape. FakeRoom uses no API.
set -euo pipefail
capture_socket="squad-typeahead-$$"
trap 'tmux -L "$capture_socket" kill-server 2>/dev/null || true' EXIT
(
    until tmux -L "$capture_socket" has-session -t demo 2>/dev/null; do sleep 0.1; done
    send() { tmux -L "$capture_socket" send-keys -t demo "$@"; }
    literal() { tmux -L "$capture_socket" send-keys -t demo -l "$1"; }
    sleep 9
    send F6
    literal '@'
    send Tab
    sleep 2
    literal 'gr'
    sleep 2
    send Enter
    sleep 1
    send C-a
    literal '$'
    send Tab
    sleep 2
    send Down Down
    sleep 1
    send C-a
    literal '/'
    send Tab
    sleep 2
    literal 'se'
    send Enter
    literal 'ROUNDS 3'
    sleep 2
    send C-a BSpace
    sleep 10
    send C-c
) &
capture_driver=$!
tmux -L "$capture_socket" -f /dev/null new-session -s demo \
    'SQUAD_FX=off TERM=xterm-256color CARGO_BUILD_JOBS=4 cargo run -q -p squad-tui --example demo'
wait "$capture_driver"
