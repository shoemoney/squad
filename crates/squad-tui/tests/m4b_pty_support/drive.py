import errno
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
os.set_blocking(master, False)
overall_deadline = time.monotonic() + 45


def read_ready(timeout):
    if not select.select([master], [], [], timeout)[0]:
        return False
    try:
        data = os.read(master, 65536)
    except BlockingIOError:
        return False
    except OSError as error:
        if error.errno in (errno.EIO, errno.ENXIO):
            return False
        raise
    transcript.extend(data)
    return bool(data)


def until(marker):
    deadline = min(overall_deadline, time.monotonic() + 15)
    while marker not in transcript:
        assert time.monotonic() < deadline, (marker, bytes(transcript[-3000:]))
        read_ready(0.1)
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
    # Darwin can block process exit while the controlling PTY has unread
    # output. Keep draining through exit, including libtest's final output.
    deadline = min(overall_deadline, time.monotonic() + 5)
    while child.poll() is None and time.monotonic() < deadline:
        read_ready(0.05)
    assert child.poll() == 0, ("PTY child exit timed out", bytes(transcript[-3000:]))
    # Darwin revokes the slave on session-leader exit (ENOTTY); the master's
    # termios remains available, as in the portable-pty CLI harness.
    assert termios.tcgetattr(master) == before, "raw mode was not restored"
    for sequence in [b"\x1b[?1049h", b"\x1b[?2004h", b"\x1b[?2004l", b"\x1b[?25h", b"\x1b[?1049l"]:
        assert sequence in transcript, (sequence, bytes(transcript[-3000:]))
finally:
    # Release the master before reaping: on Darwin a killed child may already
    # be exiting inside tty close. Never wait indefinitely during cleanup.
    os.close(master)
    os.close(slave)
    if child.poll() is None:
        try:
            os.killpg(child.pid, signal.SIGKILL)
        except (ProcessLookupError, PermissionError):
            pass
        child.wait(timeout=2)
