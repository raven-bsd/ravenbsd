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
#   sh scripts/boot-direct.sh --mode full --defaults # old q35 default-device set
#   sh scripts/boot-direct.sh --mode full --smp 2   # override the full-mode -smp 4
#   sh scripts/boot-direct.sh --mode full --exclusive # refuse a busy host; 10 runs
#   sh scripts/boot-direct.sh --exclusive --preflight-only # host state only, no build/boot
#   sh scripts/boot-direct.sh --exclusive --self-test # synthetic busy/idle preflight cases
#   sh scripts/boot-direct.sh --rebuild             # rebuild kernel and initfs first
#
# Exit: 0 when the requested target is met; 2 when the loader/kernel fails, the exclusive
# preflight refuses a busy host, or the target is missed (the output always names the exact
# blocker); 3 for a usage error. --exclusive defaults to 10 runs unless --runs is given.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"

arch=x86_64
machine=
profile=release
mode=probe
runs=3
runs_set=0
smp=
kvm=auto
nodefaults=1
rebuild=0
exclusive=0
preflight_only=0
self_test=0
out=${BOOT_DIRECT_OUT:-build/boot-direct}
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
            runs_set=1
            shift 2
            ;;
        --out)
            out=${2:?--out needs a value}
            shift 2
            ;;
        --exclusive)
            exclusive=1
            shift
            ;;
        --preflight-only)
            exclusive=1
            preflight_only=1
            shift
            ;;
        --self-test)
            exclusive=1
            self_test=1
            shift
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
        --defaults)
            nodefaults=0
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
if [ -z "$smp" ]; then
    if [ "$mode" = full ]; then
        smp=4
    else
        smp=1
    fi
fi
if [ "$kvm" = auto ]; then
    if [ -r /dev/kvm ] && [ -w /dev/kvm ]; then
        kvm=1
    else
        kvm=0
    fi
fi
if [ "$exclusive" = 1 ] && [ "$runs_set" = 0 ]; then
    runs=10
fi
case $runs in
    '' | *[!0-9]*) echo "boot-direct: invalid --runs value: $runs" >&2; exit 3 ;;
esac
[ "$runs" -ge 1 ] || { echo "boot-direct: --runs must be at least 1" >&2; exit 3; }

is_uint() {
    case $1 in
        '' | *[!0-9]*) return 1 ;;
        *) return 0 ;;
    esac
}

process_matches() {
    list=$1
    pattern=$2
    kind=$3
    if [ -n "$list" ]; then
        cat "$list"
    else
        # Match the executable name, not a shell command line that happens to mention QEMU:
        # a host-idle watcher or an editor running `ps | awk /qemu-system-/` is not a QEMU
        # overlap. The xtask qtest runner has comm "xtask" and the harness "raven-qtest".
        ps -eo pid=,comm=,args= 2>/dev/null |
            awk -v pattern="$pattern" -v kind="$kind" '
                $2 ~ pattern { print; next }
                kind == "qtest" && $2 == "xtask" && $0 ~ /(^|[[:space:]])qtest([[:space:]]|$)/ { print }
            ' || true
    fi
}

preflight_refuse() {
    printf 'boot-direct: exclusive REFUSED: %s\n' "$*" >&2
    exit 2
}

