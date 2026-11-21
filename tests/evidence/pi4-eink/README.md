<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Pi 4 E-Ink evidence directory

Status: **NO BOARD EVIDENCE.** There is no `evidence.md` and no board capture here yet. This
directory is the durable output target for K385/E5.

## Owner procedure

See `docs/releases/0.1.0-eink-evidence.md`. The host checks are:

```sh
timeout 60 sh -n scripts/pi4-eink-evidence.sh
timeout 60 sh scripts/pi4-eink-evidence.sh --check
timeout 60 sh scripts/pi4-eink-evidence.sh --dry-run
```

The board command writes `evidence.md` only when the serial log has a Pi 4 board marker and the
photo, panel, URL, NTP time, refresh interval and image SHA-256 are all present.

## Refusal

A QEMU/virt/synthetic/fixture serial log or photo is refused (exit 2), as is a missing required
field. A host check is not a board result; the owner must sign the PENDING attestation in the
generated evidence file.

```sh
timeout 60 sh scripts/pi4-eink-evidence.sh --serial /dev/null --log /tmp/x.log
# must exit non-zero: empty log / no Pi 4 board marker
```
