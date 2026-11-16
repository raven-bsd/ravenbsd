<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Pi 4 first-power-on bring-up checklist (P11)

Status: host checklist only. No Pi 4 board result is claimed. The release image and its serial
gate belong to REL3; this page makes the first power-on repeatable and tells the owner what to
check when the console is silent. The current `pi4-release` profile stages `devd` and `genet`
disabled until REL3's rework; the platform/network rows below become expected only after that
rework. The full release image build is not part of P11; use REL3's command below.

## 1. Host preflight

```sh
sh scripts/pi4-bringup.sh --check
```

`--check` changes nothing. It does four things:

1. prints the UART wiring and power reminder;
2. hashes the release image when it exists (and compares `--sha256` when supplied);
3. checks a supplied or auto-detected SD block device for size, mounts and the root disk;
4. checks a supplied or auto-detected USB-UART adapter for character-device access, permissions
   and the current 115200 8N1/no-flow-control settings.

A missing image, SD card or adapter is reported as an **owner step**, not a fake pass:
`host check PASS; N owner step(s) pending; board result remains UNVERIFIED`. A supplied
`--sha256` mismatch, a mounted SD device, the root disk or a supplied device that is not the
expected type is a hard failure.

```sh
# hash the built image and require a recorded digest
sh scripts/pi4-bringup.sh --check -i build/aarch64/ravenbsd-pi4.img --sha256 "$EXPECTED_SHA"

# check a specific SD target and UART before flashing
sh scripts/pi4-bringup.sh --check --sd /dev/sdX --device /dev/ttyUSB0

# print the ordered marker table only
sh scripts/pi4-bringup.sh --markers
```

`tests/fixtures/pi4-bringup/hash-fixture.txt` and `.sha256` exercise the image-hash path on a
host without a release image. They are not a bootable image and must never be flashed.

## 2. Hardware and power

| Item | Requirement |
|---|---|
| Board | Raspberry Pi 4 Model B; a board with the 40-pin header and the PL011 on GPIO14/15. |
| Power | 5 V / 3 A USB-C supply (official 15 W). A phone charger is not sufficient. Use a short, thick cable; remove powered USB hubs from the path. |
| SD | microSD, 8 GB or larger, class 10/A1 or better. Write the **whole block device**, never a partition. |
| UART | 3.3 V TTL USB-UART adapter (CP2102/FT232-class); jumper wires. Never use an RS-232-level adapter. |
| Network | Ethernet cable to a DHCP network for the network gate. The console/login gate does not need it. |

Undervoltage is a board-level failure, not a kernel bug. The firmware may print
`Under-voltage detected!` and later `Voltage normalised`; random reboots, a dim red LED or a
rainbow screen after early output point at the supply/cable. Replace the supply or cable before
chasing software.

## 3. SD card and release image

Build the release image with REL3's command and record its SHA-256:

```sh
RAVEN_RPI_FIRMWARE=/path/to/ravenbsd-resources/rpi-firmware/boot \
    cargo xtask image --arch aarch64 --platform pi4 --profile pi4-release
sha256sum build/aarch64/ravenbsd-pi4.img
```

Run the host check first, then flash the whole device as the owner:

```sh
sh scripts/pi4-bringup.sh --check -i build/aarch64/ravenbsd-pi4.img
sh scripts/pi4-bringup.sh --flash /dev/sdX --device /dev/ttyUSB0
```

`--flash` refuses a mounted device, the root disk, a partition and an image smaller than 1 MiB (so
the hash fixture cannot be written to a card), prints the image/device/hash, asks for the exact
device path, writes with `dd conv=fsync`, and reads the image-sized prefix back to compare the
SHA-256. It never writes a partition like `/dev/sdX1`. The FAT boot files and `config.txt` are
REL3/J3's; verify them with:

```sh
sh scripts/pi4-serial.sh --check -i build/aarch64/ravenbsd-pi4.img
```

## 4. UART wiring and serial settings

The Pi 4 PL011 is on the 40-pin header at 3.3 V:

| Pi pin | Signal | USB-UART adapter |
|---|---|---|
| 6 | GND | GND |
| 8 | GPIO14 / TXD | RXD |
| 10 | GPIO15 / RXD | TXD |

**Never connect the adapter's VCC (3.3 V or 5 V) to the Pi.** Connect GND, cross TX/RX, and use a
3.3 V TTL adapter. The firmware configuration must keep the PL011 on GPIO14/15:

```
arm_64bit=1
kernel=kernel8.img
enable_uart=1
dtoverlay=disable-bt
init_uart_clock=48000000
core_freq=250
```

