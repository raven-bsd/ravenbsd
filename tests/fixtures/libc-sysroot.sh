#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Shared libc-sysroot freshness gate for the guest fixture builds.
#
# A fixture must never link a libc archive older than the tree. The default sysroot is
# checked with `libc-pack --verify` (the stamp's source hash, the archive hashes and the
# arch/set) and packed when it is missing or stale. An explicit `RAVENLIBC_SYSROOT` is the
# caller's choice: it is never packed behind their back, but a missing or source-older
# explicit sysroot fails loudly instead of silently reusing it.
#
# Usage (after the script sets root/arch/sysroot):
#   . "$root/tests/fixtures/libc-sysroot.sh"
#   ensure_libc_sysroot "$root" "$arch" "$sysroot" "<fixture name>"

ensure_libc_sysroot() {
    _libc_root=$1
    _libc_arch=$2
    _libc_sysroot=$3
    _libc_label=${4:-fixture}
    _libc_lib=$_libc_sysroot/usr/lib

    if [ -n "${RAVENLIBC_SYSROOT:-}" ]; then
        for _libc_file in crt1.o crti.o crtn.o libc.a; do
            if [ ! -f "$_libc_lib/$_libc_file" ]; then
                printf '%s: explicit sysroot %s is missing %s\n' \
                    "$_libc_label" "$_libc_sysroot" "$_libc_file" >&2
                exit 1
            fi
        done
        if [ -n "$(find "$_libc_root/libc/ravenlibc/crates" \
            "$_libc_root/libc/ravenlibc/abi" "$_libc_root/libc/ravenlibc/crt" \
            "$_libc_root/libc/ravenlibc/Cargo.toml" \
            -newer "$_libc_lib/libc.a" -print -quit 2>/dev/null)" ]; then
            printf '%s: explicit sysroot %s is older than the libc tree; refusing to reuse it\n' \
                "$_libc_label" "$_libc_sysroot" >&2
            exit 1
        fi
        return 0
    fi

    if ! cargo xtask libc-pack --arch "$_libc_arch" --verify >/dev/null 2>&1; then
        printf '%s: %s libc sysroot is missing or older than the sources; refreshing\n' \
            "$_libc_label" "$_libc_arch" >&2
        if ! cargo xtask libc-pack --arch "$_libc_arch"; then
            printf '%s: cargo xtask libc-pack --arch %s failed\n' \
                "$_libc_label" "$_libc_arch" >&2
            exit 1
        fi
        if ! cargo xtask libc-pack --arch "$_libc_arch" --verify >/dev/null 2>&1; then
            printf '%s: libc sysroot is still stale after libc-pack\n' "$_libc_label" >&2
            exit 1
        fi
    fi
    for _libc_file in crt1.o crti.o crtn.o libc.a; do
        if [ ! -f "$_libc_lib/$_libc_file" ]; then
            printf '%s: missing %s after libc-pack\n' \
                "$_libc_label" "$_libc_lib/$_libc_file" >&2
            exit 1
        fi
    done
}
