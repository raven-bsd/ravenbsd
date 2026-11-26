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
        handle.write("S175-TERM done failures=1\n")
        handle.flush()
        os.fsync(handle.fileno())
if hang:
    time.sleep(hang)
sys.exit(code)
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


if __name__ == "__main__":
    unittest.main()
