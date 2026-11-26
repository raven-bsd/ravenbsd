#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K193/K215: build the one-disk bootable UFS demo. `cargo xtask image --profile
# qemu-ufs --output` writes the two-disk preflight ESP; the fixed demo qcow2 and
# run.sh/README are emitted under build/<arch>/ravenbsd-ufs-demo. `--build-only`
# stops after the build; no QEMU is started.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)
cd "$root"

arch=x86_64
demo=build/$arch/ravenbsd-ufs-demo
preflight=build/$arch/qemu-ufs-preflight.img
qcow2=$demo/ravenbsd-ufs-demo.qcow2
build_only=0

usage() {
    cat <<'EOF'
usage:
  scripts/qemu-ufs.sh              build the preflight ESP + demo qcow2, then boot run.sh
  scripts/qemu-ufs.sh --build-only build only; print the qcow2 path; do not start QEMU
  scripts/qemu-ufs.sh --help
EOF
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --build-only) build_only=1; shift ;;
        -h|--help) usage; exit 0 ;;
        --*) break ;;
        *) break ;;
    esac
done

stamp=$demo/.build-id
head=$(git rev-parse HEAD)
clean=0
if git diff --quiet && git diff --cached --quiet; then
    clean=1
fi
if [ "$build_only" -eq 1 ] && [ "$clean" -eq 1 ] &&
    [ -f "$qcow2" ] && [ -f "$preflight" ] && [ -f "$stamp" ] &&
    [ "$(cat "$stamp")" = "$head" ]; then
    printf '%s\n' "$qcow2"
    exit 0
fi

sh scripts/space-guard.sh "${SPACE_MIN_GB:-15}"

# The bootable UFS root carries the same static disk tools as the S99 demo.
if [ "${QEMU_UFS_SKIP_TOOLS:-0}" != 1 ]; then
    sh scripts/ufs-demo.sh --prepare-tools
fi

mkdir -p "$demo"
cargo xtask image --arch "$arch" --profile qemu-ufs --output "$preflight"
[ -f "$qcow2" ] || {
    echo "qemu-ufs: missing demo qcow2: $qcow2" >&2
    exit 1
}
printf '%s\n' "$head" > "$stamp"

if [ "$build_only" -eq 1 ]; then
    printf '%s\n' "$qcow2"
    exit 0
fi

exec "$demo/run.sh" "$@"
