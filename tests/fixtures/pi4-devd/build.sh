#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Decode the tracked hexadecimal DTB fixture into the binary the pi4-devd profile installs as
# /etc/pi4.dtb. The qtest runs this from its `pre` list before building the image.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/pi4-devd
mkdir -p "$out"
python3 - "$out" <<'PY'
import sys
from pathlib import Path

source = Path("tests/fixtures/pi4-devd/pi4-devd.dtb.hex")
text = "".join(line for line in source.read_text().splitlines() if not line.startswith("#"))
Path(sys.argv[1], "pi4-devd.dtb").write_bytes(bytes.fromhex(text))
PY
