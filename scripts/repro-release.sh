#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Host-only release-image reproducibility check.
#
# Builds the x86_64 `release` image and the Pi 4 `pi4-release` image twice each into separate
# output files, then compares size and SHA-256. If an image differs, the script compares the
# constituent build artifacts (kernel, loader, initfs) and the first differing byte so the
# non-deterministic input is named instead of guessed. It never claims a pass for an image it did
# not build or compare, and it never runs QEMU.
#
# Usage:
#   sh scripts/repro-release.sh                 # both images, two builds each
#   sh scripts/repro-release.sh --arch x86_64   # one architecture
#   sh scripts/repro-release.sh --arch aarch64
#   sh scripts/repro-release.sh --self-test     # synthetic compare-logic test, no image build
#   sh scripts/repro-release.sh --out DIR       # output root (default build/release/repro)
#
# Exit: 0 both requested images reproduced; 1 a difference or build failure; 2 Pi 4 firmware
# missing (no silent skip).

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"

out=build/release/repro
archs=all
self_test=0

while [ $# -gt 0 ]; do
    case "$1" in
        --self-test) self_test=1 ;;
        --arch)
            shift
            [ $# -gt 0 ] || { echo "repro-release: --arch needs a value" >&2; exit 2; }
            case "$1" in
                all|x86_64|aarch64) archs=$1 ;;
                *) echo "repro-release: unknown arch $1" >&2; exit 2 ;;
            esac
            ;;
        --out)
            shift
            [ $# -gt 0 ] || { echo "repro-release: --out needs a value" >&2; exit 2; }
            out=$1
            ;;
        -h|--help)
            sed -n '1,22p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *)
            echo "repro-release: unknown argument: $1" >&2
            exit 2
            ;;
    esac
    shift
done

sha256_file() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | awk '{print $1}'
    else
        echo "repro-release: sha256sum or shasum is required" >&2
        return 1
    fi
}

size_of() {
    wc -c <"$1" | tr -d ' '
}

# `identical`, `size` or `content`; the self-test exercises all three.
compare_kind() {
    if cmp -s "$1" "$2"; then
        echo identical
    elif [ "$(size_of "$1")" != "$(size_of "$2")" ]; then
        echo size
    else
        echo content
    fi
}

elf_build_id() {
    if command -v readelf >/dev/null 2>&1; then
        readelf -n "$1" 2>/dev/null | awk '/Build ID:/ { print $3; exit }'
    fi
}

kernel_version_id() {
    strings "$1" 2>/dev/null | grep -m1 -F "$2" || true
}

copy_artifacts() {
    arch=$1
    destination=$2
    mkdir -p "$destination"
    case "$arch" in
        aarch64)
            # The Pi image uses the flat kernel from its own target dir, not build/aarch64/kernel.elf
            # (that one belongs to the generic kernel build and may be stale).
            pi_kernel=build/cargo/kernel-aarch64-pi4/aarch64-unknown-none-softfloat/debug/raven-kernel
            if [ -f "$pi_kernel" ]; then
                cp "$pi_kernel" "$destination/kernel.elf"
            fi
            ;;
        *)
            for artifact in kernel.elf loader.efi; do
                if [ -f "build/$arch/$artifact" ]; then
                    cp "build/$arch/$artifact" "$destination/$artifact"
                fi
            done
            ;;
    esac
    if [ -f "build/$arch/initfs.img" ]; then
        cp "build/$arch/initfs.img" "$destination/initfs.img"
    fi
}

diagnose_artifacts() {
    arch=$1
    a=$2
    b=$3
    different=0
    for artifact in kernel.elf loader.efi initfs.img; do
        left=$a/$artifact
        right=$b/$artifact
        [ -f "$left" ] || continue
        [ -f "$right" ] || { echo "  $artifact: missing from build b" >&2; different=1; continue; }
        kind=$(compare_kind "$left" "$right")
        if [ "$kind" = identical ]; then
            echo "  $artifact: identical ($(size_of "$left") bytes, $(sha256_file "$left"))"
        else
            different=1
            echo "  $artifact: DIFFERS ($kind; $(size_of "$left") vs $(size_of "$right") bytes)" >&2
            echo "    a $(sha256_file "$left")" >&2
            echo "    b $(sha256_file "$right")" >&2
            first=$(cmp -l "$left" "$right" 2>/dev/null | head -1 || true)
            [ -n "$first" ] && echo "    first differing byte: $first" >&2
            case "$artifact" in
                *.elf)
                    echo "    a build-id: $(elf_build_id "$left")" >&2
                    echo "    b build-id: $(elf_build_id "$right")" >&2
                    ;;
                initfs.img)
                    echo "    initfs metadata (RAVENFS1 header/timestamps) is the input to inspect" >&2
                    ;;
            esac
        fi
    done
    return "$different"
}