refuse_with_state() {
    reason=$1
    reason=$(printf '%s' "$reason" | tr '"' "'")
    loadavg=$(awk '{print $1, $2, $3}' /proc/loadavg)
    load1=${BOOT_DIRECT_LOAD1:-$(printf '%s\n' "$loadavg" | awk '{print $1}')}
    load5=$(printf '%s\n' "$loadavg" | awk '{print $2}')
    load15=$(printf '%s\n' "$loadavg" | awk '{print $3}')
    nproc=${BOOT_DIRECT_NPROC:-$(nproc)}
    max_load=${BOOT_DIRECT_MAX_LOAD:-$nproc}
    mem_avail=${BOOT_DIRECT_MEM_AVAIL_KB:-$(awk '/MemAvailable:/ { print $2 }' /proc/meminfo)}
    disk_avail=${BOOT_DIRECT_DISK_AVAIL_KB:-$(df -Pk "$root" | awk 'NR == 2 { print $4 }')}
    commit=$(git rev-parse --short HEAD 2>/dev/null || printf 'unknown')
    branch=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || printf 'unknown')
    dirty=0
    if [ -n "$(git status --porcelain 2>/dev/null)" ]; then
        dirty=1
    fi
    qemu_count=$(printf '%s\n' "${qemu_procs:-}" | grep -c . || true)
    qtest_count=$(printf '%s\n' "${qtest_procs:-}" | grep -c . || true)
    if [ "$kvm" = 1 ]; then accel=kvm; else accel=tcg; fi
    {
        printf '{\n'
        printf '  "commit": "%s",\n' "$commit"
        printf '  "branch": "%s",\n' "$branch"
        printf '  "dirty": %s,\n' "$dirty"
        printf '  "utc": "%s",\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
        printf '  "mode": "%s",\n' "$mode"
        printf '  "machine": "%s",\n' "$machine"
        printf '  "smp": %s,\n' "$smp"
        printf '  "accel": "%s",\n' "$accel"
        printf '  "qemu_version": "%s",\n' "$qemu_version"
        printf '  "nproc": %s,\n' "$nproc"
        printf '  "load1": %s,\n' "$load1"
        printf '  "load5": %s,\n' "$load5"
        printf '  "load15": %s,\n' "$load15"
        printf '  "max_load": %s,\n' "$max_load"
        printf '  "mem_available_kb": %s,\n' "$mem_avail"
        printf '  "disk_available_kb": %s,\n' "$disk_avail"
        printf '  "qemu_processes": %s,\n' "$qemu_count"
        printf '  "qtest_processes": %s,\n' "$qtest_count"
        printf '  "refused": true,\n'
        printf '  "reason": "%s",\n' "$reason"
        printf '  "sample_count": 0\n'
        printf '}\n'
    } > "$state"
    printf 'boot-direct: exclusive REFUSED: %s\n' "$reason" >&2
    printf 'boot-direct: host nproc=%s load1=%s load5=%s load15=%s max_load=%s mem_available_kb=%s disk_available_kb=%s\n' \
        "$nproc" "$load1" "$load5" "$load15" "$max_load" "$mem_avail" "$disk_avail" >&2
    printf 'boot-direct: host qemu_processes=%s qtest_processes=%s sample_count=0\n' \
        "$qemu_count" "$qtest_count" >&2
    printf 'boot-direct: preflight=%s\n' "$state" >&2
    exit 2
}

