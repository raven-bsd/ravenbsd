#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Host checks for scripts/release-inventory.sh. The synthetic initfs images
# exercise the content checks without building a disk image or QEMU.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd -P)
cd "$root"

sh -n scripts/release-inventory.sh
scripts/release-inventory.sh --help >/dev/null

if scripts/release-inventory.sh >/dev/null 2>&1; then
    echo "release-inventory-test: accepted a missing image" >&2
    exit 1
fi

if scripts/release-inventory.sh /tmp/opencode/does-not-exist.img >/dev/null 2>&1; then
    echo "release-inventory-test: accepted a missing image file" >&2
    exit 1
fi

work=$(mktemp -d "${TMPDIR:-/tmp}/release-inventory-test.XXXXXX")
trap 'rm -rf "$work"' EXIT HUP INT TERM

: > "$work/empty"
printf '%s\n' 'name = "netd"' 'exec = "/sbin/netd"' \
    'needs = ["logd", "scheme:nic"]' 'provides = ["scheme:net"]' > "$work/netd.toml"
printf '%s\n' 'name = "resolvd"' 'exec = "/sbin/resolvd"' \
    'needs = ["scheme:net"]' 'provides = ["scheme:resolv"]' > "$work/resolvd.toml"
printf '%s\n' 'name = "virtio-net"' 'exec = "/sbin/virtio-net"' > "$work/drivers.toml"

mkinitfs() {
    cargo run -q -p raven-initfs --features std --example mkinitfs -- "$@"
}

good=$work/good.img
mkinitfs "$good" \
    "/bin/rsh=$work/empty:0755" \
    "/bin/login=$work/empty:0755" \
    "/bin/getty=$work/empty:0755" \
    "/bin/ls=$work/empty:0755" \
    "/bin/ping=$work/empty:0755" \
    "/bin/brush=$work/empty:0755" \
    "/etc/passwd=$work/empty:0644" \
    "/etc/shadow=$work/empty:0600" \
    "/sbin/virtio-net=$work/empty:0755" \
    "/sbin/netd=$work/empty:0755" \
    "/sbin/resolvd=$work/empty:0755" \
    "/etc/services.d/netd.toml=$work/netd.toml:0644" \
    "/etc/services.d/resolvd.toml=$work/resolvd.toml:0644" \
    "/etc/drivers.toml=$work/drivers.toml:0644"

if ! scripts/release-inventory.sh --initfs "$good" --kind uefi >"$work/good.out" 2>&1; then
    echo "release-inventory-test: complete initfs was rejected" >&2
    tail -20 "$work/good.out" >&2
    exit 1
fi
grep -q 'release-inventory: summary entries=14 flags=0' "$work/good.out" || {
    echo "release-inventory-test: complete initfs did not report flags=0" >&2
    tail -20 "$work/good.out" >&2
    exit 1
}

bad=$work/bad.img
mkinitfs "$bad" \
    "/bin/login=$work/empty:0755" \
    "/bin/getty=$work/empty:0755" \
    "/bin/ls=$work/empty:0755" \
    "/sbin/ls=$work/empty:0755" \
    "/bin/ping=$work/empty:0755" \
    "/bin/brush=$work/empty:0755" \
    "/bin/tool=$work/empty:0644" \
    "/etc/passwd=$work/empty:0644" \
    "/etc/shadow=$work/empty:0600" \
    "/sbin/virtio-net=$work/empty:0755" \
    "/sbin/netd=$work/empty:0755" \
    "/sbin/resolvd=$work/empty:0755" \
    "/etc/services.d/netd.toml=$work/netd.toml:0644" \
    "/etc/services.d/resolvd.toml=$work/resolvd.toml:0644" \
    "/etc/drivers.toml=$work/drivers.toml:0644"

if scripts/release-inventory.sh --initfs "$bad" --kind uefi >"$work/bad.out" 2>&1; then
    echo "release-inventory-test: incomplete initfs was accepted" >&2
    tail -20 "$work/bad.out" >&2
    exit 1
fi
for expected in \
    'flag=missing-critical path=/bin/rsh' \
    'flag=duplicate-launcher name=ls' \
    'flag=non-executable path=/bin/tool'
do
    grep -Fq -- "$expected" "$work/bad.out" || {
        echo "release-inventory-test: missing expected flag: $expected" >&2
        tail -20 "$work/bad.out" >&2
        exit 1
    }
done

echo "release-inventory-test: PASS"
