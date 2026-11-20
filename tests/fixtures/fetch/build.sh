#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the K199 fetch gate:
#   sh tests/fixtures/fetch/build.sh http
#   sh tests/fixtures/fetch/build.sh installed
#   sh tests/fixtures/fetch/build.sh https
#
# K219: each fixture has a unique port and stops only its own previous server. The pre reads the
# pid file, kills and waits, removes the pid file, starts the new process, and verifies both the
# process and the listening socket with a bounded retry. A stale listener without a pid file is
# stopped by port (the ports are unique to these fixtures) before the new server starts.
#
# `http` builds the base curl/wget (raven-http + raven-http-net) and starts the multi-request
# local HTTP server on 18081. `installed` only starts the 18082 server for fetch-installed;
# `https` builds the add-on curl/wget (raven-tls + OpenSSL), stages the T3 test CA, and starts a
# host `openssl s_server -WWW` on 24443. The servers are one-shot test processes with a bounded
# lifetime; they are not product services.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

mode=${1:-http}
out=$root/build/fixtures/fetch
certs=$root/build/tls/certs/test
addon=$root/build/tls/x86_64
sysroot=$addon/sysroot
target_dir=${CARGO_TARGET_DIR:-target}

http_port=18081
installed_port=18082
https_port=24443

log() {
    echo "fetch: $*"
}

die() {
    echo "fetch: $*" >&2
    exit 2
}

[ -f "$root/addons/tls/build.sh" ] || die "missing addons/tls/build.sh"
command -v python3 >/dev/null 2>&1 || die "python3 is required"
command -v openssl >/dev/null 2>&1 || die "host openssl is required"
command -v cargo >/dev/null 2>&1 || die "cargo is required"
command -v ss >/dev/null 2>&1 || die "ss is required for the listening check"

mkdir -p "$out" "$out/https-root"

listening() {
    port=$1
    ss -ltn 2>/dev/null | grep -q ":$port "
}

wait_free() {
    port=$1
    tries=0
    while listening "$port" && [ "$tries" -lt 50 ]; do
        tries=$((tries + 1))
        sleep 0.1
    done
    ! listening "$port"
}

stop_server() {
    pidfile=$1
    port=$2
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
    if listening "$port"; then
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
        if ! wait_free "$port"; then
            fuser -k "$port/tcp" >/dev/null 2>&1 || true
            wait_free "$port" || die "port $port did not become free"
        fi
    fi
}

wait_ready() {
    pidfile=$1
    pid=$2
    port=$3
    logfile=$4
    tries=0
    while [ "$tries" -lt 50 ]; do
        if kill -0 "$pid" 2>/dev/null && listening "$port"; then
            log "server pid $pid on 127.0.0.1:$port"
            return 0
        fi
        if ! kill -0 "$pid" 2>/dev/null; then
            cat "$logfile" >&2 || true
            rm -f "$pidfile"
            die "server on port $port exited during startup"
        fi
        tries=$((tries + 1))
        sleep 0.1
    done
    cat "$logfile" >&2 || true
    kill "$pid" 2>/dev/null || true
    rm -f "$pidfile"
    die "server on port $port did not listen within 5 s"
}

start_http_server() {
    pidfile=$out/http-server.pid
    logfile=$out/http-server.log
    lifetime=$1
    log "starting the local HTTP server on $http_port"
    stop_server "$pidfile" "$http_port"
    rm -f "$logfile"
    timeout "$lifetime" python3 "$root/tests/fixtures/fetch/http_server.py" \
        --port "$http_port" --seconds 600 >"$logfile" 2>&1 &
    pid=$!
    echo "$pid" >"$pidfile"
    wait_ready "$pidfile" "$pid" "$http_port" "$logfile"
}

start_installed_server() {
    pidfile=$out/fetch-installed-server.pid
    logfile=$out/fetch-installed-server.log
    lifetime=$1
    log "starting the fetch-installed HTTP server on $installed_port"
    stop_server "$pidfile" "$installed_port"
    rm -f "$logfile"
    timeout "$lifetime" python3 "$root/tests/fixtures/fetch/http_server.py" \
        --port "$installed_port" --seconds 600 >"$logfile" 2>&1 &
    pid=$!
    echo "$pid" >"$pidfile"
    wait_ready "$pidfile" "$pid" "$installed_port" "$logfile"
}

start_https_server() {
    pidfile=$out/https-server.pid
    logfile=$out/https-server.log
    log "starting the local HTTPS server on $https_port"
    stop_server "$pidfile" "$https_port"
    rm -f "$logfile"
    timeout 700 python3 "$root/tests/fixtures/fetch/tls_server.py" \
        --port "$https_port" --seconds 600 \
        --cert "$certs/server.pem" --key "$certs/server.key" \
        >"$logfile" 2>&1 &
    pid=$!
    echo "$pid" >"$pidfile"
    wait_ready "$pidfile" "$pid" "$https_port" "$logfile"
}

