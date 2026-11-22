#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K443: run the SIGCHLD/ppoll guest fixture on the base profile.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

pkill -f "qemu-system.*ssh-contract.img" 2>/dev/null || true
pkill -f "qemu-system.*ssh-contract-sigchld.img" 2>/dev/null || true
rm -f build/qtest/ssh-contract.img build/qtest/ssh-contract.vars.fd

timeout 600 cargo xtask qtest --case ssh-contract --arch x86_64
