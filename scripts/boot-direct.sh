#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# S89: build and run the first-stage direct-boot loader.
#
# The loader is a PVH ELF (`boot/direct`) that QEMU starts with `-kernel`. It consumes one
# `-initrd` module made of a small header, the Raven kernel ELF and the initfs, builds the
# kernel's `BootInfo`/page tables and jumps to the existing `_start`. The UEFI path is untouched.
#
# Usage:
#   sh scripts/boot-direct.sh                       # probe: QEMU -> kernel banner (microvm)
#   sh scripts/boot-direct.sh --mode full           # q35 + virtio-net, release login
#   sh scripts/boot-direct.sh --tcg                 # force TCG instead of KVM
#   sh scripts/boot-direct.sh --rebuild             # rebuild kernel and initfs first
#
# Exit: 0 when the requested target is met; 2 when the loader/kernel fails or the target is
# missed (the output always names the exact blocker); 3 for a usage error.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"

arch=x86_64
machine=
machine_set=0
profile=release
mode=probe
runs=3
smp=1
kvm=auto
rebuild=0
timeout_secs=120
cmdline='console=ttyS0 test-exit=1'
kernel="$root/build/x86_64/kernel.elf"
initfs="$root/build/x86_64/initfs.img"

usage() {
    sed -n 's/^# \{0,1\}//p' "$0" | sed -n '/^Usage:/,/^Exit:/p'
}

while [ $# -gt 0 ]; do
    case "$1" in
        --arch)
            arch=${2:?--arch needs a value}
            shift 2
            ;;
        --machine)
            machine=${2:?--machine needs a value}
            machine_set=1
            shift 2
            ;;
        --profile)
            profile=${2:?--profile needs a value}
            shift 2
            ;;
        --mode)
            mode=${2:?--mode needs a value}
            shift 2
            ;;
        --runs)
            runs=${2:?--runs needs a value}
            shift 2
            ;;
        --smp)
            smp=${2:?--smp needs a value}
            shift 2
            ;;
        --timeout)
            timeout_secs=${2:?--timeout needs a value}
            shift 2
            ;;
        --kernel)
            kernel=${2:?--kernel needs a value}
            shift 2
            ;;
        --initfs)
            initfs=${2:?--initfs needs a value}
            shift 2
            ;;
        --kvm)
            kvm=1
            shift
            ;;
        --tcg)
            kvm=0
            shift
            ;;
        --rebuild)
            rebuild=1
            shift
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        *)
            echo "boot-direct: unknown argument: $1" >&2
            usage >&2
            exit 3
            ;;
    esac
done

if [ "$arch" != x86_64 ]; then
    echo "boot-direct: only the x86_64 PVH path is implemented (got $arch)" >&2
    exit 3
fi
case "$mode" in
    probe|full) ;;
    *)
        echo "boot-direct: --mode must be probe or full" >&2
        exit 3
        ;;
esac
if [ -z "$machine" ]; then
    if [ "$mode" = full ]; then
        machine=q35
    else
        machine=microvm
    fi
else
    machine_set=1
fi
if [ "$mode" = full ] && [ "$machine" != q35 ]; then
    echo "boot-direct: full mode needs --machine q35 (PCI virtio-net)" >&2
    exit 3
fi
case "$machine" in
    microvm|q35) ;;
    *)
        echo "boot-direct: --machine must be microvm or q35" >&2
        exit 3
        ;;
esac
if [ "$kvm" = auto ]; then
    if [ -r /dev/kvm ] && [ -w /dev/kvm ]; then
        kvm=1
    else
        kvm=0
    fi
fi

if [ ! -f "$kernel" ] || [ "$rebuild" = 1 ]; then
    cargo xtask build kernel --arch "$arch" --release
fi
if [ ! -f "$initfs" ] || [ "$rebuild" = 1 ]; then
    cargo xtask initfs --arch "$arch" --profile "$profile"
fi
if [ ! -f "$kernel" ]; then
    echo "boot-direct: kernel not found: $kernel" >&2
    exit 2
fi
if [ ! -f "$initfs" ]; then
    echo "boot-direct: initfs not found: $initfs" >&2
    exit 2
fi

