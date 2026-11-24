#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""S180 IPC conformance runner (test-only).

Builds /bin/ipc-conformance, installs it in a release image, boots the image, and runs the probe
over `ssh`: the normal `--all` run must PASS every row, and the `--mutation-rights` run must
turn the SCM_RIGHTS rights row red. The mutation is test-only; no product code changes.
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

ROOT = Path("/media/bo/Rustworx/work/ravenbsd-minions/phil/ravenbsd")
IMAGE = Path(os.environ.get("S180_IPC_IMAGE", ROOT / "build/qtest/s180-ipc.img"))
OUT = Path(os.environ.get("S180_IPC_OUT", "/tmp/opencode/s180-ipc"))
OVMF_CODE = Path("/usr/share/OVMF/OVMF_CODE_4M.fd")
OVMF_VARS = Path("/usr/share/OVMF/OVMF_VARS_4M.fd")
ASKPASS = ROOT / "tests/ssh/askpass.sh"


def choose_port() -> int:
    value = os.environ.get("S180_IPC_PORT")
    if value:
        return int(value)
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as probe:
        probe.bind(("127.0.0.1", 0))
        return int(probe.getsockname()[1])


PORT = choose_port()
SERIAL = OUT / "serial.log"


def step(label: str, argv: list[str], timeout: int) -> None:
    print(f"S180-RUN {label}", flush=True)
    result = subprocess.run(argv, cwd=str(ROOT), capture_output=True, timeout=timeout)
    (OUT / f"{label}.log").write_bytes(result.stdout + result.stderr)
    if result.returncode != 0:
        raise SystemExit(f"S180-RUN {label} failed rc={result.returncode}; see {OUT}/{label}.log")


def build_image() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    step("fixture-build", ["sh", "tests/conformance/ipc/build.sh"], 180)
    step("addon-build", ["sh", "addons/ssh/build.sh"], 300)
    # The uutils staging cache can hold a crate built against another clone's toolchain; purge it
    # so the initfs build uses this clone's libc/toolchain.
    shutil.rmtree(ROOT / "build/cargo/uutils-x86_64", ignore_errors=True)
    step("initfs-build", ["cargo", "xtask", "initfs", "--arch", "x86_64", "--profile", "release"], 600)
    initfs = OUT / "ipc-initfs.img"
    step(
        "overlay",
        [
            "target/debug/raven-b70-overlay",
            "build/x86_64/initfs.img",
            str(initfs),
            f"/bin/ipc-conformance={ROOT}/build/conformance/ipc/ipc-conformance:0755",
        ],
        120,
    )
    step(
        "image-build",
        [
            "cargo", "xtask", "image", "--arch", "x86_64", "--profile", "release",
            "--initfs", str(initfs), "--cmdline", "console=ttyS0 test-exit=1 k449-metrics=2",
            "--output", str(IMAGE),
        ],
        900,
    )


def run_probe(command: str, timeout: int = 60) -> tuple[int, str]:
    ssh = [
        "ssh", "-p", str(PORT),
        "-o", "StrictHostKeyChecking=no",
        "-o", "UserKnownHostsFile=/dev/null",
        "-o", "PreferredAuthentications=password",
        "-o", "PubkeyAuthentication=no",
        "-o", "KbdInteractiveAuthentication=no",
        "-o", "NumberOfPasswordPrompts=1",
        "-o", "ConnectTimeout=20",
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
        return result.returncode, result.stdout.decode("utf-8", "replace")
    except subprocess.TimeoutExpired as error:
        return -1, (error.stdout or b"").decode("utf-8", "replace")


def rows(output: str) -> dict[str, str]:
    found: dict[str, str] = {}
    for line in output.splitlines():
        if not line.startswith("S180-IPC row="):
            continue
        parts = line.split()
        name = parts[1].split("=", 1)[1]
        status = parts[2].split("=", 1)[1]
        found[name] = status
    return found


def main() -> int:
    if not IMAGE.is_file():
        build_image()
    else:
        print(f"S180-RUN image reuse {IMAGE}", flush=True)

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
    try:
        deadline = time.monotonic() + 150
        while time.monotonic() < deadline:
            if qemu.poll() is not None:
                break
            if b"svcd: sshd up" in SERIAL.read_bytes():
                break
            time.sleep(0.2)

        failures: list[str] = []
        normal_rc, normal = run_probe("/bin/ipc-conformance --all")
        (OUT / "normal.log").write_text(normal, encoding="utf-8")
        print(normal, end="", flush=True)
        normal_rows = rows(normal)
        for name in ("rights", "credentials", "cloexec", "fifo", "seqpacket", "stream"):
            if normal_rows.get(name) != "PASS":
                failures.append(f"normal:{name}={normal_rows.get(name)}")
        if "S180-IPC done failures=0" not in normal:
            failures.append("normal:done-not-zero")

        mutation_rc, mutation = run_probe("/bin/ipc-conformance --mutation-rights")
        (OUT / "mutation.log").write_text(mutation, encoding="utf-8")
        print(mutation, end="", flush=True)
        mutation_rows = rows(mutation)
        if mutation_rows.get("rights") != "FAIL":
            failures.append(f"mutation:rights={mutation_rows.get('rights')}")

        print(f"S180-IPC-RUN normal_rc={normal_rc} mutation_rc={mutation_rc}", flush=True)
        if failures:
            print(f"S180-IPC-RUN FAIL reasons={';'.join(failures)}", flush=True)
            return 1
        print("S180-IPC-RUN PASS", flush=True)
        return 0
    finally:
        try:
            os.killpg(qemu.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        qemu.wait()


try:
    raise SystemExit(main())
finally:
    pass
