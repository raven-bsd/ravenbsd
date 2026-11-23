#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K434 per-step SSH gate matrix (S0-S8). Runs the pty-alloc qtest, builds the throw-away S0 echo
# initfs, then runs matrix.py. RAVEN_NET_MODE=netns (default) uses a private ken434 namespace with
# its own ravenbr0/ravendemo0; RAVEN_NET_MODE=shared uses scripts/qemu-net.sh for the rev-24 A/B.
# RAVEN_MATRIX_ONLY=s4|commands|tty10|mutation|s1-repeat|s7|s7-exit|s8|s5|s8-proof|full runs one boot-only row set.
# `full` is the single-shot K434 gate: S1, S2, S3, S4, S5/S6 (including S5-session), S7 and S8 in
# order on one image, writing serial-<row>.log deltas. The wrapper tests/ssh/run-k434-matrix.sh pins
# one tree/image hash and runs it:
#   sh tests/ssh/run-k434-matrix.sh
# The s4 mode boots once and runs three single-attempt auth-then-read proofs; mutation boots the 0755 image and
# asserts the non-root /tmp create fails:
#   timeout 600 env RAVEN_MATRIX_ONLY=s4 sh tests/ssh/matrix.sh
#   timeout 300 env RAVEN_MATRIX_ONLY=mutation RAVEN_SSH_IMAGE=build/qtest/ssh-login.img \
#     sh tests/ssh/matrix.sh
#
# Usage: sh tests/ssh/matrix.sh
# Outer acceptance timeout: 900 s; the inner commands have their own proportional caps.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
root_real=$(CDPATH= cd -- "$root" && pwd -P)
cd "$root"

bridge=ravenbr0
tap=ravendemo0
ns=ken434
host_ip=10.0.2.2
dns_ip=10.0.2.3
guest_ip=10.0.2.15
state=/tmp/opencode/ken-k434-net
conf=$state/dnsmasq.conf
pidfile=$state/dnsmasq.pid
user=$(id -un)
mode=${RAVEN_NET_MODE:-netns}
only=${RAVEN_MATRIX_ONLY:-}
out=${RAVEN_MATRIX_OUT:-build/ssh/matrix}
matrix_timeout=${RAVEN_MATRIX_TIMEOUT:-780}
close_env=
if [ -n "${RAVEN_CLOSE_TIMEOUT:-}" ]; then
    close_env="RAVEN_CLOSE_TIMEOUT=$RAVEN_CLOSE_TIMEOUT"
fi
image_env=
if [ -n "${RAVEN_SSH_IMAGE:-}" ]; then
    image_env="RAVEN_SSH_IMAGE=$RAVEN_SSH_IMAGE"
fi

if [ "$(id -u)" -eq 0 ]; then
    SUDO=
else
    sudo -n true 2>/dev/null || {
        echo "matrix: root required for the tap setup; refusing" >&2
        exit 3
    }
    SUDO="sudo -n"
fi

up=0
stale_qemu() {
    # A cap expiry can leave QEMU orphaned because the test starts it in its own session. Only
    # this clone's images are matched; another minion's QEMU keeps running.
    timeout 5 pkill -TERM -f "qemu-system-x86_64.*$root_real/build/qtest/ssh-login.img" \
        >/dev/null 2>&1 || true
    timeout 5 pkill -TERM -f "qemu-system-x86_64.*$root_real/build/fixtures/ssh-echo/echo.img" \
        >/dev/null 2>&1 || true
    # A stale serial socket is a FIFO-like file that blocks a full-tree provenance walk.
    find "$root/build/ssh" -name '*.sock' -delete 2>/dev/null || true
    sleep 1
}
down() {
    stale_qemu
    if [ "$up" -eq 1 ]; then
        if [ "$mode" = shared ]; then
            $SUDO sh scripts/qemu-net.sh down >/dev/null 2>&1 || true
        else
            if [ -f "$pidfile" ]; then
                $SUDO kill "$(cat "$pidfile")" >/dev/null 2>&1 || true
            fi
            $SUDO ip netns del "$ns" >/dev/null 2>&1 || true
        fi
        up=0
    fi
}
trap down EXIT HUP INT TERM