find_firmware() {
    if [ -n "${RAVEN_RPI_FIRMWARE:-}" ] && [ -d "$RAVEN_RPI_FIRMWARE" ]; then
        return 0
    fi
    for candidate in \
        "$root/../ravenbsd-resources/rpi-firmware/boot" \
        "$root/../../ravenbsd-resources/rpi-firmware/boot" \
        "$root/../../../ravenbsd-resources/rpi-firmware/boot" \
        "${HOME:-/nonexistent}/work/ravenos-workspace/ravenbsd-resources/rpi-firmware/boot" \
        "${HOME:-/nonexistent}/ravenbsd-resources/rpi-firmware/boot"
    do
        if [ -d "$candidate" ]; then
            RAVEN_RPI_FIRMWARE=$candidate
            export RAVEN_RPI_FIRMWARE
            return 0
        fi
    done
    return 1
}

build_image() {
    arch=$1
    side=$2
    path=$3
    log=$4
    case "$arch" in
        x86_64)
            if ! cargo xtask image --arch x86_64 --platform generic --profile release \
                --output "$path" >"$log" 2>&1; then
                echo "repro-release: x86_64 image build $side failed (see $log)" >&2
                tail -20 "$log" >&2
                return 1
            fi
            ;;
        aarch64)
            if ! sh tests/fixtures/pi4-release/build.sh >"$log.fixture" 2>&1; then
                echo "repro-release: Pi 4 fixture build failed (see $log.fixture)" >&2
                tail -20 "$log.fixture" >&2
                return 1
            fi
            if ! RAVEN_RPI_FIRMWARE="$RAVEN_RPI_FIRMWARE" \
                cargo xtask image --arch aarch64 --platform pi4 --profile pi4-release \
                --output "$path" >>"$log" 2>&1; then
                echo "repro-release: Pi 4 image build $side failed (see $log)" >&2
                tail -20 "$log" >&2
                return 1
            fi
            ;;
    esac
}

verify_kernel_id() {
    arch=$1
    case "$arch" in
        aarch64)
            kernel=build/cargo/kernel-aarch64-pi4/aarch64-unknown-none-softfloat/debug/raven-kernel
            ;;
        *)
            kernel=build/$arch/kernel.elf
            ;;
    esac
    if [ ! -f "$kernel" ]; then
        echo "repro-release: $kernel is missing after the $arch build" >&2
        return 1
    fi
    id_pattern="(^|[^0-9a-f])${head}([^0-9a-f]|$)"
    if ! strings "$kernel" 2>/dev/null | grep -Eq "$id_pattern"; then
        echo "repro-release: $arch kernel does not carry the clean-tree build id '$head'" >&2
        echo "  the image builder reused an artifact from another commit or a dirty tree" >&2
        echo "  remove the kernel target directory under build/cargo and rerun" >&2
        strings "$kernel" 2>/dev/null | grep -E 'RavenBSD kernel|20[0-9][0-9]-[0-9][0-9]-[0-9][0-9]' | head -3 >&2 || true
        return 1
    fi
    if strings "$kernel" 2>/dev/null | grep -Fq "$head-dirty"; then
        echo "repro-release: $arch kernel was built from a dirty tree" >&2
        return 1
    fi
}

check_arch() {
    arch=$1
    work=$out/$arch
    rm -rf "$work"
    mkdir -p "$work/logs"

    for side in a b; do
        echo "==> $arch image build $side"
        build_image "$arch" "$side" "$work/$side.img" "$work/logs/build-$side.log" || return 1
        if ! verify_kernel_id "$arch"; then
            return 1
        fi
        copy_artifacts "$arch" "$work/$side-artifacts"
    done

    size_a=$(size_of "$work/a.img")
    size_b=$(size_of "$work/b.img")
    hash_a=$(sha256_file "$work/a.img")
    hash_b=$(sha256_file "$work/b.img")

    echo "repro-release: $arch size a=$size_a b=$size_b"
    echo "repro-release: $arch sha256 a=$hash_a"
    echo "repro-release: $arch sha256 b=$hash_b"

    if [ "$hash_a" = "$hash_b" ] && [ "$size_a" = "$size_b" ]; then
        if ! diagnose_artifacts "$arch" "$work/a-artifacts" "$work/b-artifacts" >/dev/null; then
            echo "repro-release: $arch image is identical but a constituent artifact differs" >&2
            return 1
        fi
        echo "repro-release: $arch REPRODUCIBLE"
        return 0
    fi

    echo "repro-release: $arch NOT REPRODUCIBLE; first differing input follows" >&2
    first=$(cmp -l "$work/a.img" "$work/b.img" 2>/dev/null | head -1 || true)
    [ -n "$first" ] && echo "repro-release: $arch first differing image byte: $first" >&2
    if ! diagnose_artifacts "$arch" "$work/a-artifacts" "$work/b-artifacts" >&2; then
        echo "repro-release: $arch verdict: a constituent build artifact differs (see above)" >&2
    else
        echo "repro-release: $arch verdict: the artifacts match, so the image assembly" >&2
        echo "  (GPT/FAT metadata or the generated boot.toml) is the non-deterministic input." >&2
        echo "  Owner: xtask/src/image.rs (FAT/GPT timestamps); boot.toml is generated there." >&2
    fi
    return 1
}

