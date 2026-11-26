#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""Generate and check the RavenBSD Rust target-libc ABI profile (S724).

The authoritative model is ``libc/ravenlibc/abi/*.toml``.  This tool freezes the
constant, type and record rows that the Rust target libc (``libc`` 0.2.186 vendor
copy and the ``library/libc-ravenbsd`` tree built from patch 0002) is required to
expose, and then checks three independent things:

* the committed profile is byte-identical to a fresh generation;
* every frozen constant is declared with the model value in both the tracked
  vendor module and the module reconstructed from patch 0002 plus 0003;
* patch 0002's ravenbsd file body, with 0003 applied, reproduces the vendor
  module apart from its three-line provenance header.

``sync-patch`` rewrites the ravenbsd file body of patch 0002 from the tracked
vendor module, so the vendor copy stays the single source for that generated
file.  Hand-written implementations and target-specific code are not touched.

Usage:
    python3 toolchain/gen-libc-consts.py generate [--check]
    python3 toolchain/gen-libc-consts.py check
    python3 toolchain/gen-libc-consts.py sync-patch
    python3 toolchain/gen-libc-consts.py emit-rust-probe PATH
    python3 toolchain/gen-libc-consts.py emit-c-probe PATH

Exit status: 0 success, 1 a check failed, 2 a usage or input error.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ABI = ROOT / "libc/ravenlibc/abi"
VENDOR = ROOT / "toolchain/vendor/libc/src/unix/ravenbsd/mod.rs"
PATCH = ROOT / "toolchain/patches/0002-libc-ravenbsd.patch"
SPAWN_PATCH = ROOT / "toolchain/patches/0003-posix-spawn-ravenbsd.patch"
PROFILE = ROOT / "tests/conformance/abi-profile/libc-profile.json"
RAVENBSD_PATH = "library/libc-ravenbsd/src/unix/ravenbsd/mod.rs"

# The provenance header of the tracked vendor module.  Patch 0002 cannot carry it
# because 0003's first hunk anchors on the first line of the generated file.
PROVENANCE_HEADER = (
    "// SPDX-License-Identifier: MIT\n"
    "// Copyright (c) 2014 The Rust Project Developers\n"
    "\n"
)

# Groups whose frozen constants are also measured against the pinned FreeBSD
# donor by the independent C probe.  misc/stat are additive: only the wait and
# utime rows are in the native profile.
# Groups whose model rows already match the pinned FreeBSD donor.  Socket,
# fcntl and poll still carry the Linux/musl values in the model and are owned by
# the S744/S745/S746 cutover; they are checked against the model by the profile,
# not claimed against the donor here.
DONOR_GROUPS = {"errno", "signal"}
DONOR_MISC_PREFIXES = ("W", "UTIME")
# S713 keeps the public realtime range at 35..64; the FreeBSD 65..126 widening
# is the remaining signal-set-width row, so the donor probe does not claim it.
DONOR_SKIP = {"SIGRTMIN", "SIGRTMAX"}
# Records that the model and the pinned donor already agree on (S721/S722/S733).
# The remaining records are owned by S746 and are deliberately not claimed here.
DONOR_RECORDS = {
    "dirent",
    "iovec",
    "itimerspec",
    "pollfd",
    "rlimit",
    "stat",
    "statvfs",
    "timespec",
    "timeval",
    "tm",
}

# Records the Rust target `libc` crate does not name (the platform keeps the C
# name for the rest); their presence is proven by the C donor probe instead.
# `cmsghdr` is an open S746 row: the model still carries the 16-byte split-length
# record while the pinned donor has `socklen_t cmsg_len` and a 12-byte record.
RUST_SKIP_RECORDS = {"stack_t", "cmsghdr", "rusage"}

# Internal offsets the Rust probe names explicitly.  The model splits times into
# `timespec` members while the C crate keeps the donor `st_atime`/`st_atime_nsec`
# pair, so only the shared public members are asserted here; the donor C probe
# checks every donor field name.
RUST_OFFSET_FIELDS = {
    "stat": [
        ("st_dev", "st_dev", 0),
        ("st_ino", "st_ino", 0),
        ("st_nlink", "st_nlink", 0),
        ("st_mode", "st_mode", 0),
        ("st_uid", "st_uid", 0),
        ("st_gid", "st_gid", 0),
        ("st_rdev", "st_rdev", 0),
        ("st_size", "st_size", 0),
        ("st_blocks", "st_blocks", 0),
        ("st_blksize", "st_blksize", 0),
        ("st_atime", "st_atim", 0),
        ("st_atime_nsec", "st_atim", 8),
        ("st_mtime", "st_mtim", 0),
        ("st_mtime_nsec", "st_mtim", 8),
        ("st_ctime", "st_ctim", 0),
        ("st_ctime_nsec", "st_ctim", 8),
        ("st_birthtime", "st_birthtim", 0),
        ("st_birthtime_nsec", "st_birthtim", 8),
    ],
    "dirent": [
        ("d_fileno", "d_fileno", 0),
        ("d_off", "d_off", 0),
        ("d_reclen", "d_reclen", 0),
        ("d_type", "d_type", 0),
        ("d_namlen", "d_namlen", 0),
        ("d_name", "d_name", 0),
    ],
}


def read_text(path: Path, what: str) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except OSError as error:
        raise SystemExit(f"error: cannot read {what} {path}: {error}") from error


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def load_toml(path: Path):
    try:
        import tomllib  # Python 3.11+
    except ModuleNotFoundError:
        try:
            import tomli as tomllib  # Ubuntu 22.04 host package
        except ModuleNotFoundError as error:
            raise SystemExit(
                "error: Python 3.11+ or the `tomli` package is required to read the ABI model"
            ) from error
    try:
        return tomllib.loads(read_text(path, "ABI model"))
    except Exception as error:  # tomllib.TOMLDecodeError, OSError
        raise SystemExit(f"error: cannot parse {path}: {error}") from error


def model_files(abi: Path) -> list[Path]:
    files = sorted(p for p in abi.glob("*.toml") if p.is_file())
    if not files:
        raise SystemExit(f"error: no ABI model tables under {abi}")
    return files


class Model:
    """The native ABI model: constants, scalar types and public records."""

    def __init__(self, abi: Path) -> None:
        self.constants: dict[str, dict] = {}
        self.types: dict[str, dict] = {}
        self.records: dict[str, dict] = {}
        self.files: dict[str, str] = {}
        for path in model_files(abi):
            text = read_text(path, "ABI model")
            self.files[path.name] = sha256(text.encode())
            table = load_toml(path)
            group = table.get("group") or path.stem
            rust_type = table.get("rust_type", "i32")
            for row in table.get("consts", []):
                self.constants[row["name"]] = {
                    "group": group,
                    "rust_type": row.get("ty", rust_type),
                    "row": row,
                }
            for row in table.get("type", []):
                self.types[row["name"]] = {
                    "bits": row.get("bits"),
                    "signed": row.get("signed"),
                    "rust": row.get("rust"),
                }
            for row in table.get("record", []):
                self.records[row["name"]] = {
                    "size": row["size"],
                    "align": row["align"],
                    "fields": [
                        {"name": f["name"], "off": f["off"], "ty": f["ty"]}
                        for f in row.get("fields", [])
                    ],
                }
        if not self.constants:
            raise SystemExit("error: the ABI model has no constants")

    def value(self, name: str, stack: tuple[str, ...] = ()):
        """Resolve a model constant to an integer, following donor aliases."""
        if name in stack or name not in self.constants:
            return None
        row = self.constants[name]["row"]
        if "alias" in row:
            return self.value(row["alias"], stack + (name,))
        if "value" in row:
            value = row["value"]
            return int(value, 0) if isinstance(value, str) else value
        if "or" in row:
            total = 0
            for item in row["or"]:
                if isinstance(item, int):
                    total |= item
                else:
                    resolved = self.value(item, stack + (name,))
                    if resolved is None:
                        return None
                    total |= resolved
            return total
        if "shl" in row:
            left, shift = row["shl"]
            if isinstance(left, str):
                left = self.value(left, stack + (name,))
                if left is None:
                    return None
            return left << shift
        return None


RUST_TYPE_BITS = {
    "c_char": 8,
    "c_short": 16,
    "c_ushort": 16,
    "c_int": 32,
    "c_uint": 32,
    "c_long": 64,
    "c_ulong": 64,
    "c_longlong": 64,
    "c_ulonglong": 64,
    "size_t": 64,
    "ssize_t": 64,
    "usize": 64,
    "isize": 64,
    "i8": 8,
    "u8": 8,
    "i16": 16,
    "u16": 16,
    "i32": 32,
    "u32": 32,
    "i64": 64,
    "u64": 64,
    "crate::rlim_t": 64,
    "rlim_t": 64,
}

UNSIGNED_TYPES = {
    "c_uchar",
    "c_ushort",
    "c_uint",
    "c_ulong",
    "c_ulonglong",
    "size_t",
    "usize",
    "u8",
    "u16",
    "u32",
    "u64",
    "crate::rlim_t",
    "rlim_t",
}

CONST_RE = re.compile(
    r"^(?P<cfg>#\[cfg\([^\n]*\)\]\n)?pub const (?P<name>[A-Za-z0-9_]+):\s*"
    r"(?P<ty>[^=]+)=\s*(?P<expr>[^;]+);",
    re.M,
)
TYPE_RE = re.compile(r"^pub type ([A-Za-z0-9_]+) = ([^;]+);", re.M)


class Target:
    """The constants and type aliases declared by one Rust target module."""

    def __init__(self, text: str, label: str) -> None:
        self.label = label
        self.consts: dict[str, dict] = {}
        self.types: dict[str, str] = {}
        for match in CONST_RE.finditer(text):
            name = match.group("name")
            ty = match.group("ty").strip()
            expr = match.group("expr").strip()
            row = {
                "ty": ty,
                "expr": expr,
                "cfg": bool(match.group("cfg")),
                "value": self._evaluate(expr, ty, {}),
            }
            if name in self.consts:
                # A cfg-selected second definition is architecture-specific; keep
                # the first but mark the name as not a single-value profile row.
                self.consts[name]["cfg"] = True
                continue
            self.consts[name] = row
        for match in TYPE_RE.finditer(text):
            self.types[match.group(1)] = match.group(2).strip()

    def _evaluate(self, expr: str, ty: str, stack: dict[str, int]):
        text = expr.strip().replace("_", "")
        text = re.sub(
            r"(?<=\d)(u8|u16|u32|u64|usize|i8|i16|i32|i64|isize|ULL|UL|U|L)\b",
            "",
            text,
        )
        if re.fullmatch(r"[-+]?(0x[0-9A-Fa-f]+|0o[0-7]+|\d+)", text):
            return int(text, 0)
        if text == "!0":
            bits = RUST_TYPE_BITS.get(ty, 64)
            return (1 << bits) - 1 if ty in UNSIGNED_TYPES else -1
        bits = re.fullmatch(r"(.+?)\s*\|\s*(.+)", text)
        if bits:
            left = self._evaluate(bits.group(1), ty, stack)
            right = self._evaluate(bits.group(2), ty, stack)
            if left is not None and right is not None:
                return left | right
            return None
        shift = re.fullmatch(r"(.+?)\s*<<\s*(\d+)", text)
        if shift:
            left = self._evaluate(shift.group(1), ty, stack)
            return None if left is None else left << int(shift.group(2))
        name = text
        if name.startswith("crate::"):
            name = name[len("crate::") :]
        if name in self.consts and name not in stack:
            row = self.consts[name]
            return self._evaluate(row["expr"], row["ty"], {**stack, name: 0})
        return None


def profile_constants(model: Model, target: Target) -> dict[str, dict]:
    rows: dict[str, dict] = {}
    for name, entry in model.constants.items():
        declared = target.consts.get(name)
        if not declared or declared["cfg"] or declared["value"] is None:
            continue
        value = model.value(name)
        if value is None:
            continue
        rows[name] = {
            "group": entry["group"],
            "ty": declared["ty"],
            "value": value,
        }
    return dict(sorted(rows.items()))


def donor_constants(rows: dict[str, dict]) -> list[str]:
    names = []
    for name, row in rows.items():
        if name in DONOR_SKIP:
            continue
        if row["group"] in DONOR_GROUPS:
            names.append(name)
        elif row["group"] in {"misc", "stat"} and name.startswith(DONOR_MISC_PREFIXES):
            names.append(name)
    return names


def build_profile(model: Model, target: Target) -> dict:
    rows = profile_constants(model, target)
    types = {}
    for name, entry in model.types.items():
        if name not in target.types:
            continue
        types[name] = entry
    return {
        "schema": 1,
        "source": {
            "model": model.files,
            "target": "toolchain/vendor/libc/src/unix/ravenbsd/mod.rs",
        },
        "constants": rows,
        "types": dict(sorted(types.items())),
        "records": dict(sorted(model.records.items())),
        "donor": {
            "groups": sorted(DONOR_GROUPS),
            "misc_prefixes": list(DONOR_MISC_PREFIXES),
            "records": sorted(DONOR_RECORDS),
            "constants": donor_constants(rows),
        },
    }


def render_profile(profile: dict) -> str:
    return json.dumps(profile, indent=2, sort_keys=True) + "\n"


def load_target(path: Path) -> Target:
    return Target(read_text(path, "Rust target module"), str(path))


def profile_target(profile: dict) -> Target:
    """A synthetic target that declares every frozen row as its profile value."""
    text = "".join(
        f"pub const {name}: {row['ty']} = {row['value']};\n"
        for name, row in profile["constants"].items()
    )
    return Target(text, "generated profile")


def check_profile(profile: dict, model: Model, target: Target) -> list[str]:
    failures: list[str] = []
    expected = profile_constants(model, target)
    current = profile["constants"]
    for name in sorted(set(expected) | set(current)):
        if name not in current:
            failures.append(f"{name}: new model row is not in the profile; run generate")
            continue
        if name not in expected:
            failures.append(f"{name}: profile row is no longer declared by the target module")
            continue
        if expected[name] != current[name]:
            failures.append(
                f"{name}: profile {current[name]['value']}, model {expected[name]['value']}"
            )
    if profile["types"] != {k: v for k, v in model.types.items() if k in target.types}:
        failures.append("type table is stale; run generate")
    if profile["records"] != dict(sorted(model.records.items())):
        failures.append("record table is stale; run generate")
    return failures


def check_target(profile: dict, target: Target) -> list[str]:
    failures: list[str] = []
    for name, row in profile["constants"].items():
        declared = target.consts.get(name)
        if not declared:
            failures.append(f"{name}: missing from {target.label}")
            continue
        if declared["cfg"]:
            failures.append(f"{name}: architecture-specific definition is not a single value")
            continue
        if declared["value"] is None:
            failures.append(f"{name}: non-integer definition {declared['expr']!r}")
            continue
        if declared["value"] != row["value"]:
            failures.append(
                f"{name}: {target.label} {declared['value']}, model {row['value']}"
            )
    return failures


def patch_section(text: str, path: str) -> tuple[int, int]:
    marker = f"diff --git a/{path} b/{path}"
    start = text.find(marker)
    if start < 0:
        raise SystemExit(f"error: {path} is not in the patch")
    end = text.find("\ndiff --git", start + 1)
    return start, len(text) if end < 0 else end + 1


def patch_body(text: str, path: str) -> str:
    start, end = patch_section(text, path)
    lines = text[start:end].splitlines()
    body = []
    for line in lines:
        if line.startswith("+++") or line.startswith("---") or line.startswith("diff --git"):
            continue
        if line.startswith("new file") or line.startswith("index ") or line.startswith("@@"):
            continue
        if line.startswith("+"):
            body.append(line[1:])
    return "\n".join(body) + "\n"


def rewritten_section(section: str, body: str) -> str:
    lines = body.splitlines()
    head = []
    seen_header = False
    for line in section.splitlines():
        if line.startswith("@@"):
            seen_header = True
            head.append(f"@@ -0,0 +1,{len(lines)} @@")
            continue
        if line.startswith("+"):
            continue
        head.append(line)
    if not seen_header:
        raise SystemExit("error: the ravenbsd patch section has no hunk")
    # git hash-object of the new blob keeps the index line truthful.
    blob = hashlib.sha1(f"blob {len(body.encode())}\0".encode() + body.encode()).hexdigest()
    for index, line in enumerate(head):
        if line.startswith("index "):
            head[index] = f"index 0000000..{blob}"
    return "\n".join(head + ["+" + line for line in lines]) + "\n"


def spawn_section(path: Path) -> str:
    text = read_text(path, "posix_spawn patch")
    marker = f"--- a/{RAVENBSD_PATH}"
    start = text.find(marker)
    if start < 0:
        raise SystemExit(f"error: {RAVENBSD_PATH} is not in {path}")
    return text[start:]


def apply_section(body: str, section: str, reverse: bool, label: str) -> str:
    """Apply one unified-diff section to `body` in a temporary tree."""
    with tempfile.TemporaryDirectory(prefix="raven-libc-consts-") as temp:
        root = Path(temp)
        target = root / RAVENBSD_PATH
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(body, encoding="utf-8")
        patch_file = root / "section.patch"
        patch_file.write_text(section, encoding="utf-8")
        argv = ["patch", "-p1", "--forward", "--batch", "-d", str(root)]
        if reverse:
            argv.insert(1, "-R")
        with patch_file.open("rb") as handle:
            result = subprocess.run(
                argv,
                stdin=handle,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                check=False,
            )
        if result.returncode != 0:
            raise SystemExit(
                f"error: cannot {label} the patch section:\n"
                f"{result.stdout.decode(errors='replace')}"
            )
        return target.read_text(encoding="utf-8")


def vendor_without_header(text: str) -> str:
    if not text.startswith(PROVENANCE_HEADER):
        raise SystemExit("error: the vendor module does not start with the expected provenance header")
    return text[len(PROVENANCE_HEADER) :]


def check_patch(profile: dict, vendor: str, patch_text: str, spawn_text: str) -> list[str]:
    failures: list[str] = []
    body = patch_body(patch_text, RAVENBSD_PATH)
    target = Target(body, f"patch 0002 {RAVENBSD_PATH}")
    failures.extend(check_target(profile, target))

    reconstructed = apply_section(body, spawn_text, False, "apply")
    if reconstructed != vendor_without_header(vendor):
        failures.append(
            "patch 0002 + 0003 do not reconstruct the vendor module "
            "(apart from its provenance header)"
        )

    generated = apply_section(vendor_without_header(vendor), spawn_text, True, "reverse")
    if generated != body:
        failures.append("patch 0002 ravenbsd body is not the deterministic reverse of 0003")
    return failures


def rust_probe(profile: dict) -> str:
    lines = [
        "// SPDX-License-Identifier: BSD-3-Clause",
        "// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors",
        "// Generated by toolchain/gen-libc-consts.py; do not edit.",
        "",
        "const _: () = {",
    ]
    for name, row in profile["constants"].items():
        lines.append(f"    assert!(libc::{name} as i128 == {row['value']}i128);")
    for name, record in profile["records"].items():
        if name in RUST_SKIP_RECORDS:
            continue
        lines.append(
            f"    assert!(core::mem::size_of::<libc::{name}>() == {record['size']});"
        )
        lines.append(
            f"    assert!(core::mem::align_of::<libc::{name}>() == {record['align']});"
        )
        by_name = {field["name"]: field for field in record["fields"]}
        for public, model_name, delta in RUST_OFFSET_FIELDS.get(name, []):
            field = by_name[model_name]
            lines.append(
                f"    assert!(core::mem::offset_of!(libc::{name}, {public})"
                f" == {field['off'] + delta});"
            )
    lines += ["};", ""]
    return "\n".join(lines)


def c_probe(profile: dict) -> str:
    rows = set(profile["donor"]["constants"])
    records = set(profile["donor"]["records"])
    lines = [
        "/* SPDX-License-Identifier: BSD-3-Clause",
        " * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors",
        " * Generated by toolchain/gen-libc-consts.py; do not edit.",
        " */",
        "",
        "#include <errno.h>",
        "#include <fcntl.h>",
        "#include <signal.h>",
        "#include <sys/stat.h>",
        "#include <sys/time.h>",
        "#include <sys/resource.h>",
        "#include <sys/wait.h>",
        "#include <time.h>",
        "#include <poll.h>",
        "#include <dirent.h>",
        "#include <sys/statvfs.h>",
        "#include <sys/uio.h>",
        "#include <stddef.h>",
        "",
    ]
    for name in sorted(rows):
        lines.append(f'_Static_assert(({name}) == {profile["constants"][name]["value"]}, "{name}");')
    for name in sorted(records):
        record = profile["records"][name]
        lines.append(f'_Static_assert(sizeof(struct {name}) == {record["size"]}, "sizeof {name}");')
        lines.append(
            f'_Static_assert(_Alignof(struct {name}) == {record["align"]}, "align {name}");'
        )
        for field in record["fields"]:
            if any(token in field["name"] for token in ("pad", "spare", "reserved")):
                continue
            lines.append(
                f'_Static_assert(offsetof(struct {name}, {field["name"]}) == {field["off"]}, '
                f'"{name}.{field["name"]}");'
            )
    lines.append("")
    return "\n".join(lines)


