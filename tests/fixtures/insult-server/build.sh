#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K217/K219: prepare the deterministic insult content server and the test initfs for the
# eink-insult qtest. The release profile stages static /bin/curl and /bin/wget (K216), but the
# existing static overlay still replaces them with the same K199 client builds for the test
# initfs. The test initfs also carries /etc/eink-insult.sh (the one-send proof) and
# /etc/eink-insult.txt (the server-owned expected bytes). /tmp is a ramfs mount in this image,
# so an initfs /tmp file would be hidden; the release has /bin/brush but no /bin/sh, so the qtest
# runs the script with /bin/brush.
#
# K219: the server uses the unique port 18083. The pre reads the pid file, kills and waits,
# removes the pid file, starts the new process, and verifies both the process and the listening
# socket with a bounded retry. A stale listener without a pid file is stopped by port (18083 is
# unique to this fixture) before the new server starts.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/insult-server
port=18083
mkdir -p "$out"

log() {
    echo "insult-server: $*"
}

die() {
    echo "insult-server: $*" >&2
    exit 2
}

command -v python3 >/dev/null 2>&1 || die "python3 is required"
command -v ss >/dev/null 2>&1 || die "ss is required for the listening check"

listening() {
    ss -ltn 2>/dev/null | grep -q ":$port "
}

wait_free() {
    tries=0
    while listening && [ "$tries" -lt 50 ]; do
        tries=$((tries + 1))
        sleep 0.1
    done
    ! listening
}

stop_server() {
    pidfile=$1
    if [ -f "$pidfile" ]; then
        pid=$(cat "$pidfile")
        rm -f "$pidfile"
        if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
            log "stopping server pid $pid on port $port"
            kill "$pid" 2>/dev/null || true
            tries=0
            while kill -0 "$pid" 2>/dev/null && [ "$tries" -lt 50 ]; do
                tries=$((tries + 1))
                sleep 0.1
            done
            if kill -0 "$pid" 2>/dev/null; then
                kill -9 "$pid" 2>/dev/null || true
            fi
        fi
    fi
    if listening; then
        command -v fuser >/dev/null 2>&1 ||
            die "port $port is in use and fuser is unavailable"
        command -v readlink >/dev/null 2>&1 ||
            die "port $port is in use and readlink is unavailable"
        pids=$(fuser "$port/tcp" 2>/dev/null || true)
        [ -n "$pids" ] || die "port $port is listening but fuser found no pid"
        for pid in $pids; do
            cwd=$(readlink "/proc/$pid/cwd" 2>/dev/null || true)
            if [ "$cwd" != "$root" ]; then
                ss -ltnp 2>/dev/null | grep ":$port " >&2 || true
                die "port $port is held by pid $pid outside $root"
            fi
        done
        log "port $port is still held by a stale server in this checkout; stopping it"
        fuser -k -TERM "$port/tcp" >/dev/null 2>&1 || true
        if ! wait_free; then
            fuser -k "$port/tcp" >/dev/null 2>&1 || true
            wait_free || die "port $port did not become free"
        fi
    fi
}

wait_ready() {
    pid=$1
    logfile=$2
    tries=0
    while [ "$tries" -lt 50 ]; do
        if kill -0 "$pid" 2>/dev/null && listening; then
            log "server pid $pid on 127.0.0.1:$port"
            return 0
        fi
        if ! kill -0 "$pid" 2>/dev/null; then
            cat "$logfile" >&2 || true
            die "server on port $port exited during startup"
        fi
        tries=$((tries + 1))
        sleep 0.1
    done
    cat "$logfile" >&2 || true
    kill "$pid" 2>/dev/null || true
    die "server on port $port did not listen within 5 s"
}

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
echo "insult-server: overlaying static curl/wget and the one-send script into $out/initfs.img"
CARGO_TARGET_DIR="$out/host-target" \
    cargo run -q -p raven-b70-overlay -- \
        "$base" "$out/initfs.img" \
        "/bin/curl=$out/curl-static:755" \
        "/bin/wget=$out/wget-static:755" \
        "/etc/eink-insult.sh=$root/tests/fixtures/insult-server/eink-insult.sh:755" \
        "/etc/eink-insult.txt=$root/tests/fixtures/insult-server/insult.txt:644"

pid_file=$out/server.pid
log_file=$out/server.log
stop_server "$pid_file"
rm -f "$log_file"
timeout 360 python3 "$root/tests/fixtures/insult-server/server.py" \
    --port "$port" --max-requests 8 --idle-timeout 300 >"$log_file" 2>&1 &
pid=$!
echo "$pid" >"$pid_file"
wait_ready "$pid" "$log_file"
echo "insult-server: pid $pid on 127.0.0.1:$port"
