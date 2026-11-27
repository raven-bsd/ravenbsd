#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

"""External tests for the S175 terminal runner's final-serial collection."""

from __future__ import annotations

import os
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import run  # noqa: E402

CHILD = r"""
import os, sys, time
serial, control, code, hang, write = sys.argv[1], int(sys.argv[2]), int(sys.argv[3]), float(sys.argv[4]), int(sys.argv[5])
os.read(control, 1)
if write:
    with open(serial, "a", encoding="utf-8") as handle:
        handle.write("S175-TERM done failures=0\n")
        handle.write("S175-TERM done failures=1\n")
        handle.flush()
        os.fsync(handle.fileno())
if hang:
    time.sleep(hang)
sys.exit(code)
"""

EXIT_DURING_GRACE_CHILD = r"""
import os, sys
serial, first, second, code, tail = sys.argv[1], int(sys.argv[2]), int(sys.argv[3]), int(sys.argv[4]), sys.argv[5]
os.read(first, 1)
with open(serial, "a", encoding="utf-8") as handle:
    handle.write("S175-TERM-MUTATION-BEGIN\nS175-TERM done failures=0\nS175-TERM done failures=1\n")
    handle.flush()
    os.fsync(handle.fileno())
os.read(second, 1)
with open(serial, "a", encoding="utf-8") as handle:
    handle.write(tail + "\n")
    handle.flush()
    os.fsync(handle.fileno())
sys.exit(code)
"""

DEADLINE_CHILD = r"""
import os, signal, sys, time
serial, first, second, code, tail = sys.argv[1], int(sys.argv[2]), int(sys.argv[3]), int(sys.argv[4]), sys.argv[5]

def cleanup(_signum, _frame):
    with open(serial, "a", encoding="utf-8") as handle:
        handle.write("S175-TERM-MUTATION-BEGIN\nS175-TERM done failures=0\nS175-TERM done failures=1\n")
        handle.flush()
        os.fsync(handle.fileno())
    sys.exit(code)

signal.signal(signal.SIGTERM, cleanup)
os.read(first, 1)
while True:
    time.sleep(0.05)
"""


