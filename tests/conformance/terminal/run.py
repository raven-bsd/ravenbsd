#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""S175 terminal conformance runner (test-only).

Builds /bin/terminal-conformance into a release initfs with a one-shot service, boots the image,
and reads the probe rows from the serial console. The four gate rows (termios, pty pair, controlling
terminal, EIO) must PASS; the three signal rows are PENDING until S44 lands. The service's second
invocation (`--mutation-termios`) must turn the termios row red. No product code changes and no SSH:
the probe does not depend on the ssh-hostkeys/tmpfs ordering.

Build separately when iterating (each step has its own cap):
  sh tests/conformance/terminal/build.sh
  cargo xtask initfs --arch x86_64 --profile release
  target/debug/raven-b70-overlay build/x86_64/initfs.img build/conformance/terminal/initfs.img \
      /bin/terminal-conformance=$PWD/build/conformance/terminal/terminal-conformance:0755 \
      /etc/services.d/terminal-conformance.toml=$PWD/tests/conformance/terminal/terminal-conformance.service.toml:0644
  cargo xtask image --arch x86_64 --profile release --initfs build/conformance/terminal/initfs.img \
      --cmdline "console=ttyS0 test-exit=1" --output build/qtest/s175-terminal.img
  python3 tests/conformance/terminal/run.py --no-build
"""

from __future__ import annotations

import os
import shutil
import signal
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
IMAGE = Path(os.environ.get("S175_TERM_IMAGE", ROOT / "build/qtest/s175-terminal.img"))
OUT = Path(os.environ.get("S175_TERM_OUT", ROOT / "build/conformance/terminal/run"))
OVMF_CODE = Path("/usr/share/OVMF/OVMF_CODE_4M.fd")
OVMF_VARS = Path("/usr/share/OVMF/OVMF_VARS_4M.fd")
SERVICE = ROOT / "tests/conformance/terminal/terminal-conformance.service.toml"
PROBE = ROOT / "build/conformance/terminal/terminal-conformance"
GATE_ROWS = ("termios", "pty-both-ways", "ctty", "eio")
PENDING_ROWS = ("sigwinch", "sighup", "isig")
MUTATION_MARKER = "S175-TERM-MUTATION-BEGIN"


def step(label: str, argv: list[str], timeout: int) -> None:
    print(f"S175-TERM-RUN {label}", flush=True)
    result = subprocess.run(argv, cwd=str(ROOT), capture_output=True, timeout=timeout)
    (OUT / f"{label}.log").write_bytes(result.stdout + result.stderr)
    if result.returncode != 0:
        raise SystemExit(f"S175-TERM-RUN {label} failed rc={result.returncode}; see {OUT}/{label}.log")


def build_image() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    step("fixture-build", ["sh", "tests/conformance/terminal/build.sh"], 240)
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
            f"/bin/terminal-conformance={PROBE}:0755",
            f"/etc/services.d/terminal-conformance.toml={SERVICE}:0644",
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
    ]
    qemu = subprocess.Popen(argv, cwd=str(ROOT), stdin=subprocess.DEVNULL,
                            stdout=subprocess.DEVNULL,
                            stderr=open(OUT / "qemu.stderr", "wb"), start_new_session=True)
    failures: list[str] = []
    try:
        deadline = time.monotonic() + 180
        text = ""
        while time.monotonic() < deadline:
            if qemu.poll() is not None:
                break
            text = SERIAL.read_text(encoding="utf-8", errors="replace")
            if MUTATION_MARKER in text and text.count("S175-TERM done failures=") >= 2:
                break
            time.sleep(0.2)
        (OUT / "serial-copy.log").write_text(text, encoding="utf-8")

        if MUTATION_MARKER in text:
            normal, mutation = text.split(MUTATION_MARKER, 1)
        else:
            normal, mutation = text, ""
        normal_rows = rows(normal)
        mutation_rows = rows(mutation)
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
        if mutation_rows.get("termios") != "FAIL":
            failures.append(f"mutation:termios={mutation_rows.get('termios')}")
        if "S175-TERM done failures=" not in mutation:
            failures.append("mutation:incomplete-output")

        print(normal, end="", flush=True)
        print(mutation, end="", flush=True)
        print(f"S175-TERM-RUN rows_normal={normal_rows} rows_mutation={mutation_rows}", flush=True)
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
