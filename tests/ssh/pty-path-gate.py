#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""K495 rev175: the pty-path acceptance row (test-only gate).

Row definition:
  * three live `ssh -tt /bin/pty-path` sessions; each must print a `/dev/pts/<N>` path;
  * `ttyname(0)` must resolve: the fixture line `PTY-PATH session path=/dev/pts/<N>`;
  * the new pair must be released after close: `PTY-PATH ... release=ok`;
  * the fixture must finish `PTY-PATH ok`;
  * no orphan sshd-session/session child may remain: on a `k449-metrics=2` image the K449 `procs`
    after the sessions must not exceed the baseline (a `ps -e` `child` count is the fallback for
    images without K449; the zombie children are not in the ps table).

Pass iff all conditions hold. The mount mutation (no `/dev/pts pty:` entry) must fail the row.
Before K536 is re-landed the row is red; the exact failing output is the before-state.

Environment:
  PTY_GATE_IMAGE           image to boot (required for a real row)
  PTY_GATE_LABEL           row label in the output and the default artifact directory
  PTY_GATE_OUT             artifact directory (default /tmp/opencode/pty-gate-<label>)
  PTY_GATE_SESSIONS        session count (default 3)
  PTY_GATE_ACCEL           qemu accelerator: kvm or tcg (falls back to RAVEN_QEMU_ACCEL)
  PTY_GATE_PORT            host ssh port; an ephemeral port is chosen when unset
  PTY_GATE_EXPECT_METRICS  1: require K449 samples (the image has k449-metrics=2)
