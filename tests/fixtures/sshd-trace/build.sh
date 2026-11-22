#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K498: overlay the release initfs with a DEBUG3 sshd service and a debug log path.
# The release userland and the upstream sshd binary are unchanged. The qtest boots
# this initfs and drives one password login plus one command over guest loopback.

set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

for file in build/ssh/x86_64/bin/sshd build/ssh/x86_64/bin/ssh \
    build/ssh/x86_64/libexec/sshd-session build/ssh/x86_64/libexec/sshd-auth; do
    if [ ! -x "$file" ]; then
        echo "sshd-trace: missing $file; run sh addons/ssh/build.sh" >&2
        exit 2
    fi
done

if [ ! -f build/x86_64/initfs.img ]; then
    echo "sshd-trace: building the release initfs" >&2
    cargo xtask initfs --arch x86_64 --profile release
fi

cargo build -q -p raven-b70-overlay
overlay=${CARGO_TARGET_DIR:-target}/debug/raven-b70-overlay
out=build/fixtures/sshd-trace
mkdir -p "$out"
"$overlay" build/x86_64/initfs.img "$out/initfs.img" \
    "/etc/ssh/sshd_config=$root/tests/fixtures/sshd-trace/sshd_config:0644" \
    "/etc/services.d/sshd.toml=$root/tests/fixtures/sshd-trace/sshd.service.toml:0644"

echo "sshd-trace: initfs $out/initfs.img"
