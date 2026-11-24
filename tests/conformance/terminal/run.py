#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""S175 terminal conformance runner (test-only).

Builds /bin/terminal-conformance into a release image, boots it, and runs the probe over `ssh`.
The `--no-build` mode reuses an image built by the same tree. The four gate rows (termios, pty pair,
controlling terminal, EIO) must PASS; the three signal rows are PENDING until S44 lands. The
`--mutation-termios` run must turn the termios row red. No product code changes.

Build separately when iterating (each step has its own cap):
  sh tests/conformance/terminal/build.sh
  sh addons/ssh/build.sh
  rm -rf build/cargo/uutils-x86_64
  cargo xtask initfs --arch x86_64 --profile release
  target/debug/raven-b70-overlay build/x86_64/initfs.img build/conformance/terminal/initfs.img \
      /bin/terminal-conformance=$PWD/build/conformance/terminal/terminal-conformance:0755
  cargo xtask image --arch x86_64 --profile release --initfs build/conformance/terminal/initfs.img \
      --cmdline "console=ttyS0 test-exit=1" --output build/qtest/s175-terminal.img
  python3 tests/conformance/terminal/run.py --no-build
"""

from __future__ import annotations

import os
import shutil
import signal
import socket
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
IMAGE = Path(os.environ.get("S175_TERM_IMAGE", ROOT / "build/qtest/s175-terminal.img"))
OUT = Path(os.environ.get("S175_TERM_OUT", ROOT / "build/conformance/terminal/run"))
OVMF_CODE = Path("/usr/share/OVMF/OVMF_CODE_4M.fd")
OVMF_VARS = Path("/usr/share/OVMF/OVMF_VARS_4M.fd")
ASKPASS = ROOT / "tests/ssh/askpass.sh"
GATE_ROWS = ("termios", "pty-both-ways", "ctty", "eio")
PENDING_ROWS = ("sigwinch", "sighup", "isig")


def choose_port() -> int:
    value = os.environ.get("S175_TERM_PORT")
    if value:
        return int(value)
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as probe:
        probe.bind(("127.0.0.1", 0))
        return int(probe.getsockname()[1])


PORT = choose_port()
SERIAL = OUT / "serial.log"


def step(label: str, argv: list[str], timeout: int) -> None:
    print(f"S175-TERM-RUN {label}", flush=True)
    result = subprocess.run(argv, cwd=str(ROOT), capture_output=True, timeout=timeout)
    (OUT / f"{label}.log").write_bytes(result.stdout + result.stderr)
    if result.returncode != 0:
        raise SystemExit(f"S175-TERM-RUN {label} failed rc={result.returncode}; see {OUT}/{label}.log")


def build_image() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    step("fixture-build", ["sh", "tests/conformance/terminal/build.sh"], 240)
    step("addon-build", ["sh", "addons/ssh/build.sh"], 300)
    shutil.rmtree(ROOT / "build/cargo/uutils-x86_64", ignore_errors=True)
    step("initfs-build",
         ["cargo", "xtask", "initfs", "--arch", "x86_64", "--profile", "release"], 600)
    initfs = OUT / "terminal-initfs.img"
    step(
        "overlay",
        [
            "target/debug/raven-b70-overlay",
            "build/x86_64/initfs.img",
            str(initfs),
            f"/bin/terminal-conformance={ROOT}/build/conformance/terminal/terminal-conformance:0755",
        ],
        120,
    )
    step(
        "image-build",
        [
            "cargo", "xtask", "image", "--arch", "x86_64", "--profile", "release",
            "--initfs", str(initfs), "--cmdline", "console=ttyS0 test-exit=1",
            "--output", str(IMAGE),
        ],
        900,
    )


def run_probe(command: str, timeout: int = 30) -> tuple[int, str, str, bool]:
    """Run one probe; return (rc, stdout, stderr, timed_out).

    Raven's sshd does not close the session channel after the remote command exits, so the
    probe's `S175-TERM done failures=` marker is the completion witness. The client is bounded
    and killed after the marker/timeout; the captured output is the row evidence.
    """
    ssh = [
        "ssh", "-p", str(PORT),
        "-o", "StrictHostKeyChecking=no",
        "-o", "UserKnownHostsFile=/dev/null",
        "-o", "PreferredAuthentications=password",
        "-o", "PubkeyAuthentication=no",
        "-o", "KbdInteractiveAuthentication=no",
        "-o", "NumberOfPasswordPrompts=1",
        "-o", "ConnectTimeout=15",
        "raven@127.0.0.1", command,
    ]
    env = dict(os.environ)
    env["SSH_ASKPASS"] = str(ASKPASS)
    env["SSH_ASKPASS_REQUIRE"] = "force"
    env["DISPLAY"] = ":0"
    env["K434_SSH_PASSWORD"] = "raven"
    try:
        result = subprocess.run(ssh, cwd=str(ROOT), stdin=subprocess.DEVNULL,
                                capture_output=True, timeout=timeout, env=env,
                                start_new_session=True)
        return (result.returncode, result.stdout.decode("utf-8", "replace"),
                result.stderr.decode("utf-8", "replace"), False)
    except subprocess.TimeoutExpired as error:
        return (-1, (error.stdout or b"").decode("utf-8", "replace"),
                (error.stderr or b"").decode("utf-8", "replace"), True)


def rows(output: str) -> dict[str, str]:
    found: dict[str, str] = {}
    for line in output.splitlines():
        if not line.startswith("S175-TERM row="):
            continue
        parts = line.split()
        name = parts[1].split("=", 1)[1]
        status = parts[2].split("=", 1)[1]
        found[name] = status
    return found


def main() -> int:
    no_build = "--no-build" in sys.argv[1:]
    if not no_build or not IMAGE.is_file():
        build_image()
    else:
        print(f"S175-TERM-RUN image reuse {IMAGE}", flush=True)

    OUT.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(OVMF_VARS, OUT / "vars.fd")
    SERIAL.write_bytes(b"")
    argv = [
        "qemu-system-x86_64", "-machine", "q35", "-m", "512M", "-smp", "4",
        "-enable-kvm", "-cpu", "host",
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
    qemu = subprocess.Popen(argv, cwd=str(ROOT), stdin=subprocess.DEVNULL,
                            stdout=subprocess.DEVNULL,
                            stderr=open(OUT / "qemu.stderr", "wb"), start_new_session=True)
    failures: list[str] = []
    try:
        deadline = time.monotonic() + 180
        booted = False
        while time.monotonic() < deadline:
            if qemu.poll() is not None:
                break
            if b"svcd: sshd up" in SERIAL.read_bytes():
                booted = True
                break
            time.sleep(0.2)
        if not booted:
            failures.append("boot:sshd-not-ready")

        normal_rc, normal, normal_err, normal_timed = run_probe(
            "/bin/terminal-conformance --all")
        (OUT / "normal.log").write_text(normal, encoding="utf-8")
        (OUT / "normal.stderr").write_text(normal_err, encoding="utf-8")
        print(normal, end="", flush=True)
        normal_rows = rows(normal)
        for name in GATE_ROWS:
            if normal_rows.get(name) != "PASS":
                failures.append(f"normal:{name}={normal_rows.get(name)}")
        for name in PENDING_ROWS:
            if normal_rows.get(name) != "PENDING":
                failures.append(f"normal:{name}={normal_rows.get(name)}")
        if "S175-TERM done failures=" not in normal:
            failures.append("normal:incomplete-output")
        elif "S175-TERM done failures=0" not in normal:
            failures.append("normal:done-not-zero")
        if normal_timed and "S175-TERM done failures=" not in normal:
            failures.append("normal:probe-timeout")

        mutation_rc, mutation, mutation_err, mutation_timed = run_probe(
            "/bin/terminal-conformance --mutation-termios")
        (OUT / "mutation.log").write_text(mutation, encoding="utf-8")
        (OUT / "mutation.stderr").write_text(mutation_err, encoding="utf-8")
        print(mutation, end="", flush=True)
        mutation_rows = rows(mutation)
        if mutation_rows.get("termios") != "FAIL":
            failures.append(f"mutation:termios={mutation_rows.get('termios')}")
        if "S175-TERM done failures=" not in mutation:
            failures.append("mutation:incomplete-output")

        print(f"S175-TERM-RUN normal_rc={normal_rc} normal_timed={normal_timed} "
              f"mutation_rc={mutation_rc} mutation_timed={mutation_timed} "
              f"ssh_session_close=timeout", flush=True)
        if failures:
            print(f"S175-TERM-RUN FAIL reasons={';'.join(failures)}", flush=True)
            return 1
        print("S175-TERM-RUN PASS", flush=True)
        return 0
    finally:
        try:
            os.killpg(qemu.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        qemu.wait()


if __name__ == "__main__":
    raise SystemExit(main())
