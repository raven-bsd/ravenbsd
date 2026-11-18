#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# S99 UFS QEMU demo. Builds host-formatted UFS1/UFS2 GPT disks, boots the
# ufs-demo profile, runs the first-boot write and the second-boot persistence
# checks, and preserves the QEMU transcripts. The normal release profile is
# not touched.
#
# RAVEN_UFS_DEMO_SIZE overrides the payload size (default 16m).

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
cd "$root"

out=build/fixtures/ufs-demo
size=${RAVEN_UFS_DEMO_SIZE:-16m}
epoch=${SOURCE_DATE_EPOCH:-0}

prepare_tools() {
    mkdir -p "$out/tools"
    RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld" \
        cargo build -p lsblk -p newfs -p gpart --target x86_64-unknown-none --release
    target_dir=${CARGO_TARGET_DIR:-target}
    for tool in lsblk newfs gpart; do
        cp "$target_dir/x86_64-unknown-none/release/$tool" "$out/tools/$tool"
    done
}

prepare_ufs2() {
    mkdir -p "$out"
    cargo run -q -p ufs-mkfs -- -O 2 -s "$size" -T "$epoch" "$out/ufs2.img"
    cargo run -q -p raven-k54-mkimage -- "$out/ufs2.img" "$out/disk.img"
}

prepare_ufs1() {
    mkdir -p "$out"
    cargo run -q -p ufs-mkfs -- -O 1 -s "$size" -T "$epoch" "$out/ufs1.img"
    cargo run -q -p raven-k54-mkimage -- "$out/ufs1.img" "$out/disk-ufs1.img"
}

case "${1:-}" in
    --prepare)
        prepare_tools
        prepare_ufs2
        prepare_ufs1
        exit 0
        ;;
    --prepare-ufs1)
        prepare_tools
        prepare_ufs1
        exit 0
        ;;
    --prepare-ufs2)
        prepare_tools
        prepare_ufs2
        exit 0
        ;;
    --prepare-tools)
        prepare_tools
        exit 0
        ;;
    "")
        ;;
    *)
        echo "usage: $0 [--prepare|--prepare-ufs1|--prepare-ufs2|--prepare-tools]" >&2
        exit 2
        ;;
esac

prepare_tools
prepare_ufs2
prepare_ufs1

cargo xtask qtest --case ufs-disk --arch x86_64
cargo xtask qtest --case ufs-disk-persist --arch x86_64
cargo xtask qtest --case ufs-disk-ufs1 --arch x86_64

for transcript in ufs-disk ufs-disk-persist ufs-disk-ufs1; do
    log="build/qtest/$transcript.log"
    if [ ! -f "$log" ]; then
        echo "ufs-demo: missing transcript $log" >&2
        exit 1
    fi
    if ! grep -q 's99-persist' "$log"; then
        echo "ufs-demo: $transcript did not read the persisted marker" >&2
        exit 1
    fi
done

echo "ufs-demo: disk        $out/disk.img"
echo "ufs-demo: ufs1 disk   $out/disk-ufs1.img"
echo "ufs-demo: transcript  build/qtest/ufs-disk.log"
echo "ufs-demo: persistence build/qtest/ufs-disk-persist.log"
echo "ufs-demo: ufs1        build/qtest/ufs-disk-ufs1.log"
