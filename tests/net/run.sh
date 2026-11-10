#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Run every packetdrill-subset script against raven-netcore.
# Exit status is 0 only when every script passes or is a listed known-fail.
set -eu

cd "$(dirname "$0")/../.."

# Optional external oracle: when PACKETDRILL_BIN is set, let packetdrill's own parser
# validate the scripts first. It stays a separate GPL tool and is never linked or copied.
if [ -n "${PACKETDRILL_BIN:-}" ]; then
    sh tests/net/check-syntax.sh
fi

exec cargo run --quiet -p raven-netcore-packetdrill-harness -- "$@"