loader="$root/build/x86_64/raven-direct.elf"
loader_ld="$root/boot/direct/loader.ld"
echo "boot-direct: building the PVH loader"
RUSTFLAGS="-C relocation-model=static -C link-arg=-T$loader_ld -C link-arg=--build-id=none" \
    cargo build -p raven-direct --bin raven-direct --release \
    --target x86_64-unknown-none --target-dir "$root/target" --features loader
cp "$root/target/x86_64-unknown-none/release/raven-direct" "$loader"

payload="$root/build/x86_64/raven-direct-payload.bin"
python3 - "$kernel" "$initfs" "$payload" <<'PY'
import struct
import sys

kernel = open(sys.argv[1], "rb").read()
initfs = open(sys.argv[2], "rb").read()
# The kernel slice includes zero padding so the initfs starts on a 4 KiB boundary inside the
# PVH module. The kernel uses the page-aligned initfs range as a user mapping, and the parser
# ignores trailing ELF bytes.
padded_kernel = ((len(kernel) + 16 + 4095) // 4096) * 4096 - 16
if padded_kernel < len(kernel):
    raise SystemExit("boot-direct: kernel padding underflow")
with open(sys.argv[3], "wb") as output:
    output.write(b"RVDIRECT")
    output.write(struct.pack("<Q", padded_kernel))
    output.write(kernel)
    output.write(b"\0" * (padded_kernel - len(kernel)))
    output.write(initfs)
PY

qemu=qemu-system-x86_64
if ! command -v "$qemu" >/dev/null 2>&1; then
    echo "boot-direct: qemu-system-x86_64 not found" >&2
    exit 2
fi
qemu_version=$("$qemu" --version | head -1)
kvm_state=absent
if [ -r /dev/kvm ] && [ -w /dev/kvm ]; then
    kvm_state=present
fi
echo "boot-direct: $qemu_version"
echo "boot-direct: arch=$arch machine=$machine smp=$smp mode=$mode accel=$([ "$kvm" = 1 ] && echo kvm || echo tcg) kvm-device=$kvm_state"
echo "boot-direct: loader=$loader"
echo "boot-direct: payload=$payload ($(wc -c < "$payload") bytes)"

result=0
python3 - "$qemu" "$machine" "$loader" "$payload" "$cmdline" "$mode" "$kvm" "$timeout_secs" "$runs" "$smp" <<'PY' || result=$?
import os
import select
import statistics
import subprocess
import sys
import time

qemu, machine, loader, payload, cmdline, mode, kvm, timeout_text, runs_text, smp = sys.argv[1:]
timeout_secs = float(timeout_text)
runs = int(runs_text)

args = [qemu, "-machine", machine]
if kvm == "1":
    args.extend(["-enable-kvm", "-cpu", "host"])
args.extend([
    "-m", "512M",
    "-smp", smp,
    "-serial", "stdio",
    "-display", "none",
    "-monitor", "none",
    "-no-reboot",
    "-kernel", loader,
    "-initrd", payload,
    "-append", cmdline,
    "-device", "isa-debug-exit,iobase=0xf4,iosize=0x04",
])
if mode == "full":
    args.extend([
        "-nic", "none",
        "-netdev", "user,id=n0",
        "-device", "virtio-net-pci,netdev=n0,disable-legacy=on",
    ])


def run_once(number):
    start = time.perf_counter()
    process = subprocess.Popen(
        args,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )
    os.set_blocking(process.stdout.fileno(), False)
    buffer = b""
    loader_ms = None
    entry_ms = None
    kernel_ms = None
    login_ms = None
    lines = []
    deadline = start + timeout_secs
    while time.perf_counter() < deadline:
        ready, _, _ = select.select([process.stdout], [], [], 0.05)
        if ready:
            chunk = os.read(process.stdout.fileno(), 65536)
            if not chunk:
                break
            buffer += chunk
            while b"\n" in buffer:
                raw, buffer = buffer.split(b"\n", 1)
                line = raw.decode("utf-8", "replace").rstrip("\r")
                now = time.perf_counter() - start
                lines.append(line)
                if loader_ms is None and "direct: loader start" in line:
                    loader_ms = now
                if entry_ms is None and "direct: entering kernel" in line:
                    entry_ms = now
                if kernel_ms is None and "RavenBSD kernel" in line:
                    kernel_ms = now
                if login_ms is None and "login:" in line:
                    login_ms = now
        if login_ms is None and b"login:" in buffer:
            login_ms = time.perf_counter() - start
        if mode == "probe" and kernel_ms is not None:
            break
        if mode == "full" and login_ms is not None:
            break
        if process.poll() is not None:
            # Drain whatever the guest wrote before exiting.
            while True:
                ready, _, _ = select.select([process.stdout], [], [], 0)
                if not ready:
                    break
                chunk = os.read(process.stdout.fileno(), 65536)
                if not chunk:
                    break
                buffer += chunk
            break
    if process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
    status = process.returncode
    for raw in buffer.split(b"\n"):
        line = raw.decode("utf-8", "replace").rstrip("\r")
        if line:
            lines.append(line)
    print(
        "boot-direct: run %d loader=%.1f ms entry=%s kernel=%s login=%s exit=%s"
        % (
            number,
            (loader_ms or 0.0) * 1000.0,
            ("%.1f ms" % (entry_ms * 1000.0)) if entry_ms is not None else "not seen",
            ("%.1f ms" % (kernel_ms * 1000.0)) if kernel_ms is not None else "not seen",
            ("%.1f ms" % (login_ms * 1000.0)) if login_ms is not None else "not seen",
            status,
        )
    )
    return loader_ms, entry_ms, kernel_ms, login_ms, lines, status


def stats(values):
    values = [value for value in values if value is not None]
    if not values:
        return None, None
    ordered = sorted(values)
    median = statistics.median(ordered)
    p90 = ordered[min(len(ordered) - 1, int(round(0.9 * (len(ordered) - 1))))]
    return median, p90


def show(values, label):
    median, p90 = stats(values)
    if median is not None:
        print("boot-direct: %s median=%.1f ms p90=%.1f ms" % (label, median * 1000.0, p90 * 1000.0))
    return median, p90


results = [run_once(number) for number in range(1, runs + 1)]
loader_median, loader_p90 = show([row[0] for row in results], "loader-start")
entry_median, entry_p90 = show([row[1] for row in results], "loader-entry")
kernel_median, kernel_p90 = show([row[2] for row in results], "kernel-banner")
login_median, login_p90 = show([row[3] for row in results], "login")
if loader_median is not None and entry_median is not None:
    print("boot-direct: loader-work median=%.1f ms" % ((entry_median - loader_median) * 1000.0))

missing = [number for number, row in enumerate(results, 1) if row[2] is None]
if missing:
    print("boot-direct: no kernel banner in run(s): %s" % ", ".join(map(str, missing)))
    for number in missing:
        print("boot-direct: run %d tail:" % number)
        for line in results[number - 1][4][-25:]:
            print("boot-direct: | %s" % line)
    sys.exit(2)

if mode == "probe":
    if kernel_p90 < 0.2:
        print("boot-direct: target<200ms MET (p90 %.1f ms)" % (kernel_p90 * 1000.0))
        sys.exit(0)
    print(
        "boot-direct: target<200ms MISSED (p90 %.1f ms)" % (kernel_p90 * 1000.0)
    )
    if loader_median is not None and entry_median is not None and kernel_median is not None:
        print(
            "boot-direct: breakdown: firmware+option-rom=%.1f ms, loader-work=%.1f ms, "
            "kernel-init=%.1f ms"
            % (
                loader_median * 1000.0,
                (entry_median - loader_median) * 1000.0,
                (kernel_median - entry_median) * 1000.0,
            )
        )
    print(
        "boot-direct: remaining blocker: QEMU's PVH option ROM/firmware stage plus kernel early "
        "init; the loader itself reaches the kernel in one step"
    )
    sys.exit(2)

if login_median is None:
    print("boot-direct: login prompt not observed")
    for line in results[-1][4][-25:]:
        print("boot-direct: | %s" % line)
    sys.exit(2)
if login_p90 < 1.0:
    print("boot-direct: target<1000ms MET (login p90 %.1f ms)" % (login_p90 * 1000.0))
    sys.exit(0)
print(
    "boot-direct: target<1000ms MISSED (login p90 %.1f ms); remaining blocker: kernel early init "
    "and the S84/S95/S96 service work"
    % (login_p90 * 1000.0)
)
sys.exit(2)
PY
exit "$result"
