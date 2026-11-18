#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""Bounded before/after reconciliation for `libc-size` JSON runs.

Usage:

    python3 tests/libc/size/reconcile.py --before old.json --after new.json
    python3 tests/libc/size/reconcile.py --self-test

The helper matches static Rust rows by (arch, linkage, set, lang, probe),
prints the C74z-S12 legitimate-growth rows with the proposed `current + 3%`
budgets, and lists the shrink-first rows without ever proposing a raise. It
does not read or write `BUDGETS.toml`.
"""

from __future__ import annotations

import argparse
import json
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import NoReturn

MAX_BYTES = 8 * 1024 * 1024
MAX_ROWS = 10_000
HEADROOM_PERCENT = 3

STATIC = "static"
RUST = "rust"

LEGIT_PROBES = frozenset({"P0", "P1", "P4", "P6"})
SHRINK_PROBES = frozenset({"P2", "P5", "P7"})
P3_LEGIT_ARCHES = frozenset({"x86_64", "aarch64"})
P3_SHRINK_ARCHES = frozenset({"riscv64"})
HELD_PROBES = frozenset({"P8", "P9"})

Key = tuple[str, str, str, str, str]


@dataclass(frozen=True)
class Row:
    """One measured static Rust row."""

    arch: str
    linkage: str
    set_name: str
    lang: str
    probe: str
    s: int
    b: int
    f: int

    @property
    def key(self) -> Key:
        return (self.arch, self.linkage, self.set_name, self.lang, self.probe)

    def describe(self) -> str:
        return f"{self.arch} {self.set_name} {self.probe}"


@dataclass(frozen=True)
class Entry:
    """A matched before/after pair with its C74z-S12 class."""

    before: Row
    after: Row
    kind: str
    reason: str

    @property
    def changed(self) -> bool:
        return (
            self.before.s != self.after.s
            or self.before.b != self.after.b
            or self.before.f != self.after.f
        )


@dataclass
class Reconciliation:
    """All matched and unmatched rows from one before/after comparison."""

    legit: list[Entry]
    shrink: list[Entry]
    held: list[Entry]
    missing: list[Row]
    added: list[Row]
    ignored: int

    @property
    def empty(self) -> bool:
        return not (
            any(entry.changed for entry in self.legit)
            or any(entry.changed for entry in self.shrink)
            or any(entry.changed for entry in self.held)
            or self.missing
            or self.added
        )


def fail(message: str) -> NoReturn:
    print(f"reconcile: {message}", file=sys.stderr)
    raise SystemExit(2)


def proposed(value: int) -> int:
    """Return ceil(value * 103 / 100), the budget headroom rule."""

    return (value * (100 + HEADROOM_PERCENT) + 99) // 100


def classify(probe: str, arch: str) -> tuple[str, str]:
    """Return the C74z-S12 class and reason for one row."""

    if probe in LEGIT_PROBES:
        return "legit", "C74z-S12 legitimate growth"
    if probe == "P3" and arch in P3_LEGIT_ARCHES:
        return "legit", "C74z-S12 legitimate growth (x86/aarch64 P3)"
    if probe in SHRINK_PROBES:
        return "shrink", "C74z-S12 shrink-first"
    if probe == "P3" and arch in P3_SHRINK_ARCHES:
        return "shrink", "C74z-S13 riscv P3 measurement first"
    if probe in HELD_PROBES:
        return "held", "outside the S16 proposal"
    return "held", "unclassified row"


def load_run(path: Path) -> tuple[dict[Key, Row], int]:
    """Load one bounded `libc-size` JSON run and its ignored-row count."""

    try:
        size = path.stat().st_size
    except OSError as exc:
        fail(f"cannot stat {path}: {exc}")
    if size > MAX_BYTES:
        fail(f"{path}: {size} bytes exceeds the {MAX_BYTES}-byte bound")
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as exc:
        fail(f"cannot read {path}: {exc}")
    if not isinstance(data, dict) or data.get("schema") != 1:
        fail(f"{path}: expected a libc-size schema 1 object")
    raw_rows = data.get("rows")
    if not isinstance(raw_rows, list):
        fail(f"{path}: missing rows array")
    if len(raw_rows) > MAX_ROWS:
        fail(f"{path}: {len(raw_rows)} rows exceeds the {MAX_ROWS}-row bound")

    rows: dict[Key, Row] = {}
    ignored = 0
    for raw in raw_rows:
        if not isinstance(raw, dict):
            fail(f"{path}: non-object row")
        if raw.get("linkage") != STATIC or raw.get("lang") != RUST:
            ignored += 1
            continue
        if raw.get("status", "ok") != "ok":
            ignored += 1
            continue
        try:
            row = Row(
                arch=str(raw["arch"]),
                linkage=str(raw["linkage"]),
                set_name=str(raw["set"]),
                lang=str(raw["lang"]),
                probe=str(raw["probe"]),
                s=int(raw["S"]),
                b=int(raw["B"]),
                f=int(raw["F"]),
            )
        except (KeyError, TypeError, ValueError) as exc:
            fail(f"{path}: malformed static Rust row: {exc}")
        if row.key in rows:
            fail(f"{path}: duplicate row {row.key}")
        if row.s < 0 or row.b < 0 or row.f < 0:
            fail(f"{path}: negative size on {row.describe()}")
        rows[row.key] = row
    return rows, ignored


def reconcile(
    before: dict[Key, Row], after: dict[Key, Row], ignored: int = 0
) -> Reconciliation:
    """Match two runs and classify every pair."""

    legit: list[Entry] = []
    shrink: list[Entry] = []
    held: list[Entry] = []
    missing: list[Row] = []
    added: list[Row] = []

    for key in sorted(set(before) | set(after)):
        old = before.get(key)
        new = after.get(key)
        if old is None:
            assert new is not None
            added.append(new)
            continue
        if new is None:
            missing.append(old)
            continue
        kind, reason = classify(new.probe, new.arch)
        entry = Entry(before=old, after=new, kind=kind, reason=reason)
        if kind == "legit":
            legit.append(entry)
        elif kind == "shrink":
            shrink.append(entry)
        else:
            held.append(entry)

    return Reconciliation(
        legit=legit,
        shrink=shrink,
        held=held,
        missing=missing,
        added=added,
        ignored=ignored,
    )


def delta(before: int, after: int) -> str:
    return f"{before}->{after} ({after - before:+})"


def print_entry(entry: Entry) -> None:
    old = entry.before
    new = entry.after
    prior_s = proposed(old.s)
    prior_f = proposed(old.f)
    prior_b = proposed(old.b)
    next_s = proposed(new.s)
    next_f = proposed(new.f)
    next_b = proposed(new.b)
    print(
        f"{entry.before.describe():<20} "
        f"S {delta(old.s, new.s)} "
        f"F {delta(old.f, new.f)} "
        f"B {delta(old.b, new.b)}; "
        f"proposed S/F/B {next_s}/{next_f}/{next_b}; "
        f"budget dS/dF/dB {next_s - prior_s:+}/{next_f - prior_f:+}/{next_b - prior_b:+}"
    )


def print_report(
    before_path: Path, after_path: Path, result: Reconciliation
) -> None:
    print("C74bw size reconciliation")
    print(f"before: {before_path}")
    print(f"after:  {after_path}")
    print(
        f"matched static Rust rows: {len(result.legit) + len(result.shrink) + len(result.held)}"
        f" (ignored {result.ignored} non-static/non-Rust/skipped rows)"
    )

    changed_legit = [entry for entry in result.legit if entry.changed]
    print("\nLEGITIMATE GROWTH (proposed current + 3%; not applied)")
    if changed_legit:
        for entry in changed_legit:
            print_entry(entry)
    else:
        print("  (none: legitimate rows are byte-identical)")

    print("\nSHRINK-FIRST (listed, never auto-raised)")
    if result.shrink:
        for entry in result.shrink:
            print(
                f"{entry.before.describe():<20} "
                f"S {delta(entry.before.s, entry.after.s)} "
                f"F {delta(entry.before.f, entry.after.f)} "
                f"B {delta(entry.before.b, entry.after.b)}; "
                f"HOLD ({entry.reason})"
            )
    else:
        print("  (none matched)")

    changed_held = [entry for entry in result.held if entry.changed]
    if changed_held:
        print("\nHELD (outside the S16 proposal; no budget)")
        for entry in changed_held:
            print(
                f"{entry.before.describe():<20} "
                f"S {delta(entry.before.s, entry.after.s)} "
                f"F {delta(entry.before.f, entry.after.f)} "
                f"B {delta(entry.before.b, entry.after.b)}; "
                f"HOLD ({entry.reason})"
            )

    if result.missing:
        print("\nMISSING FROM AFTER")
        for row in result.missing:
            print(f"  {row.describe()} S/B/F {row.s}/{row.b}/{row.f}")
    if result.added:
        print("\nADDED IN AFTER")
        for row in result.added:
            print(f"  {row.describe()} S/B/F {row.s}/{row.b}/{row.f}")

    if result.empty:
        print("\nEMPTY DIFF: every matched static Rust row is byte-identical.")
    else:
        print("\nPROPOSAL ONLY: no BUDGETS.toml change was made.")


def self_test() -> None:
    """Exercise the empty, legitimate-shrink, shrink-first and edge cases."""

    def row(
        probe: str,
        arch: str,
        set_name: str,
        s: int,
        b: int,
        f: int,
    ) -> Row:
        return Row(arch, STATIC, set_name, RUST, probe, s, b, f)

    same = {row("P0", "x86_64", "tiny", 1000, 500, 1100).key: row(
        "P0", "x86_64", "tiny", 1000, 500, 1100)}
    if not reconcile(same, same).empty:
        raise AssertionError("identical runs must produce an empty diff")

    before = {row("P0", "x86_64", "tiny", 1000, 500, 1100).key: row(
        "P0", "x86_64", "tiny", 1000, 500, 1100)}
    after = {row("P0", "x86_64", "tiny", 900, 400, 1000).key: row(
        "P0", "x86_64", "tiny", 900, 400, 1000)}
    result = reconcile(before, after)
    if len(result.legit) != 1 or not result.legit[0].changed:
        raise AssertionError("P0 shrink must be a legitimate-growth row")
    entry = result.legit[0]
    diffs = (
        proposed(entry.after.s) - proposed(entry.before.s),
        proposed(entry.after.f) - proposed(entry.before.f),
        proposed(entry.after.b) - proposed(entry.before.b),
    )
    if diffs != (-103, -103, -103):
        raise AssertionError(f"unexpected budget diff: {diffs}")

    before_p2 = {row("P2", "x86_64", "tiny", 1000, 500, 1100).key: row(
        "P2", "x86_64", "tiny", 1000, 500, 1100)}
    after_p2 = {row("P2", "x86_64", "tiny", 900, 400, 1000).key: row(
        "P2", "x86_64", "tiny", 900, 400, 1000)}
    result_p2 = reconcile(before_p2, after_p2)
    if result_p2.legit or len(result_p2.shrink) != 1:
        raise AssertionError("P2 must be shrink-first, never a budget proposal")

    p3_x86 = {row("P3", "x86_64", "tiny", 1000, 500, 1100).key: row(
        "P3", "x86_64", "tiny", 1000, 500, 1100)}
    p3_riscv = {row("P3", "riscv64", "tiny", 1000, 500, 1100).key: row(
        "P3", "riscv64", "tiny", 1000, 500, 1100)}
    if not reconcile(p3_x86, p3_x86).legit:
        raise AssertionError("P3 x86_64 must be legitimate")
    if not reconcile(p3_riscv, p3_riscv).shrink:
        raise AssertionError("P3 riscv64 must be shrink-first")

    before_missing = {row("P1", "x86_64", "tiny", 1, 1, 1).key: row(
        "P1", "x86_64", "tiny", 1, 1, 1)}
    after_added = {row("P4", "x86_64", "tiny", 1, 1, 1).key: row(
        "P4", "x86_64", "tiny", 1, 1, 1)}
    edge = reconcile(before_missing, after_added)
    if not edge.missing or not edge.added:
        raise AssertionError("missing/added rows must be reported")

    print("self-test: PASS (empty, P0 +3%, P2 hold, P3 arch split, missing/added)")


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(
        description="Bounded libc-size before/after reconciliation"
    )
    parser.add_argument("--before", type=Path, help="before libc-size JSON")
    parser.add_argument("--after", type=Path, help="after libc-size JSON")
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="run the built-in same-run/shrink/edge tests",
    )
    args = parser.parse_args(argv)

    if args.self_test:
        self_test()
        return 0
    if args.before is None or args.after is None:
        parser.error("--before and --after are required (or use --self-test)")

    try:
        before_rows, before_ignored = load_run(args.before)
        after_rows, after_ignored = load_run(args.after)
    except SystemExit as exc:
        return int(exc.code or 0)
    result = reconcile(before_rows, after_rows, before_ignored + after_ignored)
    print_report(args.before, args.after, result)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
