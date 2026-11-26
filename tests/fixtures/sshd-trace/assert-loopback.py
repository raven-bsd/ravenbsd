#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""K547 loopback-only session-close assertion.

No tap and no NIC. The guest boots with the DEBUG3 sshd, the console starts the
guest loopback client through the ELF askpass, and `/bin/sshd-trace-logdump`
writes only the close-sequence lines through the kernel debug writer. The
script asserts the measured order:

    exec -> 293 bytes -> child exit -> exit-status -> EOF -> CLOSE -> session close

The run depends on K548: until the kernel stops returning EACCES from the
EVQ_CTL source-rights gate for a write-only duplex, the client dies with
`poll: Permission denied`, exits 255 and disconnects before the child-exit
report. In that state the script reports BLOCKED-K548, not a pass.
"""

from __future__ import annotations

import os
import re
import sys
import time
from pathlib import Path

import pexpect

ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / "build" / "fixtures" / "sshd-trace"
IMAGE = OUT / "sshd-trace.img"
SERIAL = OUT / "loopback-serial.log"
SUMMARY = OUT / "loopback-summary.txt"
OVMF_CODE = Path(os.environ.get("RAVEN_OVMF_CODE", "/usr/share/OVMF/OVMF_CODE_4M.fd"))
OVMF_VARS = Path(os.environ.get("RAVEN_OVMF_VARS", "/usr/share/OVMF/OVMF_VARS_4M.fd"))

SEQUENCE = [
    ("exec", r"Starting session: command for raven"),
    ("bytes-read", r"channel 0: read 293 from"),
    ("bytes-sent", r"channel 0: sent ext data 293"),
    ("sigchld", r"Received SIGCHLD\."),
    ("session-pid", r"session_by_pid: pid \d+"),
    ("exit-status", r"session_exit_message: session 0 channel 0 pid \d+ exit 0"),
    ("eof", r"channel 0: send eof"),
    ("close", r"channel 0: send close"),
    ("session-close", r"Close session: user raven"),
]


def log(kind: str, text: str) -> None:
    print(f"loopback-trace: {kind}: {text}", flush=True)


def console(child: pexpect.spawn, command: str, marker: str, timeout: int) -> bool:
    child.sendline(command)
    try:
        child.expect(marker, timeout=timeout)
        return True
    except pexpect.TIMEOUT:
        return False


def sequence_failure(lines: list[str]) -> str | None:
    position = 0
    for name, pattern in SEQUENCE:
        found = -1
        for index in range(position, len(lines)):
            if re.search(pattern, lines[index]):
                found = index
                break
        if found < 0:
            return name
        position = found + 1
    return None


def selftest() -> int:
    good = [
        "debug1: Starting session: command for raven from UNKNOWN port 65535 id 0",
        "debug2: channel 0: read 293 from efd 15",
        "debug2: channel 0: sent ext data 293",
        "debug1: Received SIGCHLD.",
        "debug1: session_by_pid: pid 42",
        "debug1: session_exit_message: session 0 channel 0 pid 42 exit 0",
        "debug2: channel 0: send eof",
        "debug2: channel 0: send close for remote id 0",
        "verbose: Close session: user raven from UNKNOWN port 65535 id 0",
    ]
    failure = sequence_failure(good)
    if failure is not None:
        log("selftest", f"full sequence rejected at {failure}")
        return 1
    missing = [line for line in good if "send eof" not in line]
    failure = sequence_failure(missing)
    if failure != "eof":
        log("selftest", f"missing eof reported {failure}")
        return 1
    reordered = good.copy()
    eof_index = next(index for index, line in enumerate(reordered) if "send eof" in line)
    close_index = next(index for index, line in enumerate(reordered) if "send close" in line)
    reordered[eof_index], reordered[close_index] = reordered[close_index], reordered[eof_index]
    failure = sequence_failure(reordered)
    if failure != "close":
        log("selftest", f"reordered close/eof reported {failure}")
        return 1
    log("selftest", "PASS (full, missing eof, reordered close)")
    return 0


def main() -> int:
    if not IMAGE.is_file():
        log("error", f"missing {IMAGE}; run tests/fixtures/sshd-trace/build.sh")
        return 2
    for path in (SERIAL, SUMMARY):
        path.unlink(missing_ok=True)
    vars_path = OUT / "vars-loopback.fd"
    vars_path.write_bytes(OVMF_VARS.read_bytes())
    argv = [
        "-machine", "q35", "-m", "512M", "-smp", "4",
        "-serial", "stdio", "-display", "none", "-monitor", "none", "-no-reboot",
        "-drive", f"if=pflash,format=raw,readonly=on,file={OVMF_CODE}",
        "-drive", f"if=pflash,format=raw,file={vars_path}",
        "-drive", f"format=raw,file={IMAGE}",
        "-device", "isa-debug-exit,iobase=0xf4,iosize=0x04",
        "-nic", "none",
    ]
    child = pexpect.spawn("qemu-system-x86_64", argv, cwd=str(ROOT),
                          encoding="utf-8", codec_errors="replace", timeout=240)
    state = "boot"
    with open(SERIAL, "w", encoding="utf-8") as serial:
        child.logfile = serial
        try:
            child.expect("svcd: getty up", timeout=240)
            child.expect(r"login: ?", timeout=120)
            child.sendline("root")
            child.expect(r"(?i)password: ?", timeout=30)
            child.sendline("raven")
            child.expect("raven# ", timeout=120)
            console(child, "/bin/brush /etc/ssh/trace-client.sh > /tmp/loop.log 2>&1 & "
                           "M=Q; echo ${M}S", "QS", 90)
            time.sleep(20)
            console(child, "cat /tmp/loop.log; M=Q; echo ${M}L", "QL", 60)
            console(child, "/bin/sshd-trace-logdump", "LOGDONE", 90)
            state = "collected"
        finally:
            child.close(force=True)
    text = SERIAL.read_text(encoding="utf-8", errors="replace")
    loop_exit = re.search(r"sshd-trace-loop: exit=(\d+)", text)
    poll_eacces = "poll: Permission denied" in text
    loop_ok = "sshd-trace-loop-ok" in text
    summary = [
        f"state={state}",
        f"loop_ok={loop_ok}",
        f"loop_exit={loop_exit.group(1) if loop_exit else 'missing'}",
        f"poll_eacces={poll_eacces}",
    ]
    if poll_eacces or (loop_exit and loop_exit.group(1) != "0"):
        SUMMARY.write_text("\n".join(summary) + "\nblocked=k548\n", encoding="utf-8")
        log("blocked", "K548 poll EACCES / non-zero client exit; close sequence not reachable")
        return 2
    if not loop_ok:
        SUMMARY.write_text("\n".join(summary) + "\nblocked=client-timeout\n", encoding="utf-8")
        log("blocked", "guest loopback client did not print the command marker")
        return 2
    lines = [line[len("LOGLINE "):] for line in text.splitlines() if line.startswith("LOGLINE ")]
    failure = sequence_failure(lines)
    if failure is not None:
        SUMMARY.write_text("\n".join(summary) + f"\nmissing={failure}\n", encoding="utf-8")
        log("fail", f"missing ordered step {failure}")
        return 1
    SUMMARY.write_text("\n".join(summary) + "\nresult=PASS\n", encoding="utf-8")
    log("pass", f"ordered sequence {len(SEQUENCE)}/{len(SEQUENCE)}; serial {SERIAL}")
    return 0


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "--selftest":
        sys.exit(selftest())
    sys.exit(main())
