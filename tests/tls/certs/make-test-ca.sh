#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Generate the T3 test PKI under build/tls/certs/:
#   ca.pem / ca.key                     trusted test CA (1970-2049)
#   server.pem / server.key              server cert for IP SAN 10.0.2.2 (1970-2049)
#   untrusted-ca.pem / untrusted-ca.key  unrelated CA for the negative chain test
#   untrusted-server.pem / .key          server cert under the untrusted CA
#
# The generated files are test fixtures and are never committed. Run verify.sh
# for the positive and negative verification checks.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=$root/build/tls/certs/test
pki=$out/pki

command -v openssl >/dev/null 2>&1 || {
    echo "tls-certs: openssl is required" >&2
    exit 2
}

rm -rf "$out"
mkdir -p "$pki/newcerts"
: > "$pki/index.txt"
printf '01\n' > "$pki/serial"

echo "tls-certs: generating the trusted test CA"
openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 \
    -out "$pki/ca.key" 2>/dev/null
openssl req -new -key "$pki/ca.key" -config tests/tls/certs/tls-certs.cnf \
    -subj "/CN=RavenBSD T3 test CA" -out "$pki/ca.csr"
openssl ca -batch -selfsign -notext -config tests/tls/certs/tls-certs.cnf \
    -extensions ca_ext -startdate 700101000000Z -enddate 491231235959Z \
    -in "$pki/ca.csr" -keyfile "$pki/ca.key" -out "$pki/ca.pem"
cp "$pki/ca.pem" "$out/ca.pem"

echo "tls-certs: generating the trusted server certificate"
openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 \
    -out "$out/server.key" 2>/dev/null
openssl req -new -key "$out/server.key" -config tests/tls/certs/tls-certs.cnf \
    -subj "/CN=10.0.2.2" -out "$out/server.csr"
openssl ca -batch -notext -config tests/tls/certs/tls-certs.cnf \
    -cert "$out/ca.pem" -keyfile "$pki/ca.key" -extensions server_ext \
    -startdate 700101000000Z -enddate 491231235959Z \
    -in "$out/server.csr" -out "$out/server.pem"

echo "tls-certs: generating the untrusted CA and server certificate"
openssl req -x509 -newkey rsa:2048 -noenc -days 2 \
    -subj "/CN=RavenBSD T3 untrusted CA" \
    -keyout "$out/untrusted-ca.key" -out "$out/untrusted-ca.pem" 2>/dev/null
openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 \
    -out "$out/untrusted-server.key" 2>/dev/null
openssl req -new -key "$out/untrusted-server.key" \
    -subj "/CN=10.0.2.2" -out "$out/untrusted-server.csr"
openssl x509 -req -in "$out/untrusted-server.csr" \
    -CA "$out/untrusted-ca.pem" -CAkey "$out/untrusted-ca.key" -CAcreateserial \
    -days 2 -extfile tests/tls/certs/server.ext -extensions server_ext \
    -out "$out/untrusted-server.pem" 2>/dev/null

echo "tls-certs: $out/ca.pem"
echo "tls-certs: $out/server.pem"
echo "tls-certs: $out/untrusted-ca.pem"
echo "tls-certs: $out/untrusted-server.pem"
