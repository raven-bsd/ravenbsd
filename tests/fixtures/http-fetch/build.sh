#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the E3a live-fetch gate: static http-fetch probe, patched release initfs and one-shot
# local HTTP server. The server waits for one connection and exits; it is not a service.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/http-fetch
mkdir -p "$out"

echo "http-fetch: building the static probe"
RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld" \
    cargo build -p raven-http-net --bin http-fetch --target x86_64-unknown-none --release
target_dir=${CARGO_TARGET_DIR:-target}
cp "$target_dir/x86_64-unknown-none/release/http-fetch" "$out/http-fetch"

echo "http-fetch: building the release initfs"
cargo xtask image --arch x86_64 --profile release --output "$out/base.img"

echo "http-fetch: adding the probe to the initfs"
cargo run -q -p http-fetch-patch -- \
    build/x86_64/initfs.img "$out/http-fetch" "$out/initfs.img"

pid_file=$out/server.pid
log=$out/server.log
if [ -f "$pid_file" ]; then
    kill "$(cat "$pid_file")" 2>/dev/null || true
    rm -f "$pid_file"
fi
rm -f "$log"
nohup python3 "$root/tests/fixtures/http-fetch/server.py" >"$log" 2>&1 &
echo $! >"$pid_file"
sleep 0.3
if ! kill -0 "$(cat "$pid_file")" 2>/dev/null; then
    echo "http-fetch: local server failed to start" >&2
    cat "$log" >&2 || true
    exit 1
fi
echo "http-fetch: local server pid $(cat "$pid_file")"
