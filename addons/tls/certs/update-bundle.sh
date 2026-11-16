#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Stage or install a PEM CA bundle for the OpenSSL add-on. The bundle is never
# committed: the image builder takes it from the host's ca-certificates package.
#
# Usage:
#   sh addons/tls/certs/update-bundle.sh --check
#   sh addons/tls/certs/update-bundle.sh --stage [--source <bundle.pem>]
#   sh addons/tls/certs/update-bundle.sh --install <image-root> [--source <bundle.pem>]
#
# The installed path is <image-root>/addons/etc/ssl/certs/ca-bundle.pem; the
# staged copy is build/tls/certs/ca-bundle.pem.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

mode=check
source_bundle=${TLS_CA_BUNDLE:-}
install_root=

usage() {
    echo "usage: $0 [--check|--stage|--install <root>] [--source <bundle.pem>]" >&2
}

die() {
    echo "tls-certs: $*" >&2
    exit 2
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --check) mode=check ;;
        --stage) mode=stage ;;
        --install)
            shift
            [ "$#" -ge 1 ] || die "--install needs an image root"
            install_root=$1
            mode=install
            ;;
        --source)
            shift
            [ "$#" -ge 1 ] || die "--source needs a bundle path"
            source_bundle=$1
            ;;
        -h | --help)
            usage
            exit 0
            ;;
        *) die "unknown argument $1" ;;
    esac
    shift
done

if [ -z "$source_bundle" ]; then
    for candidate in \
        /etc/ssl/certs/ca-certificates.crt \
        /etc/pki/tls/certs/ca-bundle.crt \
        /etc/ssl/certs/ca-bundle.crt \
        /etc/ssl/cert.pem; do
        if [ -f "$candidate" ]; then
            source_bundle=$candidate
            break
        fi
    done
fi
[ -n "$source_bundle" ] || die "no host CA bundle found; pass --source"
[ -f "$source_bundle" ] || die "CA bundle not found: $source_bundle"
command -v openssl >/dev/null 2>&1 || die "openssl is required"

count=$(grep -c -- '-----BEGIN CERTIFICATE-----' "$source_bundle" || true)
[ "$count" -gt 0 ] || die "no PEM certificates in $source_bundle"
if grep -q -- '-----BEGIN .*PRIVATE KEY-----' "$source_bundle"; then
    die "refusing a bundle that contains a private key"
fi

if command -v sha256sum >/dev/null 2>&1; then
    digest=$(sha256sum "$source_bundle" | cut -d' ' -f1)
else
    digest=$(openssl dgst -sha256 "$source_bundle" | cut -d= -f2 | tr -d ' ')
fi
subject=$(openssl x509 -in "$source_bundle" -noout -subject 2>/dev/null | head -n 1 || true)
echo "tls-certs: bundle=$source_bundle certificates=$count sha256=$digest"
[ -n "$subject" ] && echo "tls-certs: first=$subject"

stage() {
    destination=$root/build/tls/certs/ca-bundle.pem
    mkdir -p "$(dirname "$destination")"
    cp "$source_bundle" "$destination"
    chmod 0644 "$destination"
    echo "tls-certs: staged $destination"
}

case "$mode" in
    check) ;;
    stage) stage ;;
    install)
        [ -n "$install_root" ] || die "--install needs an image root"
        destination=$install_root/addons/etc/ssl/certs/ca-bundle.pem
        mkdir -p "$(dirname "$destination")"
        cp "$source_bundle" "$destination"
        chmod 0644 "$destination"
        echo "tls-certs: installed $destination"
        ;;
esac