`enable_uart=1` routes the console to GPIO14/15; `dtoverlay=disable-bt` keeps the PL011 at
`0xFE20_1000` instead of handing it to Bluetooth. Capture at **115200 8N1, no flow control, no
echo**, with `console=ttyAMA0`. `dtoverlay=disable-bt` is what makes `ttyAMA0` the console; if
the log names `ttyS0`, the mini-UART/Bluetooth path is active.

## 5. Expected marker sequence

The first RavenBSD line on the Pi 4 is the kernel banner. The Pi 4 has no UEFI and no RavenBSD
loader line: the firmware `start4.elf` loads the flat `kernel8.img` directly. If the log contains
`loader: RavenBSD loader 0.1 (aarch64)`, `BdsDxe`, OVMF or GRUB, you booted the x86_64 UEFI image
or the wrong medium, not the flat Pi 4 release image.

| Stage | Exact marker | Meaning | Source |
|---|---|---|---|
| firmware | no RavenBSD text | `start4.elf` runs silently; a rainbow screen is normal | firmware |
| kernel | `RavenBSD kernel 0.1 (aarch64)` | first RavenBSD line; release gate | REL3 |
| board | `board: Raspberry Pi 4 Model B` | live firmware DTB model | S15 |
| memory | `mem:` | usable memory map | S15 |
| cmdline | `cmdline: console=ttyAMA0` | Pi 4 PL011 console | S15 |
| init | `process: starting /init` | svcd starts the initfs | S15 |
| devd | `devd: dtb launch genet` | P8 platform launch; requires REL3 enable | P8 |
| genet | `GENET-REV raw=` | GENET register bring-up record | O4 |
| netd | `netd: interface eth0 ` | DHCP-first lease; requires REL4a | REL4 |
| login | `login:` | getty/login prompt | REL3 |
| password | `assword:` | matches the `Password:` prompt | REL3 |
| prompt | `raven#` | release rsh shell | REL3 |
| ls | `ls /bin` | owner command | REL3 |
| ls-rsh | `rsh` | release shell is staged | REL3 |
| ls-brush | `brush` | brush is staged (it may not spawn a session) | REL2a |
| ping | `bytes from` | gateway ping reply | REL3 |
| genet-net | `GENET-NET-OK` | full link/DHCP/ping evidence | REL4 |
| clean | `init exited (0)` | clean release exit | REL3 |

The machine-readable copy is `tests/fixtures/pi4-bringup/expected-sequence.txt`; the REL3 gate
markers remain authoritative in `tests/fixtures/pi4-release/gate-markers.txt`. A later stage is
not evidence when an earlier one is missing. The platform/network rows are expected only after
REL3 enables `devd`/`genet` and REL4 supplies DHCP-first; they are UNVERIFIED on this host.

## 6. Triage table

| Symptom | Likely cause | Action |
|---|---|---|
| No serial output at all | wrong pins, no GND, adapter VCC/level, `enable_uart=0`, Bluetooth owns PL011, wrong baud, no permission | Check pins 6/8/10 with TX/RX crossed; never connect VCC; use a 3.3 V TTL adapter; set `enable_uart=1`, `dtoverlay=disable-bt`, `console=ttyAMA0`; capture at 115200 8N1; check `stty -F /dev/ttyUSB0 -a` and the `dialout` group. |
| Serial garbage / random bytes | baud or UART clock mismatch, RS-232-level adapter, floating GND, wrong UART | Confirm 115200 8N1, 3.3 V TTL and common GND; check `init_uart_clock=48000000`; use `ttyAMA0` with `dtoverlay=disable-bt`. |
| `BdsDxe`, OVMF, GRUB or `loader: RavenBSD loader` lines | x86_64 UEFI image or the wrong boot medium | Use the flat Pi 4 `kernel8.img` image with `config.txt`; the Pi 4 has no UEFI and no RavenBSD loader line. |
| Undervoltage, random reboots, rainbow after early output | phone charger, thin cable, powered hub, insufficient supply | Use 5 V / 3 A USB-C (official 15 W), a short cable, and no powered hub; look for `Under-voltage detected!`; replace the supply before debugging software. |
| HDMI shows output, serial is silent | PL011 assigned to Bluetooth, wrong pins, `ttyS0` console, adapter not 3.3 V | Set `enable_uart=1` and `dtoverlay=disable-bt`; use GPIO14/15 pins 8/10; use `console=ttyAMA0`; check the adapter and GND. |
| No `board:` line, `gic=fallback uart=fallback`, or an FDT parse error | missing/wrong `pi4.dtb`, wrong firmware tree, or a non-Pi image | Put the firmware `bcm2711-rpi-4-b.dtb` on the FAT boot partition; run `scripts/pi4-serial.sh --check`; verify `config.txt` and the image manifest; do not claim the board path until `board: Raspberry Pi 4` appears. |
| No `devd: dtb launch genet`, no `GENET-REV`, no `nic:` | `devd`/`genet` still staged disabled, missing driver/manifest, or the platform grant failed | Check `profiles/pi4-release.d/devd.manifest` and `genet.manifest` for `enable = true`; REL3's rework enables them; check the `svcd` service log and `devd` error line. P8 supplies the launch; REL3 stages the profile. |
| `netd` runs but no `eth0` lease | no link/cable, no DHCP server, wrong VLAN, QEMU-mode `10.0.2.x` constants, `netd` not in `dhcp-first` mode | Check the Ethernet link LEDs and cable; confirm a DHCP server on the segment; pass `dhcp-first` (REL4a); check `resolvd`; run `scripts/pi4-net-evidence.sh`. A static or `169.254/16` address is not a DHCP lease. |
| Login rejected | wrong account, bad `shadow`, getty not started, serial input not reaching the console | Use the release `root` credentials; check `/etc/passwd`, `/etc/shadow` and the getty manifest in the image; the release gate fails on `login: incorrect`/`failed`. |
| `brush` panics or the shell exits | brush is staged but the release session uses `rsh`; the interactive backend may fail | Record the exact first line; the release gate only requires `raven#` from `rsh` and `brush` in `ls /bin`; do not claim a brush session. |
| `panic` or `EXCEPTION` | kernel/driver fault | Stop; record the first panic line, commit, image SHA and log; do not claim later gates. |
| No `init exited (0)` | the session never exited or a service failed | Type `exit` at `raven#`; if the board still does not reach a clean exit, attach the log and treat the release gate as failed. |

