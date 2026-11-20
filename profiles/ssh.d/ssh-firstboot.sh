#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Generate the host keys once and start sshd.
#
# The UFS/release root has a writable /etc/ssh, so keys persist across reboot. The initfs
# release image has a read-only /etc; it falls back to /tmp/ssh and the keys are ephemeral.

set -eu

if [ -d /etc/ssh ] && [ -w /etc/ssh ]; then
    keydir=/etc/ssh
    ephemeral=no
else
    keydir=/tmp/ssh
    ephemeral=yes
fi

mkdir -p "$keydir"
chmod 700 "$keydir"

if [ "$ephemeral" = no ]; then
    [ -f "$keydir/ssh_host_ed25519_key" ] || ssh-keygen -A
else
    for type in rsa ecdsa ed25519; do
        key="$keydir/ssh_host_${type}_key"
        if [ ! -f "$key" ]; then
            ssh-keygen -q -t "$type" -N "" -f "$key"
        fi
    done
fi

exec /sbin/sshd -D -e -f /etc/ssh/sshd_config \
    -h "$keydir/ssh_host_rsa_key" \
    -h "$keydir/ssh_host_ecdsa_key" \
    -h "$keydir/ssh_host_ed25519_key"