class FinalSerialTests(unittest.TestCase):
    def _start(self, serial: Path, exit_code: int, *, write: bool = True, hang: float = 0):
        read_fd, write_fd = os.pipe()
        child = subprocess.Popen(
            [
                sys.executable,
                "-c",
                CHILD,
                str(serial),
                str(read_fd),
                str(exit_code),
                str(hang),
                str(int(write)),
            ],
            pass_fds=(read_fd,),
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        os.close(read_fd)
        return child, write_fd

    def _collect(self, serial: Path, child: subprocess.Popen[bytes], write_fd: int):
        captures: list[str] = []
        sent = False

        def after_capture(text: str) -> None:
            nonlocal sent
            captures.append(text)
            if not sent and "S175-TERM-MUTATION-BEGIN" in text:
                sent = True
                os.write(write_fd, b"x")
                os.close(write_fd)

        text, failure = run.collect_serial(
            serial,
            child,
            time.monotonic() + 5,
            completion_grace=2,
            on_capture=after_capture,
        )
        child.wait(timeout=5)
        return text, failure, captures

    def _start_with_controls(
        self, script: str, serial: Path, exit_code: int, *, tail: str = ""
    ) -> tuple[subprocess.Popen[bytes], int, int]:
        first_read, first_write = os.pipe()
        second_read, second_write = os.pipe()
        child = subprocess.Popen(
            [
                sys.executable,
                "-c",
                script,
                str(serial),
                str(first_read),
                str(second_read),
                str(exit_code),
                tail,
            ],
            pass_fds=(first_read, second_read),
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        os.close(first_read)
        os.close(second_read)
        return child, first_write, second_write

    def test_tail_written_during_grace_is_read_after_exit(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            serial = Path(directory) / "serial.log"
            serial.write_text("", encoding="utf-8")
            child, first_write, second_write = self._start_with_controls(
                EXIT_DURING_GRACE_CHILD, serial, 33, tail="S175-TERM-TAIL"
            )
            released = False
            captures: list[str] = []

            def after_capture(text: str) -> None:
                nonlocal released
                captures.append(text)
                if not released and text.count("S175-TERM done failures=") >= 2:
                    released = True
                    os.write(second_write, b"x")
                    os.close(second_write)

            os.write(first_write, b"x")
            os.close(first_write)
            text, failure = run.collect_serial(
                serial,
                child,
                time.monotonic() + 5,
                completion_grace=5,
                on_capture=after_capture,
            )
            child.wait(timeout=5)
            self.assertTrue(captures)
            self.assertNotIn("S175-TERM-TAIL", captures[-1])
            self.assertIn("S175-TERM-TAIL", text)
            self.assertIsNone(failure)
            self.assertEqual(child.returncode, 33)

    def test_deadline_before_complete_output_fails_despite_cleanup_output(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            serial = Path(directory) / "serial.log"
            serial.write_text("", encoding="utf-8")
            child, first_write, second_write = self._start_with_controls(
                DEADLINE_CHILD, serial, 33
            )
            os.close(second_write)
            os.write(first_write, b"x")
            os.close(first_write)
            text, failure = run.collect_serial(
                serial, child, time.monotonic() + 0.6, completion_grace=5
            )
            child.wait(timeout=5)
            self.assertEqual(failure, "serial:deadline")
            self.assertIn("S175-TERM done failures=1", text)
            self.assertIsNotNone(child.poll())

    def test_final_marker_after_capture_and_nonzero_exit_fails(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            serial = Path(directory) / "serial.log"
            serial.write_text(
                "S175-TERM-MUTATION-BEGIN\n"
                "S175-TERM row=termios status=FAIL detail=mutation\n",
                encoding="utf-8",
            )
            child, write_fd = self._start(serial, 1)
            text, failure, captures = self._collect(serial, child, write_fd)
            self.assertIn("S175-TERM-MUTATION-BEGIN", captures[0])
            self.assertNotIn("S175-TERM done failures=", captures[0])
            self.assertIn("S175-TERM done failures=1", text)
            self.assertFalse(run.mutation_incomplete(text))
            self.assertEqual(failure, "qemu:exit=1")

    def test_complete_output_with_alive_child_is_terminated_not_success(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            serial = Path(directory) / "serial.log"
            serial.write_text(
                "S175-TERM-MUTATION-BEGIN\n"
                "S175-TERM row=termios status=FAIL detail=mutation\n",
                encoding="utf-8",
            )
            child, write_fd = self._start(serial, 33, hang=10)
            text, failure, _ = self._collect(serial, child, write_fd)
            self.assertIn("S175-TERM done failures=1", text)
            self.assertEqual(failure, "qemu:terminated")
            self.assertIsNotNone(child.poll())

    def test_missing_marker_remains_a_failure(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            serial = Path(directory) / "serial.log"
            serial.write_text(
                "S175-TERM-MUTATION-BEGIN\n"
                "S175-TERM row=termios status=FAIL detail=mutation\n",
                encoding="utf-8",
            )
            child, write_fd = self._start(serial, 33, write=False)
            text, failure, _ = self._collect(serial, child, write_fd)
            self.assertIsNone(failure)
            self.assertTrue(run.mutation_incomplete(text))

    def test_exit_policy_never_accepts_unknown_or_nonzero(self) -> None:
        self.assertEqual(run.qemu_exit_failure(None), "qemu:running")
        self.assertEqual(run.qemu_exit_failure(1), "qemu:exit=1")
        self.assertIsNone(run.qemu_exit_failure(0))
        self.assertIsNone(run.qemu_exit_failure(33))


BOUNDARY_CHILD = r"""
import os, sys
serial, control, code = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
os.read(control, 1)
with open(serial, "a", encoding="utf-8") as handle:
    handle.write("S175-TERM done failures=1\n")
    handle.flush()
    os.fsync(handle.fileno())
sys.exit(code)
"""

BOUNDARY_MARKER = "S175-TERM done failures=1"


def boundary_violations(text: str, failure: str | None, returncode: int | None) -> list[str]:
    """Boundary contract violations: an expired deadline stays a deadline failure."""
    violations = []
    if failure != "serial:deadline":
        violations.append(f"failure={failure!r}")
    if BOUNDARY_MARKER not in text:
        violations.append("missing boundary marker")
    if returncode != 33:
        violations.append(f"returncode={returncode}")
    return violations


class DeadlineIgnoringCollector:
    """Test double: hides a deadline verdict from the collector result."""

    def __init__(self, collector):
        self.collector = collector

    def __call__(self, *args, **kwargs):
        text, _failure = self.collector(*args, **kwargs)
        return text, None


class FinalReadOmittingCollector:
    """Test double: drops the post-loop read, returning only in-loop captured text."""

    def __init__(self, collector):
        self.collector = collector

    def __call__(self, serial, qemu, deadline, *, on_capture=None, **kwargs):
        captured = [""]

        def capture(text: str) -> None:
            captured[0] = text
            if on_capture is not None:
                on_capture(text)

        _text, failure = self.collector(
            serial, qemu, deadline, on_capture=capture, **kwargs
        )
        return captured[0], failure


class BoundaryClock:
    def __init__(self, child, deadline):
        self.child = child
        self.deadline = deadline
        self.calls = 0

    def now(self):
        self.calls += 1
        if self.calls == 1:
            return 0.0
        # The child writes the missing marker and exits before the collector's post-loop poll.
        self.child.wait(timeout=2)
        return self.deadline + 1


class DeadlineBoundaryTests(unittest.TestCase):
    def _run_boundary(self, collector):
        with tempfile.TemporaryDirectory() as directory:
            serial = Path(directory) / "serial.log"
            serial.write_text(
                "S175-TERM-MUTATION-BEGIN\nS175-TERM done failures=0\n",
                encoding="utf-8",
            )
            read_fd, write_fd = os.pipe()
            child = subprocess.Popen(
                [sys.executable, "-c", BOUNDARY_CHILD, str(serial), str(read_fd), "33"],
                pass_fds=(read_fd,),
                stdin=subprocess.DEVNULL,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            os.close(read_fd)
            sent = False

            def after_capture(text: str) -> None:
                nonlocal sent
                if not sent and "S175-TERM-MUTATION-BEGIN" in text:
                    sent = True
                    os.write(write_fd, b"x")
                    os.close(write_fd)

            deadline = 1.0
            text, failure = collector(
                serial,
                child,
                deadline,
                completion_grace=5,
                on_capture=after_capture,
                now=BoundaryClock(child, deadline).now,
                poll_interval=0,
            )
            child.wait(timeout=5)
            return text, failure, child.returncode

    def test_deadline_boundary_child_exit_is_still_deadline_failure(self) -> None:
        text, failure, returncode = self._run_boundary(run.collect_serial)
        self.assertEqual(boundary_violations(text, failure, returncode), [])

    def test_boundary_regression_rejects_buggy_collector_doubles(self) -> None:
        doubles = (
            DeadlineIgnoringCollector(run.collect_serial),
            FinalReadOmittingCollector(run.collect_serial),
        )
        for double in doubles:
            with self.subTest(double=type(double).__name__):
                text, failure, returncode = self._run_boundary(double)
                self.assertNotEqual(boundary_violations(text, failure, returncode), [])
        text, failure, returncode = self._run_boundary(run.collect_serial)
        self.assertEqual(boundary_violations(text, failure, returncode), [])


if __name__ == "__main__":
    unittest.main()
