#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""S182: run the shared conformance tables on aarch64 under QEMU virt.

The tables live under ``tests/conformance/<topic>/<name>.toml`` and use the product qtest
case schema.  This runner discovers them, copies each case with only the architecture-specific
details changed (``arch``, the aarch64 console and a unique image/log name), and executes the
copy through ``cargo xtask qtest --arch aarch64``.  The expected values, steps, forbids and
timeouts are copied verbatim, so both architectures assert the same contract.

No table is skipped.  A table that declares ``skip =`` or carries an x86-only loader/QEMU
setting fails here; an aarch64 limitation is recorded as a divergence in the ticket/report
(``divergences/README.md``), not hidden as a skip.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
CONFORMANCE = ROOT / "tests" / "conformance"
OUT = ROOT / "build" / "conformance" / "aarch64"
SELFTEST = CONFORMANCE / "aarch64" / "selftest" / "boot-uname-aarch64.toml"
DIVERGENCES = CONFORMANCE / "aarch64" / "divergences"

# The qtest case image build is cached by profile; the margin covers the image assembly and
# a fresh xtask build on a quiet host.  The table's own timeout_secs bounds the guest.
BUILD_MARGIN_SECS = 420
DEFAULT_CASE_TIMEOUT_SECS = 120

NAME_RE = re.compile(r'^\s*name\s*=\s*"([^"]+)"', re.MULTILINE)
ARCH_RE = re.compile(r'^\s*arch\s*=\s*"[^"]+"', re.MULTILINE)
TIMEOUT_RE = re.compile(r"^\s*timeout_secs\s*=\s*(\d+)", re.MULTILINE)
SKIP_RE = re.compile(r"^\s*skip\s*=", re.MULTILINE)
LOADER_RE = re.compile(r"^\s*loader_kernel\s*=", re.MULTILINE)
X86_QEMU_RE = re.compile(r"\bq35\b|if=ide")


class TableError(Exception):
    """A table cannot be run on aarch64 without a recorded divergence."""


def relative(path: Path) -> str:
    """A stable display path relative to the repository root."""
    try:
        return str(path.relative_to(ROOT))
    except ValueError:
        return str(path)


def discover(include_mutations: bool) -> list[Path]:
    """Return the shared tables in deterministic order, excluding this runner's directory."""
    rows: list[Path] = []
    for path in sorted(CONFORMANCE.glob("*/*.toml")):
        if path.parent.name == "aarch64":
            continue
        if not include_mutations and "mutation" in path.name:
            continue
        rows.append(path)
    return rows


def discover_self_contained() -> list[Path]:
    """Return rows that are self-contained runners instead of shared qtest tables."""
    rows: list[Path] = []
    for path in sorted(CONFORMANCE.iterdir()):
        if not path.is_dir() or path.name == "aarch64":
            continue
        if any(path.glob("*.toml")):
            continue
        if (path / "run.py").is_file() or (path / "build.sh").is_file():
            rows.append(path)
    return rows


def translate(path: Path) -> tuple[Path, str]:
    """Copy one table to the aarch64 work directory with only arch details changed."""
    text = path.read_text(encoding="utf-8")
    if SKIP_RE.search(text):
        raise TableError(
            f"{relative(path)}: table declares skip=; record the aarch64 divergence, do not skip"
        )
    if LOADER_RE.search(text):
        raise TableError(
            f"{relative(path)}: loader_kernel is an x86-64 test kernel; aarch64 needs its own table"
        )
    if X86_QEMU_RE.search(text):
        raise TableError(
            f"{relative(path)}: x86-only QEMU setting (q35/if=ide); aarch64 needs its own table"
        )

    name_match = NAME_RE.search(text)
    if name_match is None:
        raise TableError(f"{relative(path)}: missing name=")
    base = name_match.group(1)
    name = base if "-aarch64" in base else f"{base}-aarch64"

    text = NAME_RE.sub(f'name = "{name}"', text, count=1)
    arch_match = ARCH_RE.search(text)
    if arch_match is not None:
        text = text[: arch_match.start()] + 'arch = "aarch64"' + text[arch_match.end() :]
    else:
        text = NAME_RE.sub(
            lambda match: match.group(0) + '\narch = "aarch64"', text, count=1
        )
    text = text.replace("console=ttyS0", "console=ttyAMA0")

    OUT.mkdir(parents=True, exist_ok=True)
    destination = OUT / f"{name}.toml"
    destination.write_text(text, encoding="utf-8")
    return destination, name


def case_timeout(path: Path) -> int:
    """The table's guest timeout plus the image-build margin."""
    match = TIMEOUT_RE.search(path.read_text(encoding="utf-8"))
    guest = int(match.group(1)) if match is not None else DEFAULT_CASE_TIMEOUT_SECS
    return guest + BUILD_MARGIN_SECS