run_preflight() {
    [ "$exclusive" = 1 ] || return 0
    mkdir -p "$out"
    stamp=$(date -u +%Y%m%dT%H%M%SZ)
    state=$out/preflight-$stamp-$$.json
    qemu_bin=qemu-system-x86_64
    if command -v "$qemu_bin" >/dev/null 2>&1; then
        qemu_version=$("$qemu_bin" --version | head -1)
    else
        preflight_refuse "qemu-system-x86_64 not found"
    fi

    qemu_procs=$(process_matches "${BOOT_DIRECT_QEMU_LIST:-}" 'qemu-system-' qemu)
    if [ -n "$qemu_procs" ]; then
        refuse_with_state "busy: QEMU process: $(printf '%s\n' "$qemu_procs" | head -n 1)"
    fi
    qtest_procs=$(process_matches "${BOOT_DIRECT_QTEST_LIST:-}" 'xtask.*qtest|raven-qtest' qtest)
    if [ -n "$qtest_procs" ]; then
        refuse_with_state "busy: qtest process: $(printf '%s\n' "$qtest_procs" | head -n 1)"
    fi

    loadavg=$(awk '{print $1, $2, $3}' /proc/loadavg)
    load1=${BOOT_DIRECT_LOAD1:-$(printf '%s\n' "$loadavg" | awk '{print $1}')}
    load5=$(printf '%s\n' "$loadavg" | awk '{print $2}')
    load15=$(printf '%s\n' "$loadavg" | awk '{print $3}')
    nproc=${BOOT_DIRECT_NPROC:-$(nproc)}
    is_uint "$nproc" && [ "$nproc" -gt 0 ] || preflight_refuse "invalid CPU count: $nproc"
    max_load=${BOOT_DIRECT_MAX_LOAD:-$nproc}
    is_uint "$max_load" && [ "$max_load" -gt 0 ] || preflight_refuse "invalid max load: $max_load"
    if awk -v l="$load1" -v m="$max_load" 'BEGIN { exit !(l > m) }'; then
        refuse_with_state "busy: load average $load1 exceeds $max_load (nproc $nproc)"
    fi
    mem_avail=${BOOT_DIRECT_MEM_AVAIL_KB:-$(awk '/MemAvailable:/ { print $2 }' /proc/meminfo)}
    is_uint "$mem_avail" || preflight_refuse "cannot read available memory"
    disk_avail=${BOOT_DIRECT_DISK_AVAIL_KB:-$(df -Pk "$root" | awk 'NR == 2 { print $4 }')}
    is_uint "$disk_avail" || preflight_refuse "cannot read free disk space"
    commit=$(git rev-parse --short HEAD 2>/dev/null || printf 'unknown')
    branch=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || printf 'unknown')
    dirty=0
    if [ -n "$(git status --porcelain 2>/dev/null)" ]; then
        dirty=1
    fi
    qemu_count=$(printf '%s\n' "$qemu_procs" | grep -c . || true)
    qtest_count=$(printf '%s\n' "$qtest_procs" | grep -c . || true)
    samples=$runs
    if [ "$preflight_only" = 1 ]; then
        samples=0
    fi
    if [ "$kvm" = 1 ]; then accel=kvm; else accel=tcg; fi

    {
        printf '{\n'
        printf '  "commit": "%s",\n' "$commit"
        printf '  "branch": "%s",\n' "$branch"
        printf '  "dirty": %s,\n' "$dirty"
        printf '  "utc": "%s",\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
        printf '  "mode": "%s",\n' "$mode"
        printf '  "machine": "%s",\n' "$machine"
        printf '  "smp": %s,\n' "$smp"
        printf '  "accel": "%s",\n' "$accel"
        printf '  "qemu_version": "%s",\n' "$qemu_version"
        printf '  "nproc": %s,\n' "$nproc"
        printf '  "load1": %s,\n' "$load1"
        printf '  "load5": %s,\n' "$load5"
        printf '  "load15": %s,\n' "$load15"
        printf '  "max_load": %s,\n' "$max_load"
        printf '  "mem_available_kb": %s,\n' "$mem_avail"
        printf '  "disk_available_kb": %s,\n' "$disk_avail"
        printf '  "qemu_processes": %s,\n' "$qemu_count"
        printf '  "qtest_processes": %s,\n' "$qtest_count"
        printf '  "refused": false,\n'
        printf '  "sample_count": %s\n' "$samples"
        printf '}\n'
    } > "$state"

    printf 'boot-direct: exclusive READY\n'
    printf 'boot-direct: host commit=%s branch=%s dirty=%s utc=%s\n' "$commit" "$branch" "$dirty" "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    printf 'boot-direct: host nproc=%s load1=%s load5=%s load15=%s max_load=%s mem_available_kb=%s disk_available_kb=%s\n' \
        "$nproc" "$load1" "$load5" "$load15" "$max_load" "$mem_avail" "$disk_avail"
    printf 'boot-direct: host qemu_processes=%s qtest_processes=%s accel=%s smp=%s runs=%s\n' \
        "$qemu_count" "$qtest_count" "$accel" "$smp" "$samples"
    printf 'boot-direct: preflight=%s\n' "$state"
}

