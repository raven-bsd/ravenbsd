#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""K434 rev 190: aarch64 release with a virtio-net-device.

Boots the aarch64 release image on QEMU `virt` with a virtio-mmio NIC and asserts the row the
ticket needs: `svcd: netd up`, `svcd: sshd up`, and sshd's `Server listening on 0.0.0.0 port 22.`.
The test is red until the guest enumerates virtio-mmio and the driver publishes `nic:info`.

Usage:
  python3 tests/ssh/aarch64-net.py [--image build/qtest/release-aarch64-k434-r189.img]
"""

from __future__ import annotations

import argparse
import os
import shutil
import sys
from pathlib import Path

import pexpect

sys.dont_write_bytecode = True

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
sys.path.insert(0, str(ROOT / "tests/soak"))
import run as soak  # noqa: E402

DEFAULT_IMAGE = ROOT / "build/qtest/release-aarch64-k434-r189.img"
OUT = ROOT / "build/k434-aarch64-net"
MARKER = "DONE-42"
COMMAND = "cat /tmp/sshd.log 2>/dev/null; echo DONE-$((6*7))"


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image", type=Path, default=DEFAULT_IMAGE)
    parser.add_argument("--timeout", type=float, default=180.0)
    args = parser.parse_args(argv)
    image = args.image if args.image.is_absolute() else ROOT / args.image
    if not image.is_file():
        print(f"aarch64-net: missing image {image}", flush=True)
        return 2
    OUT.mkdir(parents=True, exist_ok=True)
    vars_source = os.environ.get("RAVEN_AAVMF_VARS", soak.DEFAULT_AAVMF_VARS)
    vars_path = OUT / ".vars.fd"
    shutil.copyfile(vars_source, vars_path)
    serial = OUT / "serial.log"
    command = soak.qemu_argv("aarch64", soak.MODES["tcg"], image, vars_path)
    command += ["-device", "virtio-net-device,netdev=n0", "-netdev", "user,id=n0"]
    result = 1
    child = pexpect.spawn(
        command[0], command[1:], encoding="utf-8", codec_errors="replace", timeout=args.timeout
    )
    with serial.open("w", encoding="utf-8") as log:
        child.logfile = log
        try:
            child.expect("login:", timeout=180)
            child.sendline("root")
            child.expect("Password:", timeout=30)
            child.sendline("raven")
            child.expect("# ", timeout=60)
            child.sendline(COMMAND)
            child.expect(MARKER, timeout=120)
            result = 0
        except (pexpect.EOF, pexpect.TIMEOUT) as error:
            print(f"aarch64-net: console wait {type(error).__name__}", file=sys.stderr)
            print((child.before or "")[-1200:], file=sys.stderr)
        finally:
            child.close(force=True)
    vars_path.unlink(missing_ok=True)
    text = serial.read_text(encoding="utf-8", errors="replace")
    netd = "svcd: netd up" in text
    sshd = "svcd: sshd up" in text
    listening = "Server listening on 0.0.0.0 port 22." in text
    nic = "netd: ready" in text
    print(
        f"aarch64-net: netd_up={int(netd)} netd_ready={int(nic)} sshd_up={int(sshd)} "
        f"listening={int(listening)} marker={int(result == 0)}"
    )
    for line in text.splitlines():
        if any(token in line for token in ("svcd: netd", "svcd: sshd", "netd: ready", "nic:info", "Server listening")):
            print("serial:", line.strip()[-200:])
    return 0 if result == 0 and netd and sshd and listening else 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