mkdir -p build/ssh/matrix "$state"
stale_qemu
if [ -z "$only" ]; then
    # S8's dynamic pty case comes from merged K430. Its measured run after K442 is ~39 s.
    echo "matrix: running the pty-alloc qtest" >&2
    pty_exit=0
    timeout 75 cargo xtask qtest --case pty-alloc --arch x86_64 \
        > build/ssh/matrix/pty-alloc.log 2>&1 || pty_exit=$?
    echo "$pty_exit" > build/ssh/matrix/pty-alloc.exit

    # S0 is a separate throw-away `server` initfs. A build failure is reported by the S0 row.
    echo "matrix: building the S0 echo fixture" >&2
    timeout 120 sh tests/fixtures/ssh-echo/build.sh || echo "matrix: S0 fixture build failed" >&2

    # S8's NULL-name fixture is staged in the release image by profiles/release.d/90-ssh-openpty.toml.
    echo "matrix: building the S8 openpty fixture" >&2
    timeout 120 sh tests/fixtures/ssh-openpty/build.sh \
        || echo "matrix: S8 openpty fixture build failed" >&2
fi

if [ "$mode" = shared ]; then
    echo "matrix: shared qemu-net setup (rev-24 A/B)" >&2
    $SUDO sh scripts/qemu-net.sh down >/dev/null 2>&1 || true
    $SUDO sh scripts/qemu-net.sh up
    up=1
    RAVEN_SSH_HOST="$guest_ip" RAVEN_QEMU_TAP="$tap" \
        RAVEN_MATRIX_ONLY="$only" RAVEN_MATRIX_OUT="$out" $close_env $image_env \
        timeout "$matrix_timeout" python3 "$root/tests/ssh/matrix.py"
else
    echo "matrix: private netns $ns bridge=$bridge tap=$tap guest=$guest_ip" >&2
    $SUDO ip netns del "$ns" >/dev/null 2>&1 || true
    $SUDO ip netns add "$ns"
    $SUDO ip netns exec "$ns" ip link set lo up
    $SUDO ip netns exec "$ns" ip link add "$bridge" type bridge
    $SUDO ip netns exec "$ns" ip addr add "$host_ip/24" dev "$bridge"
    $SUDO ip netns exec "$ns" ip addr add "$dns_ip/24" dev "$bridge"
    $SUDO ip netns exec "$ns" ip link set "$bridge" up
    $SUDO ip netns exec "$ns" ip tuntap add dev "$tap" mode tap user "$user"
    $SUDO ip netns exec "$ns" ip link set "$tap" master "$bridge"
    $SUDO ip netns exec "$ns" ip link set "$tap" up
    {
        printf 'interface=%s\n' "$bridge"
        printf 'bind-interfaces\n'
        printf 'listen-address=%s\n' "$dns_ip"
        printf 'dhcp-range=%s,%s,255.255.255.0,12h\n' "$guest_ip" "$guest_ip"
        printf 'dhcp-option=3,%s\n' "$host_ip"
        printf 'dhcp-option=6,%s\n' "$dns_ip"
        printf 'pid-file=%s\n' "$pidfile"
        printf 'log-facility=%s\n' "$state/dnsmasq.log"
    } > "$conf"
    $SUDO ip netns exec "$ns" dnsmasq --conf-file="$conf"
    up=1

    timeout "$matrix_timeout" $SUDO ip netns exec "$ns" runuser -u "$user" -- \
        env RAVEN_SSH_HOST="$guest_ip" RAVEN_QEMU_TAP="$tap" \
            RAVEN_MATRIX_ONLY="$only" RAVEN_MATRIX_OUT="$out" $close_env $image_env \
        python3 "$root/tests/ssh/matrix.py"
fi
