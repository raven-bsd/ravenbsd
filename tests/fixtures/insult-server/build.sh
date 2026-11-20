#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K201: prepare the deterministic insult content server and the test initfs for the eink-insult
# qtest. The release profile stages dynamic PIE /bin/curl and /bin/wget, but the login shell's
# spawn path cannot execute an interpreter image, so it reports them as "command not found". The
# K199 fetch gate overlays statically linked builds of the same clients into its test initfs; this
# script does the same. The production staging fix is outside K201's Owns.
# The server binds 127.0.0.1:18081 for QEMU user-net (guest 10.0.2.2), serves at most 8 requests
# and exits on idle. A previous pid is stopped first so a rerun cannot reuse a stale server.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/insult-server
mkdir -p "$out"

echo "insult-server: building static curl/wget for the test initfs"
RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld" \
    CARGO_TARGET_DIR="$out/static-target" \
    cargo build -p curl -p wget --target x86_64-unknown-none --release
cp "$out/static-target/x86_64-unknown-none/release/curl" "$out/curl-static"
cp "$out/static-target/x86_64-unknown-none/release/wget" "$out/wget-static"

base=$out/base-initfs.img
if [ ! -s "$base" ]; then
    echo "insult-server: building the release initfs"
    RAVEN_USER_RELEASE=1 cargo xtask initfs --arch x86_64 --profile release
    cp build/x86_64/initfs.img "$base"
fi
echo "insult-server: overlaying static curl/wget into $out/initfs.img"
CARGO_TARGET_DIR="$out/host-target" \
    cargo run -q -p raven-b70-overlay -- \
        "$base" "$out/initfs.img" \
        "/bin/curl=$out/curl-static:755" \
        "/bin/wget=$out/wget-static:755"

pid_file=$out/server.pid
log=$out/server.log

if [ -f "$pid_file" ]; then
    kill "$(cat "$pid_file")" 2>/dev/null || true
    rm -f "$pid_file"
fi
rm -f "$log"
nohup python3 "$root/tests/fixtures/insult-server/server.py" \
    --port 18081 --max-requests 8 --idle-timeout 300 >"$log" 2>&1 &
echo $! >"$pid_file"
sleep 0.3
if ! kill -0 "$(cat "$pid_file")" 2>/dev/null; then
    echo "insult-server: failed to start" >&2
    cat "$log" >&2 || true
    exit 1
fi
echo "insult-server: pid $(cat "$pid_file") on 127.0.0.1:18081"
