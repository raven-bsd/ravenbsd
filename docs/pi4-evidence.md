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

## 7. Board evidence runbook (C74am)

Status: the host image, manifest/FDT check, no-TEXTREL dynamic `/bin` and the QEMU-virt
`dynamic-userland` qtest are OBSERVED. No Pi 4 board serial/GENET log exists, the USB-TTL
adapter is still missing, and this host's QEMU has no `raspi4b` machine. The board gates in this
runbook are UNVERIFIED until the owner captures a log.

### 7.1 Host preflight

Build the release image and record the exact artifact:

```sh
RAVEN_RPI_FIRMWARE=/path/to/ravenbsd-resources/rpi-firmware/boot \
    cargo xtask image --arch aarch64 --platform pi4 --profile pi4-release
sha256sum build/aarch64/ravenbsd-pi4.img
sh scripts/pi4-release.sh --check
cargo xtask qtest --case dynamic-userland --arch aarch64
```

On master `d97f7c1a` the observed image was 335544320 bytes with SHA-256
`0a3e917d45e80d38c2c2e842639bca77a38a6457c86bfd4b03a254d7cf8c64d4`; the manifest/initfs/FDT
check passed, the staged dynamic `/bin` had no `DT_TEXTREL`/`DF_TEXTREL`, and
`dynamic-userland --arch aarch64` passed on QEMU virt. These are host/QEMU results, not a board
result. Before flashing, confirm the image really stages `/bin/ping` (the release check matches
the path string; a missing file is a preflight failure, not a ping-gate result).

### 7.2 Flash and capture

The owner writes the SD card and captures the PL011 UART; VCC from the adapter is never
connected. The adapter is a 3.3 V USB-UART on GPIO14/15 with GND, 115200 8N1, no flow control.

```sh
# owner only; writes the image to the SD card
scripts/pi4-release.sh --flash /dev/sdX --device /dev/ttyUSB0

# or capture an already-flashed board
scripts/pi4-serial.sh -d /dev/ttyUSB0 -i build/aarch64/ravenbsd-pi4.img
```

Use the log path printed by the capture script (`build/pi4/<image-sha256>.log`). Its header
records the UTC start, host, commit, image path, image SHA-256, device/baud and manifest check.
Do not edit the captured log. Stop with Ctrl-C and keep the `# stopped:` footer.

### 7.3 Expected board transcript

Log in as `root`/`raven`; the release shell prompt is `raven#`. Capture these gates exactly:

```text
login:
Password:
raven# /bin/uname -a
RavenBSD pi4 0.1 ... aarch64
raven# dmesg | grep -i genet
genet: link is up ...
netd: DHCP lease ... gateway ...
raven# ping -c 3 <gateway>
64 bytes from <gateway>: icmp_seq=1 ttl=64 ...
init exited (0)
```

The release image does not guarantee a staged `readelf`/`ldd`. For the dynamic gate, run the
host check on the same artifact and append its output to the capture header:

```sh
llvm-readelf -d build/aarch64/user/raven-rsh | grep NEEDED
# append: # host dynamic: 0x... (NEEDED) Shared library: [libc.so.1]
```

If the board has a staged tool, include its `/bin/rsh` output too; otherwise record
`board-side readelf: NOT OBSERVED` rather than guessing. The image SHA-256 in the header binds
the host dynamic line to the flashed image.

### 7.4 Transcript checker

```sh
scripts/board-pi4-check.sh --log build/pi4/<image-sha256>.log
scripts/board-pi4-check.sh --log build/pi4/<image-sha256>.log --dry-run
```

Exit 0 means the transcript markers are complete. Exit 1 means a required marker is missing.
Exit 2 means the log was refused: a QEMU/tcg/emulator marker, a header marking the log
virt/virtual/synthetic/fixture/sample, or no Pi 4 board marker (`bcm2711`/`raspi4b`/`Raspberry
Pi 4`). The checker never opens a device and never prints a board PASS; the owner's attestation
and the hashes remain part of the release packet. `tests/board/pi4-check-selftest.sh` exercises
the complete, virt, synthetic, no-board and missing-ping cases.

### 7.5 QEMU virt evidence and the raspi4b gap

`dynamic-userland --arch aarch64` runs on QEMU virt with the release `libc.so.1`; it proves the
aarch64 dynamic userland shape, not a Pi 4 board boot. This host's QEMU has no `raspi4b`
machine, so a literal raspi4b disk boot cannot be produced here. Do not present the QEMU serial
as a board log; the checker refuses a QEMU/virt header.

### 7.6 Release packet fields

| Field | Source |
|---|---|
| Commit | `git rev-parse HEAD` |
| Image + SHA-256 | `build/aarch64/ravenbsd-pi4.img`, `sha256sum` |
| Log + SHA-256 | `build/pi4/<image-sha256>.log`, `sha256sum` |
| Adapter/device/baud | capture header (`# device:`) |
| Board model/revision | serial log or board label (`bcm2711`/`raspi4b`) |
| Firmware revision | `rpi-firmware` entry in `donors.lock` |
| Gate results | login, uname, dynamic NEEDED, GENET/DHCP, ping, clean exit |
| First failing line | exact log line, or `none` |
| Result | owner-attested PASS/FAIL; the checker only reports complete/incomplete/refused |

### 7.7 Blocker

The USB-TTL adapter is still missing, so no board transcript, GENET link/DHCP lease or ping
reply exists. The board result remains UNVERIFIED; this runbook and checker are host-side
evidence for the run that the owner performs when the adapter arrives.
