#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Decode the P8 Pi 4 device-tree fixture into the /etc/pi4.dtb source the
# release image stages for devd. The synthetic tree is the host-test P8 tree,
# not the live firmware DTB; the board path remains UNVERIFIED.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/pi4-release
mkdir -p "$out"
python3 - "$out" <<'PY'
import sys
from pathlib import Path

source = Path("tests/fixtures/pi4-devd/pi4-devd.dtb.hex")
text = "".join(line for line in source.read_text().splitlines() if not line.startswith("#"))
Path(sys.argv[1], "pi4.dtb").write_bytes(bytes.fromhex(text))
PY
echo "pi4-release: $out/pi4.dtb"
