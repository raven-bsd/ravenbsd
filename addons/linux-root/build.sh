#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# B32 Linux root recipe: stage an Alpine minirootfs at build/linux-root/compat/linux and build the
# qtest initfs. The tarball is user-supplied; it is never committed. The only network operation is
# the explicit `--fetch` mode.
#
# Usage:
#   sh addons/linux-root/build.sh --check    verify the pinned recipe (no tarball needed)
#   sh addons/linux-root/build.sh --fetch    download the pinned tarball (user-invoked)
#   sh addons/linux-root/build.sh --stage    verify the tarball and stage the root tree
#   sh addons/linux-root/build.sh --qtest    stage and build the linux-root qtest initfs (default)

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
cd "$root"

# shellcheck source=recipe.env
. "$root/addons/linux-root/recipe.env"

sha256_file="$root/addons/linux-root/${ALPINE_FILE}.sha256"
downloads="$root/build/linux-root/downloads"
stage_root="$root/build/linux-root/compat/linux"
manifest="$root/build/linux-root/manifest.txt"
initfs_out="$root/build/linux-root/initfs.img"

log() {
    echo "linux-root: $*"
}

die() {
    echo "linux-root: $*" >&2
    exit 2
}

usage() {
    die "usage: build.sh [--check|--fetch|--stage|--qtest]"
}

sha256_of() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | awk '{print $1}'
    else
        die "sha256sum or shasum is required"
    fi
}

find_tarball() {
    if [ -n "${RAVEN_ALPINE_MINIROOTFS:-}" ]; then
        if [ -f "$RAVEN_ALPINE_MINIROOTFS" ]; then
            echo "$RAVEN_ALPINE_MINIROOTFS"
            return 0
        fi
        die "RAVEN_ALPINE_MINIROOTFS=$RAVEN_ALPINE_MINIROOTFS is not a file"
    fi
    for candidate in "$downloads/$ALPINE_FILE" "$root/addons/linux-root/downloads/$ALPINE_FILE"; do
        if [ -f "$candidate" ]; then
            echo "$candidate"
            return 0
        fi
    done
    return 0
}

verify_recipe() {
    branch="v${ALPINE_VERSION%.*}"
    expected_file="alpine-minirootfs-${ALPINE_VERSION}-${ALPINE_ARCH}.tar.gz"
    [ "$ALPINE_FILE" = "$expected_file" ] ||
        die "recipe.env: ALPINE_FILE does not match version/arch"
    case "$ALPINE_URL" in
        "https://dl-cdn.alpinelinux.org/alpine/$branch/releases/$ALPINE_ARCH/$ALPINE_FILE") ;;
        *) die "recipe.env: ALPINE_URL is not the pinned Alpine release URL" ;;
    esac
    case "$ALPINE_SHA256" in
        *[!0-9a-f]* | "") die "recipe.env: ALPINE_SHA256 is not lowercase hex" ;;
    esac
    [ "${#ALPINE_SHA256}" -eq 64 ] || die "recipe.env: ALPINE_SHA256 is not 64 hex digits"
    [ -f "$sha256_file" ] || die "missing $sha256_file"
    read -r recorded_hash recorded_file <"$sha256_file" ||
        die "cannot read $sha256_file"
    [ "$recorded_hash" = "$ALPINE_SHA256" ] || die "sha256 file hash does not match recipe.env"
    [ "$recorded_file" = "$ALPINE_FILE" ] || die "sha256 file name does not match recipe.env"
    grep -qF "$ALPINE_URL" "$root/addons/linux-root/README.md" ||
        die "README.md does not mention the pinned URL"
    grep -qF "$ALPINE_SHA256" "$root/addons/linux-root/README.md" ||
        die "README.md does not mention the pinned SHA-256"
}

verify_tarball() {
    tarball=$1
    [ -f "$tarball" ] || die "tarball not found: $tarball"
    actual_hash=$(sha256_of "$tarball")
    [ "$actual_hash" = "$ALPINE_SHA256" ] ||
        die "SHA-256 mismatch for $tarball: got $actual_hash"
    actual_size=$(wc -c <"$tarball" | tr -d ' ')
    [ "$actual_size" = "$ALPINE_SIZE" ] ||
        die "size mismatch for $tarball: got $actual_size, want $ALPINE_SIZE"
    listing=$(tar -tzf "$tarball")
    for member in ./bin/busybox ./lib/ld-musl-x86_64.so.1 ./etc/alpine-release; do
        echo "$listing" | grep -qx "$member" ||
            die "$tarball does not contain $member"
    done
}

