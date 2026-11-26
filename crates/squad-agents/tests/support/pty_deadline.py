"""Run one isolated PTY test with a wall-clock process-tree deadline."""
import os
import signal
import subprocess
import sys
import tempfile
import time


def kill_tree(root):
    # PTY children call setsid(), so killing only the test's group misses them.
    rows = subprocess.run(
        ["ps", "-axo", "pid=,ppid=,pgid=,stat="], capture_output=True, text=True,
        timeout=2, check=True,
    ).stdout.splitlines()
    rows = [(int(pid), int(parent), int(group), state) for pid, parent, group, state in (row.split() for row in rows)]
    owned = {root}
    while True:
        descendants = {pid for pid, parent, group, state in rows if parent in owned}
        if descendants <= owned:
            break
        owned |= descendants
    groups = {group for pid, parent, group, state in rows if pid in owned}
    print("Owned PTY processes (pid, parent, group, state): " + repr([row for row in rows if row[0] in owned]), file=sys.stderr, flush=True)
    print("Killing owned PTY process groups: " + repr(sorted(groups)), file=sys.stderr, flush=True)
    for group in groups:
        assert group != os.getpgrp(), "supervisor must be outside the test session"
        try:
            os.killpg(group, signal.SIGKILL)
        except ProcessLookupError:
            pass
        except PermissionError:
            # Darwin can reject signals to a process already exiting. Killing
            # its parent closes the master, releasing a blocked slave close.
            print("Process group already exiting: " + str(group), file=sys.stderr, flush=True)


def main():
    timeout = float(sys.argv[1])
    command = sys.argv[2:]
    # A descendant can retain stdout after the test exits. A regular file avoids
    # an unbounded pipe/EOF wait in both normal completion and failure cleanup.
    with tempfile.TemporaryFile() as output:
        child = subprocess.Popen(command, stdout=output, stderr=subprocess.STDOUT, start_new_session=True)
        deadline = time.monotonic() + timeout
        while child.poll() is None and time.monotonic() < deadline:
            time.sleep(min(0.02, max(0, deadline - time.monotonic())))
        timed_out = child.poll() is None
        if timed_out:
            print("PTY overall deadline exceeded (" + str(timeout) + "s): " + repr(command), file=sys.stderr, flush=True)
            try:
                kill_tree(child.pid)
            finally:
                # No unbounded wait, including when Darwin is already in exit.
                try:
                    os.killpg(child.pid, signal.SIGKILL)
                except (ProcessLookupError, PermissionError):
                    pass
                try:
                    child.kill()
                except (ProcessLookupError, PermissionError):
                    pass
                try:
                    child.wait(timeout=2)
                except subprocess.TimeoutExpired:
                    print("PTY test did not reap after SIGKILL", file=sys.stderr, flush=True)
        output.seek(0, os.SEEK_END)
        size = output.tell()
        output.seek(max(0, size - 65536))
        if timed_out or child.returncode != 0:
            sys.stderr.buffer.write(output.read())
            sys.stderr.flush()
        return 124 if timed_out else child.returncode


if __name__ == "__main__":
    sys.exit(main())