## 7. Capture and evidence

Capture with REL3's script; P11 does not duplicate the transcript grader:

```sh
# capture (sets 115200 8N1 and timestamps the log)
sh scripts/pi4-release.sh --device /dev/ttyUSB0 -i build/aarch64/ravenbsd-pi4.img \
    -l build/pi4/<image-sha256>.log

# grade the captured log against the REL3 gates
sh scripts/pi4-release.sh --log build/pi4/<image-sha256>.log

# optional platform/network evidence
sh scripts/pi4-genet-evidence.sh build/pi4/<image-sha256>.log
sh scripts/pi4-net-evidence.sh build/pi4/<image-sha256>.log
```

Record every line from the log; leave a field `NOT OBSERVED` rather than guessing:

```text
Pi 4 bring-up evidence
======================
Date (UTC):
Commit:                       # git rev-parse HEAD
Image:                        # path and size
Image SHA-256:                # sha256sum build/aarch64/ravenbsd-pi4.img
Firmware revision:            # rpi-firmware entry in donors.lock
Board model/revision:         # from the log or the board label
SD card:                      # model and capacity
Power supply:                 # 5 V / 3 A USB-C model
UART adapter and device:      # e.g. CP2102 /dev/ttyUSB0, 115200 8N1
Wiring:                       # pins 6/8/10, no VCC, common GND
Capture log:                  # build/pi4/<image-sha256>.log
Manifest check:               # PASSED / first failing file
Kernel/login/shell gates:     # PASS/FAIL per REL3 marker
Platform/network gates:       # PASS/FAIL/UNVERIFIED per GENET/REL4
First failing line:           # exact line, or "none"
Result:                       # PASS / FAIL / UNVERIFIED
```

No step in this document has been performed on a Pi 4 for P11. The host check, the script syntax,
the image-hash fixture and the marker table are OBSERVED on the development host; the board result
is UNVERIFIED until the owner attaches the captured log.

## 8. Relationship to the other kits

* `docs/pi4.md` — Pi 4 boot path, SD contents, UART wiring.
* `docs/pi4-evidence.md` — J3 capture and evidence template.
* `docs/pi4-release.md` — REL3 release profile, image check and serial gate.
* `docs/pi4-genet-board.md` — GENET register/PHY/MAC markers.
* `docs/pi4-net.md` — link/DHCP/ping (`GENET-NET-*`) gate.
* `scripts/pi4-serial.sh` — capture and FAT manifest; `scripts/pi4-release.sh` — image check and
  release transcript; `scripts/pi4-genet-evidence.sh` / `scripts/pi4-net-evidence.sh` — optional
  platform/network transcripts.

P11 only adds the preflight/flash checklist and the marker table. It does not build the image,
change the profile, capture the board or grade the release gate.

## 9. Provenance

No donor code was read or copied. The script, fixtures and this document are first-party
BSD-3-Clause work.
