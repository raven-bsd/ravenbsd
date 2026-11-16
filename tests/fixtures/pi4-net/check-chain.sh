#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Host-side check of the resolved Pi 4 release network launch/readiness chain.
# It reads the repository profile and driver table, never a board log.
#
# Usage: tests/fixtures/pi4-net/check-chain.sh
#
# Exit 0 only when devd owns the single GENET launch, the driver selects the
# platform handle slots, netd waits for scheme:nic, and the DTB/driver tables are
# staged in the resolved pi4-release profile.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

profile=profiles/pi4-release.toml
fragments=profiles/pi4-release.d
drivers=userland/servers/devd/drivers.toml
devd_manifest="$fragments/devd.manifest"
genet_manifest="$fragments/genet.manifest"
netd_manifest="$fragments/netd.manifest"
userspace="$fragments/20-userspace.toml"
dtb="$fragments/50-dtb.toml"

fail=0
say() { printf 'PI4-NET-CHAIN: %s\n' "$1"; }

[ -f "$profile" ] || { say "FAIL missing $profile"; exit 1; }
[ -f "$devd_manifest" ] || { say "FAIL missing $devd_manifest"; exit 1; }
[ -f "$genet_manifest" ] || { say "FAIL missing $genet_manifest"; exit 1; }
[ -f "$netd_manifest" ] || { say "FAIL missing $netd_manifest"; exit 1; }
[ -f "$drivers" ] || { say "FAIL missing $drivers"; exit 1; }

devd_ok=0
devd_args=$(grep '^args = ' "$devd_manifest" || true)
if printf '%s\n' "$devd_args" | grep -Fq '"dtb"' \
    && printf '%s\n' "$devd_args" | grep -Fq '"launch"'; then
    say "devd_args=dtb,launch"
    devd_ok=1
else
    say "devd_args=missing"
    fail=1
fi

genet_block=$(sed -n '/^name = "genet"/,/^$/p' "$drivers")
genet_args=$(printf '%s\n' "$genet_block" | grep '^args = ' || true)
if printf '%s\n' "$genet_args" | grep -Fq '"slots=platform"'; then
    say "genet_driver=slots=platform"
else
    say "genet_driver=missing-slots=platform"
    fail=1
fi

genet_exec=$(printf '%s\n' "$genet_block" | grep -F 'exec = "/sbin/genet"' || true)
devd_launch=0
if [ "$devd_ok" -eq 1 ] && [ -n "$genet_exec" ]; then
    devd_launch=1
fi

svcd_launch=0
if grep -Fq 'src = "profiles/pi4-release.d/genet.manifest"' "$userspace" \
    && grep -Fq 'exec = "/sbin/genet"' "$genet_manifest" \
    && ! grep -Eq '^enable = false$' "$genet_manifest"; then
    svcd_launch=1
fi
genet_launchers=$((devd_launch + svcd_launch))
say "genet_launchers=$genet_launchers"
if [ "$genet_launchers" -ne 1 ]; then
    say "FAIL duplicate genet launcher (devd=$devd_launch svcd=$svcd_launch)"
    fail=1
fi

netd_needs=$(grep '^needs = ' "$netd_manifest" || true)
if printf '%s\n' "$netd_needs" | grep -Fq '"scheme:nic"'; then
    say "netd_needs=scheme:nic"
else
    say "netd_needs=missing-scheme:nic"
    fail=1
fi

if grep -Fq 'path = "/etc/pi4.dtb"' "$dtb"; then
    say "dtb_staged=yes"
else
    say "dtb_staged=no"
    fail=1
fi

if grep -Fq 'src = "userland/servers/devd/drivers.toml"' "$userspace" \
    && grep -Fq 'path = "/etc/drivers.toml"' "$userspace"; then
    say "drivers_staged=yes"
else
    say "drivers_staged=no"
    fail=1
fi

if [ "$fail" -ne 0 ]; then
    say "FAIL"
    exit 1
fi

say "PASS"