stage_tree() {
    tarball=$1
    verify_tarball "$tarball"
    rm -rf "$root/build/linux-root/compat"
    mkdir -p "$stage_root"
    tar -xzf "$tarball" -C "$stage_root" --no-same-owner --no-same-permissions
    [ -f "$stage_root/bin/busybox" ] || die "staged tree has no bin/busybox"
    [ -f "$stage_root/lib/ld-musl-x86_64.so.1" ] || die "staged tree has no musl loader"
    [ -f "$stage_root/etc/alpine-release" ] || die "staged tree has no etc/alpine-release"
    release=$(cat "$stage_root/etc/alpine-release")
    files=$(find "$stage_root" -type f | wc -l | tr -d ' ')
    bytes=$(du -sb "$stage_root" | awk '{print $1}')
    mkdir -p "$(dirname "$manifest")"
    {
        echo "source=alpine-minirootfs"
        echo "version=$ALPINE_VERSION"
        echo "arch=$ALPINE_ARCH"
        echo "url=$ALPINE_URL"
        echo "sha256=$ALPINE_SHA256"
        echo "size=$ALPINE_SIZE"
        echo "licence=$ALPINE_LICENCE"
        echo "release=$release"
        echo "files=$files"
        echo "bytes=$bytes"
    } >"$manifest"
    log "staged version=$release files=$files bytes=$bytes at build/linux-root/compat/linux"
}

build_qtest_initfs() {
    stage_tree "$1"
    target=x86_64-unknown-none
    target_dir=${CARGO_TARGET_DIR:-target}
    rustflags="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld"

    RUSTFLAGS="$rustflags" \
        cargo build -p raven-b32-linux-root --target "$target" --release
    RUSTFLAGS="$rustflags" \
        cargo build -p linuxd --target "$target" --release

    set -- \
        "/init=$target_dir/$target/release/raven-b32-linux-root" \
        "/sbin/linuxd=$target_dir/$target/release/linuxd"
    for file in $(find "$stage_root" -type f | sort); do
        relative=${file#"$stage_root"/}
        mode=$(stat -c '%a' "$file")
        set -- "$@" "/compat/linux/$relative=$file:$mode"
    done
    cargo run -q -p raven-initfs --features std --example mkinitfs -- \
        "$initfs_out" "$@"
    log "qtest initfs: build/linux-root/initfs.img"
}

case "${1:---qtest}" in
    --check)
        verify_recipe
        tarball=$(find_tarball)
        if [ -n "$tarball" ]; then
            verify_tarball "$tarball"
            log "recipe ok version=$ALPINE_VERSION sha256=$ALPINE_SHA256 tarball=present"
        else
            log "recipe ok version=$ALPINE_VERSION sha256=$ALPINE_SHA256 tarball=absent"
            log "supply the tarball with --fetch or RAVEN_ALPINE_MINIROOTFS=<file> for --stage/--qtest"
        fi
        ;;
    --fetch)
        verify_recipe
        mkdir -p "$downloads"
        destination="$downloads/$ALPINE_FILE"
        if [ -f "$destination" ] && [ "$(sha256_of "$destination")" = "$ALPINE_SHA256" ]; then
            log "tarball already present: $destination"
        else
            command -v curl >/dev/null 2>&1 || die "curl is required for --fetch"
            log "fetching $ALPINE_URL"
            curl -fL --retry 2 --output "$destination" "$ALPINE_URL"
        fi
        verify_tarball "$destination"
        log "tarball verified: $destination"
        ;;
    --stage)
        verify_recipe
        tarball=$(find_tarball)
        [ -n "$tarball" ] || die "no tarball; run --fetch or set RAVEN_ALPINE_MINIROOTFS"
        stage_tree "$tarball"
        ;;
    --qtest)
        verify_recipe
        tarball=$(find_tarball)
        [ -n "$tarball" ] ||
            die "no Alpine minirootfs tarball; run 'sh addons/linux-root/build.sh --fetch' or set RAVEN_ALPINE_MINIROOTFS"
        build_qtest_initfs "$tarball"
        ;;
    *)
        usage
        ;;
esac
