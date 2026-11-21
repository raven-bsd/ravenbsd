#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K371 K194 tap harness. Derived from the first-party
# tests/scripts/qemu-net-selftest.sh serial loop, with a looped nonblocking serial
# write so a full pty cannot truncate the longer guest ping command. It boots the
# release image, logs in, proves host->guest and guest->host/internet ping, and
# records the raw serial log. The tap/bridge lifecycle is owned by default.sh.

import os
import re
import select
import subprocess
import sys
import time

if len(sys.argv) != 6:
    print("usage: tap.py <image> <tap> <serial-log> <ovmf-code> <ovmf-vars>", file=sys.stderr)
    sys.exit(2)

image, tap, log_path, code, vars_path = sys.argv[1:6]
log = open(log_path, "w", encoding="utf-8", buffering=1)
proc = None
fd = None


def stop():
    if proc is not None:
        try:
            proc.terminate()
            proc.wait(timeout=5)
        except Exception:
            pass
    if fd is not None:
        try:
            os.close(fd)
        except OSError:
            pass
    log.close()


def fail(message):
    log.write("FAIL: %s\n" % message)
    print("net-postpf tap: FAIL: %s" % message, file=sys.stderr, flush=True)
    stop()
    sys.exit(1)


args = [
    "qemu-system-x86_64",
    "-machine",
    "q35",
    "-accel",
    "tcg",
    "-m",
    "512M",
    "-smp",
    "1",
    "-serial",
    "pty",
    "-display",
    "none",
    "-monitor",
    "none",
    "-no-reboot",
    "-drive",
    "if=pflash,format=raw,readonly=on,file=%s" % code,
    "-drive",
    "if=pflash,format=raw,file=%s" % vars_path,
    "-drive",
    "format=raw,file=%s" % image,
    "-netdev",
    "tap,id=n0,ifname=%s,script=no,downscript=no" % tap,
    "-device",
    "virtio-net-pci,netdev=n0,disable-legacy=on",
]
try:
    proc = subprocess.Popen(args, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, bufsize=1)
except OSError as error:
    fail("cannot start QEMU: %s" % error)

slave = None
deadline = time.time() + 30
while time.time() < deadline:
    line = proc.stdout.readline()
    if not line:
        break
    log.write(line)
    match = re.search(r"char device redirected to (/dev/pts/\d+)", line)
    if match:
        slave = match.group(1)
        break
if not slave:
    fail("QEMU did not create a serial pty")
try:
    fd = os.open(slave, os.O_RDWR | os.O_NOCTTY | os.O_NONBLOCK)
except OSError as error:
    fail("cannot open %s: %s" % (slave, error))

buffer = ""


def read_until(pattern, timeout):
    global buffer
    end = time.time() + timeout
    rx = re.compile(pattern, re.IGNORECASE)
    while time.time() < end:
        ready, _, _ = select.select([fd], [], [], 0.5)
        if ready:
            try:
                chunk = os.read(fd, 4096)
            except BlockingIOError:
                continue
            if not chunk:
                break
            text = chunk.decode("utf-8", "replace")
            buffer += text
            log.write(text)
        if rx.search(buffer):
            return True
    return False


def send(value):
    # Pace one byte at a time: the guest's 16550 input path can lose the tail of
    # a burst once the first byte has raised the RX interrupt. The qtest's own
    # step writer is similarly paced; a single os.write of the full ping command
    # reproduces the first-byte-only failure recorded in selftest-failure.log.
    for byte in value.encode():
        data = bytes([byte])
        while data:
            try:
                written = os.write(fd, data)
            except BlockingIOError:
                select.select([], [fd], [], 0.5)
                continue
            if written <= 0:
                fail("serial write made no progress")
            data = data[written:]
        time.sleep(0.005)


def guest(command, pattern, timeout):
    # Wait for the command's output, not for `raven# `: the brush line editor
    # redraws the prompt after every character, so a prompt wait returns before
    # the ping runs (the first-byte-only failure in selftest-failure.log). The
    # ping result line cannot appear in the command echo.
    global buffer
    buffer = ""
    send(command + "\n")
    if not read_until(pattern, timeout):
        fail("`%s` did not match %s" % (command, pattern))
    print("PASS guest: %s" % command, flush=True)


if not read_until(r"login:", 120):
    fail("no login prompt")
send("root\n")
if not read_until(r"password:", 30):
    fail("no password prompt")
send("raven\n")
if not read_until(r"raven# ", 60):
    fail("no shell prompt")

host_ping = subprocess.run(
    ["ping", "-c", "3", "-W", "1", "10.0.2.15"],
    stdout=subprocess.PIPE,
    stderr=subprocess.STDOUT,
    text=True,
    timeout=20,
)
log.write("\n--- host ping 10.0.2.15 ---\n" + host_ping.stdout)
if host_ping.returncode != 0 or "0% packet loss" not in host_ping.stdout:
    fail("host ping 10.0.2.15 failed")
print("PASS host: ping 10.0.2.15", flush=True)

guest("ping -c 3 10.0.2.2", r"0% packet loss", 60)
guest("ping -c 3 1.1.1.1", r"0% packet loss", 90)

send("exit 0\n")
read_until(r"init exited \(0\)", 30)
try:
    proc.wait(timeout=10)
except subprocess.TimeoutExpired:
    proc.terminate()
    proc.wait(timeout=5)
log.close()
print("PASS: default release tap host/guest/internet ping", flush=True)