self_test() {
    tmp=$(mktemp -d "${TMPDIR:-/tmp}/boot-direct-selftest.XXXXXX")
    trap 'rm -rf "$tmp"' EXIT HUP INT TERM
    : > "$tmp/empty"
    printf 'qemu-system-x86_64 -machine q35 -m 512M\n' > "$tmp/qemu"
    printf 'cargo xtask qtest --case release-login --arch x86_64\n' > "$tmp/qtest"

    if BOOT_DIRECT_QEMU_LIST="$tmp/qemu" BOOT_DIRECT_QTEST_LIST="$tmp/empty" \
        BOOT_DIRECT_LOAD1=0 BOOT_DIRECT_NPROC=4 \
        sh "$0" --exclusive --preflight-only --mode probe --out "$tmp/out-qemu" \
        >"$tmp/out-qemu.log" 2>&1; then
        echo "boot-direct: self-test FAIL: busy QEMU accepted" >&2
        exit 1
    fi
    grep -q 'REFUSED: busy: QEMU process' "$tmp/out-qemu.log" || {
        cat "$tmp/out-qemu.log" >&2
        echo "boot-direct: self-test FAIL: busy QEMU reason missing" >&2
        exit 1
    }

    if BOOT_DIRECT_QEMU_LIST="$tmp/empty" BOOT_DIRECT_QTEST_LIST="$tmp/qtest" \
        BOOT_DIRECT_LOAD1=0 BOOT_DIRECT_NPROC=4 \
        sh "$0" --exclusive --preflight-only --mode probe --out "$tmp/out-qtest" \
        >"$tmp/out-qtest.log" 2>&1; then
        echo "boot-direct: self-test FAIL: busy qtest accepted" >&2
        exit 1
    fi
    grep -q 'REFUSED: busy: qtest process' "$tmp/out-qtest.log" || {
        cat "$tmp/out-qtest.log" >&2
        echo "boot-direct: self-test FAIL: busy qtest reason missing" >&2
        exit 1
    }

    if BOOT_DIRECT_QEMU_LIST="$tmp/empty" BOOT_DIRECT_QTEST_LIST="$tmp/empty" \
        BOOT_DIRECT_LOAD1=99 BOOT_DIRECT_NPROC=4 \
        sh "$0" --exclusive --preflight-only --mode probe --out "$tmp/out-load" \
        >"$tmp/out-load.log" 2>&1; then
        echo "boot-direct: self-test FAIL: high load accepted" >&2
        exit 1
    fi
    grep -q 'REFUSED: busy: load average' "$tmp/out-load.log" || {
        cat "$tmp/out-load.log" >&2
        echo "boot-direct: self-test FAIL: high-load reason missing" >&2
        exit 1
    }

    if ! BOOT_DIRECT_QEMU_LIST="$tmp/empty" BOOT_DIRECT_QTEST_LIST="$tmp/empty" \
        BOOT_DIRECT_LOAD1=0 BOOT_DIRECT_NPROC=4 BOOT_DIRECT_MEM_AVAIL_KB=4194304 \
        BOOT_DIRECT_DISK_AVAIL_KB=10485760 \
        sh "$0" --exclusive --preflight-only --mode probe --out "$tmp/out-idle" \
        >"$tmp/out-idle.log" 2>&1; then
        cat "$tmp/out-idle.log" >&2
        echo "boot-direct: self-test FAIL: idle host refused" >&2
        exit 1
    fi
    if ! ls "$tmp/out-idle"/preflight-*.json >/dev/null 2>&1; then
        echo "boot-direct: self-test FAIL: idle preflight JSON missing" >&2
        exit 1
    fi
    grep -q 'exclusive READY' "$tmp/out-idle.log" || {
        cat "$tmp/out-idle.log" >&2
        echo "boot-direct: self-test FAIL: idle READY missing" >&2
        exit 1
    }
    echo "boot-direct: self-test PASS"
}

if [ "$self_test" = 1 ]; then
    self_test
    exit 0
fi
if [ "$preflight_only" = 1 ]; then
    run_preflight
    exit 0
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
echo "boot-direct: arch=$arch machine=$machine smp=$smp mode=$mode nodefaults=$nodefaults accel=$([ "$kvm" = 1 ] && echo kvm || echo tcg) kvm-device=$kvm_state"
echo "boot-direct: loader=$loader"
echo "boot-direct: payload=$payload ($(wc -c < "$payload") bytes)"

