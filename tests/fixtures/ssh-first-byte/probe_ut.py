#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

"""Unit tests for the first-byte probe's sshd-ready wait.

Run from the fixture directory: ``python3 -m unittest probe_ut``.
No QEMU is started: a temporary serial file and a timed writer stand in for the
guest.  The synthetic early connect is the negative: a connect before the ready
marker would read 0 bytes and be logged as a banner failure.

``ImageAlignmentTest`` is the case/image contract: the qtest case must boot the
same image the pre-step probe boots, and that image must be built from the
current tree.  A stale or foreign image can carry a different console login (the
release root password), which is the K567 case-wrapper failure: the smoke waits
for a password prompt the image never prints.
"""

from __future__ import annotations

import re
import tempfile
import threading
import time
import unittest
from pathlib import Path

import probe

READY = b"svcd: sshd up\n"
HERE = Path(__file__).resolve().parent


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


class FailurePathTest(unittest.TestCase):
    """An empty serial must never be the whole failure.

    The rev 348 failure was ``pre[0] RuntimeError: sshd service never started`` with a
    0-byte ``serial.log``: QEMU died before the guest booted (tap missing or TUNSETIFF
    denied) and the probe waited the full 180 s on a silent file.  These tests pin the
    real diagnostics: the tap is checked first, a QEMU exit is reported with its stderr,
    and the serial is seeded with the run context.
    """

    def test_missing_tap_is_named_before_qemu(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with self.assertRaises(RuntimeError) as caught:
                probe.ensure_tap("k567-tap", root=root)
            message = str(caught.exception)
            self.assertIn("k567-tap", message)
            self.assertIn("qemu-net.sh up", message)
            (root / "k567-tap").mkdir()
            probe.ensure_tap("k567-tap", root=root)

    def test_qemu_exit_before_ready_carries_its_error(self) -> None:
        class Exited:
            def poll(self) -> int:
                return 1

        with tempfile.TemporaryDirectory() as directory:
            serial = Path(directory) / "serial.log"
            serial.write_bytes(b"")
            stderr = Path(directory) / "qemu.log"
            stderr.write_text(
                "qemu-system-x86_64: -netdev tap: TUNSETIFF failed: Operation not permitted\n"
            )
            with self.assertRaises(RuntimeError) as caught:
                probe.wait_serial(
                    serial, READY, time.monotonic(), 1.0,
                    child=Exited(), stderr_path=stderr,
                )
            message = str(caught.exception)
            self.assertIn("qemu exited", message)
            self.assertIn("TUNSETIFF", message)

    def test_serial_header_is_written_before_qemu(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            serial = Path(directory) / "serial.log"
            probe.write_serial_header(
                serial, "build/qtest/x.img", "ravendemo0",
                ["qemu-system-x86_64", "-m", "512M"],
            )
            text = serial.read_text(encoding="utf-8")
            self.assertIn("build/qtest/x.img", text)
            self.assertIn("ravendemo0", text)
            self.assertIn("qemu-system-x86_64", text)


class ImageAlignmentTest(unittest.TestCase):
    """The case, the fixture and the probe name one freshly built image."""

    def _case_image(self) -> str:
        text = (HERE / "ssh-first-byte.toml").read_text(encoding="utf-8")
        match = re.search(r"^image\s*=\s*\"([^\"]+)\"\s*$", text, re.MULTILINE)
        self.assertIsNotNone(match, "the case declares no image")
        return match.group(1)

    def _run_sh(self) -> str:
        return (HERE / "run.sh").read_text(encoding="utf-8")

    def test_case_boots_the_image_the_probe_boots(self) -> None:
        case_image = self._case_image()
        run_sh = self._run_sh()
        image = re.search(r"^image=(\S+)\s*$", run_sh, re.MULTILINE)
        self.assertIsNotNone(
            image, "run.sh does not name the image it builds and probes"
        )
        self.assertEqual(
            image.group(1),
            case_image,
            "the qtest case and the fixture probe must boot the same image",
        )
        self.assertIn(
            'RAVEN_SSH_IMAGE="$image"',
            run_sh,
            "the probe must be pointed at the case image, not its own default",
        )

    def test_image_is_built_from_the_release_profile_at_head(self) -> None:
        run_sh = self._run_sh()
        self.assertRegex(
            run_sh,
            r"cargo xtask image --arch x86_64 --profile release\b",
            "the fixture image must be the release profile",
        )
        self.assertRegex(
            run_sh,
            r"--output \"\$image\"",
            "the fixture image must be written to the case image path",
        )
        self.assertRegex(
            run_sh,
            r"rev-parse HEAD",
            "the image stamp must track the tree revision",
        )

    def test_first_byte_bound_is_not_loosened(self) -> None:
        self.assertEqual(
            probe.FIRST_BYTE_BOUND,
            8.0,
            "the first-byte assertion stays at the measured 8 s budget",
        )


if __name__ == "__main__":
    unittest.main()
