<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Pi 4 SD image and boot evidence kit (J3)

Status: host-side kit only. The image build and manifest check are OBSERVED on the development
host; the board capture is UNVERIFIED. No real Pi 4 log is included or claimed by this document.

This kit makes a Pi 4 boot record repeatable: build the K35 image, verify its FAT boot manifest,
capture the UART with timestamps, and record the commit, image hash and first failure in a fixed
template.

## 1. Prerequisites

* Rust and the repository toolchain (`make doctor`).
* The read-only firmware checkout `ravenbsd-resources/rpi-firmware/boot` (never committed).
* `mtools` (`mtype`) for the manifest check; `stty` and `sha256sum` for capture.
* `picocom` for a clean capture; the script falls back to `cat` and says so.
* A USB-UART adapter wired to the Pi 4 PL011: 115200 8N1, no flow control. The wiring is in
  `docs/pi4.md` section 4; the adapter's VCC is never connected to the Pi.

The firmware search is relative to the clone. If the clone lives on an external disk with the
resources elsewhere, point the build at them:

```
RAVEN_RPI_FIRMWARE=/path/to/ravenbsd-resources/rpi-firmware/boot \
    make image ARCH=aarch64 PLATFORM=pi4
```

## 2. Build and manifest

```
make image ARCH=aarch64 PLATFORM=pi4
sha256sum build/aarch64/ravenbsd-pi4.img
scripts/pi4-serial.sh --check -i build/aarch64/ravenbsd-pi4.img
```

`--check` reads the FAT boot partition with `mtype` and verifies
`tests/fixtures/pi4-image/manifest.txt`:

| File | Requirement |
|---|---|
| `config.txt` | `arm_64bit=1`, `kernel=kernel8.img`, `enable_uart=1`, `uart_2ndstage=1` |
| `kernel8.img` | at least 64 KiB; the first 16 bytes are not zero |
| `bcm2711-rpi-4-b.dtb` | at least 1 KiB; FDT magic `0xd00dfeed` |
| `bootcode.bin` | at least 4 KiB |
| `start4.elf` | at least 1 MiB |
| `fixup4.dat` | at least 1 KiB |
| `LICENCE.broadcom` | present |
| `overlays/disable-bt.dtbo` | at least 512 bytes; FDT magic |

The check does not verify the donor firmware revision or a byte-for-byte image hash; record the
revision from `ravenbsd-resources/donors.lock` and the full image SHA-256 in the evidence block.
The manifest is a staging check: `config.txt` and the kernel/DTB are ours, the firmware files are
the owner's add-on files.

## 3. Capture

Connect the adapter and run:

```
scripts/pi4-serial.sh -d /dev/ttyUSB0 -i build/aarch64/ravenbsd-pi4.img
```

The script:

1. runs the manifest check and fails before opening the device if a boot file is missing;
2. writes a header with the UTC start, host, commit, image path, full image SHA-256, device and
   manifest into `build/pi4/<image-sha256>.log`;
3. configures the port with `stty` (`raw`, 8N1, no CRTSCTS, no echo) and captures with `picocom`
   (or `cat`);
4. prefixes every line with a UTC timestamp and appends to the log;
5. writes a `# stopped:` footer on Ctrl-C or normal exit.

Options: `-l` selects a log path, `-b` the baud rate, `--no-check` skips the manifest step, and
`-c` runs only the manifest check. `sh -n scripts/pi4-serial.sh` checks the syntax without a device.

## 4. Evidence template

Copy this block into the report or the ticket's evidence note and fill every line from the log.
Leave a line as `NOT OBSERVED` rather than guessing.

```text
Pi 4 boot evidence
==================
Date (UTC):
Commit:                       # git rev-parse HEAD
Image:                        # path and size
Image SHA-256:                # sha256sum build/aarch64/ravenbsd-pi4.img
Firmware revision:            # rpi-firmware entry in donors.lock
Board model/revision:         # from the log or the board label
SD card:                      # model and capacity
UART adapter and device:      # e.g. CP2102 /dev/ttyUSB0, 115200 8N1
Log file:                     # build/pi4/<image-sha256>.log

Manifest check:               # PASSED / first failing file
Kernel banner:                # exact first RavenBSD line
Memory map:                   # exact "mem:" line
EMMC2 controller:             # version, capabilities 0x40/0x44, clock
Card initialisation:          # CMD0/CMD8, ACMD41, CID/CSD summary, RCA, bus width
Block path:                   # disk:0 info, GPT partitions, mount, file I/O
First failing line:           # exact log line, or "none"
Result:                       # PASS / FAIL (first failure)
```

## 5. Board steps (UNVERIFIED)

1. Build the image and run the manifest check on the host; record the image SHA-256 and firmware
   revision.
2. Flash the image to the SD card with `dd` (or the owner's usual writer), insert it, connect the
   UART adapter and power the board.
3. Capture the whole boot with `scripts/pi4-serial.sh`; do not edit the log.
4. Record the kernel banner, memory map, EMMC2 controller/card lines, blockd/GPT and root mount.
5. If the boot stops, record the first failing line; do not claim later steps from a partial log.

No step above has been performed for this ticket. The script and manifest check are host-tested;
the board result remains UNVERIFIED until the owner runs them on a Pi 4.

## 6. Relationship to the profile fragment

`profiles/pi4.d/10-emmc2-blockd.toml` installs the EMMC2 and blockd service declarations:
EMMC2 provides `disk:`, blockd consumes `disk:0` and provides `block:`. The `emmc2` program entry
and its MMIO/DMA adapter are the integration step, so the fragment is file-only today and the
existing `pi4` profile still builds. When the driver service entry lands, the fragment gets the
two program entries and the capture template's block-path lines become meaningful.
