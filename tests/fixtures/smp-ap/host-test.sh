#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build and run the host test for the x86_64 per-AP start protocol. The production module is
# included by path, so this exercises the exact sequence the kernel sends.
set -eu

here=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
root=$(CDPATH= cd -- "$here/../../.." && pwd)
out=${1:-"$root/build/qtest/smp-ap-sequence-test"}

mkdir -p "$(dirname "$out")"
rustc --edition 2024 --test -o "$out" "$here/sequence_test.rs"
"$out" --test-threads=1
