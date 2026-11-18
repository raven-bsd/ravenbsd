<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Board evidence checker tests

`scripts/board-pi4-check.sh` validates a captured Pi 4 serial log for the release
evidence packet. It never opens a serial device and never claims a board PASS.

Required capture header (written by `scripts/pi4-serial.sh`):

* `# image sha256: <64 hex>`
* `# commit: <7-40 hex>`
* `# device: /dev/...`

Required transcript markers:

| Gate | Marker |
|---|---|
| login | `login:` and `password:` |
| prompt | `raven#` |
| uname | `uname` output containing `RavenBSD` and `aarch64` |
| dynamic | `NEEDED` and `libc.so.1` |
| GENET | `genet` and `link ... up` |
| DHCP | `dhcp` |
| ping | `bytes from` |
| clean exit | `init exited (0)` |
| board | `bcm2711`, `raspi4b` or `Raspberry Pi 4` |

The checker refuses (`exit 2`) any log with a QEMU/tcg/emulator marker, a header
marking it virt/virtual/synthetic/fixture/sample, or no Pi 4 board marker. A
missing transcript marker is `exit 1` (incomplete). `exit 0` means the transcript
shape is complete; the release result still needs the owner's attestation and the
image/log hashes.

`pi4-check-selftest.sh` exercises the complete, virt, synthetic, no-board,
missing-ping, dry-run and help paths with temporary logs:

```sh
sh tests/board/pi4-check-selftest.sh
```

The self-test is a checker test, not a board result.
