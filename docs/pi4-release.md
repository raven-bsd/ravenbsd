<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Pi 4 release image and serial gate (REL3)

Status: host image and transcript checker OBSERVED. No Pi 4 board transcript exists; every board
gate below is UNVERIFIED. The release session uses `/bin/rsh`; `/bin/login` and getty provide the
login prompt. brush is staged but cannot spawn a session yet.

## 1. Profile

`profiles/pi4-release.toml` extends `base` (the loader permits one level and `pi4` already extends
`base`) and carries the Pi 4 boot cmdline and EMMC2/blockd staging entries. The release fragments
add:

* shell services and first-party tools;
* the aarch64 brush and uutils `ls` slice;
* `/etc/passwd`, `group`, `shadow`, `profile`, `motd`, `/bin/getty` and `/bin/login`;
* netd in DHCP-first mode (`netd nic dhcp-first`, `needs = scheme:nic`);
* pcid, devd (`dtb launch`, enabled), GENET (`scheme:nic`, enabled) and platd (disabled until
  the platform-handle launch); `/etc/drivers.toml` is the P8 table with the GENET/EMMC2 entries;
* `/etc/pi4.dtb` from the P8 host fixture (`tests/fixtures/pi4-release/build.sh`); the live
  firmware DTB remains a board follow-up.

Build:

```sh
RAVEN_RPI_FIRMWARE=/path/to/rpi-firmware/boot \
    cargo xtask image --arch aarch64 --platform pi4 --profile pi4-release
```

## 2. Host check

```sh
scripts/pi4-release.sh --check
```

It runs `scripts/pi4-serial.sh --check` (firmware/DTB/FAT), verifies the `kernel8.img` banner and
checks the release initfs paths: `/sbin/{vfsd,netd,platd,genet,pcid,devd}`,
`/bin/{rsh,login,getty,brush,ls,ping}`, the account files, `/etc/drivers.toml` and `/etc/pi4.dtb`.
The DTB check compares the FDT magic/totalsize/CRC-32 of the staged blob with the validated
fixture. The manifest linter fails if devd/genet/netd are disabled, devd is not `dtb launch`, the
expected `scheme:dev`/`scheme:plat`/`scheme:nic` needs or provides are missing, the drivers table
lacks the `genet`/`slots=platform` entry, or a duplicate genet launcher is present. It never opens
a serial port and never claims a board result.

## 2.1 Reproducibility

```sh
scripts/pi4-release.sh --repro
```

The repro check removes `build/fixtures`, rebuilds the Pi 4 DTB fixture, builds the release image
twice, compares both SHA-256 values and runs the full `--check` on both. It writes only under
`build/`. A mismatch or a failed check exits nonzero and prints the logs.

## 3. Flash and capture

```sh
# owner only; writes the image to the SD card block device
scripts/pi4-release.sh --flash /dev/sdX --device /dev/ttyUSB0

# or capture an already-flashed image
scripts/pi4-release.sh --device /dev/ttyUSB0

# check an existing transcript
scripts/pi4-release.sh --log /path/to/serial.log
```

Capture uses `scripts/pi4-serial.sh` conventions: 115200 8N1, `console=ttyAMA0`, timestamped log,
image SHA-256 header. The checker requires the boot/kernel banner, login prompt, password prompt,
`raven#`, `ls /bin` with `rsh` and `brush` visible, a ping reply (`bytes from`), and `init exited
(0)`. It fails on panic/EXCEPTION, failed login or ping timeout. Markers live in
`tests/fixtures/pi4-release/gate-markers.txt`; no fixture substitutes for a board log.

## 4. Platform and network dependencies

P8 `devd dtb launch` is enabled and grants the platform BAR/DMA/IRQ handles to the matched
drivers. The release image stages the P8 GENET entry (`bar=0`, `resources=dma+irq`, `serve`) and
enables the GENET service; netd waits for `scheme:nic` and uses DHCP-first addressing (REL4b).
`platd` remains disabled because its mailbox/GPIO/RNG/DMA handles are not in the P8 driver table.

Two known dependencies remain outside REL3:

* **P10 (Kevin)**: the init root-device grant is reserved for `pcid`; devd cannot create the
  platform grants until that policy is split. The image stages the services but the board launch
  is not claimed.
* **REL4c (Phil)**: GENET slot alignment and the live ring/handle layout remain board-tested
  follow-ups.

Without a serial log, the release gate cannot claim link, DHCP, gateway ping or the GENET
`nic:` service. The P8 host fixture DTB is staged as `/etc/pi4.dtb`; the live firmware DTB path is
still a board follow-up.

## 5. Board checklist (UNVERIFIED)

1. Build the image, record its SHA-256 and run `scripts/pi4-release.sh --check`.
2. Flash the SD card and capture the PL011 UART at 115200 8N1.
3. Log in as `root`/`raven`; confirm `raven#`, `ls /bin` and `ping <gateway>`.
4. Run `scripts/pi4-release.sh --log <serial.log>`; attach the PASS/FAIL output, commit and log
   SHA-256.
5. If the boot stops, attach the first failing line; do not claim later gates.

The GENET board values and the netd DHCP/ping transcript remain UNVERIFIED until P8 and a real
board log exist.

## 6. Provenance

No donor code was read or copied for REL3; the profile, scripts, fixtures and this document are
first-party BSD-3-Clause work.
