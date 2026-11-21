#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K375 fixture build: compile the host fake peer and assemble the minimal guest initfs with a
# real OpenSSH sshd. The OpenSSH binaries come from addons/ssh (read-only for this ticket); the
# guest service set is the server-profile network stack plus sshd, staged only under
# build/fixtures/ssh-peer and never in the product image.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

arch=x86_64
out=build/fixtures/ssh-peer
mkdir -p "$out"

cc -O2 -Wall -Wextra -o "$out/peer" tests/fixtures/ssh-peer/peer.c

for file in \
    "build/ssh/$arch/bin/sshd" \
    "build/ssh/$arch/libexec/sshd-session" \
    "build/ssh/$arch/libexec/sshd-auth"; do
    if [ ! -x "$file" ]; then
        sh addons/ssh/build.sh
        break
    fi
done
for file in \
    "build/ssh/$arch/bin/sshd" \
    "build/ssh/$arch/libexec/sshd-session" \
    "build/ssh/$arch/libexec/sshd-auth"; do
    [ -x "$file" ] || {
        echo "ssh-peer: missing $file after addons/ssh/build.sh" >&2
        exit 1
    }
done

packages="raven-init raven-logd raven-initfsd raven-ramfsd raven-nulld raven-zerod \
raven-randd raven-vfsd raven-pcid raven-devd virtio-net netd raven-rsh raven-consd"
missing=
for package in $packages; do
    [ -x "build/$arch/user/$package" ] || missing="$missing $package"
done
if [ -n "$missing" ]; then
    set --
    for package in $missing; do
        set -- "$@" -p "$package"
    done
    cargo xtask build user --arch "$arch" "$@"
fi

guest=$out/guest
mkdir -p "$guest"
stage() {
    source=$1
    name=$2
    cp "$source" "$guest/$name"
    llvm-strip "$guest/$name"
}
for package in $packages; do
    stage "build/$arch/user/$package" "$package"
done
stage "build/ssh/$arch/bin/sshd" sshd
stage "build/ssh/$arch/libexec/sshd-session" sshd-session
stage "build/ssh/$arch/libexec/sshd-auth" sshd-auth

if [ ! -s "$out/host_ed25519_key" ]; then
    rm -f "$out/host_ed25519_key" "$out/host_ed25519_key.pub"
    ssh-keygen -q -t ed25519 -N '' -f "$out/host_ed25519_key"
fi

cargo run -q -p raven-initfs --features std --example mkinitfs -- "$out/initfs.img" \
    "/init=$guest/raven-init" \
    "/sbin/logd=$guest/raven-logd" \
    "/sbin/initfsd=$guest/raven-initfsd" \
    "/sbin/ramfsd=$guest/raven-ramfsd" \
    "/sbin/nulld=$guest/raven-nulld" \
    "/sbin/zerod=$guest/raven-zerod" \
    "/sbin/randd=$guest/raven-randd" \
    "/sbin/vfsd=$guest/raven-vfsd" \
    "/sbin/pcid=$guest/raven-pcid" \
    "/sbin/devd=$guest/raven-devd" \
    "/sbin/virtio-net=$guest/virtio-net" \
    "/sbin/netd=$guest/netd" \
    "/sbin/consd=$guest/raven-consd" \
    "/bin/rsh=$guest/raven-rsh" \
    "/sbin/sshd=$guest/sshd" \
    "/usr/libexec/sshd-session=$guest/sshd-session:0755" \
    "/usr/libexec/sshd-auth=$guest/sshd-auth:0755" \
    "/etc/services.d/logd.toml=etc/services.d/logd.toml" \
    "/etc/services.d/initfsd.toml=etc/services.d/initfsd.toml" \
    "/etc/services.d/ramfsd.toml=etc/services.d/ramfsd.toml" \
    "/etc/services.d/nulld.toml=etc/services.d/nulld.toml" \
    "/etc/services.d/zerod.toml=etc/services.d/zerod.toml" \
    "/etc/services.d/randd.toml=etc/services.d/randd.toml" \
    "/etc/services.d/vfsd.toml=etc/services.d/vfsd.toml" \
    "/etc/services.d/pcid.toml=etc/services.d/pcid.toml" \
    "/etc/services.d/devd.toml=tests/fixtures/ssh-peer/devd.toml" \
    "/etc/services.d/netd.toml=tests/fixtures/ssh-peer/netd.toml" \
    "/etc/services.d/consd.toml=etc/services.d/consd.toml" \
    "/etc/services.d/sshd.toml=tests/fixtures/ssh-peer/sshd.toml" \
    "/etc/services.d/sshd-check.toml=tests/fixtures/ssh-peer/sshd-check.toml" \
    "/etc/drivers.toml=profiles/server.d/drivers.conf" \
    "/etc/mounts=tests/fixtures/ssh-peer/mounts" \
    "/etc/svcd.conf=tests/fixtures/ssh-peer/svcd.conf" \
    "/etc/ssh/sshd_config=tests/fixtures/ssh-peer/sshd_config:0644" \
    "/etc/ssh/ssh_host_ed25519_key=$out/host_ed25519_key:0600" \
    "/etc/passwd=tests/fixtures/ssh-peer/passwd:0644" \
    "/etc/group=tests/fixtures/ssh-peer/group:0644" \
    "/etc/shadow=tests/fixtures/ssh-peer/shadow:0600" \
    "/var/empty/.keep=tests/fixtures/ssh-peer/var-empty.keep:0644"

printf 'ssh-peer: built %s/peer and %s/initfs.img\n' "$out" "$out"