"""

from __future__ import annotations

import os
import re
import shutil
import signal
import socket
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path("/media/bo/Rustworx/work/ravenbsd-minions/phil/ravenbsd")
IMAGE = Path(os.environ.get("PTY_GATE_IMAGE", ROOT / "build/qtest/pty-path-mutation.img"))
LABEL = os.environ.get("PTY_GATE_LABEL", "before")
SESSIONS = int(os.environ.get("PTY_GATE_SESSIONS", "3"))
EXPECT_METRICS = os.environ.get("PTY_GATE_EXPECT_METRICS", "0") == "1"
SSHD_LOG = os.environ.get("PTY_GATE_SSHD_LOG", "0") == "1"
OUT = Path(os.environ.get("PTY_GATE_OUT", f"/tmp/opencode/pty-gate-{LABEL}"))
OVMF_CODE = Path("/usr/share/OVMF/OVMF_CODE_4M.fd")
OVMF_VARS = Path("/usr/share/OVMF/OVMF_VARS_4M.fd")
ASKPASS = ROOT / "tests/ssh/askpass.sh"


def choose_port() -> int:
    """Use PTY_GATE_PORT, or an ephemeral loopback port so parallel rows cannot collide."""
    value = os.environ.get("PTY_GATE_PORT")
    if value:
        return int(value)
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as probe:
        probe.bind(("127.0.0.1", 0))
        return int(probe.getsockname()[1])


PORT = choose_port()
METRICS_RE = re.compile(
    r"K449-METRICS .* procs=(\d+) threads=(\d+) ofds=(\d+)"
)
SERIAL = OUT / "serial.log"
OUT.mkdir(parents=True, exist_ok=True)
shutil.copyfile(OVMF_VARS, OUT / "vars.fd")
SERIAL.write_bytes(b"")

accel = os.environ.get("PTY_GATE_ACCEL", os.environ.get("RAVEN_QEMU_ACCEL", "kvm"))
argv = [
    "qemu-system-x86_64", "-machine", "q35", "-m", "512M", "-smp", "4",
    "-serial", f"file:{SERIAL}", "-display", "none", "-monitor", "none", "-no-reboot",
    "-snapshot",
    "-drive", f"if=pflash,format=raw,readonly=on,file={OVMF_CODE}",
    "-drive", f"if=pflash,format=raw,file={OUT / 'vars.fd'}",
    "-drive", f"format=raw,file={IMAGE}",
    "-device", "isa-debug-exit,iobase=0xf4,iosize=0x04",
    "-nic", "none",
    "-netdev", f"user,id=n0,hostfwd=tcp:127.0.0.1:{PORT}-:22",
    "-device", "virtio-net-pci,netdev=n0,disable-legacy=on",
]
if accel == "kvm":
    argv.extend(["-enable-kvm", "-cpu", "host"])
elif accel == "tcg":
    argv.extend(["-accel", "tcg"])
else:
    raise SystemExit(f"pty-path-gate: unknown accelerator {accel}")

qemu = subprocess.Popen(argv, cwd=str(ROOT), stdin=subprocess.DEVNULL,
                        stdout=subprocess.DEVNULL,
                        stderr=open(OUT / "qemu.stderr", "wb"), start_new_session=True)


def kill() -> None:
    try:
        os.killpg(qemu.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    qemu.wait()


def ssh_env() -> dict[str, str]:
    env = dict(os.environ)
    env["SSH_ASKPASS"] = str(ASKPASS)
    env["SSH_ASKPASS_REQUIRE"] = "force"
    env["DISPLAY"] = ":0"
    env["K434_SSH_PASSWORD"] = "raven"
    return env


def ssh_argv(tty: bool, command: str) -> list[str]:
    return [
        "ssh", "-tt" if tty else "-T", "-p", str(PORT),
        "-o", "StrictHostKeyChecking=no",
        "-o", "UserKnownHostsFile=/dev/null",
        "-o", "PreferredAuthentications=password",
        "-o", "PubkeyAuthentication=no",
        "-o", "KbdInteractiveAuthentication=no",
        "-o", "NumberOfPasswordPrompts=1",
        "-o", "ConnectTimeout=20",
        "raven@127.0.0.1", command,
    ]


def serial_text() -> str:
    try:
        return SERIAL.read_text(errors="replace")
    except FileNotFoundError:
        return ""


def run(command: str, tty: bool, timeout: int = 12) -> tuple[int, str]:
    try:
        result = subprocess.run(ssh_argv(tty, command), cwd=str(ROOT),
                                stdin=subprocess.DEVNULL, capture_output=True,
                                timeout=timeout, env=ssh_env(), start_new_session=True)
        return result.returncode, result.stdout.decode("utf-8", "replace")
    except subprocess.TimeoutExpired as error:
        return -1, (error.stdout or b"").decode("utf-8", "replace")


def metrics_snapshot() -> tuple[int, int, int] | None:
    """Newest K449-METRICS procs/threads/ofds from a `k449-metrics=2` image."""
    matches = METRICS_RE.findall(serial_text())
    if not matches:
        return None
    procs, threads, ofds = matches[-1]
    return int(procs), int(threads), int(ofds)


def wait_metrics(marker: int, timeout: float = 10.0) -> tuple[int, int, int] | None:
    """Wait for a K449-METRICS sample appended after `marker`; newest one wins."""
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        matches = METRICS_RE.findall(serial_text()[marker:])
        if matches:
            procs, threads, ofds = matches[-1]
            return int(procs), int(threads), int(ofds)
        time.sleep(0.25)
    return None


def ps_snapshot() -> tuple[int, int, list[str]]:
    """Run `ps -e` and count the process rows printed on the serial.

    The first-party ps header is occasionally interleaved with the ptyd stats, so the row shape
    (four numeric columns followed by the command) is used instead of the header.
    """
    before = len(serial_text())
    run("ps -e", tty=False, timeout=12)
    time.sleep(0.5)
    text = serial_text()[before:]
    rows: list[str] = []
    for line in text.splitlines():
        fields = line.split()
        if len(fields) >= 5 and all(field.isdigit() for field in fields[:4]):
            rows.append(line.strip())
    child_rows = sum(1 for line in rows if line.split()[-1] == "child")
    return len(rows), child_rows, rows


def main() -> int:
    deadline = time.monotonic() + 150
    while time.monotonic() < deadline:
        if qemu.poll() is not None:
            break
        if b"svcd: sshd up" in SERIAL.read_bytes():
            break
        time.sleep(0.2)

    failures: list[str] = []
    baseline_marker = len(serial_text())
    baseline_metrics = wait_metrics(baseline_marker) if EXPECT_METRICS else metrics_snapshot()
    baseline_rows, baseline_child, _ = ps_snapshot()
    if EXPECT_METRICS:
        # The baseline `ps` probe is itself an ssh session. Re-baseline after it so its own
        # session cannot be mistaken for a pty-path orphan.
        settled = wait_metrics(len(serial_text()))
        if settled is None:
            failures.append("k449 settled baseline sample missing")
        else:
            baseline_metrics = settled
    print(f"PTY-PATH-GATE {LABEL} baseline rows={baseline_rows} child_rows={baseline_child} "
          f"metrics={baseline_metrics}", flush=True)

    if EXPECT_METRICS and baseline_metrics is None:
        failures.append("k449 baseline sample missing")
    for number in range(1, SESSIONS + 1):
        exit_code, output = run("/bin/pty-path", tty=True, timeout=15)
        text = "\n".join(line for line in output.splitlines() if "PTY-PATH" in line)
        path_seen = re.search(r"/dev/pts/[0-9]+", output) is not None
        ttyname_ok = re.search(r"PTY-PATH session path=/dev/pts/[0-9]+", output) is not None
        release_ok = re.search(r"PTY-PATH pair .* release=ok", output) is not None
        fixture_ok = "PTY-PATH ok" in output
        print(f"PTY-PATH-GATE {LABEL} session={number} exit={exit_code} "
              f"path={path_seen} ttyname={ttyname_ok} release={release_ok} "
              f"fixture_ok={fixture_ok}", flush=True)
        print(text, flush=True)
        if not path_seen:
            failures.append(f"session{number}:no /dev/pts/<N>")
        if not ttyname_ok:
            failures.append(f"session{number}:ttyname(0) unresolved")
        if not release_ok:
            failures.append(f"session{number}:pair not released")
        if not fixture_ok:
            failures.append(f"session{number}:PTY-PATH FAIL")

    after_marker = len(serial_text())
    after_metrics = wait_metrics(after_marker) if EXPECT_METRICS else None
    after_rows, after_child, table = ps_snapshot()
    if EXPECT_METRICS and after_metrics is None:
        failures.append("k449 after sample missing")
    if baseline_metrics is not None and after_metrics is not None:
        orphan = after_metrics[0] - baseline_metrics[0]
        orphan_source = "k449.procs"
    else:
        orphan = after_child - baseline_child
        orphan_source = "ps.child"
    print(f"PTY-PATH-GATE {LABEL} after rows={after_rows} child_rows={after_child} "
          f"metrics={after_metrics} orphan={orphan} source={orphan_source}", flush=True)
    for line in table:
        if line.split()[-1] == "child":
            print(f"PTY-PATH-GATE {LABEL} child {line}", flush=True)
    if orphan > 0:
        failures.append(f"orphan sshd-session children={orphan}")

    if SSHD_LOG:
        code, output = run("cat /tmp/sshd.log", tty=False, timeout=12)
        print(f"PTY-PATH-GATE {LABEL} sshdlog exit={code}", flush=True)
        for line in output.splitlines():
            if re.search(r"TIOCSCTTY|controlling|setsid|Accepted password|Failed to", line):
                print(f"PTY-PATH-GATE {LABEL} sshdlog {line.strip()}", flush=True)
        for line in output.splitlines()[-20:]:
            print(f"PTY-PATH-GATE {LABEL} sshdlog-tail {line}", flush=True)

    if failures:
        print(f"PTY-PATH-GATE {LABEL} FAIL reasons={';'.join(failures)}", flush=True)
        return 1
    print(f"PTY-PATH-GATE {LABEL} PASS", flush=True)
    return 0


try:
    raise SystemExit(main())
finally:
    kill()
