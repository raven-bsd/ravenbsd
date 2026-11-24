#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""Unit tests for scripts/contract-source-gate.py.

The fixtures build a throw-away repository, a locked donor checkout and a
ticket tree so the gate never needs the real ravenbsd-resources workspace.
Each test names the exact citation failure it proves: missing path, missing
symbol, wrong donor revision, missing lock, missing header and a missing
repository path.
"""

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
GATE = REPO / "scripts" / "contract-source-gate.py"

TICKET = """# T001 - fixture ticket

Lane: alice     Wave: 1     Milestone: M-0.1.0
Feature: Kernel
Feature-Order: 1
Item: Conformance
Item-Order: 1
Contract-source: {source}
Depends: none     Blocks: none
Estimated size: S     Type: test
Status: todo
Risk: fixture
"""


def git(cwd, *args):
    return subprocess.run(
        ["git", "-C", str(cwd), *args],
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()


class Fixture:
    def __init__(self, base):
        self.base = Path(base)
        self.root = self.base / "repo"
        self.resources = self.base / "resources"
        self.tickets = self.base / "tickets"
        self.donor = self.resources / "freebsd"
        for path in (self.root, self.resources, self.donor, self.tickets):
            path.mkdir(parents=True, exist_ok=True)

        (self.donor / "src").mkdir()
        (self.donor / "src" / "foo.c").write_text(
            "int foo(void) { return 0; }\nint bar(void) { return foo(); }\n",
            encoding="utf-8",
        )
        (self.donor / "src" / "bar.c").write_text("int bar(void);\n", encoding="utf-8")
        git(self.donor, "init", "-q")
        git(self.donor, "add", ".")
        git(
            self.donor,
            "-c",
            "user.name=fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-q",
            "-m",
            "fixture",
        )
        self.revision = git(self.donor, "rev-parse", "HEAD")
        self.write_lock(self.revision)

    def write_lock(self, revision):
        (self.resources / "donors.lock").write_text(
            f"freebsd  {revision}  A  https://example.invalid/freebsd.git\n",
            encoding="utf-8",
        )

    def ticket(self, source=None, name="T001.md", status="todo"):
        lane = self.tickets / "alice" / status
        lane.mkdir(parents=True, exist_ok=True)
        if source is None:
            body = "\n".join(
                line for line in TICKET.splitlines() if not line.startswith("Contract-source:")
            )
            body += "\n"
        else:
            body = TICKET.format(source=source)
        (lane / name).write_text(body, encoding="utf-8")

    def run_gate(self):
        command = [
            sys.executable,
            str(GATE),
            "--root",
            str(self.root),
            "--tickets",
            str(self.tickets),
            "--resources",
            str(self.resources),
            "--json",
        ]
        proc = subprocess.run(command, capture_output=True, text=True, timeout=15)
        try:
            payload = json.loads(proc.stdout)
        except json.JSONDecodeError:
            payload = None
        return proc, payload


class GateTests(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self._tmp.cleanup)
        self.fixture = Fixture(self._tmp.name)

    def test_locked_donor_path_and_symbol_pass(self):
        self.fixture.ticket("POSIX and FreeBSD `src/foo.c` (`foo`)")
        proc, payload = self.fixture.run_gate()
        self.assertEqual(proc.returncode, 0, proc.stderr + proc.stdout)
        self.assertEqual(payload["result"], "pass")
        self.assertEqual(payload["tickets_scanned"], 1)
        self.assertEqual(payload["citations"], 1)

    def test_missing_donor_path_fails(self):
        self.fixture.ticket("POSIX and FreeBSD `src/missing.c`")
        proc, payload = self.fixture.run_gate()
        self.assertEqual(proc.returncode, 1, proc.stderr + proc.stdout)
        self.assertIn("path", [item["kind"] for item in payload["mismatches"]])
        self.assertIn("src/missing.c", payload["mismatches"][0]["detail"])

    def test_missing_donor_symbol_fails(self):
        self.fixture.ticket("POSIX and FreeBSD `src/foo.c` (`not_in_the_file`)")
        proc, payload = self.fixture.run_gate()
        self.assertEqual(proc.returncode, 1, proc.stderr + proc.stdout)
        self.assertIn("symbol", [item["kind"] for item in payload["mismatches"]])

    def test_donor_head_behind_lock_fails(self):
        (self.fixture.donor / "src" / "bar.c").write_text(
            "int bar(void) { return 1; }\n", encoding="utf-8"
        )
        git(self.fixture.donor, "add", ".")
        git(
            self.fixture.donor,
            "-c",
            "user.name=fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-q",
            "-m",
            "second",
        )
        self.fixture.ticket("POSIX and FreeBSD `src/foo.c`")
        proc, payload = self.fixture.run_gate()
        self.assertEqual(proc.returncode, 1, proc.stderr + proc.stdout)
        self.assertIn("revision", [item["kind"] for item in payload["mismatches"]])

    def test_missing_donors_lock_fails_closed(self):
        (self.fixture.resources / "donors.lock").unlink()
        self.fixture.ticket("POSIX and FreeBSD `src/foo.c`")
        proc, _ = self.fixture.run_gate()
        self.assertEqual(proc.returncode, 2, proc.stderr + proc.stdout)

    def test_missing_ticket_header_fails(self):
        self.fixture.ticket(None)
        proc, payload = self.fixture.run_gate()
        self.assertEqual(proc.returncode, 1, proc.stderr + proc.stdout)
        self.assertIn("header", [item["kind"] for item in payload["mismatches"]])

    def test_done_ticket_without_header_is_legacy(self):
        self.fixture.ticket(None, status="done")
        proc, payload = self.fixture.run_gate()
        self.assertEqual(proc.returncode, 0, proc.stderr + proc.stdout)
        self.assertEqual(payload["legacy_skipped"], 1)

    def test_repository_path_is_checked_too(self):
        rows = self.fixture.root / "tests" / "abi" / "rows.txt"
        rows.parent.mkdir(parents=True)
        rows.write_text("row\n", encoding="utf-8")
        self.fixture.ticket("CONTRACTS.md C19 and `tests/abi/rows.txt`")
        proc, payload = self.fixture.run_gate()
        self.assertEqual(proc.returncode, 0, proc.stderr + proc.stdout)
        self.assertEqual(payload["result"], "pass")

    def test_missing_repository_path_fails(self):
        self.fixture.ticket("CONTRACTS.md C19 and `tests/abi/missing.txt`")
        proc, payload = self.fixture.run_gate()
        self.assertEqual(proc.returncode, 1, proc.stderr + proc.stdout)
        self.assertIn("path", [item["kind"] for item in payload["mismatches"]])


if __name__ == "__main__":
    unittest.main()
