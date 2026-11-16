#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Start the QMP key helper before QEMU. The helper belongs to its own session so it survives the
# pre-command shell; it waits for the QMP socket the qtest creates, then injects the keys.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/keyboard
mkdir -p "$out"
rm -f "$out/qmp.sock" "$out/keys.log"

setsid python3 tests/fixtures/keyboard/send-keys.py "$root/$out/qmp.sock" \
    </dev/null >"$out/keys.log" 2>&1 &
