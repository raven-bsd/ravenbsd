#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""Validate the K280 merged-TCP verification artifacts.

The verifier does not turn a failing test into a pass. It checks that every
required qtest result is present and terminal, and that every K253 scenario has
a non-empty pcap and guest serial log. A test `fail` is a valid finding; a
missing or malformed artifact is a verification failure.

Usage:
    python3 tests/tcp-merged/verify-results.py \
        --qtest-dir build/qtest \
        --netconf-dir /tmp/opencode/k280-netconf \
        --netconf-log-dir /tmp/opencode
    python3 tests/tcp-merged/verify-results.py --selftest
"""

from __future__ import annotations

import argparse
import json
import shutil
import sys
import tempfile
from pathlib import Path

QTESTS = (
    "socket-inet",
    "socket-inet-kvm-smp2",
    "socket-inet-kvm-smp4",
    "socket-inet-tcg-smp4",
    "net-host",
    "fetch-https",
)
NETCONF = ("handshake", "zero-window", "window-update", "retransmit", "fin-rst")
TERMINAL = {"pass", "fail", "timeout", "error", "crash"}


def qtest_status(qtest_dir: Path, case: str) -> tuple[bool, str]:
    path = qtest_dir / f"{case}.json"
    if not path.is_file():
        return False, f"{case}: MISSING {path}"
    try:
        report = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as error:
        return False, f"{case}: BAD JSON {error}"
    result = report.get("result")
    if result not in TERMINAL:
        return False, f"{case}: NON-TERMINAL result={result!r}"
    return True, f"{case}: {result} ({report.get('duration_ms', '?')} ms)"


def netconf_status(netconf_dir: Path, log_dir: Path, scenario: str) -> tuple[bool, str]:
    pcap = netconf_dir / f"{scenario}.pcap"
    serial = netconf_dir / f"{scenario}.serial.log"
    missing = [str(path) for path in (pcap, serial) if not path.is_file() or path.stat().st_size == 0]
    if missing:
        return False, f"{scenario}: MISSING/EMPTY {', '.join(missing)}"
    log = log_dir / f"k280-netconf-{scenario}.log"
    verdict = "artifact-only"
    if log.is_file():
        for line in log.read_text(encoding="utf-8", errors="replace").splitlines():
            if f"net-conformance: {scenario} pass " in line:
                verdict = "pass"
            elif f"net-conformance: {scenario} fail " in line:
                verdict = "fail"
    return True, f"{scenario}: {verdict} ({pcap.stat().st_size} B pcap)"


def check(qtest_dir: Path, netconf_dir: Path, log_dir: Path) -> int:
    failures = 0
    print("case\tstatus\tdetail")
    for case in QTESTS:
        ok, detail = qtest_status(qtest_dir, case)
        print(f"{case}\t{'PASS' if ok else 'FAIL'}\t{detail}")
        failures += 0 if ok else 1
    for scenario in NETCONF:
        ok, detail = netconf_status(netconf_dir, log_dir, scenario)
        print(f"netconf/{scenario}\t{'PASS' if ok else 'FAIL'}\t{detail}")
        failures += 0 if ok else 1
    print(f"verify-results: {failures} artifact failure(s)")
    return 0 if failures == 0 else 1


def write_fixture(root: Path) -> None:
    qtest = root / "qtest"
    netconf = root / "netconf"
    logs = root / "logs"
    for directory in (qtest, netconf, logs):
        directory.mkdir(parents=True)
    for index, case in enumerate(QTESTS):
        (qtest / f"{case}.json").write_text(
            json.dumps({"case": case, "result": "pass" if index % 2 == 0 else "fail",
                        "duration_ms": 1, "exit_status": 0}),
            encoding="utf-8",
        )
    for scenario in NETCONF:
        (netconf / f"{scenario}.pcap").write_bytes(b"pcap")
        (netconf / f"{scenario}.serial.log").write_text("serial\n", encoding="utf-8")
        (logs / f"k280-netconf-{scenario}.log").write_text(
            f"net-conformance: {scenario} pass (1s) fixture\n", encoding="utf-8"
        )


def selftest() -> int:
    with tempfile.TemporaryDirectory(prefix="k280-verify-") as temporary:
        root = Path(temporary)
        write_fixture(root)
        baseline = check(root / "qtest", root / "netconf", root / "logs")
        if baseline != 0:
            print("verify-results: selftest FAIL (valid fixture rejected)", file=sys.stderr)
            return 1
        (root / "qtest" / "socket-inet.json").unlink()
        mutated = check(root / "qtest", root / "netconf", root / "logs")
        if mutated == 0:
            print("verify-results: selftest FAIL (missing artifact accepted)", file=sys.stderr)
            return 1
        (root / "netconf" / "handshake.pcap").write_bytes(b"")
        shutil.copy(root / "qtest" / "net-host.json", root / "qtest" / "socket-inet.json")
        empty = check(root / "qtest", root / "netconf", root / "logs")
        if empty == 0:
            print("verify-results: selftest FAIL (empty pcap accepted)", file=sys.stderr)
            return 1
    print("verify-results: selftest PASS (valid fixture accepted; missing/empty rejected)")
    return 0


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--qtest-dir", default="build/qtest")
    parser.add_argument("--netconf-dir", default="/tmp/opencode/k280-netconf")
    parser.add_argument("--netconf-log-dir", default="/tmp/opencode")
    parser.add_argument("--selftest", action="store_true")
    args = parser.parse_args(argv)
    if args.selftest:
        return selftest()
    return check(Path(args.qtest_dir), Path(args.netconf_dir), Path(args.netconf_log_dir))


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
