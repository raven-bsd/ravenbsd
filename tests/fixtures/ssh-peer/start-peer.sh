#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K375 start the fake peer before QEMU boots. The hostfwd port appears before the guest sshd
# listens, so a connect or an immediate zero-byte close means "not ready" (status 2 or 3) and the
# loop retries. The qtest itself stays in the foreground; this client is the case's host peer.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/ssh-peer
port=${SSH_PEER_PORT:-2222}
idle_ms=${SSH_PEER_IDLE_MS:-4000}
wait_s=${SSH_PEER_WAIT:-60}
deadline=$(( $(date +%s) + wait_s ))
rm -f "$out/peer.log" "$out/peer.attempts.log" "$out/peer.status"

while :; do
    status=0
    "$out/peer" --host 127.0.0.1 --port "$port" --mode client \
        --send-banner --send-kexinit --idle-ms "$idle_ms" --log "$out/peer.log" || status=$?
    if [ -s "$out/peer.log" ]; then
        {
            printf '\n=== attempt status=%s ===\n' "$status"
            cat "$out/peer.log"
        } >> "$out/peer.attempts.log"
    fi
    if [ "$status" -eq 0 ]; then
        echo 0 > "$out/peer.status"
        exit 0
    fi
    if [ "$status" -ne 2 ] && [ "$status" -ne 3 ]; then
        echo "$status" > "$out/peer.status"
        exit "$status"
    fi
    if [ "$(date +%s)" -ge "$deadline" ]; then
        echo 3 > "$out/peer.status"
        exit 3
    fi
    sleep 0.2
done
