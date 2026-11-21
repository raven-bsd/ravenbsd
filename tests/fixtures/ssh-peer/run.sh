#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K375 end-to-end tracer run. First proves the tool against the local Linux OpenSSH server, then
# boots the guest sshd through the qtest hostfwd case and prints the byte-exact peer log. The
# guest result is evidence either way: a KEX_ECDH_REPLY or the exact bytes before the stall.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/ssh-peer
linux_banner=/tmp/opencode/ssh-peer-linux.log
linux_kex=/tmp/opencode/ssh-peer-linux-kex.log
mkdir -p "$out" /tmp/opencode

sh tests/fixtures/ssh-peer/build.sh
cp "$out/peer" /tmp/opencode/ssh-peer

printf '=== Linux control: banner only ===\n'
timeout 30 /tmp/opencode/ssh-peer --host 127.0.0.1 --port 22 --mode client \
    --idle-ms 3000 --log "$linux_banner"
printf '=== Linux control: banner and KEXINIT ===\n'
timeout 30 /tmp/opencode/ssh-peer --host 127.0.0.1 --port 22 --mode client \
    --send-banner --send-kexinit --idle-ms 5000 --log "$linux_kex"

if ! grep -q 'banner.*SSH-2\.0-OpenSSH' "$linux_banner"; then
    printf 'ssh-peer: %s has no OpenSSH banner\n' "$linux_banner" >&2
    exit 1
fi
if ! grep -q 'banner.*SSH-2\.0-OpenSSH' "$linux_kex"; then
    printf 'ssh-peer: %s has no OpenSSH banner\n' "$linux_kex" >&2
    exit 1
fi
if ! grep -q 'SSH2_MSG_KEXINIT' "$linux_kex"; then
    printf 'ssh-peer: %s has no KEXINIT record\n' "$linux_kex" >&2
    exit 1
fi
if ! grep -q 'SSH2_MSG_KEX_ECDH_REPLY' "$linux_kex"; then
    printf 'ssh-peer: Linux KEX control has no KEX_ECDH_REPLY\n' >&2
    exit 1
fi
printf 'linux control: banner and KEXINIT captured, KEX_ECDH_REPLY present\n'

rm -f "$out/peer.log" "$out/peer.status"
if ! timeout 210 cargo xtask qtest --case ssh-peer --arch x86_64; then
    printf 'ssh-peer: guest qtest failed\n' >&2
    exit 1
fi
if [ ! -s "$out/peer.log" ]; then
    printf 'ssh-peer: guest produced no peer log\n' >&2
    exit 1
fi

printf '=== guest peer log ===\n'
cat "$out/peer.log"
printf '=== end guest peer log ===\n'

if ! grep -q 'banner.*SSH-2\.0-' "$out/peer.log"; then
    printf 'ssh-peer: guest sent no SSH banner\n' >&2
    exit 1
fi
if ! grep -q 'SSH2_MSG_KEXINIT' "$out/peer.log"; then
    printf 'ssh-peer: guest sent no KEXINIT record\n' >&2
    exit 1
fi
if grep -q 'SSH2_MSG_KEX_ECDH_REPLY' "$out/peer.log"; then
    printf 'guest result: KEX_ECDH_REPLY captured\n'
else
    printf 'guest result: no KEX_ECDH_REPLY; the last bytes and idle/eof line are the stall evidence\n'
fi
exit 0
