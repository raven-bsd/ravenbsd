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
* netd/resolvd/ping/route/netstat;
* pcid, devd, platd and the GENET program/service, with the platform launch staged disabled
  (P8);
* `/etc/drivers.toml` from `userland/servers/devd/drivers.toml`.

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
`/bin/{rsh,login,getty,brush,ls,ping}` and the account/driver files. It never opens a serial port
and never claims a board result.

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

## 4. Platform and network gap (P8)

`platd`, `devd` and `genet` are staged with `enable = false`: the devd DTB path currently only
reports matches and does not grant MMIO/DMA/IRQ or spawn the platform driver. Until P8 launches
them, `plat:mac`, `nic:` and the GENET link are unavailable; the release gate cannot claim
link/DHCP/ping. The driver/service manifests are in `profiles/pi4-release.d/` with honest
`needs`/`readiness` and no board claim.

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
