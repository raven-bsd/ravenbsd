#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

"""Unit tests for the first-byte probe's sshd-ready wait.

Run from the fixture directory: ``python3 -m unittest probe_ut``.
No QEMU is started: a temporary serial file and a timed writer stand in for the
guest.  The synthetic early connect is the negative: a connect before the ready
marker would read 0 bytes and be logged as a banner failure.
"""

from __future__ import annotations

import tempfile
import threading
import time
import unittest
from pathlib import Path

import probe

READY = b"svcd: sshd up\n"


class ReadyWaitTest(unittest.TestCase):
    def test_wait_serial_blocks_until_the_marker(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            serial = Path(directory) / "serial.log"
            serial.write_bytes(b"boot loading\n")
            started = time.monotonic()
            self.assertIsNone(
                probe.wait_serial(serial, READY, started, 0.2),
                "no ready marker yet must not return a time",
            )
            writer = threading.Timer(
                0.3, lambda: serial.write_bytes(b"boot loading\n" + READY)
            )
            writer.start()
            try:
                ready = probe.wait_serial(serial, READY, started, 1.0)
            finally:
                writer.cancel()
            self.assertIsNotNone(ready)
            self.assertGreaterEqual(
                ready - started, 0.25, "the wait returned before the marker was written"
            )

    def test_early_connect_would_time_out_and_is_not_attempted(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            serial = Path(directory) / "serial.log"
            serial.write_bytes(b"boot loading\n")
            started = time.monotonic()
            marker_written: list[float] = []

            def writer() -> None:
                time.sleep(0.3)
                # Record the intent time before the write: wait_serial may observe the file
                # before this thread appends, so the connect must be compared to the writer's
                # own ready time, not to a later append.
                marker_written.append(time.monotonic())
                serial.write_bytes(b"boot loading\n" + READY)

            calls: list[tuple[float, int]] = []

            def connect() -> None:
                now = time.monotonic()
                ready = bool(marker_written) and now >= marker_written[0]
                # A connect before the marker is the synthetic early client: it
                # would read 0 bytes and time out on the 8 s budget.
                calls.append((now - started, 1 if ready else 0))

            thread = threading.Thread(target=writer)
            thread.start()
            try:
                ready = probe.connect_after_ready(
                    serial, READY, started, 1.0, connect
                )
            finally:
                thread.join()
            self.assertIsNotNone(ready)
            self.assertEqual(len(calls), 1, "exactly one connect after the marker")
            delta, bytes_read = calls[0]
            self.assertEqual(
                bytes_read, 1,
                "the connect ran before the ready marker (would have timed out)",
            )
            self.assertGreaterEqual(delta, 0.25)


if __name__ == "__main__":
    unittest.main()
