import fcntl
import os
import pty
import select
import signal
import struct
import subprocess
import sys
import termios
import time

master, slave = pty.openpty()
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 120, 0, 0))
before = termios.tcgetattr(slave)
env = dict(os.environ, TERM="xterm-256color", SQUAD_PTY_CHILD="1")


def setup():
    os.setsid()
    fcntl.ioctl(0, termios.TIOCSCTTY, 0)


child = subprocess.Popen(
    [sys.argv[1], "--ignored", "--exact", "pty_child", "--nocapture"],
    stdin=slave, stdout=slave, stderr=slave, env=env, preexec_fn=setup,
)
transcript = bytearray()


def until(marker):
    deadline = time.monotonic() + 15
    while marker not in transcript:
        assert time.monotonic() < deadline, (marker, bytes(transcript[-3000:]))
        if select.select([master], [], [], 0.1)[0]:
            transcript.extend(os.read(master, 65536))
        assert child.poll() is None or marker in transcript, bytes(transcript[-3000:])


try:
    for label, data in [
        (b"paste", b"\x1b[200~@claude literal\nsecond line\x1b[201~"),
        (b"newline", b"\x0a"),
        (b"altenter", b"\x1b\r"),
        (b"search", b"\x06a\x1b"),
        (b"copy", b"\x19"),
        (b"preflight", b"y"),
    ]:
        until(b"PTY_READY_" + label)
        os.write(master, data)
    until(b"PTY_READY_resize")
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
    os.kill(child.pid, signal.SIGWINCH)
    until(b"PTY_COMPLETE")
    assert child.wait(timeout=5) == 0, bytes(transcript[-3000:])
    assert termios.tcgetattr(slave) == before, "raw mode was not restored"
    for sequence in [b"\x1b[?1049h", b"\x1b[?2004h", b"\x1b[?2004l", b"\x1b[?25h", b"\x1b[?1049l"]:
        assert sequence in transcript, (sequence, bytes(transcript[-3000:]))
finally:
    if child.poll() is None:
        child.kill()
        child.wait()
    os.close(master)
    os.close(slave)
