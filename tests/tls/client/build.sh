#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the T2 target TLS client and pack a server-profile initfs overlay:
# the client binary, its CA bundle, the one-shot service manifest, the test
# `/etc/svcd.conf` and a `/dev/urandom` mount backed by `rand:` so OpenSSL's
# DEVRANDOM seeding path works on the server profile.
#
# Usage: sh tests/tls/client/build.sh

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

arch=${RAVEN_QT_ARCH:-x86_64}
if [ "$arch" != x86_64 ]; then
    echo "tls-client: only x86_64 is in T2" >&2
    exit 2
fi

out=$root/build/tls/client
addon=$root/build/tls/$arch
sysroot=$addon/sysroot
cargo_dir=$out/cargo
pki=$out/pki

log() {
    echo "tls-client: $*"
}

die() {
    echo "tls-client: $*" >&2
    exit 2
}

[ -f "$root/addons/tls/build.sh" ] || die "missing addons/tls/build.sh"
if [ ! -f "$addon/lib/libssl.a" ] || [ ! -f "$addon/lib/libcrypto.a" ] ||
    [ ! -f "$sysroot/usr/lib/libc.a" ] || [ ! -f "$root/build/tls/tls_compat.o" ]; then
    log "building the OpenSSL add-on first"
    sh addons/tls/build.sh --build
fi
command -v openssl >/dev/null 2>&1 || die "host openssl is required"
command -v cargo >/dev/null 2>&1 || die "cargo is required"

mkdir -p "$out" "$cargo_dir"

# The guest realtime clock is zero until the RTC ticket lands, so the test PKI starts at 1970.
if [ ! -f "$pki/ca.pem" ] || [ ! -f "$pki/server.pem" ] || [ ! -f "$pki/server.key" ]; then
    rm -rf "$pki"
    mkdir -p "$pki/newcerts"
    : > "$pki/index.txt"
    printf '01\n' > "$pki/serial"
    log "generating the test CA and server certificate"
    openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 \
        -out "$pki/ca.key" 2>/dev/null
    openssl req -new -key "$pki/ca.key" -config tests/tls/client/tls-client.cnf \
        -subj "/CN=RavenBSD TLS test CA" -out "$pki/ca.csr"
    openssl ca -batch -selfsign -notext -config tests/tls/client/tls-client.cnf \
        -extensions ca_ext -startdate 700101000000Z -enddate 491231235959Z \
        -in "$pki/ca.csr" -keyfile "$pki/ca.key" -out "$pki/ca.pem"
    openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 \
        -out "$pki/server.key" 2>/dev/null
    openssl req -new -key "$pki/server.key" -config tests/tls/client/tls-client.cnf \
        -subj "/CN=10.0.2.2" -out "$pki/server.csr"
    openssl ca -batch -notext -config tests/tls/client/tls-client.cnf \
        -extensions server_ext -startdate 700101000000Z -enddate 491231235959Z \
        -in "$pki/server.csr" -out "$pki/server.pem"
fi

log "building raven-tls-client for x86_64-unknown-ravenbsd"
RUSTUP_TOOLCHAIN=ravenbsd cargo rustc -q -p raven-tls-client \
    --target x86_64-unknown-ravenbsd --release --target-dir "$cargo_dir" -- \
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
    -C "link-arg=$sysroot/usr/lib/crtn.o" \
    -C strip=symbols

binary=$cargo_dir/x86_64-unknown-ravenbsd/release/raven-tls-client
[ -f "$binary" ] || die "cargo did not produce $binary"

log "packing the server-profile initfs"
RAVEN_USER_RELEASE=1 cargo xtask initfs --arch "$arch" --profile server >/dev/null
cargo run -q -p raven-b70-overlay -- \
    "$root/build/$arch/initfs.img" \
    "$out/tls-client-initfs.img" \
    "/sbin/tls-client=$binary:755" \
    "/tls-ca.pem=$pki/ca.pem:644" \
    "/etc/services.d/90-tls-client.toml=$root/tests/tls/client/tls-client.d/90-tls-client.toml:644" \
    "/etc/svcd.conf=$root/tests/tls/client/tls-client.d/svcd.conf:644" \
    "/etc/mounts=$root/tests/tls/client/tls-client.d/mounts:644" >/dev/null
log "built: $out/tls-client-initfs.img"
