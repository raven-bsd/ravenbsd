#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Host behaviour evidence for the bounded first-party `talk` protocol.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../../.." && pwd)
target=$root/build/posix-utility/talk-host

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

fail() {
    printf 'FAIL: %s\n' "$1" >&2
    exit 1
}

cd "$root"
cargo nextest run -p talk >"$tmp/nextest.log" 2>&1 || {
    cat "$tmp/nextest.log"
    exit 1
}
grep -q 'two_peer_registry_and_pty_transcript' "$tmp/nextest.log" || {
    cat "$tmp/nextest.log"
    exit 1
}
printf 'ok: two-peer registry and pty transcript\n'
printf 'ok: talk host tests\n'

(
    CARGO_TARGET_DIR=$target cargo build --quiet --locked -p talk
)
TALK=$target/debug/talk
if [ ! -x "$TALK" ]; then
    printf 'talk test: missing host binary\n' >&2
    exit 1
fi

cd "$tmp"

"$TALK" --loopback >loopback 2>loopback.err
[ -s loopback ] || fail 'loopback produced no transcript'
grep -q '^invitation: Message from local' loopback || fail 'loopback invitation missing'
grep -q '^screen: split' loopback || fail 'loopback split screen missing'
grep -q '^caller> hello$' loopback || fail 'caller-to-peer text missing'
grep -q '^peer< hello$' loopback || fail 'peer receipt missing'
grep -q '^peer> hi$' loopback || fail 'peer-to-caller text missing'
grep -q '^caller< hi$' loopback || fail 'caller receipt missing'
grep -q 'local loopback closed (caller=0 peer=0)' loopback || fail 'loopback did not close cleanly'
printf 'ok: local loopback forwards both directions and closes cleanly\n'

if "$TALK" >out 2>err; then
    fail 'missing address was accepted'
fi
grep -q 'usage: talk' err || fail 'usage diagnostic missing'
printf 'ok: missing address is a usage error\n'

if "$TALK" -x alice >out 2>err; then
    fail 'unknown option was accepted'
fi
grep -q 'usage: talk' err || fail 'unknown-option diagnostic missing'
printf 'ok: unknown option is a usage error\n'

if "$TALK" alice@remote >out 2>err; then
    fail 'network address was accepted'
fi
grep -q 'network talk to alice@remote is not implemented' err || fail 'network diagnostic missing'
printf 'ok: network addresses are explicitly unsupported\n'

if "$TALK" alice >out 2>err; then
    fail 'local address was accepted without a registry'
fi
grep -q 'no terminal registry entry for alice' err || fail 'registry diagnostic missing'
printf 'ok: local addresses report the missing terminal registry\n'

if "$TALK" -l alice >out 2>err; then
    fail 'listener mode was accepted without a transport'
fi
grep -q 'no peer transport for a local listener' err || fail 'listener diagnostic missing'
printf 'ok: listener mode reports the missing peer transport\n'
