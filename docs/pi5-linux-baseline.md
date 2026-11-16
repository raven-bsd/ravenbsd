<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Pi 5 Linux baseline (R10)

Status: **host-side evidence kit only.** No Raspberry Pi 5 board log is captured in this
repository. The exact board, Raspberry Pi OS and RP1 firmware versions are therefore UNVERIFIED;
the capture script fills them in on the board. R10 is the oracle for R9/R11 and makes no claim
that the values below have been observed on silicon.

## 1. Why

R1 modelled the BCM2712 PCIe root complex and RP1; R3/R4 modelled the RP1 Ethernet and xHCI
functions. The real Linux baseline is still missing: PCIe topology, RP1 identity/BARs, Ethernet
PHY/MAC, xHCI, SD and the RP1 firmware path/version are unverified. Linux on the Pi 5 is the
behavioural oracle. R10 records it without copying GPL source or the firmware blob.

## 2. What `scripts/pi5-baseline.sh` captures

Run it on the Pi 5 under Raspberry Pi OS, preferably as root:

```
sudo scripts/pi5-baseline.sh -o tests/fixtures/pi5/linux-baseline/<stamp>-<serial>
```

Each command has its own text file with the command line and exit status:

| File | Content |
|---|---|
| `capture-info.txt` | UTC start, host, uid, privilege, commit, output path. |
| `uname.txt`, `os-release.txt`, `cmdline.txt`, `cpuinfo.txt`, `meminfo.txt` | Board/OS identity and boot parameters. |
| `lspci-*.txt` | `lspci -t`, `-nn`, `-vv`, `-xxx`; config-space register dump bounded to the standard 256 bytes per function. |
| `lsusb*.txt` | Tree and verbose USB descriptors (xHCI devices). |
| `dmesg.txt`, `dmesg-filtered.txt` | Full kernel log plus the RP1/PCIe/PHY/xHCI/SD/firmware subset. |
| `ip-*.txt` | Addresses, links, routes and neighbours. |
| `ethtool-*.txt` | Per-interface driver, features, stats and link settings. |
| `device-tree.txt` | `dtc -I fs -O dts /proc/device-tree`, or a bounded property hex dump when `dtc` is absent. |
| `pci-config.txt` | First 256 bytes of each `/sys/bus/pci/devices/*/config`. |
| `rp1-firmware-files.txt`, `rp1-firmware-hashes.txt` | Every `*rp1*` file found under `/lib/firmware`, `/usr/lib/firmware`, `/boot/firmware` and `/boot`, with SHA-256 hashes. Paths/hashes only; no blob. |
| `modinfo-rp1-fw*.txt` | The `rp1-fw` module metadata and module path. |
| `vcgencmd-*`, `rpi-eeprom-*` | Bootloader/EEPROM/clock/temperature evidence when the tools exist. |
| `mmc-extcsd.txt` | SD card EXT_CSD when `mmc-utils` and `/dev/mmcblk0` exist. |
| `mmio-*.txt` | Optional one-word `devmem` reads from explicit `--mmio address:width` arguments; no address is guessed. |
| `SHA256SUMS` | Hashes over the captured text files. |

`--check` lists which tools are available. Missing optional tools are recorded as a failed command,
not a silent skip. The script must be run as root (or with passwordless sudo) for complete
`dmesg`/`lspci`/`ethtool` evidence; the header records the privilege mode.

## 3. The RP1 firmware path/hash

The ticket named
`ravenbsd-resources/linux-rpi/drivers/firmware/rp1-fw.c` (donor revision
`ab4372076bb840600e522ae776887ae2bc488fa6`, GPL-2.0, tier C; read only). That file is the
**mailbox protocol driver**, not a firmware-file loader:

* it binds `raspberrypi,rp1-firmware` with `mboxes`/`shmem`, allocates the shared buffer and sends
  `GET_FIRMWARE_VERSION` / `GET_FEATURE` messages;
* `GET_FIRMWARE_VERSION` returns five 32-bit words (`160-bit version`), logged by Linux as
  `RP1 Firmware version %08x...`;
* it contains no firmware filename or filesystem load path.

The script therefore captures the firmware **identity** from `dmesg`/the mailbox version line and
searches the standard firmware directories for `*rp1*` files, recording paths and SHA-256 hashes.
The actual RP1 firmware path/version must be filled from the board capture; no blob is copied.

The RP1 MFD donor `drivers/mfd/rp1.c` (same revision) records the PCI identity
`PCI_VENDOR_ID_RPI = 0x1de4`, `PCI_DEVICE_ID_RP1_C0 = 0x0001`, `PCI_DEVICE_REV_RP1_C0 = 2` and the
BAR dump format. These are donor facts to compare with the captured `lspci -nn` output, not
silicon evidence. The mailbox/shmem nodes are at RP1-local `0x40178000` (PIO) and
`0x4040ff00` (firmware shmem) in the donor DTS; R1 already records the RP1 window translation.

## 4. Exact versions to fill from the board

Do not substitute a host or a VM. Fill every row from the captured files:

