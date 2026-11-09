#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""Audit the RavenBSD libc errno table.

The primary contract is `crates/raven-abi/src/errno.rs` (DR-13: Linux values).
The named permissible oracle for errno names outside that subset is musl's
`arch/generic/bits/errno.h` (MIT donor, tier A), which follows the Linux UAPI.

Usage: python3 toolchain/audit-errno.py [--patch PATH] [--oracle PATH]
Exit 0 when every comparable constant matches; 1 on a mismatch; 2 on missing input.
"""

import argparse
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

PATCH = ROOT / "toolchain/patches/0002-libc-ravenbsd.patch"
CONTRACT = ROOT / "crates/raven-abi/src/errno.rs"
ORACLE = ROOT.parents[2] / "ravenbsd-resources/musl/arch/generic/bits/errno.h"
RAVENBSD_FILE = "library/libc-ravenbsd/src/unix/ravenbsd/mod.rs"

PATCH_CONST = re.compile(r"^\+pub const (E[A-Z0-9_]+): c_int = ([0-9]+|[A-Za-z_][A-Za-z0-9_]*);", re.M)
CONTRACT_CONST = re.compile(r"^\s*(E[A-Z0-9_]+)\s*=\s*([0-9]+),", re.M)
ORACLE_CONST = re.compile(r"^#define\s+(E[A-Z0-9_]+)\s+([0-9]+|[A-Za-z_][A-Za-z0-9_]*)\s*$", re.M)


def patch_section(text: str, relative_path: str) -> str:
    marker = f"diff --git a/{relative_path} b/{relative_path}"
    start = text.find(marker)
    if start < 0:
        raise SystemExit(f"error: {relative_path} not found in the patch")
    end = text.find("\ndiff --git", start + 1)
    return text[start:] if end < 0 else text[start:end]


def resolve(name: str, values: dict[str, str], stack: tuple[str, ...] = ()) -> int | None:
    if name in stack:
        return None
    if name not in values:
        return None
    value = values[name]
    if value.isdigit():
        return int(value)
    return resolve(value, values, stack + (name,))


def read_text(path: Path) -> str:
    try:
        return path.read_text()
    except OSError as error:
        raise SystemExit(f"error: cannot read {path}: {error}") from error


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--patch", type=Path, default=PATCH)
    parser.add_argument("--contract", type=Path, default=CONTRACT)
    parser.add_argument("--oracle", type=Path, default=ORACLE)
    args = parser.parse_args()

    patch = read_text(args.patch)
    contract_text = read_text(args.contract)
    oracle_text = read_text(args.oracle)

    section = patch_section(patch, RAVENBSD_FILE)
    ravenbsd = {name: value for name, value in PATCH_CONST.findall(section)}
    if not ravenbsd:
        raise SystemExit(f"error: no errno constants found in {args.patch}")

    contract = {name: int(value) for name, value in CONTRACT_CONST.findall(contract_text)}
    oracle = {name: value for name, value in ORACLE_CONST.findall(oracle_text)}

    mismatches = []
    checked = 0
    unchecked = []
    for name in sorted(ravenbsd):
        actual = resolve(name, ravenbsd)
        if actual is None:
            mismatches.append(f"{name}: cannot resolve alias `{ravenbsd[name]}`")
            continue
        if name in contract:
            checked += 1
            if actual != contract[name]:
                mismatches.append(
                    f"{name}: RavenBSD={actual}, raven-abi contract={contract[name]}"
                )
            continue
        expected = resolve(name, oracle)
        if expected is None:
            unchecked.append(name)
            continue
        checked += 1
        if actual != expected:
            mismatches.append(f"{name}: RavenBSD={actual}, musl oracle={expected}")

    if mismatches:
        print("\n".join(mismatches))
        print(
            f"errno audit: FAILED ({checked} checked, {len(unchecked)} with no oracle, "
            f"{len(mismatches)} mismatch(es))"
        )
        return 1

    print(
        f"errno audit: ok ({checked} checked against raven-abi and musl, "
        f"{len(unchecked)} with no oracle)"
    )
    if unchecked:
        print("unchecked names: " + ", ".join(unchecked))
    return 0


if __name__ == "__main__":
    sys.exit(main())
