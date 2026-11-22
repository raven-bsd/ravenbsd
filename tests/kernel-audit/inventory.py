# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""Reproducible kernel-surface inventory for the 0.1.0 gap audit.

Parses the kernel syscall dispatch table, the scheme opcode enum, the driver
entry points and the security-class `pub fn` surface.  Every emitted line is
`name<TAB>value/kind<TAB>path:line` so `gaps.tsv` can be checked against a
stable key.

`--check` exits 1 when a syscall in the kernel dispatch table has no
`SYS_<NAME>` row in `docs/kernel/audit/gaps.tsv`.
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
GAPS = ROOT / "docs" / "kernel" / "audit" / "gaps.tsv"

CONST_RE = re.compile(r"\bpub const ([A-Z][A-Z0-9_]*): usize = (0x[0-9A-Fa-f]+);")
ENUM_RE = re.compile(r"^\s*([A-Z][A-Z0-9_]*)\s*=\s*(0x[0-9A-Fa-f]+);")
RAW_IF_RE = re.compile(r"if number == ([A-Za-z0-9_:]+)::([A-Z][A-Z0-9_]*)")
SYSCALL_RE = re.compile(r"Syscall::([A-Z][A-Z0-9_]*)")

SECURITY_FILES = [
    "kernel/src/process/handle.rs",
    "kernel/src/process/status.rs",
    "kernel/src/process/exec.rs",
    "kernel/src/process/robust.rs",
    "kernel/src/process/priority.rs",
]
SECURITY_NAME_RE = re.compile(
    r"pub fn ([a-z0-9_]*(?:cred|uid|gid|group|limit|right|cap|chroot|setuid|setgid)[a-z0-9_]*)"
)


def rel(path: Path) -> str:
    return path.relative_to(ROOT).as_posix()


def iter_lines(path: Path):
    with path.open("r", encoding="utf-8", errors="replace") as handle:
        for number, line in enumerate(handle, 1):
            yield number, line.rstrip("\n")


def symbol_table() -> dict[str, tuple[str, str]]:
    """Map every ABI/syscall constant name to (value, path:line)."""
    table: dict[str, tuple[str, str]] = {}
    paths = sorted((ROOT / "crates" / "raven-abi" / "src").glob("*.rs"))
    paths += sorted((ROOT / "kernel" / "src" / "syscall").glob("*.rs"))
    for path in paths:
        for number, line in iter_lines(path):
            for match in CONST_RE.finditer(line):
                table.setdefault(match.group(1), (match.group(2), f"{rel(path)}:{number}"))
            match = ENUM_RE.match(line)
            if match:
                table.setdefault(match.group(1), (match.group(2), f"{rel(path)}:{number}"))
    return table


def abi_enum() -> list[tuple[str, str, str]]:
    """Every `raven-abi::syscall::Syscall` variant."""
    path = ROOT / "crates" / "raven-abi" / "src" / "syscall.rs"
    rows: list[tuple[str, str, str]] = []
    # The variants live in the `syscalls! { ... }` invocation, not the macro body.
    for number, line in iter_lines(path):
        match = ENUM_RE.match(line)
        if match:
            rows.append((match.group(1), match.group(2), f"{rel(path)}:{number}"))
    return rows


def syscall_dispatch() -> list[tuple[str, str, str]]:
    """Return (name, value, dispatch path:line) for every syscall in the table."""
    table = symbol_table()
    path = ROOT / "kernel" / "src" / "syscall" / "mod.rs"
    found: dict[str, tuple[str, str]] = {}
    fallback = f"{rel(path)}:325"
    for number, line in iter_lines(path):
        code = line.split("//", 1)[0]
        match = RAW_IF_RE.search(code)
        if match:
            name = match.group(2)
            value = table.get(name, ("?", "?"))[0]
            found.setdefault(name, (value, f"{rel(path)}:{number}"))
        for match in SYSCALL_RE.finditer(code):
            name = match.group(1)
            value = table.get(name, ("?", "?"))[0]
            found.setdefault(name, (value, f"{rel(path)}:{number}"))
        if "Some(call) => basic::dispatch(call, frame)" in code:
            fallback = f"{rel(path)}:{number}"
    basic = ROOT / "kernel" / "src" / "syscall" / "basic.rs"
    basic_sites: dict[str, str] = {}
    for number, line in iter_lines(basic):
        for match in SYSCALL_RE.finditer(line.split("//", 1)[0]):
            basic_sites.setdefault(match.group(1), f"{rel(basic)}:{number}")
    for name, value, _where in abi_enum():
        if name not in found:
            found[name] = (value, basic_sites.get(name, fallback))
    return [
        (name, value, where)
        for name, (value, where) in sorted(found.items(), key=lambda item: (item[1][0], item[0]))
    ]