if [ "$exclusive" = 1 ]; then
    run_preflight
fi

result=0
python3 - "$qemu" "$machine" "$loader" "$payload" "$cmdline" "$mode" "$kvm" "$timeout_secs" "$runs" "$smp" "$nodefaults" <<'PY' || result=$?
import os
import select
import statistics
import subprocess
import sys
import time

qemu, machine, loader, payload, cmdline, mode, kvm, timeout_text, runs_text, smp, nodefaults = sys.argv[1:]
timeout_secs = float(timeout_text)
runs = int(runs_text)

args = [qemu, "-no-user-config", "-machine", machine]
if machine == "q35" and nodefaults == "1":
    args.append("-nodefaults")
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
    post_deadline = None
    while time.perf_counter() < deadline:
        now = time.perf_counter() - start
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
            if post_deadline is None:
                post_deadline = now + 10.0
            markers = ["svcd: getty up", "svcd: netd up", "netd: interface eth0"]
            ready = all(any(marker in line for line in lines) for marker in markers)
            if ready or now >= post_deadline:
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
    def delta(later, earlier):
        if later is None or earlier is None:
            return None
        return later - earlier

    def fmt(value):
        return ("%.1f ms" % (value * 1000.0)) if value is not None else "not seen"

    firmware = loader_ms
    loader_work = delta(entry_ms, loader_ms)
    kernel_init = delta(kernel_ms, entry_ms)
    services = delta(login_ms, kernel_ms)
    print(
        "boot-direct: run %d loader-start=%s entry=%s kernel=%s login=%s "
        "firmware=%s loader-work=%s kernel-init=%s services=%s exit=%s"
        % (
            number,
            fmt(loader_ms),
            fmt(entry_ms),
            fmt(kernel_ms),
            fmt(login_ms),
            fmt(firmware),
            fmt(loader_work),
            fmt(kernel_init),
            fmt(services),
            status,
        )
    )
    return loader_ms, entry_ms, kernel_ms, login_ms, lines, status


def stats(values):
    values = [value for value in values if value is not None]
    if not values:
        return None, None, None
    ordered = sorted(values)
    median = statistics.median(ordered)
    p90 = ordered[min(len(ordered) - 1, int(round(0.9 * (len(ordered) - 1))))]
    return median, p90, ordered[-1]


def show(values, label):
    median, p90, maximum = stats(values)
    if median is not None:
        print(
            "boot-direct: %s p50=%.1f ms p90=%.1f ms max=%.1f ms"
            % (label, median * 1000.0, p90 * 1000.0, maximum * 1000.0)
        )
    return median, p90, maximum


results = [run_once(number) for number in range(1, runs + 1)]
loader_median, loader_p90, _ = show([row[0] for row in results], "loader-start")
entry_median, entry_p90, _ = show([row[1] for row in results], "loader-entry")
kernel_median, kernel_p90, _ = show([row[2] for row in results], "kernel-banner")
login_median, login_p90, _ = show([row[3] for row in results], "login")
firmware_values = [row[0] for row in results]
loader_values = [
    (row[1] - row[0]) if row[1] is not None and row[0] is not None else None for row in results
]
kernel_values = [
    (row[2] - row[1]) if row[2] is not None and row[1] is not None else None for row in results
]
service_values = [
    (row[3] - row[2]) if row[3] is not None and row[2] is not None else None for row in results
]
show(firmware_values, "split-firmware")
show(loader_values, "split-loader")
show(kernel_values, "split-kernel")
show(service_values, "split-login")

if mode == "full":
    required = ["svcd: getty up", "svcd: netd up", "netd: interface eth0"]
    for number, row in enumerate(results, 1):
        lines = row[4]
        missing_service = [marker for marker in required if not any(marker in line for line in lines)]
        if missing_service:
            print(
                "boot-direct: run %d missing release service marker(s): %s"
                % (number, ", ".join(missing_service))
            )
            for line in lines[-25:]:
                print("boot-direct: | %s" % line)
            sys.exit(2)
    print("boot-direct: release getty, netd and the virtio network are up")

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
