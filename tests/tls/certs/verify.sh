#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Verify the T3 test PKI: a trusted chain, an untrusted chain, the IP SAN and
# the 1970-2049 validity window. Every "rejected" case is expected to fail.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=$root/build/tls/certs/test
status=0

# Always regenerate: the fixture is cheap, and a previous failed run must not
# leave stale keys or certificates that mask a config change.
sh tests/tls/certs/make-test-ca.sh >/dev/null

pass() {
    echo "tls-certs: PASS $*"
}

fail() {
    echo "tls-certs: FAIL $*" >&2
    status=1
}

expect_ok() {
    if "$@" >/dev/null 2>&1; then
        pass "$*"
    else
        fail "expected success: $*"
    fi
}

expect_fail() {
    if "$@" >/dev/null 2>&1; then
        fail "expected rejection: $*"
    else
        pass "rejected: $*"
    fi
}

expect_ok openssl verify -CAfile "$out/ca.pem" "$out/server.pem"
expect_ok openssl verify -CAfile "$out/ca.pem" -verify_ip 10.0.2.2 "$out/server.pem"
expect_fail openssl verify -CAfile "$out/ca.pem" -verify_ip 10.0.2.3 "$out/server.pem"
expect_fail openssl verify -CAfile "$out/untrusted-ca.pem" "$out/server.pem"
expect_ok openssl verify -CAfile "$out/untrusted-ca.pem" "$out/untrusted-server.pem"
expect_fail openssl verify -CAfile "$out/ca.pem" "$out/untrusted-server.pem"

dates=$(openssl x509 -in "$out/ca.pem" -noout -startdate -enddate)
echo "$dates" | grep -q '1970' || fail "CA notBefore is not 1970: $dates"
echo "$dates" | grep -q '2049' || fail "CA notAfter is not 2049: $dates"

san=$(openssl x509 -in "$out/server.pem" -noout -ext subjectAltName)
echo "$san" | grep -q 'IP Address:10.0.2.2' || fail "server SAN lacks 10.0.2.2: $san"

count=$(grep -c -- '-----BEGIN CERTIFICATE-----' "$out/ca.pem" || true)
[ "$count" -eq 1 ] || fail "CA bundle has $count certificates, expected 1"
if grep -q -- '-----BEGIN .*PRIVATE KEY-----' "$out/ca.pem"; then
    fail "CA bundle contains a private key"
fi

if [ "$status" -ne 0 ]; then
    echo "tls-certs: FAILED" >&2
    exit 1
fi
echo "tls-certs: PASSED"