def scheme_ops() -> list[tuple[str, str, str]]:
    path = ROOT / "kernel" / "src" / "ipc" / "protocol.rs"
    in_enum = False
    rows: list[tuple[str, str, str]] = []
    for number, line in iter_lines(path):
        if "pub enum Opcode {" in line:
            in_enum = True
            continue
        if in_enum and line.startswith("}"):
            break
        if in_enum:
            match = re.match(r"\s*([A-Z][A-Za-z0-9_]*)\s*=\s*(\d+),", line)
            if match:
                rows.append((match.group(1), match.group(2), f"{rel(path)}:{number}"))
    return rows


def driver_entries() -> list[tuple[str, str, str]]:
    rows: list[tuple[str, str, str]] = []
    for relative in ("kernel/src/syscall/driver.rs", "kernel/src/driver/mod.rs"):
        path = ROOT / relative
        for number, line in iter_lines(path):
            match = re.search(r"pub fn ([a-z0-9_]+)", line)
            if match:
                rows.append((match.group(1), "fn", f"{rel(path)}:{number}"))
    return rows


def security_entries() -> list[tuple[str, str, str]]:
    rows: list[tuple[str, str, str]] = []
    for relative in SECURITY_FILES:
        path = ROOT / relative
        if not path.exists():
            continue
        for number, line in iter_lines(path):
            match = re.search(r"pub fn ([a-z0-9_]+)", line)
            if match:
                rows.append((match.group(1), "fn", f"{rel(path)}:{number}"))
    path = ROOT / "kernel" / "src" / "process" / "mod.rs"
    for number, line in iter_lines(path):
        match = SECURITY_NAME_RE.search(line)
        if match:
            rows.append((match.group(1), "fn", f"{rel(path)}:{number}"))
    return rows


def commit() -> str:
    try:
        return subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True, stderr=subprocess.DEVNULL
        ).strip()
    except (OSError, subprocess.CalledProcessError):
        return "unknown"


def emit() -> str:
    lines = [
        "# K451 kernel inventory",
        f"# commit: {commit()}",
        "# name\tvalue/kind\tpath:line",
        "",
        "[SYSSCALLS]",
    ]
    for name, value, where in syscall_dispatch():
        lines.append(f"{name}\t{value}\t{where}")
    lines += ["", "[SCHEME_OPS]"]
    for name, value, where in scheme_ops():
        lines.append(f"{name}\t{value}\t{where}")
    lines += ["", "[DRIVERS]"]
    for name, kind, where in driver_entries():
        lines.append(f"{name}\t{kind}\t{where}")
    lines += ["", "[SECURITY]"]
    for name, kind, where in security_entries():
        lines.append(f"{name}\t{kind}\t{where}")
    return "\n".join(lines) + "\n"


def gaps_syscall_rows() -> set[str]:
    rows: set[str] = set()
    if not GAPS.exists():
        return rows
    with GAPS.open("r", encoding="utf-8") as handle:
        for line in handle:
            if line.startswith("#") or not line.strip():
                continue
            fields = line.rstrip("\n").split("\t")
            if len(fields) < 2 or fields[0] != "syscall-abi":
                continue
            for match in re.finditer(r"SYS_([A-Z0-9_]+)", fields[1]):
                rows.add(match.group(1))
    return rows


def check() -> int:
    registered = gaps_syscall_rows()
    missing = [name for name, _, _ in syscall_dispatch() if name not in registered]
    if missing:
        print("inventory: missing gaps.tsv rows for syscalls:", file=sys.stderr)
        for name in missing:
            print(f"  SYS_{name}", file=sys.stderr)
        return 1
    print(f"inventory: {len(registered)} syscall rows registered")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="require a gaps.tsv row per syscall")
    args = parser.parse_args()
    if args.check:
        return check()
    sys.stdout.write(emit())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
