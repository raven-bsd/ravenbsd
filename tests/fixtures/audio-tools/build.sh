#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Generate the B45 tone WAV on the host; the file is installed into the qtest image.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

mkdir -p build/fixtures/audio-tools
cargo run -q -p audio-tools-gen -- build/fixtures/audio-tools/tone440.wav
