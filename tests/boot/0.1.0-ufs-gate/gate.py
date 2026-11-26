#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""0.1.0 UFS root boot gate driver.

Boots the staged 0.1.0 UFS image under QEMU, asserts that the UFS2 root is
mounted read-write, that a marker command runs in the login session, and that
the guest powers off cleanly. Every failure names the stage that did not pass.
"""

import argparse
import hashlib
import json
import os
import re
import select
import shutil
import subprocess
import sys
import time

PROFILE = "0.1.0-ufs"
OVMF_CODE = "/usr/share/OVMF/OVMF_CODE_4M.fd"
OVMF_VARS = "/usr/share/OVMF/OVMF_VARS_4M.fd"

STAGES = ("image", "boot", "ufs-mount", "login", "session", "rw-mount", "shutdown")

ANSI_CSI = re.compile(r"\x1b\[[0-9;?]*[A-Za-z]")
ANSI_OSC = re.compile(r"\x1b\][^\x07\x1b]*(\x07|\x1b\\)")

# A boot transcript that carries any of these is red in the current stage. The
# UFS mount errors also make a read-only-root mutation fail at `ufs-mount`.
VIOLATION = re.compile(
    r"(?i)panicked|EXCEPTION|SegmentOutsideUserRange|no UFS root device|bad superblock"
)


class GateFailure(Exception):
    """A gate stage did not pass."""

    def __init__(self, stage, detail):
        super().__init__(f"{stage}: {detail}")
        self.stage = stage
        self.detail = detail


def strip_ansi(data):
    """Return serial bytes as plain text without terminal control sequences."""
    text = data.decode("utf-8", "replace")
    text = ANSI_OSC.sub("", text)
    text = ANSI_CSI.sub("", text)
    return text.replace("\r", "")


def violation(text):
    """Return the first forbidden serial line, or None."""
    for line in text.splitlines():
        if VIOLATION.search(line):
            return line.strip()
    return None


def qemu_args(arch, accel, esp, disk, vars_path, code_path, mutation):
    """Build the QEMU argv for the UFS gate.

    The UFS root is the first virtio-blk disk, so the bootstrap manifest
    (`FSD_UFS_DEVICE=block:0`, `FSD_UFS_PARTITION=1`) resolves to it; the ESP is
    attached as a plain drive, matching the in-tree ufs-server case.
    """
    if arch != "x86_64":
        raise ValueError(f"unsupported arch `{arch}`")
    if accel == "kvm":
        accel_args = ["-enable-kvm", "-cpu", "host"]
    elif accel == "tcg":
        accel_args = ["-cpu", "max"]
    else:
        raise ValueError(f"unsupported accelerator `{accel}`")
    if mutation not in ("none", "root-readonly"):
        raise ValueError(f"unsupported mutation `{mutation}`")
    readonly = ",readonly=on" if mutation == "root-readonly" else ""
    return [
        "qemu-system-x86_64",
        "-machine", "q35",
        "-m", "512M",
        "-smp", "4",
        *accel_args,
        "-serial", "stdio",
        "-display", "none",
        "-monitor", "none",
        "-no-reboot",
        "-drive", f"if=pflash,format=raw,readonly=on,file={code_path}",
        "-drive", f"if=pflash,format=raw,file={vars_path}",
        "-drive", f"if=none,id=ufs,format=raw{readonly},file={disk}",
        "-device", "virtio-blk-pci,drive=ufs,disable-legacy=on",
        "-drive", f"format=raw,file={esp}",
        "-netdev", "user,id=n0",
        "-device", "virtio-net-pci,netdev=n0,disable-legacy=on",
    ]


def sha256_file(path):
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def image_contains(path, marker):
    """Whether the raw root image contains the guest marker bytes."""
    with open(path, "rb") as handle:
        return marker in handle.read()


class Serial:
    """QEMU stdio serial driver with stage-tagged waits."""

    def __init__(self, process, total_timeout):
        self.process = process
        self.raw = bytearray()
        self.text = ""
        self.deadline = time.monotonic() + total_timeout

    def _drain(self):
        while True:
            ready, _, _ = select.select([self.process.stdout], [], [], 0.2)
            if not ready:
                return
            chunk = os.read(self.process.stdout.fileno(), 4096)
            if not chunk:
                return
            self.raw.extend(chunk)
            self.text = strip_ansi(bytes(self.raw))

    def expect(self, pattern, timeout, stage):
        regex = re.compile(pattern, re.MULTILINE)
        end = time.monotonic() + timeout
        while time.monotonic() < end:
            if regex.search(self.text):
                return
            if time.monotonic() > self.deadline:
                raise GateFailure(stage, "gate total timeout expired")
            self._drain()
            if regex.search(self.text):
                return
            hit = violation(self.text)
            if hit is not None:
                raise GateFailure(stage, f"forbidden serial line: {hit}")
            if self.process.poll() is not None:
                raise GateFailure(
                    stage,
                    f"QEMU exited with {self.process.returncode} before `{pattern}`",
                )
        raise GateFailure(stage, f"timeout {timeout}s waiting for `{pattern}`")

    def send(self, line):
        self.process.stdin.write((line + "\n").encode())
        self.process.stdin.flush()


def parse_args(argv):
    parser = argparse.ArgumentParser(prog="0.1.0-ufs-gate")
    parser.add_argument("--arch", required=True)
    parser.add_argument("--accel", required=True, choices=("kvm", "tcg"))
    parser.add_argument("--esp", required=True)
    parser.add_argument("--disk", required=True)
    parser.add_argument("--root", required=True, help="product checkout root")
    parser.add_argument("--out", required=True, help="prjmgmt evidence directory")
    parser.add_argument("--mutation", default="none", choices=("none", "root-readonly"))
    parser.add_argument("--total-timeout", type=int, default=300)
    return parser.parse_args(argv)


def artifact_suffix(args):
    if args.mutation == "none":
        return f"{args.arch}-{args.accel}"
    return f"{args.arch}-{args.accel}-{args.mutation}"


def run(args):
    if args.arch != "x86_64":
        print(f"s85m: unsupported arch `{args.arch}` (the 0.1.0 UFS profile targets x86_64)", file=sys.stderr)
        return 2
    code = os.environ.get("RAVEN_OVMF_CODE", OVMF_CODE)
    vars_src = os.environ.get("RAVEN_OVMF_VARS", OVMF_VARS)
    for path in (args.esp, args.disk, code, vars_src):
        if not os.path.isfile(path):
            print(f"s85m: missing input {path}", file=sys.stderr)
            return 2
    if args.accel == "kvm" and not os.access("/dev/kvm", os.R_OK | os.W_OK):
        print("s85m: /dev/kvm is not accessible", file=sys.stderr)
        return 2

    os.makedirs(args.out, exist_ok=True)
    suffix = artifact_suffix(args)
    serial_path = os.path.join(args.out, f"serial-{suffix}.log")
    summary_path = os.path.join(args.out, f"manifest-{suffix}.json")
    vars_path = os.path.join(args.out, f"ovmf-vars-{suffix}.fd")
    shutil.copyfile(vars_src, vars_path)

    argv = qemu_args(args.arch, args.accel, args.esp, args.disk, vars_path, code, args.mutation)
    print(f"s85m: arch={args.arch} accel={args.accel} mutation={args.mutation}")
    process = subprocess.Popen(
        argv,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        bufsize=0,
    )
    serial = Serial(process, args.total_timeout)
    started = time.monotonic()
    boot_to_login = None
    stage = "boot"
    failure = None
    exit_code = None
    try:
        serial.expect(r"^RavenBSD kernel 0\.1 \(x86_64\)", 240, "boot")
        stage = "ufs-mount"
        serial.expect(r"^svcd: root pivoted to UFS at /$", 180, "ufs-mount")
        stage = "login"
        serial.expect(r"(?mi)^login: ?$", 180, "login")
        serial.send("root")
        serial.expect(r"(?mi)^Password: ?$", 30, "login")
        serial.send("raven")
        stage = "session"
        serial.expect(r"(?m)^raven# $", 120, "session")
        boot_to_login = round(time.monotonic() - started, 1)
        serial.send("echo S85M-MARKER-OK")
        serial.expect(r"(?m)^S85M-MARKER-OK$", 30, "session")
        stage = "rw-mount"
        serial.send("/sbin/mount")
        serial.expect(r"(?m)^ufs: on /$", 30, "rw-mount")
        serial.send("echo S85M-RW-OK > /s85m-rw.txt && cat /s85m-rw.txt")
        serial.expect(r"(?m)^S85M-RW-OK$", 30, "rw-mount")
        stage = "shutdown"
        serial.send("/sbin/poweroff")
        serial.expect(r"(?m)^power: poweroff$", 120, "shutdown")
        try:
            exit_code = process.wait(timeout=60)
        except subprocess.TimeoutExpired:
            raise GateFailure("shutdown", "QEMU did not exit after `power: poweroff`")
        if exit_code != 0:
            raise GateFailure("shutdown", f"QEMU exited with {exit_code} after poweroff")
        if not image_contains(args.disk, b"S85M-RW-OK"):
            raise GateFailure("rw-mount", "marker file did not reach the UFS root image")
    except GateFailure as error:
        failure = error
        stage = error.stage
    finally:
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=10)
        if os.path.exists(vars_path):
            os.remove(vars_path)

    serial._drain()
    with open(serial_path, "wb") as handle:
        handle.write(bytes(serial.raw))

    summary = {
        "profile": PROFILE,
        "arch": args.arch,
        "accel": args.accel,
        "mutation": args.mutation,
        "result": "fail" if failure else "pass",
        "stage": stage if failure else None,
        "detail": failure.detail if failure else None,
        "boot_to_login_seconds": boot_to_login,
        "total_seconds": round(time.monotonic() - started, 1),
        "qemu_exit": exit_code,
        "images": {
            "esp": {
                "path": os.path.relpath(args.esp, args.root),
                "bytes": os.path.getsize(args.esp),
                "sha256": sha256_file(args.esp),
            },
            "root": {
                "path": os.path.relpath(args.disk, args.root),
                "bytes": os.path.getsize(args.disk),
                "sha256": sha256_file(args.disk),
            },
        },
        "serial": {
            "path": serial_path,
            "bytes": os.path.getsize(serial_path),
            "sha256": sha256_file(serial_path),
        },
    }
    with open(summary_path, "w", encoding="utf-8") as handle:
        json.dump(summary, handle, indent=2, sort_keys=True)
        handle.write("\n")

    if failure:
        print(f"s85m: FAIL stage={failure.stage} detail={failure.detail}")
        tail = serial.text.splitlines()[-20:]
        for line in tail:
            print(f"serial: {line}")
        if args.mutation != "none":
            print(f"mutation: {args.mutation} rejected at stage={failure.stage}")
        print(f"s85m: manifest {summary_path}")
        return 1
    print(
        f"s85m: PASS boot_to_login={boot_to_login}s "
        f"total={summary['total_seconds']}s stages={' '.join(STAGES)}"
    )
    print(f"s85m: manifest {summary_path}")
    return 0


def main(argv):
    args = parse_args(argv)
    return run(args)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
