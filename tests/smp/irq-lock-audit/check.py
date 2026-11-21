#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""S102 IRQ-mask lock inventory and regression check.

The audit covers every lock object and every ``.lock()``/``.try_lock()``
call site in ``kernel/src/**``.  The checked-in ``inventory.tsv`` is the
machine-readable result; ``docs/irq-lock-audit.md`` carries the review
narrative and the classification for every non-safe site.

``--check`` fails when:

* a lock declaration is missing from the audit document;
* a call site is missing from, or stale in, ``inventory.tsv``;
* a row has no reviewed classification; or
* a ``NEEDS-PROOF``/``UNSAFE`` row is not cited in the audit document.

``--write`` regenerates the inventory from the policy in this file.  A new
lock or call site is classified by the policy and must be reviewed before
the document can be merged.

Usage:
    python3 tests/smp/irq-lock-audit/check.py --check
    python3 tests/smp/irq-lock-audit/check.py --write
    python3 tests/smp/irq-lock-audit/check.py --dump
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
KERNEL = ROOT / "kernel" / "src"
DOC = ROOT / "docs" / "irq-lock-audit.md"
INVENTORY = ROOT / "tests" / "smp" / "irq-lock-audit" / "inventory.tsv"

# The only interrupt-masking lock type in the tree.
IRQ_NAMES = {
    "QUEUES",
    "TURNSTILES",
    "CLIENT_THREAD",
    "CYCLICS",
    "ENTROPY_POOL",
    "IOAPICS",
    "COUNTER_LOCK",
    "waiters",
}
# log::SpinMutex: a plain spinlock with no IRQ masking and no preempt guard.
SPIN_NAMES = {"SERIAL", "ALLOCATOR", "CPUS", "REAP"}
# The architecture signal scratch is extern `spin::Mutex`, same non-masking shape.
EXTERNAL_SPIN_NAMES = {"SIGNAL_CONTEXT"}
TEST_NAMES = {"M"}

# VT-d and driver locks are process-context today, but their fault/init paths
# need an explicit invariant before they may be called safe.
DRIVER_NAMES = {
    "UNIT",
    "iova",
    "tables",
    "context_tables",
    "domains",
    "by_sid",
    "table_frames",
    "DMA_DOMAIN",
    "BLOBS",
    "DEVICE_HINTS",
    "PLATFORM_OWNED",
    "WAKE_EVENT",
    "CONSUMER",
    "HEAP",
    "STACK_CACHE",
}

# Exact-condition rows: the closure supplied to WaitQueue::enqueue_if runs with
# the wait-queue IrqSpinLock held.  A blocking Mutex or allocation there can
# sleep with interrupts masked.  Each of these rows is inside such a closure.
# Line-pinned because only the condition row is under the queue lock; the
# function's first state lock is an ordinary process-context critical section.
UNSAFE_LINES = {
    ("kernel/src/syscall/event.rs", 239, "state"),
    ("kernel/src/syscall/record_lock.rs", 348, "TABLE"),
    ("kernel/src/syscall/process_ctl.rs", 1000, "ids"),
    ("kernel/src/syscall/process_ctl.rs", 1007, "ids"),
    ("kernel/src/syscall/process_ctl.rs", 1014, "ids"),
    ("kernel/src/syscall/process_ctl.rs", 1021, "ids"),
    ("kernel/src/syscall/process_ctl.rs", 1029, "state"),
    ("kernel/src/process/mod.rs", 733, "pending_signals"),
    ("kernel/src/process/handle.rs", 209, "state"),
    ("kernel/src/process/pipe.rs", 234, "state"),
    ("kernel/src/process/pipe.rs", 298, "state"),
    ("kernel/src/process/duplex.rs", 563, "state"),
    ("kernel/src/process/duplex.rs", 660, "state"),
    ("kernel/src/process/duplex.rs", 789, "state"),
    ("kernel/src/process/duplex.rs", 896, "state"),
}

# The wait-queue API itself runs a caller closure under an IRQ-mask lock.
UNSAFE_SITES = {
    ("kernel/src/sched/wait.rs", "enqueue_if", "waiters"),
    # Nested locks reached from the `wait_once` condition's readiness scan.
    ("kernel/src/syscall/event.rs", "readiness", "handles"),
    ("kernel/src/syscall/event.rs", "readiness", "state"),
    ("kernel/src/syscall/event.rs", "readiness", "scheme_queue"),
    ("kernel/src/process/pipe.rs", "readiness", "state"),
    ("kernel/src/process/duplex.rs", "readiness", "state"),
    # Nested locks reached from the `children_for` condition.
    ("kernel/src/process/mod.rs", "lookup_pid", "PID_REGISTRY"),
    ("kernel/src/process/mod.rs", "processes", "PID_REGISTRY"),
}

UNSAFE_FAMILIES = {"SERIAL", "REAP", "SHARED_FRAMES"}
NEEDS_PROOF_FAMILIES = {"ALLOCATOR", "CPUS", "SIGNAL_CONTEXT"} | DRIVER_NAMES

# IrqSpinLock critical sections that allocate an allocator-backed container.
# The allocation can take the blocking heap Mutex with interrupts masked; the
# invariant must be proven or the allocation moved outside the lock.
NEEDS_PROOF_SITES = {
    ("kernel/src/event/mod.rs", "register_queue", "QUEUES"),
    ("kernel/src/sync.rs", "register_thread", "TURNSTILES"),
    ("kernel/src/sync.rs", "donation_begin_call", "TURNSTILES"),
    ("kernel/src/sync.rs", "lock", "TURNSTILES"),
    ("kernel/src/sync.rs", "unlock", "TURNSTILES"),
    ("kernel/src/sync.rs", "lockstat", "TURNSTILES"),
    ("kernel/src/time.rs", "cyclic_add", "CYCLICS"),
}

DECL_RE = re.compile(
    r"^[ \t]*(?:pub(?:\([^)]*\))?\s+)?static\s+(?:mut\s+)?([A-Za-z_]\w*)\s*:\s*(.*)$",
    re.M,
)
FIELD_RE = re.compile(
    r"^[ \t]*(?:pub(?:\([^)]*\))?\s+)?([A-Za-z_]\w*)\s*:\s*"
    r"(IrqSpinLock|SpinMutex|spin::Mutex|Mutex)\s*<",
    re.M,
)
CALL_RE = re.compile(r"([A-Za-z_]\w*)(?:\[[^\]]*\])?\s*\.\s*(try_lock|lock)\s*\(\s*\)")
# The scheduler takes a per-CPU slot through `CPUS.get(...).unwrap_or(&CPUS[0]).lock()`;
# the receiver is a method chain, so the generic call regex cannot see it.
CPUS_CALL_RE = re.compile(r"CPUS[^\n]*?\.\s*(try_lock|lock)\s*\(\s*\)")


def family_of_type(ty: str) -> str:
    if "IrqSpinLock" in ty:
        return "irq-spinlock"
    if "SpinMutex" in ty:
        return "spin-mutex"
    if "spin::Mutex" in ty:
        return "external-spin"
    return "blocking-mutex"


def nearest_fn(lines: list[str], index: int) -> str:
    for line in reversed(lines[:index]):
        match = re.match(
            r"\s*(?:pub(?:\([^)]*\))?\s+)?(?:const\s+)?(?:unsafe\s+)?"
            r"(?:extern\s+\"C\"\s+)?fn\s+([A-Za-z0-9_]+)",
            line,
        )
        if match:
            return match.group(1)
    return "?"


def declarations() -> list[tuple[str, int, str, str]]:
    found: list[tuple[str, int, str, str]] = []
    for path in sorted(KERNEL.rglob("*.rs")):
        text = path.read_text(encoding="utf-8")
        rel = str(path.relative_to(ROOT))
        for match in DECL_RE.finditer(text):
            ty = match.group(2).strip()
            if any(token in ty for token in ("IrqSpinLock", "SpinMutex", "spin::Mutex", "Mutex")):
                line = text[: match.start()].count("\n") + 1
                found.append((rel, line, match.group(1), family_of_type(ty)))
        for match in FIELD_RE.finditer(text):
            line = text[: match.start()].count("\n") + 1
            found.append((rel, line, match.group(1), family_of_type(match.group(2))))
    # A declaration and its initializer can both match the static regex only once;
    # deduplicate identical file/line/name/type tuples.
    return sorted(set(found))


def calls() -> list[tuple[str, int, str, str, str]]:
    found: list[tuple[str, int, str, str, str]] = []
    for path in sorted(KERNEL.rglob("*.rs")):
        text = path.read_text(encoding="utf-8")
        lines = text.splitlines()
        rel = str(path.relative_to(ROOT))
        for match in CALL_RE.finditer(text):
            receiver = match.group(1)
            line = text[: match.start(2)].count("\n") + 1
            found.append((rel, line, receiver, nearest_fn(lines, line - 1), match.group(2)))
        for match in CPUS_CALL_RE.finditer(text):
            line = text[: match.start(1)].count("\n") + 1
            if not any(row[0] == rel and row[1] == line and row[2] == "CPUS" for row in found):
                found.append((rel, line, "CPUS", nearest_fn(lines, line - 1), match.group(1)))
    return sorted(found)


def classify(
    file: str, line: int, receiver: str, function: str, call: str, family: str
) -> str:
    if file.endswith("_ut.rs") or (receiver in TEST_NAMES and file == "kernel/src/sync.rs"):
        return "TEST-ONLY"
    if function == "hold_lock_for_test":
        return "TEST-ONLY"
    if call == "try_lock" and receiver == "SERIAL":
        return "SAFE"
    if (file, line, receiver) in UNSAFE_LINES:
        return "UNSAFE"
    if (file, function, receiver) in UNSAFE_SITES:
        return "UNSAFE"
    if receiver in UNSAFE_FAMILIES:
        return "UNSAFE"
    if (file, function, receiver) in NEEDS_PROOF_SITES:
        return "NEEDS-PROOF"
    if receiver in NEEDS_PROOF_FAMILIES:
        return "NEEDS-PROOF"
    if receiver == "address_space" and function == "resolve_page_fault":
        return "NEEDS-PROOF"
    if family == "irq-spinlock":
        return "IRQ-SAFE"
    if family in {"spin-mutex", "external-spin"}:
        return "NEEDS-PROOF"
    return "SAFE"


def rows() -> list[tuple[str, int, str, str, str, str]]:
    families = {name: family for _, _, name, family in declarations()}
    items = []
    for file, line, receiver, function, call in calls():
        family = families.get(receiver)
        if family is None:
            if receiver in IRQ_NAMES:
                family = "irq-spinlock"
            elif receiver in SPIN_NAMES:
                family = "spin-mutex"
            elif receiver in EXTERNAL_SPIN_NAMES:
                family = "external-spin"
            else:
                family = "blocking-mutex"
        items.append(
            (
                file,
                line,
                receiver,
                function,
                call,
                classify(file, line, receiver, function, call, family),
            )
        )
    return items


def write_inventory(items: list[tuple[str, int, str, str, str, str]]) -> None:
    INVENTORY.parent.mkdir(parents=True, exist_ok=True)
    with INVENTORY.open("w", encoding="utf-8") as handle:
        handle.write("file\tline\treceiver\tfunction\tcall\tclassification\n")
        for item in items:
            handle.write("\t".join(str(value) for value in item) + "\n")
    print(f"irq-lock-audit: wrote {INVENTORY.relative_to(ROOT)} ({len(items)} sites)")


def read_inventory() -> list[tuple[str, int, str, str, str, str]]:
    if not INVENTORY.is_file():
        return []
    lines = INVENTORY.read_text(encoding="utf-8").splitlines()
    if not lines or lines[0] != "file\tline\treceiver\tfunction\tcall\tclassification":
        return []
    parsed = []
    for line in lines[1:]:
        cells = line.split("\t")
        if len(cells) != 6:
            return []
        try:
            parsed.append((cells[0], int(cells[1]), cells[2], cells[3], cells[4], cells[5]))
        except ValueError:
            return []
    return parsed


def check() -> int:
    if not DOC.is_file():
        print(f"irq-lock-audit: missing {DOC.relative_to(ROOT)}", file=sys.stderr)
        return 1
    text = DOC.read_text(encoding="utf-8")

    missing_decls = [
        f"{file}:{line} {name}"
        for file, line, name, _ in declarations()
        if f"{file}:{line}" not in text
    ]
    if missing_decls:
        for item in missing_decls:
            print(f"irq-lock-audit: undocumented lock declaration: {item}", file=sys.stderr)
        return 1

    current = rows()
    recorded = read_inventory()
    if not recorded:
        print(f"irq-lock-audit: missing or unreadable {INVENTORY.relative_to(ROOT)}", file=sys.stderr)
        return 1
    current_keys = {(file, line, receiver, function, call) for file, line, receiver, function, call, _ in current}
    recorded_keys = {(file, line, receiver, function, call) for file, line, receiver, function, call, _ in recorded}
    for key in sorted(current_keys - recorded_keys):
        print(
            "irq-lock-audit: new/changed lock site not in inventory: "
            f"{key[0]}:{key[1]} {key[2]} {key[3]} {key[4]}",
            file=sys.stderr,
        )
    for key in sorted(recorded_keys - current_keys):
        print(
            "irq-lock-audit: stale inventory row: "
            f"{key[0]}:{key[1]} {key[2]} {key[3]} {key[4]}",
            file=sys.stderr,
        )
    if current_keys != recorded_keys:
        return 1

    allowed = {"SAFE", "IRQ-SAFE", "TEST-ONLY", "NEEDS-PROOF", "UNSAFE"}
    for file, line, receiver, function, call, classification in recorded:
        if classification not in allowed:
            print(
                f"irq-lock-audit: unreviewed classification {classification!r} at "
                f"{file}:{line} {receiver} {function}",
                file=sys.stderr,
            )
            return 1
        if classification in {"NEEDS-PROOF", "UNSAFE"} and f"{file}:{line}" not in text:
            print(
                f"irq-lock-audit: {classification} site not cited in {DOC.name}: "
                f"{file}:{line} {receiver} {function}",
                file=sys.stderr,
            )
            return 1

    for marker in ("lock_cpu(", "enqueue_if(", "push_reap(", "reap_thread("):
        for path in sorted(KERNEL.rglob("*.rs")):
            file_lines = path.read_text(encoding="utf-8").splitlines()
            rel = str(path.relative_to(ROOT))
            for index, line in enumerate(file_lines, 1):
                if marker in line and f"{rel}:{index}" not in text:
                    print(
                        f"irq-lock-audit: wrapper site not cited in {DOC.name}: "
                        f"{rel}:{index} {marker}",
                        file=sys.stderr,
                    )
                    return 1

    print(
        f"irq-lock-audit: PASS ({len(current)} sites, "
        f"{sum(1 for row in current if row[5] == 'IRQ-SAFE')} irq-safe, "
        f"{sum(1 for row in current if row[5] == 'UNSAFE')} unsafe, "
        f"{sum(1 for row in current if row[5] == 'NEEDS-PROOF')} needs-proof, "
        f"{sum(1 for row in current if row[5] == 'TEST-ONLY')} test-only)"
    )
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="verify the document and inventory")
    parser.add_argument("--write", action="store_true", help="regenerate the inventory")
    parser.add_argument("--dump", action="store_true", help="print the inventory to stdout")
    args = parser.parse_args()
    if args.write:
        write_inventory(rows())
        return 0
    if args.dump:
        for row in rows():
            print("\t".join(str(value) for value in row))
        return 0
    return check()


if __name__ == "__main__":
    sys.exit(main())
