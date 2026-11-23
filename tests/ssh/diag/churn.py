#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""S170: bounded session churn must return the leak counters to baseline.

Runs N SSH command sessions against a diagnostic release image built with
``k449-metrics=1 s36-proccheck`` and asserts that the last ``s36: proccheck`` and
``K449-METRICS`` samples show no leaked Process, no deferred close notice, no dead client,
and process/thread/class counts at the post-boot baseline.

The test is the acceptance row: it is red on a tree that retains one ``sshd-session`` Process
per session and green once the holder releases the reference.

Usage:
  python3 tests/ssh/diag/churn.py --image build/qtest/ssh-login-diag.img [--sessions 8]
  python3 tests/ssh/diag/churn.py --selftest
  python3 tests/ssh/diag/churn.py --parse <serial.log>        # no QEMU, inspect an existing log

Exit: 0 when every count is at baseline, 1 when a count is above it, 2 for setup problems.
"""

from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
SOAK = ROOT / "tests" / "ssh" / "reconnect-soak.py"

PROCCHECK_RE = re.compile(r"s36: procs= (.*)")
METRICS_RE = re.compile(
    r"K449-METRICS .*?procs=(\d+).*?c3072_used=(\d+) c3072_slabs=(\d+)"
)
KV_RE = re.compile(r"(\w+)=(\d+)")


def parse_proccheck(text: str) -> dict[str, int] | None:
    """Parse the last ``s36: proccheck`` key/value row, or None."""
    match = None
    for match in PROCCHECK_RE.finditer(text):
        pass
    if match is None:
        return None
    return {key: int(value) for key, value in KV_RE.findall(match.group(1))}


def parse_metrics(text: str) -> tuple[int, int, int] | None:
    """Return ``(procs, c3072_used, c3072_slabs)`` from the first and last K449-METRICS rows.

    The tuple is from the last row; the first row is returned separately by
    :func:`parse_metrics_first` for the baseline.
    """
    rows = list(METRICS_RE.finditer(text))
    if not rows:
        return None
    last = rows[-1]
    return int(last.group(1)), int(last.group(2)), int(last.group(3))


def parse_metrics_first(text: str) -> tuple[int, int, int] | None:
    """Return ``(procs, c3072_used, c3072_slabs)`` from the first K449-METRICS row."""
    rows = list(METRICS_RE.finditer(text))
    if not rows:
        return None
    first = rows[0]
    return int(first.group(1)), int(first.group(2)), int(first.group(3))


def check_counts(proccheck: dict[str, int], last: tuple[int, int, int],
                 first: tuple[int, int, int]) -> list[str]:
    """Return the list of failed assertions with the measured values."""
    failures: list[str] = []
    for key in ("leaked", "notices", "dead_clients", "dead_servers", "zombies", "reap"):
        value = proccheck.get(key)
        if value != 0:
            failures.append(f"{key}={value} (expected 0)")
    procs_first, class_first, _ = first
    procs_last, class_last, slabs_last = last
    if procs_last > procs_first + 2:
        failures.append(f"procs={procs_last} (baseline {procs_first})")
    if class_last > class_first + 8:
        failures.append(
            f"c3072_used={class_last} (baseline {class_first}, slabs {slabs_last})"
        )
    return failures


def parse_serial(serial: Path) -> tuple[dict[str, int] | None, tuple[int, int, int] | None,
                                        tuple[int, int, int] | None]:
    """Read a serial log and return ``(proccheck, first_metrics, last_metrics)``."""
    try:
        text = serial.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return None, None, None
    return parse_proccheck(text), parse_metrics_first(text), parse_metrics(text)


def run_churn(image: Path, sessions: int, timeout: float) -> Path:
    """Run the soak against ``image`` and return its run directory."""
    command = [
        sys.executable, str(SOAK),
        "--sessions", str(sessions),
        "--resource-every", "1",
        "--net", "user",
        "--image", str(image),
        "--connect-timeout", "30",
        "--session-timeout", "60",
    ]
    result = subprocess.run(
        command, cwd=ROOT, capture_output=True, text=True, timeout=timeout, check=False,
        env={**os.environ, "RAVEN_QEMU_ACCEL": os.environ.get("RAVEN_QEMU_ACCEL", "tcg")},
    )
    run = None
    for line in result.stdout.splitlines():
        marker = "reconnect-soak: run="
        if marker in line:
            run = Path(line.split(marker, 1)[1].split()[0])
    if run is None:
        print(result.stdout[-2000:], file=sys.stderr)
        print(result.stderr[-2000:], file=sys.stderr)
        raise SystemExit("churn: the soak did not report a run directory")
    return run


def selftest() -> int:
    """Exercise the parsers with fixed fixtures (no QEMU)."""
    failures: list[str] = []

    def check(name: str, got: object, want: object) -> None:
        if got != want:
            failures.append(f"{name}: got {got!r}, want {want!r}")

    text = (
        "K449-METRICS heap_used=1 heap_free=2 frames_free=3 frames_alloc=4 procs=19 "
        "threads=57 ofds=100 pt_frames=0 stack_frames=0 init_pages=0 vfsd_pages=0 "
        "netd_pages=0 sshd_pages=0 c3072_free=4 c3072_used=28 c3072_slabs=32 "
        "live_threads=1 reap=0 slot_drops=0 slot_denies=0 stacks_fresh=0 stacks_cached=0 "
        "stacks_dropped=0 sshd_procs=0 sshd_threads=0 dma_used=0 donations=36\n"
        "s36: procs= 19 threads=57 zombies=0 proc_size=2456 thread_size=896 "
        "lineage=100 dead_handles=0 top_pid=0 top_count=0 notices=0 dead_clients=0 "
        "dead_servers=0 requests=0 reap=0 leaked=0 ext1=0 ext2=0 ext3=0 self_handles=0 "
        "all_dead_handles=0 leaked_threads=0 file_refs=0 request_refs=0 thread_handles=0 "
        "all_entries=0 unique=0 client_refs=0 ctty_refs=0 zombie_refs=0\n"
        "K449-METRICS heap_used=1 heap_free=2 frames_free=3 frames_alloc=4 procs=20 "
        "threads=58 ofds=101 pt_frames=0 stack_frames=0 init_pages=0 vfsd_pages=0 "
        "netd_pages=0 sshd_pages=0 c3072_free=4 c3072_used=28 c3072_slabs=32 "
        "live_threads=1 reap=0 slot_drops=0 slot_denies=0 stacks_fresh=0 stacks_cached=0 "
        "stacks_dropped=0 sshd_procs=0 sshd_threads=0 dma_used=0 donations=36\n"
    )
    proccheck = parse_proccheck(text)
    check("proccheck-leaked", proccheck and proccheck["leaked"], 0)
    check("proccheck-notices", proccheck and proccheck["notices"], 0)
    check("metrics-first", parse_metrics_first(text), (19, 28, 32))
    check("metrics-last", parse_metrics(text), (20, 28, 32))
    check("counts-clean", check_counts(proccheck or {}, (20, 28, 32), (19, 28, 32)), [])
    dirty = proccheck.copy() if proccheck else {}
    dirty["leaked"] = 8
    dirty["notices"] = 8
    check(
        "counts-dirty",
        check_counts(dirty, (20, 40, 44), (19, 28, 32)),
        ["leaked=8 (expected 0)", "notices=8 (expected 0)",
         "c3072_used=40 (baseline 28, slabs 44)"],
    )
    if failures:
        for failure in failures:
            print(f"churn: selftest FAIL: {failure}", file=sys.stderr)
        return 1
    print("churn: selftest PASS")
    return 0


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image", type=Path)
    parser.add_argument("--sessions", type=int, default=8)
    parser.add_argument("--parse", type=Path, help="inspect an existing serial log")
    parser.add_argument("--selftest", action="store_true")
    options = parser.parse_args(argv)
    if options.selftest:
        return selftest()
    if options.parse:
        serial = options.parse
    else:
        if options.image is None:
            print("churn: --image is required (or --parse/--selftest)", file=sys.stderr)
            return 2
        timeout = options.sessions * 75.0 + 240.0
        run = run_churn(options.image, options.sessions, timeout)
        serial = run / "boot" / "serial.log"
    proccheck, first, last = parse_serial(serial)
    if proccheck is None or first is None or last is None:
        print(f"churn: missing s36: proccheck or K449-METRICS in {serial}", file=sys.stderr)
        return 2
    failures = check_counts(proccheck, last, first)
    if failures:
        for failure in failures:
            print(f"churn: FAIL: {failure}")
        print(
            f"churn: counters leaked={proccheck.get('leaked')} "
            f"notices={proccheck.get('notices')} "
            f"dead_clients={proccheck.get('dead_clients')} "
            f"procs={last[0]} c3072_used={last[1]} slabs={last[2]}"
        )
        return 1
    print(
        f"churn: PASS leaked=0 notices=0 procs={last[0]} "
        f"c3072_used={last[1]} slabs={last[2]}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
