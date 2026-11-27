#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""Audit the RavenBSD native ABI domain against the pinned FreeBSD donor.

The native contract is `crates/raven-abi/src/errno.rs`; the Rust target constants live in the
`ravenbsd` section of `toolchain/patches/0002-libc-ravenbsd.patch`. Both must carry the values of
`freebsd:sys/sys/errno.h` at the S700 pin (`donors.lock`), not the Linux UAPI. S724 widens the
default audit beyond errno: the native profile (`libc-profile.json`) covers signal flags, socket
and fcntl flags and the public record layouts in the same target module and patch.

The Linux personality keeps its own table in `crates/raven-linux-abi/src/errno.rs`. It is never
part of the native result and is compared only with `--linux`, against the pinned
`linux-raw-sys` x86_64 table.

Usage: python3 toolchain/audit-errno.py [--patch PATH] [--contract PATH] [--oracle PATH]
                                      [--linux] [--skip-profile]
Exit 0 when every selected check agrees; 1 on a mismatch; 2 on missing or empty input.
"""

import argparse
import importlib.util
import json
import os
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

PATCH = ROOT / "toolchain/patches/0002-libc-ravenbsd.patch"
CONTRACT = ROOT / "crates/raven-abi/src/errno.rs"


def discover_resources() -> Path:
    """Find the donor checkout: explicit env first, then any ancestor, then the workspace."""
    explicit = os.environ.get("RAVENBSD_RESOURCES")
    if explicit:
        return Path(explicit)
    for parent in (ROOT, *ROOT.parents):
        candidate = parent / "ravenbsd-resources"
        if (candidate / "donors.lock").is_file():
            return candidate
    # The product clone may live beside the workspace through a symlink; the workspace keeps the
    # donors. This is the documented local layout, not a guess.
    return Path.home() / "work/ravenos-workspace/ravenbsd-resources"


DEFAULT_RESOURCES = discover_resources()
ORACLE = DEFAULT_RESOURCES / "freebsd/sys/sys/errno.h"
LOCK = DEFAULT_RESOURCES / "donors.lock"
RAVENBSD_FILE = "library/libc-ravenbsd/src/unix/ravenbsd/mod.rs"

PATCH_CONST = re.compile(r"^\+pub const (E[A-Z0-9_]+): c_int = ([0-9]+|[A-Za-z_][A-Za-z0-9_]*);", re.M)
CONTRACT_CONST = re.compile(r"^\s*(E[A-Z0-9_]+)\s*=\s*([0-9]+),", re.M)
ORACLE_CONST = re.compile(r"^#define\s+(E[A-Z0-9_]+)\s+([0-9]+|[A-Za-z_][A-Za-z0-9_]*)\s*(?:/\*.*)?$", re.M)

# Names the Rust target module exposes without a separate donor `#define`; each is the donor
# alias it resolves to. A name outside this map and the donor is a hard failure.
ALIASES = {
    "EWOULDBLOCK": "EAGAIN",
    "ENOTSUP": "EOPNOTSUPP",
    "EDEADLOCK": "EDEADLK",
}


def patch_section(text: str, relative_path: str) -> str:
    marker = f"diff --git a/{relative_path} b/{relative_path}"
    start = text.find(marker)
    if start < 0:
        raise SystemExit(f"error: {relative_path} not found in the patch")
    end = text.find("\ndiff --git", start + 1)
    return text[start:] if end < 0 else text[start:end]


def errno_block(section: str) -> str:
    """The `// errno.h` group: epoll/exit constants share the `E` prefix but not the domain."""
    start = section.find("+// errno.h")
    if start < 0:
        raise SystemExit("error: the ravenbsd section has no // errno.h group")
    rest = section[start:]
    lines = []
    for line in rest.splitlines()[1:]:
        if line.startswith("+//") or not line.startswith("+pub const E"):
            break
        lines.append(line)
    return "\n".join(lines)


def resolve(name: str, values: dict[str, str], stack: tuple[str, ...] = ()) -> int | None:
    if name in stack:
        return None
    if name not in values:
        return None
    value = values[name]
    if value.isdigit():
        return int(value)
    return resolve(value, values, stack + (name,))


def read_text(path: Path, what: str) -> str:
    try:
        return path.read_text()
    except OSError as error:
        print(f"error: cannot read {what} {path}: {error}", file=sys.stderr)
        raise SystemExit(2) from error


def pinned_freebsd(lock: Path) -> str:
    for line in read_text(lock, "donor lock").splitlines():
        fields = line.split()
        if len(fields) >= 2 and fields[0] == "freebsd":
            return fields[1]
    print(f"error: {lock} has no freebsd revision", file=sys.stderr)
    raise SystemExit(2)


def load_target_profile():
    """Load the S724 generator module without leaving a stale byte-code cache."""
    path = ROOT / "toolchain/gen-libc-consts.py"
    spec = importlib.util.spec_from_file_location("gen_libc_consts", path)
    if spec is None or spec.loader is None:
        print(f"error: cannot load {path}", file=sys.stderr)
        raise SystemExit(2)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def profile_audit(tool) -> tuple[int, list[str]]:
    """Compare the target module and the patch with the frozen native profile."""
    profile_path = ROOT / "tests/conformance/abi-profile/libc-profile.json"
    try:
        profile = json.loads(read_text(profile_path, "native profile"))
    except json.JSONDecodeError as error:
        return 0, [f"{profile_path}: invalid JSON: {error}"]
    model = tool.Model(tool.ABI)
    vendor = read_text(tool.VENDOR, "vendor target module")
    target = tool.Target(vendor, str(tool.VENDOR))
    failures = tool.check_profile(profile, model, target)
    failures.extend(tool.check_target(profile, target))
    failures.extend(
        tool.check_patch(
            profile,
            vendor,
            read_text(tool.PATCH, "libc patch"),
            tool.spawn_section(tool.SPAWN_PATCH),
        )
    )
    return len(profile["constants"]), failures


LINUX_CONTRACT = ROOT / "crates/raven-linux-abi/src/errno.rs"
LINUX_TABLE = "src/x86_64/errno.rs"
LINUX_CONST = re.compile(r"^pub const ([A-Z][A-Z0-9_]*): u(?:16|32) = (\d+);", re.M)


def linux_audit(lock: Path) -> tuple[str, int, list[str]]:
    """Compare the Linux-personality errno table with the pinned linux-raw-sys table."""
    revision = ""
    for line in read_text(lock, "donor lock").splitlines():
        fields = line.split()
        if len(fields) >= 2 and fields[0] == "linux-raw-sys":
            revision = fields[1]
    if not revision:
        return "", 0, [f"{lock}: no linux-raw-sys revision"]
    table = DEFAULT_RESOURCES / "linux-raw-sys" / LINUX_TABLE
    oracle = dict(LINUX_CONST.findall(read_text(table, "linux-raw-sys errno table")))
    contract = dict(LINUX_CONST.findall(read_text(LINUX_CONTRACT, "Linux errno contract")))
    if not oracle or not contract:
        return revision, 0, ["Linux errno table or contract is empty"]
    mismatches = []
    for name in sorted(set(oracle) | set(contract)):
        if oracle.get(name) != contract.get(name):
            mismatches.append(
                f"{name}: Linux personality={contract.get(name)}, "
                f"linux-raw-sys {revision}={oracle.get(name)}"
            )
    return revision, len(contract), mismatches


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--patch", type=Path, default=PATCH)
    parser.add_argument("--contract", type=Path, default=CONTRACT)
    parser.add_argument("--oracle", type=Path, default=ORACLE)
    parser.add_argument("--lock", type=Path, default=LOCK)
    parser.add_argument(
        "--linux",
        action="store_true",
        help="also compare the separate Linux-personality table (never part of the native result)",
    )
    parser.add_argument(
        "--skip-profile",
        action="store_true",
        help="only run the errno audit (legacy behaviour)",
    )
    args = parser.parse_args()

    revision = pinned_freebsd(args.lock)
    patch = read_text(args.patch, "Rust target patch")
    contract_text = read_text(args.contract, "native contract")
    oracle_text = read_text(args.oracle, "FreeBSD donor header")

    section = patch_section(patch, RAVENBSD_FILE)
    ravenbsd = {name: value for name, value in PATCH_CONST.findall(errno_block(section))}
    if not ravenbsd:
        print(f"error: no errno constants found in {args.patch}", file=sys.stderr)
        return 2

    contract = {name: int(value) for name, value in CONTRACT_CONST.findall(contract_text)}
    if not contract:
        print(f"error: no errno constants found in {args.contract}", file=sys.stderr)
        return 2

    oracle = {name: value for name, value in ORACLE_CONST.findall(oracle_text)}
    if "EAGAIN" not in oracle:
        print(f"error: {args.oracle} does not look like FreeBSD sys/sys/errno.h", file=sys.stderr)
        return 2

    mismatches = []
    checked = 0
    for name in sorted(ravenbsd):
        actual = resolve(name, ravenbsd)
        if actual is None:
            mismatches.append(f"{name}: cannot resolve alias `{ravenbsd[name]}`")
            continue
        expected = resolve(name, oracle)
        if expected is None:
            alias = ALIASES.get(name)
            expected = resolve(alias, oracle) if alias else None
        if expected is None:
            mismatches.append(f"{name}: not defined by FreeBSD sys/sys/errno.h @ {revision}")
            continue
        checked += 1
        if actual != expected:
            mismatches.append(
                f"{name}: Rust target={actual}, FreeBSD {revision}={expected}"
            )
        if name in contract and contract[name] != expected:
            mismatches.append(
                f"{name}: contract={contract[name]}, FreeBSD {revision}={expected}"
            )

    for name, value in sorted(contract.items()):
        expected = resolve(name, oracle)
        if expected is None:
            alias = ALIASES.get(name)
            expected = resolve(alias, oracle) if alias else None
        if expected is None:
            mismatches.append(f"{name}: contract value {value} has no FreeBSD definition")
            continue
        checked += 1
        if value != expected:
            mismatches.append(f"{name}: contract={value}, FreeBSD {revision}={expected}")

    profile_rows = 0
    if not args.skip_profile:
        tool = load_target_profile()
        profile_rows, profile_failures = profile_audit(tool)
        mismatches.extend(f"native profile: {failure}" for failure in profile_failures)

    linux_revision = ""
    linux_rows = 0
    if args.linux:
        linux_revision, linux_rows, linux_failures = linux_audit(args.lock)
        mismatches.extend(f"Linux domain: {failure}" for failure in linux_failures)

    if mismatches:
        print("\n".join(mismatches))
        print(f"errno audit: FAILED ({len(mismatches)} mismatch(es))")
        return 1

    print(
        f"errno audit: ok ({checked} checks against FreeBSD {revision}; "
        f"{len(ravenbsd)} target rows, {len(contract)} contract rows)"
    )
    if not args.skip_profile:
        print(
            f"native profile: ok ({profile_rows} constants; signal, socket, flags and "
            f"record layouts in the target module and patch 0002)"
        )
    if args.linux:
        print(
            f"Linux domain (separate): ok ({linux_rows} rows against "
            f"linux-raw-sys {linux_revision})"
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())