def qtest_result(name: str) -> str | None:
    """The qtest JSON result, or None when the harness did not produce one."""
    result_path = ROOT / "build" / "qtest" / f"{name}.json"
    if not result_path.is_file():
        return None
    try:
        data = json.loads(result_path.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        return None
    return str(data.get("result", "")).lower()


def qtest_skipped(name: str) -> bool:
    """True when the qtest log records a skip, even if the process exited zero."""
    log = ROOT / "build" / "qtest" / f"{name}.log"
    if not log.is_file():
        return False
    text = log.read_text(encoding="utf-8", errors="replace")
    return re.search(r"(^|: )skipped$", text, re.MULTILINE) is not None


def run_case(case: Path, name: str, plan: bool) -> dict[str, object]:
    """Run one translated case through the QEMU aarch64 harness."""
    if plan:
        print(f"conformance-aarch64: {name} plan case={relative(case)}")
        return {"name": name, "status": "plan", "case": relative(case)}

    timeout = case_timeout(case)
    command = [
        "cargo",
        "xtask",
        "qtest",
        "--case",
        str(case),
        "--arch",
        "aarch64",
    ]
    started = time.monotonic()
    try:
        completed = subprocess.run(
            command,
            cwd=ROOT,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            timeout=timeout,
            check=False,
        )
        output = completed.stdout or ""
        code = completed.returncode
    except subprocess.TimeoutExpired as expired:
        output = (expired.stdout or "") if isinstance(expired.stdout, str) else ""
        output += f"\nconformance-aarch64: runner timeout after {timeout}s\n"
        code = 124
    except OSError as error:
        output = f"conformance-aarch64: cannot start qtest: {error}\n"
        code = 127

    duration = round(time.monotonic() - started, 1)
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / f"{name}.runner.log").write_text(output, encoding="utf-8")

    result = qtest_result(name)
    skipped = result == "skip" or qtest_skipped(name)
    passed = code == 0 and not skipped
    status = "pass" if passed else "fail"
    detail = "" if passed else f" exit={code} result={result} timeout={timeout}s"
    print(f"conformance-aarch64: {name} {status} ({duration}s){detail}")
    if not passed:
        tail = "\n".join(output.splitlines()[-20:])
        if tail:
            print(tail, file=sys.stderr)
    return {
        "name": name,
        "status": status,
        "case": relative(case),
        "exit": code,
        "result": result,
        "duration_s": duration,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Run the shared conformance tables on aarch64")
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="run the bundled aarch64 selftest instead of the discovered tables",
    )
    parser.add_argument("--table", help="run one table path instead of discovery")
    parser.add_argument(
        "--plan",
        action="store_true",
        help="translate and report the cases without launching QEMU",
    )
    parser.add_argument(
        "--allow-empty",
        action="store_true",
        help="exit 0 when no shared tables exist yet (runner readiness check)",
    )
    parser.add_argument(
        "--include-mutations",
        action="store_true",
        help="also run tables whose file name contains 'mutation'",
    )
    args = parser.parse_args()

    if args.self_test and args.table:
        print("conformance-aarch64: --self-test and --table are mutually exclusive", file=sys.stderr)
        return 2

    if args.self_test:
        rows = [SELFTEST]
        self_rows: list[Path] = []
    elif args.table:
        path = Path(args.table)
        rows = [path if path.is_absolute() else ROOT / path]
        self_rows = []
    else:
        rows = discover(args.include_mutations)
        self_rows = discover_self_contained()

    if not rows and not self_rows:
        if not args.allow_empty:
            print(
                "conformance-aarch64: no tables or self-contained rows under "
                "tests/conformance/*/ (S172 harness not merged); use --allow-empty for the "
                "readiness check",
                file=sys.stderr,
            )
            return 2
        print(
            "conformance-aarch64: tables=0 self_contained=0 passed=0 failed=0 "
            "divergences=0 skipped=0"
        )
        print("conformance-aarch64: PASS (ready; no shared tables yet)")
        return 0

    results: list[dict[str, object]] = []
    for path in rows:
        try:
            case, name = translate(path)
        except TableError as error:
            print(f"conformance-aarch64: {relative(path)} error: {error}", file=sys.stderr)
            results.append({"name": relative(path), "status": "error"})
            continue
        results.append(run_case(case, name, args.plan))

    divergences: list[dict[str, object]] = []
    for path in self_rows:
        record = DIVERGENCES / f"{path.name}.md"
        if record.is_file():
            print(
                f"conformance-aarch64: {relative(path)} divergence recorded={relative(record)}"
            )
            divergences.append({"name": relative(path), "status": "divergence"})
        else:
            print(
                f"conformance-aarch64: {relative(path)} UNRECORDED DIVERGENCE: no aarch64 "
                f"table and no {relative(record)}",
                file=sys.stderr,
            )
            divergences.append({"name": relative(path), "status": "unrecorded-divergence"})

    failed = [row for row in results if row["status"] not in ("pass", "plan")]
    passed = [row for row in results if row["status"] == "pass"]
    print(
        f"conformance-aarch64: tables={len(results)} self_contained={len(self_rows)} "
        f"passed={len(passed)} failed={len(failed)} divergences={len(divergences)} skipped=0"
    )
    if failed or divergences:
        print("conformance-aarch64: FAIL")
        return 1
    print("conformance-aarch64: PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
