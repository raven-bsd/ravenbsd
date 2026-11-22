#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K498: build the release initfs and overlay it with a DEBUG3 sshd service, the
# debug log path and the SSH_ASKPASS helper. Then wrap the overlay initfs in the
# release image. The release userland and the upstream sshd binary are unchanged;
# only the test initfs differs. The qtest boots this image and drives one guest
# loopback password login plus one command.

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

out=build/fixtures/sshd-trace
mkdir -p "$out"
cargo build -q -p raven-b70-overlay
overlay=${CARGO_TARGET_DIR:-target}/debug/raven-b70-overlay
"$overlay" build/x86_64/initfs.img "$out/initfs.img" \
    "/etc/ssh/sshd_config=$root/tests/fixtures/sshd-trace/sshd_config:0644" \
    "/etc/services.d/sshd.toml=$root/tests/fixtures/sshd-trace/sshd.service.toml:0644" \
    "/etc/ssh/askpass.sh=$root/tests/fixtures/sshd-trace/askpass.sh:0755" \
    "/etc/ssh/trace-client.sh=$root/tests/fixtures/sshd-trace/trace-client.sh:0755"

image=$out/sshd-trace.img
if [ ! -f "$image" ] || [ "$out/initfs.img" -nt "$image" ]; then
    cargo xtask image --arch x86_64 --profile release \
        --initfs "$out/initfs.img" --output "$image"
fi

echo "sshd-trace: image $image"
