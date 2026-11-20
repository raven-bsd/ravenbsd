#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the K199 fetch gate:
#   sh tests/fixtures/fetch/build.sh http
#   sh tests/fixtures/fetch/build.sh https
#
# `http` builds the base curl/wget (raven-http + raven-http-net) and starts the multi-request
# local HTTP server. `https` builds the add-on curl/wget (raven-tls + OpenSSL), stages the T3 test
# CA, and starts a host `openssl s_server -WWW`. The servers are one-shot test processes with a
# bounded lifetime; they are not product services.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

mode=${1:-http}
out=$root/build/fixtures/fetch
certs=$root/build/tls/certs/test
addon=$root/build/tls/x86_64
sysroot=$addon/sysroot
target_dir=${CARGO_TARGET_DIR:-target}

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

mkdir -p "$out" "$out/https-root"

stop_server() {
    pidfile=$1
    if [ -f "$pidfile" ]; then
        kill "$(cat "$pidfile")" 2>/dev/null || true
        rm -f "$pidfile"
    fi
}

stop_server "$out/http-server.pid"
stop_server "$out/https-server.pid"

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
            die "usage: sh tests/fixtures/fetch/build.sh http|https"
            ;;
    esac
}

start_http_server() {
    log "starting the local HTTP server on 18081"
    rm -f "$out/http-server.log"
    timeout 240 python3 "$root/tests/fixtures/fetch/http_server.py" \
        --port 18081 --seconds 180 >"$out/http-server.log" 2>&1 &
    echo $! >"$out/http-server.pid"
    sleep 0.3
    if ! kill -0 "$(cat "$out/http-server.pid")" 2>/dev/null; then
        cat "$out/http-server.log" >&2 || true
        die "HTTP server failed to start"
    fi
}

start_https_server() {
    log "starting the local HTTPS server on 24443"
    rm -f "$out/https-server.log"
    timeout 240 python3 "$root/tests/fixtures/fetch/tls_server.py" \
        --port 24443 --seconds 180 \
        --cert "$certs/server.pem" --key "$certs/server.key" \
        >"$out/https-server.log" 2>&1 &
    echo $! >"$out/https-server.pid"
    sleep 1
    if ! kill -0 "$(cat "$out/https-server.pid")" 2>/dev/null; then
        cat "$out/https-server.log" >&2 || true
        die "HTTPS server failed to start"
    fi
}

case "$mode" in
    http)
        build_http_tools
        build_initfs
        start_http_server
        log "ready: $out/http-initfs.img"
        ;;
    https)
        build_tls_tools
        build_initfs
        start_https_server
        log "ready: $out/https-initfs.img"
        ;;
    *)
        die "usage: sh tests/fixtures/fetch/build.sh http|https"
        ;;
esac