| Field | Value | Captured from |
|---|---|---|
| Board model/revision | UNVERIFIED | `/proc/device-tree/model`, board label |
| Board serial | UNVERIFIED | `/proc/device-tree/serial-number`, `capture-info.txt` |
| Raspberry Pi OS release | UNVERIFIED | `/etc/os-release` |
| Kernel | UNVERIFIED | `uname -a` |
| Bootloader/EEPROM | UNVERIFIED | `vcgencmd bootloader_version`, `rpi-eeprom-update` |
| RP1 firmware version | UNVERIFIED | `dmesg` `RP1 Firmware version ...` |
| RP1 firmware path/SHA-256 | UNVERIFIED | `rp1-firmware-files.txt`, `rp1-firmware-hashes.txt` |
| RP1 PCI ID/revision | UNVERIFIED | `lspci -nn`, `lspci -xxx` |
| Device tree | UNVERIFIED | `device-tree.txt` |
| Linux donor tree revision | `ab4372076bb840600e522ae776887ae2bc488fa6` | `donors.lock` (not the board OS) |

## 5. Baseline summary to fill from the board

| Area | What to record |
|---|---|
| PCIe | Controller, link speed/width, bus number, RP1 BDF, BAR start/size/flags, MSI/MSI-X. |
| RP1 | Vendor/device/revision, `bar0..n` start/end, chip ID/platform registers if readable. |
| Ethernet | Interface name, driver `macb`, GEM window, interrupt, four clock rates, PHY ID/address, `phy-mode`, link speed/duplex, MAC, RGMII delays, ring sizes/DMA mask. |
| xHCI | Two USB controllers, MMIO windows, interrupts, root-hub ports, attached devices. |
| SD | `dwcmshc` window, `RP1_INT_SDIO0`, clock rates, card CID/CSD/EXT_CSD, bus width/speed. |
| UART | Which RP1 UART is enabled, its window/clock/IRQ, and whether it can be a console. |
| Firmware | RP1 mailbox/shmem path, firmware version, any `rp1` firmware file hash. |

## 6. Values R9/R11 must confirm

R9 (RP1 Ethernet macb/GEM) must confirm from this baseline:

1. RP1 PCI identity/BAR and the Ethernet window at RP1-local `0xc0_4010_0000` (`0x4000`).
2. RP1 interrupt 6 mapping through the MFD/MSI domain and the level/edge setting.
3. `pclk`/`hclk`/`tsu_clk`/`tx_clk` rates 12/12/29/16 and the PHY `rgmii-id` delays.
4. PHY address 1 identity, BMSR/BMCR behaviour, reset GPIO 32/power-down.
5. GEM register offsets, descriptor 32/64-bit encoding, ring sizes, DMA mask and coalescing.
6. MAC address source (the DTB placeholder is all zero) and the filter/hash registers.

R11 (RP1 SD/UART) must confirm:

1. The `rp1_mmc0` window at RP1-local `0x40180000`, `RP1_INT_SDIO0` and the `bus`/`core`/
   `timeout`/`sdio` clocks.
2. SDHCI version/capabilities, ADMA/DMA, card init, bus width/speed and the blockd path.
3. Which RP1 UART is enabled, its window/clock/IRQ and whether it is usable as a console.
4. The RP1 firmware mailbox/shmem behaviour and the firmware version/load path above.

## 7. Storage and provenance

* Store the capture under `tests/fixtures/pi5/linux-baseline/<stamp>-<serial>/`; commit the text
  files and `SHA256SUMS`, not the kernel, GPL source or firmware blob.
* Review serial numbers, MAC addresses and site-specific paths before committing. The directory is
  evidence, not a place for secrets.
* The donor files read for behaviour are listed in section 8. No GPL code was copied, translated or
  closely paraphrased; the RP1 blob remains outside the repository.
* The script is `sh -n` clean and `--check` reports the available tools. The host smoke run used a
  temporary output directory and is not committed.

## 8. Donors read

| Source | Revision | Licence | Use |
|---|---|---|---|
| `linux-rpi/drivers/firmware/rp1-fw.c` | `ab4372076bb840600e522ae776887ae2bc488fa6` | GPL-2.0 (tier C) | Mailbox ops, `GET_FIRMWARE_VERSION`/`GET_FEATURE`, shmem/mailbox binding, no file path. Read only. |
| `linux-rpi/drivers/mfd/rp1.c` | same | GPL-2.0 (tier C) | RP1 PCI IDs `0x1de4:0x0001`/rev 2, BAR dump, MSI/MSI-X and interrupt-domain shape. Read only. |
| `linux-rpi/arch/arm64/boot/dts/broadcom/rp1.dtsi` | same | GPL-2.0 (tier C) | RP1 firmware/mailbox/shmem, PIO and `rp1_mmc0` node values. Read only. |
| R1/R3/R4 docs and reports | project | BSD-3-Clause | The model boundaries and the values R9/R11 still need. |

No donor code or blob is copied into this repository. No `PROVENANCE.toml` import entry is added.

## 9. Board steps (UNVERIFIED)

1. Flash Raspberry Pi OS (record the exact release) and boot the Pi 5 with the UART adapter.
2. Run `sudo scripts/pi5-baseline.sh -o tests/fixtures/pi5/linux-baseline/<stamp>-<serial>`.
3. Check `SHA256SUMS`, fill the exact-version table and the baseline summary, and commit the text
   capture.
4. Re-run R9/R11 host tests with the observed register/clock/PHY values; do not claim a driver
   result until the board log exists.
5. If a capture command needs a privileged tool that is absent, record it as a gap instead of
   substituting a guessed value.

None of steps 1-5 has been run for R10. There is no Pi 5 board log, RP1 firmware hash or Linux
baseline in this workspace.
