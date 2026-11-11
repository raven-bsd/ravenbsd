#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the loader test kernel and copy it to build/fixtures/testkernel/kernel.elf.
# This fixture is a kernel (not an initfs); the loader-testkernel case installs it with
# `loader_kernel`. Flags follow tickets/stuart/reports/S02.md.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/testkernel
mkdir -p "$out"
RUSTFLAGS="-C relocation-model=static" \
    cargo build -p raven-testkernel --target x86_64-unknown-none --release
cp target/x86_64-unknown-none/release/raven-testkernel "$out/kernel.elf"
