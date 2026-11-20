#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K194 bounded bidirectional-network proof: bring up the tap/bridge helper, boot
# the release image on the tap, prove host->guest and guest->host ping, guest
# internet and DNS, then remove every object and check the cleanup.
#
# The helper needs root; this test uses passwordless sudo and refuses cleanly
# (exit 3) when it is unavailable. Outer acceptance timeout: 300 s.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
cd "$root"

image=${RAVEN_QEMU_IMAGE:-build/x86_64/ravenbsd-x86_64.img}
[ -f "$image" ] || {
    echo "qemu-net-selftest: missing image $image" >&2
    exit 3
}

code=${RAVEN_OVMF_CODE:-/usr/share/OVMF/OVMF_CODE_4M.fd}
[ -f "$code" ] || code=/usr/share/OVMF/OVMF_CODE.fd
vars_src=${RAVEN_OVMF_VARS:-/usr/share/OVMF/OVMF_VARS_4M.fd}
[ -f "$vars_src" ] || vars_src=/usr/share/OVMF/OVMF_VARS.fd
if [ ! -f "$code" ] || [ ! -f "$vars_src" ]; then
    echo "qemu-net-selftest: OVMF firmware not found" >&2
    exit 3
fi

if [ "$(id -u)" -eq 0 ]; then
    SUDO=
else
    if ! sudo -n true 2>/dev/null; then
        echo "qemu-net-selftest: root required (passwordless sudo unavailable); refusing" >&2
        exit 3
    fi
    SUDO="sudo -n"
fi

out=build/qemu-net
mkdir -p "$out"
up_log=$out/up.txt
down_log=$out/down.txt
serial_log=$out/serial.log
vars=$(mktemp "${TMPDIR:-/tmp}/raven-baker-ovmf.XXXXXX")
cp "$vars_src" "$vars"

tap=ravendemo0
bridge=ravenbr0
cleanup() {
    pkill -f "qemu-system-x86_64.*ifname=$tap" 2>/dev/null || true
    $SUDO sh scripts/qemu-net.sh down >"$down_log" 2>&1 || true
    rm -f "$vars"
}
trap cleanup EXIT HUP INT TERM

echo "qemu-net-selftest: up"
$SUDO sh scripts/qemu-net.sh up | tee "$up_log"
tap=$(sed -n 's/^tap=\([^ ]*\).*/\1/p' "$up_log" | head -n 1)
bridge=$(sed -n 's/.* bridge=\([^ ]*\).*/\1/p' "$up_log" | head -n 1)
[ -n "$tap" ] || { echo "qemu-net-selftest: up did not print a contract" >&2; exit 3; }
[ -n "$bridge" ] || bridge=ravenbr0

echo "qemu-net-selftest: boot guest on $tap"
timeout 180 python3 - "$image" "$tap" "$serial_log" "$code" "$vars" <<'PY'
import os
import re
import select
import subprocess
import sys
import tempfile
import time

image, tap, log_path, code, vars_path = sys.argv[1:6]
log = open(log_path, "w", encoding="utf-8", buffering=1)

args = [
    "qemu-system-x86_64",
    "-machine", "q35",
    "-accel", "tcg",
    "-m", "512M",
    "-smp", "1",
    "-serial", "pty",
    "-display", "none",
    "-monitor", "none",
    "-no-reboot",
    "-drive", f"if=pflash,format=raw,readonly=on,file={code}",
    "-drive", f"if=pflash,format=raw,file={vars_path}",
    "-drive", f"format=raw,file={image}",
    "-netdev", f"tap,id=n0,ifname={tap},script=no,downscript=no",
    "-device", "virtio-net-pci,netdev=n0,disable-legacy=on",
]
proc = subprocess.Popen(args, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, bufsize=1)


def fail(message):
    log.write(f"FAIL: {message}\n")
    try:
        proc.terminate()
        proc.wait(timeout=5)
    except Exception:
        pass
    log.close()
    print(f"qemu-net-selftest: FAIL: {message}", file=sys.stderr)
    sys.exit(1)


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
    fail(f"cannot open {slave}: {error}")

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
    os.write(fd, value.encode())


def guest(command, pattern, timeout=30):
    global buffer
    buffer = ""
    send(command + "\n")
    if not read_until(r"raven# ", timeout):
        fail(f"no prompt after `{command}`")
    if not re.search(pattern, buffer, re.IGNORECASE):
        fail(f"`{command}` did not match {pattern}")
    print(f"PASS guest: {command}")


if not read_until(r"login:", 150):
    fail("no login prompt")
send("root\n")
if not read_until(r"password:", 30):
    fail("no Password prompt")
send("raven\n")
if not read_until(r"raven# ", 60):
    fail("no shell prompt")

host_ping = subprocess.run(
    ["ping", "-c", "3", "-W", "1", "10.0.2.15"],
    stdout=subprocess.PIPE,
    stderr=subprocess.STDOUT,
    text=True,
    timeout=15,
)
log.write("\n--- host ping 10.0.2.15 ---\n" + host_ping.stdout)
if host_ping.returncode != 0 or "0% packet loss" not in host_ping.stdout:
    fail("host ping 10.0.2.15 failed")
print("PASS host: ping 10.0.2.15")

guest("ping -c 3 10.0.2.2", r"0% packet loss")
guest("ping -c 3 1.1.1.1", r"0% packet loss")
# The guest ping binary is numeric-only and the release image's static
# /etc/resolv.conf has no nameserver line (resolvd waits for a DHCP lease); the
# guest can reach the DNS address, and the host dig below proves the DNS server.
guest("ping -c 3 10.0.2.3", r"0% packet loss")
send("exit 0\n")
read_until(r"init exited \(0\)", 30)
try:
    proc.wait(timeout=10)
except subprocess.TimeoutExpired:
    proc.terminate()
    proc.wait(timeout=5)
log.close()
print("PASS guest: host and internet; DNS server reachable")
PY
status=$?
if [ "$status" -ne 0 ]; then
    echo "qemu-net-selftest: guest proof failed (see $serial_log)" >&2
    tail -n 40 "$serial_log" >&2 || true
    exit 1
fi

command -v dig >/dev/null 2>&1 || {
    echo "qemu-net-selftest: dig is required for the DNS proof" >&2
    exit 3
}
dns_log=$out/dns.txt
timeout 10 dig +short @10.0.2.3 example.com >"$dns_log" 2>&1 || true
if [ ! -s "$dns_log" ] || ! grep -Eq '^[0-9a-fA-F:.]+$' "$dns_log"; then
    echo "qemu-net-selftest: dnsmasq did not resolve example.com (see $dns_log)" >&2
    exit 1
fi
echo "qemu-net-selftest: PASS dns: dig @10.0.2.3 example.com -> $(head -n 1 "$dns_log")"

echo "qemu-net-selftest: down"
$SUDO sh scripts/qemu-net.sh down | tee "$down_log"
if ip link show "$bridge" >/dev/null 2>&1; then
    echo "qemu-net-selftest: bridge $bridge still exists" >&2
    exit 1
fi
if ip link show "$tap" >/dev/null 2>&1; then
    echo "qemu-net-selftest: tap $tap still exists" >&2
    exit 1
fi
if [ -s /run/raven-qemu-net-dnsmasq.pid ]; then
    echo "qemu-net-selftest: dnsmasq pid file still exists" >&2
    exit 1
fi

echo "qemu-net-selftest: PASS (host/guest ping, internet, DNS server reachable, cleanup)"
echo "qemu-net-selftest: K203: guest name resolution is not claimed (numeric-only raven-ping, no nameserver; see docs/qemu-network.md)"