self_test() {
    tmp=$(mktemp -d "${TMPDIR:-/tmp}/repro-release.XXXXXX")
    trap 'rm -rf "$tmp"' EXIT HUP INT TERM

    printf 'release-repro\n' >"$tmp/same-a"
    cp "$tmp/same-a" "$tmp/same-b"
    printf 'abcdef' >"$tmp/content-a"
    printf 'abcdeg' >"$tmp/content-b"
    printf 'abc' >"$tmp/size-a"
    printf 'abcd' >"$tmp/size-b"

    failures=0
    check_kind() {
        expected=$1
        left=$2
        right=$3
        actual=$(compare_kind "$left" "$right")
        if [ "$actual" != "$expected" ]; then
            echo "repro-release: self-test FAIL: $left/$right is $actual, expected $expected" >&2
            failures=$((failures + 1))
        fi
    }
    check_kind identical "$tmp/same-a" "$tmp/same-b"
    check_kind content "$tmp/content-a" "$tmp/content-b"
    check_kind size "$tmp/size-a" "$tmp/size-b"

    hash=$(sha256_file "$tmp/same-a")
    if [ "${#hash}" -ne 64 ]; then
        echo "repro-release: self-test FAIL: sha256 is not 64 hex characters" >&2
        failures=$((failures + 1))
    fi
    first=$(cmp -l "$tmp/content-a" "$tmp/content-b" 2>/dev/null | head -1 || true)
    case "$first" in
        6\ *) ;;
        *)
            echo "repro-release: self-test FAIL: first differing byte should be 6, got '$first'" >&2
            failures=$((failures + 1))
            ;;
    esac

    if [ "$failures" -ne 0 ]; then
        echo "repro-release: self-test FAILED ($failures case(s))" >&2
        return 1
    fi
    echo "repro-release: self-test PASS (identical/content/size classification, sha256, byte offset)"
    return 0
}

if [ "$self_test" -eq 1 ]; then
    self_test
    exit $?
fi

if [ -n "$(git status --porcelain)" ]; then
    echo "repro-release: working tree is dirty; a release image must be built from a clean commit" >&2
    echo "  commit or stash the changes first (git status lists them)" >&2
    exit 2
fi
head=$(git rev-parse --short HEAD)
epoch=$(git log -1 --format=%ct)
SOURCE_DATE_EPOCH=$epoch
export SOURCE_DATE_EPOCH
echo "repro-release: commit=$head SOURCE_DATE_EPOCH=$epoch"

# The kernel embeds `RAVEN_BUILD_ID`; cargo reruns the kernel build when the id env changes, but
# a cached artifact from another tree state must never be accepted. The image builder keeps its
# own target directory per component/arch (`build/cargo/kernel-*`), so clean those exact caches
# before the pair. This is cheap: one crate per architecture.
cargo clean -p raven-kernel --target-dir build/cargo/kernel-x86_64 >/dev/null 2>&1 || true
cargo clean -p raven-kernel --target-dir build/cargo/kernel-aarch64-pi4 >/dev/null 2>&1 || true

if [ "$archs" = all ]; then
    list="x86_64 aarch64"
elif [ "$archs" = x86_64 ]; then
    list="x86_64"
else
    list="aarch64"
fi

case " $list " in
    *" aarch64 "*)
        if ! find_firmware; then
            echo "repro-release: Raspberry Pi firmware not found; set RAVEN_RPI_FIRMWARE" >&2
            echo "  (refusing to skip the Pi 4 image silently)" >&2
            exit 2
        fi
        ;;
esac

mkdir -p "$out"
failed=0
for arch in $list; do
    if ! check_arch "$arch"; then
        failed=1
    fi
done

if [ "$failed" -ne 0 ]; then
    echo "repro-release: FAILED (at least one image is not reproducible)" >&2
    exit 1
fi
echo "repro-release: PASS (all requested images are byte-identical across two builds)"
