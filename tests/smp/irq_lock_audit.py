#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""S102 IRQ-mask lock inventory and regression check.

Scans kernel/src for `.lock()`/`.try_lock()` sites and every `IrqSpinLock`
declaration, classifies the site by lock family and IRQ reachability, and writes
the TSV consumed by docs/irq-lock-audit.md.

Usage:
    python3 tests/smp/irq_lock_audit.py --check
    python3 tests/smp/irq_lock_audit.py --out tests/smp/irq-lock-inventory.tsv
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
KERNEL = ROOT / "kernel" / "src"
DOC = ROOT / "docs" / "irq-lock-audit.md"
INVENTORY = ROOT / "tests" / "smp" / "irq-lock-inventory.tsv"

IRQ_LOCKS = {
    "QUEUES",
    "TURNSTILES",
    "CLIENT_THREAD",
    "CYCLICS",
    "ENTROPY_POOL",
    "IOAPICS",
    "COUNTER_LOCK",
    "waiters",
}

# Receivers whose holder/waiter contexts need an explicit proof (page-fault,
# timer/ISR, scheduler handoff, or a driver/VT-d fault path).
HIGH_RISK = {
    "ALLOCATOR",
    "SHARED_FRAMES",
    "HEAP",
    "CPUS",
    "REAP",
    "STACK_CACHE",
    "WAKE_EVENT",
    "CONSUMER",
    "UNIT",
    "DMA_DOMAIN",
    "BLOBS",
    "DEVICE_HINTS",
    "PLATFORM_OWNED",
}

TEST_ONLY = {"CLIENT_THREAD", "COUNTER_LOCK"}


def nearest_fn(lines: list[str], index: int) -> str:
    for line in reversed(lines[:index]):
        match = re.match(r"\s*(?:pub\s+)?(?:unsafe\s+)?fn\s+([A-Za-z0-9_]+)", line)
        if match:
            return match.group(1)
    return "?"


def classify(path: Path, receiver: str, line_no: int, lines: list[str]) -> str:
    base = receiver.split(".")[-1]
    if base in TEST_ONLY:
        return "TEST-ONLY"
    if base in IRQ_LOCKS:
        return "IRQ-SAFE"
    if base in HIGH_RISK or "vtd.rs" in str(path) or "memory/frame" in str(path) or "memory/heap.rs" in str(path) or "dtrace/" in str(path) or "CPUS" in " ".join(lines[max(0, line_no - 2):line_no]):
        return "NEEDS-PROOF"
    return "SAFE"


def scan() -> list[tuple[str, int, str, str, str, str]]:
    rows = []
    for path in sorted(KERNEL.rglob("*.rs")):
        lines = path.read_text(encoding="utf-8").splitlines()
        rel = str(path.relative_to(ROOT))
        for index, line in enumerate(lines):
            if ".lock()" not in line and ".try_lock()" not in line:
                continue
            match = re.search(r"([A-Za-z_][\w\.]*)\s*\.(?:try_)?lock\(\)", line)
            receiver = match.group(1) if match else "?"
            base = receiver.split(".")[-1]
            family = "irq-spinlock" if base in IRQ_LOCKS else "spin-mutex"
            cls = classify(path, receiver, index + 1, lines)
            rows.append(
                (rel, index + 1, receiver, nearest_fn(lines, index), family, cls)
            )
    return rows


def irq_declarations() -> list[tuple[str, int, str]]:
    found = []
    for path in sorted(KERNEL.rglob("*.rs")):
        for index, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
            match = re.search(r"(?:static|pub)\s+([A-Za-z_][\w]*)\s*:\s*IrqSpinLock", line)
            if match:
                found.append((str(path.relative_to(ROOT)), index, match.group(1)))
    return found


def write_inventory(rows: list[tuple[str, int, str, str, str, str]]) -> None:
    INVENTORY.parent.mkdir(parents=True, exist_ok=True)
    with INVENTORY.open("w", encoding="utf-8") as handle:
        handle.write("file\tline\treceiver\tfunction\tfamily\tclassification\n")
        for row in rows:
            handle.write("\t".join(str(value) for value in row) + "\n")
    print(f"irq-lock-audit: wrote {INVENTORY.relative_to(ROOT)} ({len(rows)} sites)")


def check(rows: list[tuple[str, int, str, str, str, str]]) -> int:
    if not DOC.is_file():
        print(f"irq-lock-audit: missing {DOC.relative_to(ROOT)}", file=sys.stderr)
        return 1
    text = DOC.read_text(encoding="utf-8")
    missing = [name for _, _, name in irq_declarations() if name not in text]
    if missing:
        print(f"irq-lock-audit: undocumented IrqSpinLock: {', '.join(missing)}", file=sys.stderr)
        return 1
    high = [row for row in rows if row[5] == "NEEDS-PROOF"]
    for file, line, receiver, function, _, _ in high:
        if f"`{file}:{line}`" not in text:
            print(
                f"irq-lock-audit: high-risk site not documented: {file}:{line} {receiver} {function}",
                file=sys.stderr,
            )
            return 1
    print(
        f"irq-lock-audit: PASS ({len(rows)} sites, "
        f"{sum(1 for r in rows if r[5] == 'IRQ-SAFE')} irq-safe, "
        f"{len(high)} needs-proof, 0 undocumented)"
    )
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--out", type=Path, default=INVENTORY)
    args = parser.parse_args()
    rows = scan()
    if args.check:
        return check(rows)
    write_inventory(rows)
    return 0


if __name__ == "__main__":
    sys.exit(main())