build_http_tools() {
    log "building base curl/wget (x86_64-unknown-none)"
    RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld" \
        cargo build -p curl -p wget --target x86_64-unknown-none --release >/dev/null
    cp "$target_dir/x86_64-unknown-none/release/curl" "$out/curl-http"
    cp "$target_dir/x86_64-unknown-none/release/wget" "$out/wget-http"
}

ensure_addon() {
    stale=0
    if [ ! -f "$addon/lib/libssl.a" ] || [ ! -f "$addon/lib/libcrypto.a" ] ||
        [ ! -f "$sysroot/usr/lib/libc.a" ] || [ ! -f "$root/build/tls/tls_compat.o" ]; then
        stale=1
    elif [ -n "$(find "$root/libc/ravenlibc/crates" "$root/libc/ravenlibc/abi" \
        "$root/libc/ravenlibc/crt" -newer "$sysroot/usr/lib/libc.a" -print -quit 2>/dev/null)" ]; then
        # The add-on sysroot carries its own libc copy. A stale copy makes the TLS client run
        # against an older fd/stdio/net stack than the shell and kernel; rebuild the add-on.
        stale=1
    fi
    if [ "$stale" -eq 0 ]; then
        return 0
    fi
    log "building the OpenSSL add-on (host clang 14 ordering workaround)"
    wrap=$out/clang-wrap
    mkdir -p "$wrap"
    cat >"$wrap/clang" <<EOF
#!/bin/sh
exec /usr/bin/clang -isystem "$sysroot/usr/include" -Wl,--allow-multiple-definition "\$@"
EOF
    chmod +x "$wrap/clang"
    PATH="$wrap:$PATH" sh addons/tls/build.sh --build >/dev/null
}

build_tls_tools() {
    ensure_addon
    log "generating the T3 test certificates"
    sh tests/tls/certs/make-test-ca.sh >/dev/null

    log "building add-on curl/wget (x86_64-unknown-ravenbsd)"
    tls_target=$out/tls-target
    for tool in curl wget; do
        RUSTUP_TOOLCHAIN=ravenbsd cargo rustc -q -p "$tool" --bin "$tool" --features tls \
            --target x86_64-unknown-ravenbsd --release --target-dir "$tls_target" -- \
            -C relocation-model=static \
            -C link-arg=-no-pie \
            -C link-arg=--no-eh-frame-hdr \
            -C "link-arg=-T$root/tests/libc/target/x86_64.ld" \
            -C "link-arg=$sysroot/usr/lib/crt1.o" \
            -C "link-arg=$sysroot/usr/lib/crti.o" \
            -C "link-arg=-L$addon/lib" \
            -C "link-arg=-L$sysroot/usr/lib" \
            -C link-arg=-lssl \
            -C link-arg=-lcrypto \
            -C link-arg=-lc \
            -C link-arg=-lclang_rt.builtins \
            -C "link-arg=$root/build/tls/tls_compat.o" \
            -C link-arg=--allow-multiple-definition \
            -C "link-arg=$sysroot/usr/lib/crtn.o" \
            -C strip=symbols
        cp "$tls_target/x86_64-unknown-ravenbsd/release/$tool" "$out/$tool-tls"
    done
}

build_initfs() {
    base=$out/base-initfs.img
    if [ ! -f "$base" ]; then
        log "building the release initfs"
        RAVEN_USER_RELEASE=1 cargo xtask initfs --arch x86_64 --profile release >/dev/null
        cp "$root/build/x86_64/initfs.img" "$base"
    fi
    case "$mode" in
        http)
            cargo run -q -p raven-b70-overlay -- \
                "$base" "$out/http-initfs.img" \
                "/bin/curl=$out/curl-http:755" \
                "/bin/wget=$out/wget-http:755" >/dev/null
            ;;
        https)
            cargo run -q -p raven-b70-overlay -- \
                "$base" "$out/https-initfs.img" \
                "/bin/curl=$out/curl-tls:755" \
                "/bin/wget=$out/wget-tls:755" \
                "/tls-ca.pem=$certs/ca.pem:644" \
                "/tls-untrusted.pem=$certs/untrusted-ca.pem:644" >/dev/null
            ;;
        *)
            die "usage: sh tests/fixtures/fetch/build.sh http|installed|https"
            ;;
    esac
}

case "$mode" in
    http)
        build_http_tools
        build_initfs
        start_http_server 700
        log "ready: $out/http-initfs.img on $http_port"
        ;;
    installed)
        start_installed_server 400
        log "ready: fetch-installed server on $installed_port"
        ;;
    https)
        build_tls_tools
        build_initfs
        start_https_server
        log "ready: $out/https-initfs.img on $https_port"
        ;;
    *)
        die "usage: sh tests/fixtures/fetch/build.sh http|installed|https"
        ;;
esac
