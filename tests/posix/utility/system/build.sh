#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the minimal P4d system/process utility initfs. It has the core services
# and rsh from the shell profile but only the system tools under test, so the
# 120 s initfsd readiness window is not consumed by the full uutils/man image.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../../.." && pwd)
cd "$root"

out=build/posix/utility/system
mkdir -p "$out"

cargo xtask build user --arch x86_64 --release \
    -p raven-init -p raven-logd -p raven-sysd -p raven-randd -p raven-zerod \
    -p raven-nulld -p raven-initfsd -p raven-ramfsd -p raven-vfsd \
    -p raven-consd -p raven-rsh -p raven-uname -p raven-hostname -p raven-ps \
    -p raven-env -p raven-getconf -p raven-kill -p raven-nice -p raven-nohup \
    -p whoami >"$out/build-user.log" 2>&1

for tool in id; do
    if [ ! -x "$root/build/x86_64/uutils/$tool" ]; then
        echo "posix-util-system: missing build/x86_64/uutils/$tool; run cargo xtask build uutils --arch x86_64" >&2
        exit 1
    fi
done

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    "/init=$root/build/x86_64/user/raven-init:0755" \
    "/sbin/logd=$root/build/x86_64/user/raven-logd:0755" \
    "/sbin/sysd=$root/build/x86_64/user/raven-sysd:0755" \
    "/sbin/randd=$root/build/x86_64/user/raven-randd:0755" \
    "/sbin/zerod=$root/build/x86_64/user/raven-zerod:0755" \
    "/sbin/nulld=$root/build/x86_64/user/raven-nulld:0755" \
    "/sbin/initfsd=$root/build/x86_64/user/raven-initfsd:0755" \
    "/sbin/ramfsd=$root/build/x86_64/user/raven-ramfsd:0755" \
    "/sbin/vfsd=$root/build/x86_64/user/raven-vfsd:0755" \
    "/sbin/consd=$root/build/x86_64/user/raven-consd:0755" \
    "/bin/rsh=$root/build/x86_64/user/raven-rsh:0755" \
    "/bin/uname=$root/build/x86_64/user/raven-uname:0755" \
    "/bin/hostname=$root/build/x86_64/user/raven-hostname:0755" \
    "/bin/ps=$root/build/x86_64/user/raven-ps:0755" \
    "/bin/whoami=$root/build/x86_64/user/whoami:0755" \
    "/bin/id=$root/build/x86_64/uutils/id:0755" \
    "/bin/env=$root/build/x86_64/user/raven-env:0755" \
    "/bin/getconf=$root/build/x86_64/user/raven-getconf:0755" \
    "/bin/kill=$root/build/x86_64/user/raven-kill:0755" \
    "/bin/nice=$root/build/x86_64/user/raven-nice:0755" \
    "/bin/nohup=$root/build/x86_64/user/raven-nohup:0755" \
    "/etc/services.d/logd.toml=$root/etc/services.d/logd.toml:0644" \
    "/etc/services.d/sysd.toml=$root/etc/services.d/sysd.toml:0644" \
    "/etc/services.d/randd.toml=$root/etc/services.d/randd.toml:0644" \
    "/etc/services.d/zerod.toml=$root/etc/services.d/zerod.toml:0644" \
    "/etc/services.d/nulld.toml=$root/etc/services.d/nulld.toml:0644" \
    "/etc/services.d/initfsd.toml=$root/etc/services.d/initfsd.toml:0644" \
    "/etc/services.d/ramfsd.toml=$root/etc/services.d/ramfsd.toml:0644" \
    "/etc/services.d/vfsd.toml=$root/etc/services.d/vfsd.toml:0644" \
    "/etc/services.d/consd.toml=$root/etc/services.d/consd.toml:0644" \
    "/etc/services.d/rsh.toml=$root/etc/services.d/rsh.toml:0644" \
    "/etc/mounts=$root/etc/mounts:0644" \
    "/etc/svcd.conf=$root/etc/svcd.conf:0644" \
    "/etc/rc.conf=$root/tests/posix/utility/system/rc.conf:0644" \
    "/etc/passwd=$root/etc/passwd:0644" \
    "/etc/group=$root/etc/group:0644" \
    "/etc/shadow=$root/etc/shadow:0600"

echo "posix-util-system: $out/initfs.img"
