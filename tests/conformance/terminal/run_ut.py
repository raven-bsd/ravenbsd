#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

"""External tests for the S175 terminal runner's final-serial collection."""

from __future__ import annotations

import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import run  # noqa: E402


class FakeQemu:
    def __init__(self, polls: list[int | None], returncode: int | None) -> None:
        self._polls = iter(polls)
        self.returncode = returncode

    def poll(self) -> int | None:
        value = next(self._polls, self.returncode)
        if value is not None:
            self.returncode = value
        return value


class FinalSerialTests(unittest.TestCase):
    def test_final_marker_is_captured_after_child_exit(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            serial = Path(directory) / "serial.log"
            serial.write_text(
                "S175-TERM-MUTATION-BEGIN\n"
                "S175-TERM row=termios status=FAIL detail=mutation\n",
                encoding="utf-8",
            )
            child = subprocess.Popen(
                [
                    sys.executable,
                    "-c",
                    "import time; from pathlib import Path; "
                    f"time.sleep(0.05); p=Path({str(serial)!r}); "
                    "p.write_text(p.read_text() + 'S175-TERM done failures=1\\n')",
                ]
            )
            try:
                text = run.collect_serial(serial, FakeQemu([None, 33], 33), time.monotonic() + 2)
            finally:
                child.wait(timeout=1)
            self.assertIn("S175-TERM-MUTATION-BEGIN", text)
            self.assertIn("S175-TERM done failures=1", text)
            self.assertFalse(run.mutation_incomplete(text))

    def test_missing_marker_remains_a_failure(self) -> None:
        text = "S175-TERM-MUTATION-BEGIN\nS175-TERM row=termios status=FAIL detail=mutation\n"
        self.assertTrue(run.mutation_incomplete(text))

    def test_nonzero_child_exit_is_not_success(self) -> None:
        self.assertEqual(run.qemu_exit_failure(1), "qemu:exit=1")
        self.assertIsNone(run.qemu_exit_failure(0))
        self.assertIsNone(run.qemu_exit_failure(33))


if __name__ == "__main__":
    unittest.main()
