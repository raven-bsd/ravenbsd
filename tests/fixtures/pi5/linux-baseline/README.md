<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Pi 5 Linux baseline evidence (R10)

**No board baseline is captured in this repository yet.** This directory is the destination for
the text evidence that `scripts/pi5-baseline.sh` collects on a Raspberry Pi 5 running Raspberry Pi
OS. Do not fill it with host data or invented values.

Run on the Pi 5, preferably as root:

```
sudo scripts/pi5-baseline.sh -o tests/fixtures/pi5/linux-baseline/2026-...-<serial>
```

The output directory contains `capture-info.txt`, the command outputs (`lspci-vv.txt`,
`lsusb-v.txt`, `dmesg.txt`, `ethtool-*.txt`, `ip-*.txt`), the device tree, `pci-config.txt`, the
RP1 firmware path/hash files and `SHA256SUMS`. Review serial numbers, MAC addresses and any
site-specific paths before committing. The kernel, GPL source trees and the RP1 firmware blob
must never be copied here; only paths and SHA-256 hashes.

The donor `ravenbsd-resources/linux-rpi/drivers/firmware/rp1-fw.c` is the mailbox protocol driver
(GET_FIRMWARE_VERSION/GET_FEATURE), not a firmware file loader. The script therefore records
`modinfo`, `dmesg`, `rpi-eeprom-*` and hashes any `rp1`-named files it finds under the standard
firmware directories; the real firmware path/version must come from the board capture.

`docs/pi5-linux-baseline.md` records the exact versions and the values R9/R11 still have to
confirm. The script is `sh -n` clean and `--check` reports which host tools are available; there is
no Pi 5 board log in this workspace.
