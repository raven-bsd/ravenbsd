#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K97: verify the dynamic-userland image inputs before QEMU boots them.
#
# Builds the `shell-test` initfs (which is `shell` plus the one-shot svcd config), then checks
# that the staged /bin programs are dynamic PIEs with PT_INTERP and DT_NEEDED libc.so.1, that
# /init and the /sbin services stay static, and that libc-link staged the loader and libc.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

arch=x86_64
profile=shell-test
out="build/$arch"

cargo xtask initfs --arch "$arch" --profile "$profile" >/dev/null

readelf=${READELF:-readelf}
if ! command -v "$readelf" >/dev/null 2>&1; then
    echo "dynamic-userland: readelf not found" >&2
    exit 1
fi

check_dynamic() {
    binary=$1
    path="$out/user/$binary"
    if ! "$readelf" -l "$path" | grep -q 'INTERP'; then
        echo "dynamic-userland: $binary has no PT_INTERP" >&2
        exit 1
    fi
    if ! "$readelf" -l "$path" | grep -q '/libexec/ld-elf.so.1'; then
        echo "dynamic-userland: $binary has the wrong interpreter" >&2
        exit 1
    fi
    if ! "$readelf" -d "$path" | grep -q 'NEEDED.*libc\.so\.1'; then
        echo "dynamic-userland: $binary has no DT_NEEDED libc.so.1" >&2
        exit 1
    fi
    echo "dynamic-userland: $binary PT_INTERP + DT_NEEDED libc.so.1 ok"
}

check_static() {
    binary=$1
    path="$out/user/$binary"
    if "$readelf" -l "$path" | grep -q 'INTERP'; then
        echo "dynamic-userland: $binary must stay static" >&2
        exit 1
    fi
    echo "dynamic-userland: $binary static ok"
}

check_dynamic raven-rsh
check_dynamic raven-uname
check_static raven-init
check_static raven-initfsd

for staged in "$out/libc.so.1" "$out/ld-elf.so.1"; do
    if [ ! -f "$staged" ]; then
        echo "dynamic-userland: missing $staged" >&2
        exit 1
    fi
done
echo "dynamic-userland: staging ok"
