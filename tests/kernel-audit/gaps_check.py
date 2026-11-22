# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""Register invariants for `docs/kernel/audit/gaps.tsv`.

The register is the audit's measured source of counts; this check rejects a
malformed, duplicated or under-specified row before the report can claim a
number.  It also pins the six SSH-cleanup rows so the register cannot drift
from tickets/kernel-cleanup.tsv (the ticket names K443, K444, K440 and K426).
"""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
GAPS = ROOT / "docs" / "kernel" / "audit" / "gaps.tsv"

STATUSES = {"PROVEN", "PARTIAL", "UNTESTED", "MISSING", "SUSPECT"}
SEVERITIES = {"P0", "P1", "P2"}
COLUMNS = 8
AREAS = {
    "syscall-abi",
    "usercopy",
    "process",
    "session",
    "signals",
    "mm",
    "vfs-scheme",
    "sched-smp",
    "time",
    "ipc",
    "drivers",
    "boot-loader",
    "security",
    "kstat-tracing",
    "robustness",
    "ssh-cleanup",
}
SSH_TICKETS = {"K443", "K444", "K440", "K426"}


def fail(message: str) -> None:
    print(f"gaps-check: {message}", file=sys.stderr)


def main() -> int:
    failures = 0
    rows: dict[tuple[str, str], int] = {}
    counts = {status: 0 for status in STATUSES}
    areas: dict[str, int] = {}
    ssh_rows = 0
    ssh_tickets: set[str] = set()

    if not GAPS.exists():
        fail(f"missing {GAPS.relative_to(ROOT)}")
        return 1

    with GAPS.open("r", encoding="utf-8") as handle:
        for number, line in enumerate(handle, 1):
            if line.startswith("#") or not line.strip():
                continue
            fields = line.rstrip("\n").split("\t")
            if len(fields) != COLUMNS:
                fail(f"line {number}: {len(fields)} columns, expected {COLUMNS}")
                failures += 1
                continue
            area, contract, normative, _raven, status, test, severity, ticket = fields
            for name, value in (
                ("area", area),
                ("contract", contract),
                ("normative", normative),
            ):
                if not value.strip():
                    fail(f"line {number}: empty {name}")
                    failures += 1
            if area not in AREAS:
                fail(f"line {number}: unknown area {area!r}")
                failures += 1
            if status not in STATUSES:
                fail(f"line {number}: bad status {status!r}")
                failures += 1
            else:
                counts[status] += 1
            if severity not in SEVERITIES:
                fail(f"line {number}: bad severity {severity!r}")
                failures += 1
            if status == "PROVEN" and test == "-":
                fail(f"line {number}: PROVEN without a test")
                failures += 1
            if status == "PROVEN" and test != "-":
                path = test.split("::", 1)[0]
                if not path or not (ROOT / path).exists():
                    fail(f"line {number}: test path does not exist: {path}")
                    failures += 1
            key = (area, contract)
            if key in rows:
                fail(f"line {number}: duplicate {area}/{contract} (first at line {rows[key]})")
                failures += 1
            else:
                rows[key] = number
            areas[area] = areas.get(area, 0) + 1
            if area == "ssh-cleanup":
                ssh_rows += 1
                for known in SSH_TICKETS:
                    if known in ticket:
                        ssh_tickets.add(known)

    for area in sorted(AREAS):
        if area not in areas:
            fail(f"missing mandatory area {area}")
            failures += 1
    if ssh_rows < 6:
        fail(f"ssh-cleanup has {ssh_rows} rows, expected at least 6")
        failures += 1
    missing = SSH_TICKETS - ssh_tickets
    if missing:
        fail(f"ssh-cleanup is missing rows for {sorted(missing)}")
        failures += 1

    total = sum(counts.values())
    print(
        "gaps-check: "
        + f"{total} rows, "
        + ", ".join(f"{status}={counts[status]}" for status in sorted(counts))
        + f", areas={len(areas)}"
    )
    if failures:
        print(f"gaps-check: FAIL ({failures} defect(s))", file=sys.stderr)
        return 1
    print("gaps-check: PASSED")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