def write_text(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["generate", "check", "sync-patch", "emit-rust-probe", "emit-c-probe"])
    parser.add_argument("output", nargs="?")
    parser.add_argument("--check", action="store_true", help="generate only to compare")
    parser.add_argument("--profile", type=Path, default=PROFILE)
    parser.add_argument("--abi", type=Path, default=ABI)
    parser.add_argument("--vendor", type=Path, default=VENDOR)
    parser.add_argument("--patch", type=Path, default=PATCH)
    parser.add_argument("--spawn-patch", type=Path, default=SPAWN_PATCH)
    args = parser.parse_args(argv)

    model = Model(args.abi)
    vendor = read_text(args.vendor, "vendor target module")
    target = Target(vendor, str(args.vendor))
    profile = build_profile(model, target)
    rendered = render_profile(profile)

    if args.command == "generate":
        if args.check:
            if not args.profile.is_file() or args.profile.read_text() != rendered:
                print(f"profile is stale: {args.profile}")
                return 1
            print(f"profile is current ({len(profile['constants'])} constants)")
            return 0
        write_text(args.profile, rendered)
        print(f"wrote {args.profile} ({len(profile['constants'])} constants)")
        return 0

    if args.command in {"emit-rust-probe", "emit-c-probe"}:
        if not args.output:
            raise SystemExit("error: an output path is required")
        text = rust_probe(profile) if args.command == "emit-rust-probe" else c_probe(profile)
        write_text(Path(args.output), text)
        print(f"wrote {args.output}")
        return 0

    if not args.profile.is_file():
        print(f"error: missing profile {args.profile}; run generate", file=sys.stderr)
        return 2
    committed = json.loads(args.profile.read_text())
    failures = check_profile(committed, model, target)
    if render_profile(committed) != rendered:
        failures.append("committed profile is not byte-identical to a fresh generation")

    patch_text = read_text(args.patch, "libc patch")
    spawn_text = spawn_section(args.spawn_patch)

    if args.command == "sync-patch":
        body = apply_section(vendor_without_header(vendor), spawn_text, True, "reverse")
        start, end = patch_section(patch_text, RAVENBSD_PATH)
        section = patch_text[start:end]
        updated = patch_text[:start] + rewritten_section(section, body) + patch_text[end:]
        args.patch.write_text(updated, encoding="utf-8")
        print(f"rewrote {RAVENBSD_PATH} in {args.patch} ({len(body.splitlines())} lines)")

    failures.extend(check_target(committed, target))
    failures.extend(check_patch(committed, vendor, patch_text, spawn_text))

    if failures:
        for failure in failures:
            print(f"FAIL: {failure}")
        print(f"libc profile: FAILED ({len(failures)} failure(s))")
        return 1
    print(
        f"libc profile: ok ({len(committed['constants'])} constants, "
        f"{len(committed['records'])} records, vendor and patch agree)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
