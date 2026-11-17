<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Pi 4 release image and serial gate (REL3)

Status: host image and transcript checker OBSERVED. No Pi 4 board transcript exists; every board
gate below is UNVERIFIED. The release session uses `/bin/rsh`; `/bin/login` and getty provide the
login prompt. brush is staged but cannot spawn a session yet.

Host verification (rev 42): the release image builds reproducibly from the external firmware tree,
`scripts/pi4-release.sh --check` passes, `scripts/pi4-release.sh --repro` reports the same SHA-256
for both builds (`257adad5873677c74fa947d78235c6e374a6073a042e3ab88af010005ce0a829`), and
`cargo xtask qtest --case release-login --arch x86_64` passes as the QEMU control. The board
remains UNVERIFIED: the serial adapter is not connected and the root BAR/DMA/IRQ policy is open.

## 1. Profile

`profiles/pi4-release.toml` extends `base` (the loader permits one level and `pi4` already extends
`base`) and carries the Pi 4 boot cmdline and EMMC2/blockd staging entries. The release fragments
add:

* shell services and first-party tools;
* the aarch64 brush and uutils `ls` slice;
* `/etc/passwd`, `group`, `shadow`, `profile`, `motd`, `/bin/getty` and `/bin/login`;
* netd in DHCP-first mode (`netd nic dhcp-first`, `needs = scheme:nic`);
* pcid, devd (`dtb launch`, enabled, `platform-root`), the disabled platd/genet service entries,
  and the release `drivers.conf` with the GENET `slots=platform` entry plus the P12 platd
  multi-window plan (`mailbox=3:64 gpio=4:180 rng=5:40 dma=6:256`);
* `/etc/pi4.dtb` from the P8 host fixture (`tests/fixtures/pi4-release/build.sh`); the live
  firmware DTB remains a board follow-up.

Build:

```sh
RAVEN_RPI_FIRMWARE=/path/to/rpi-firmware/boot \
    cargo xtask image --arch aarch64 --platform pi4 --profile pi4-release
```

The firmware is external and never committed. The rev-42 host run used
`RAVEN_RPI_FIRMWARE=/home/bo/work/ravenos-workspace/ravenbsd-resources/rpi-firmware/boot` (the
workspace `ravenbsd-resources/rpi-firmware/boot` checkout). `scripts/pi4-release.sh` also probes
that path relative to the repository when the variable is unset, but only the variable or a real
firmware directory produces an image; the repository carries no firmware blobs.

## 2. Host check

```sh
scripts/pi4-release.sh --check
```

It runs `scripts/pi4-serial.sh --check` (firmware/DTB/FAT), verifies the `kernel8.img` banner and
checks the release initfs paths: `/sbin/{vfsd,netd,platd,genet,pcid,devd}`,
`/bin/{rsh,login,getty,brush,ls,ping}`, the account files, `/etc/drivers.toml` and `/etc/pi4.dtb`.
The DTB check compares the FDT magic/totalsize/CRC-32 of the staged blob with the validated
fixture. The manifest linter fails if devd/netd are disabled, devd is not `dtb launch`, the
devd `platform-root` grant or `scheme:plat`/`scheme:dev`/`scheme:nic` provides are missing, the
genet/platd svcd entries are enabled (duplicate launchers), netd is not DHCP-first behind
`scheme:nic`, the drivers table lacks the GENET `slots=platform` or P12 platd multi-window entry,
or a duplicate genet/platd launcher is present. It never opens a serial port and never claims a
board result.

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

P8 `devd dtb launch` is enabled with the `platform-root` grant (P10 merged). P8b's multi-window
launcher is merged, so the release `drivers.conf` now carries the P12 platd entry with three MMIO
windows and one DMA buffer:

```
bar=0 bar=1 bar=2 resources=dma dma_size=256
mailbox=3:64 gpio=4:180 rng=5:40 dma=6:256
```

`devd` creates windows 3/4/5 in declaration order and DMA slot 6. The platd and genet svcd service
entries stay disabled so `devd` is the only launcher. netd waits for `scheme:nic` and uses
DHCP-first addressing (REL4b).

The host fixture DTB is the P8b platd tree with the three-window node; it is staged as
`/etc/pi4.dtb`. The live firmware DTB, the real window mapping, the GENET link/DHCP lease and the
gateway ping remain UNVERIFIED without the owner's serial log. A missing board transcript is not a
pass, and the platd/genet service entries are not duplicated by svcd.

## 4.1 Remaining board blockers

The host checks prove the image contents, not a Pi 4 boot. The exact open items are:

1. **No serial capture.** The hardware UART adapter is not connected, so there is no
   `/dev/ttyUSB0` transcript. Every gate below stays UNVERIFIED until the owner captures the
   PL011 console at 115200 8N1.
2. **Root BAR/DMA/IRQ policy is open.** P10 supplies the `platform-root` grant to `devd`, but the
   kernel root device capability is not range-scoped: `devd` could create MMIO, DMA and IRQ
   handles outside the windows decoded from the device tree (`docs/grants.md`, residual risks).
   Closing that needs a range-limited platform capability in `crates/raven-abi`/the kernel and is
   outside REL3, so the board security claim stays open.
3. **Live firmware DTB.** The release stages the host P8b fixture as `/etc/pi4.dtb`; the board
   firmware's own DTB and the real BAR/window mapping have not been compared on hardware.
4. **GENET hardware.** The driver is compiled and the handle/token handoff is proven by the
   `pi4-devd` fixture, but no GENET register access, PHY/link result, DHCP lease or gateway ping
   exists without the board log.
5. **PL011 login.** The QEMU `release-login` case proves the x86_64 release session; the Pi 4
   `ttyAMA0` getty/login path still needs board evidence.

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
