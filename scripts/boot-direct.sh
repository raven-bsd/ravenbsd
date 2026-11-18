#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# S89: direct QEMU boot probe for the release kernel.
#
# QEMU `-kernel` on x86 requires a multiboot or PVH ELF entry. The RavenBSD kernel is a plain
# ELF64 whose `_start` expects a loader-built `raven_bootinfo::BootInfo` in `rdi`, in long mode
# with paging on. Until the kernel grows a PVH note + 32-bit entry shim (or a first-stage loader
# loads it), this script reports the exact blocker and exits 2. It never fakes a boot or a timing.
#
# usage: sh scripts/boot-direct.sh [--arch x86_64] [--kernel PATH]
# Exit: 0 only when a real kernel banner/login is observed; 2 for the missing-entry blocker.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"

arch=x86_64
kernel="$root/build/x86_64/kernel.elf"
while [ $# -gt 0 ]; do
    case "$1" in
        --arch)
            arch=${2:?--arch needs a value}
            shift 2
            ;;
        --kernel)
            kernel=${2:?--kernel needs a value}
            shift 2
            ;;
        *)
            echo "boot-direct: unknown argument: $1" >&2
            exit 2
            ;;
    esac
done

if [ "$arch" != x86_64 ]; then
    echo "boot-direct: only the x86_64 PVH/multiboot path is in scope (got $arch)" >&2
    exit 2
fi
if [ ! -f "$kernel" ]; then
    echo "boot-direct: kernel not found: $kernel" >&2
    echo "  build the release image first: cargo xtask image --arch x86_64 --profile release" >&2
    exit 2
fi
if ! command -v qemu-system-x86_64 >/dev/null 2>&1; then
    echo "boot-direct: qemu-system-x86_64 not found" >&2
    exit 2
fi

log=$(mktemp "${TMPDIR:-/tmp}/raven-boot-direct.XXXXXX")
trap 'rm -f "$log"' EXIT INT TERM

echo "boot-direct: qemu-system-x86_64 $(qemu-system-x86_64 --version | head -1 | awk '{print $4}')"
echo "boot-direct: kernel=$kernel"
echo "boot-direct: direct QEMU command: -machine microvm -kernel ... -serial stdio"

start=$(date +%s%N)
set +e
qemu-system-x86_64 \
    -machine microvm \
    -m 512M \
    -smp 4 \
    -kernel "$kernel" \
    -append "console=ttyS0" \
    -serial stdio \
    -display none \
    -monitor none \
    -no-reboot \
    >"$log" 2>&1
status=$?
set -e
end=$(date +%s%N)
elapsed_ms=$(( (end - start) / 1000000 ))

if grep -q "RavenBSD kernel" "$log"; then
    # A future PVH/multiboot entry made it; report the observed banner and do not invent login.
    echo "boot-direct: kernel banner observed in ${elapsed_ms} ms"
    exit 0
fi

echo "boot-direct: QEMU exit=$status in ${elapsed_ms} ms"
sed -n '1,5p' "$log" | sed 's/^/boot-direct: /'
if grep -q "PVH ELF Note" "$log"; then
    echo "boot-direct: BLOCKED: the kernel ELF has no PVH note (XEN_ELFNOTE_PHYS32_ENTRY)" >&2
    echo "  required: a 32-bit PVH entry that builds raven_bootinfo::BootInfo and enters" >&2
    echo "  kernel _start in long mode with paging on, or a first-stage multiboot/PVH loader." >&2
    echo "  owner: Stuart (kernel entry/S83 lane); see docs/qemu-direct-boot.md." >&2
    exit 2
fi
echo "boot-direct: direct boot did not reach the kernel banner" >&2
exit 2
