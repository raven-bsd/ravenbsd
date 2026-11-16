<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Pi 4 release image and serial gate (REL3)

Status: host-side release image and gate helper. The image builds and its FAT/kernel/initfs manifest
is OBSERVED on the development host. No Pi 4 board transcript exists; every board gate below is
UNVERIFIED. Do not fill the transcript from a fixture.

## 1. Profile

`profiles/pi4-release.toml` extends `base` and adds the current shell service set, first-party
tools, network services and the Pi platform service:

```
logd sysd randd zerod nulld initfsd ramfsd vfsd consd rsh
sleep date hostname dmesg mount umount ps
netd resolvd ping route netstat
platd
```

The `emmc2`/`blockd` declarations are staged disabled in `profiles/pi4-release.d` for the J5 root
path. The profile does **not** include login/getty (REL1), brush or the uutils `ls` slice: those
have no aarch64 build on this base. `profiles/pi4-release.toml` records this explicitly; do not
copy x86_64 binaries into the aarch64 image.

Build:

```
RAVEN_RPI_FIRMWARE=/path/to/rpi-firmware/boot \
    cargo xtask image --arch aarch64 --platform pi4 --profile pi4-release
```

The result is `build/aarch64/ravenbsd-pi4.img` (320 MiB: ESP and a ROOT partition; see
`docs/pi4-evidence.md` for the current LBA layout).

## 2. Host check

```
scripts/pi4-release.sh --check
```

The check runs `scripts/pi4-serial.sh --check` (firmware, DTB and FAT manifest), extracts
`kernel8.img` and verifies the banner marker, and looks for the release initfs paths
`/sbin/vfsd`, `/sbin/netd`, `/sbin/platd`, `/bin/rsh` and `/bin/ping`. It reports
`/bin/login`, `/bin/brush` and `/bin/ls` as PENDING; their absence is expected on this base and is
the remaining release gate, not a pass.

## 3. Serial gate

```
scripts/pi4-release.sh --device /dev/ttyUSB0
```

The script runs the J3 capture (`scripts/pi4-serial.sh`) and checks the required markers in
`tests/fixtures/pi4-release/gate-markers.txt`:

| Gate | Marker | Current status |
|---|---|---|
| kernel | `RavenBSD kernel 0.1 (aarch64)` | host check OBSERVED; board UNVERIFIED |
| login | `login:` | PENDING REL1 |
| brush | `brush` | PENDING (no aarch64 brush slice) |
| `ls /bin` | `rsh` in the listing | PENDING (no aarch64 uutils) |
| `ping <gateway>` | `bytes from` | program present; board UNVERIFIED |

Pass an existing transcript with `--log PATH` to check it without opening a device. A log missing
any marker fails; a log is never synthesized.

## 4. Board checklist (UNVERIFIED)

No board run was performed for REL3. The owner must:

1. Build the aarch64 release image with the firmware path above and record its SHA-256.
2. Run `scripts/pi4-release.sh --check`; record the manifest and kernel/initfs lines.
3. Flash the image, connect the PL011 UART at 115200 8N1 and run
   `scripts/pi4-release.sh --device /dev/ttyUSB0`.
4. Capture the kernel banner, login prompt, `root`/`raven` login, brush prompt, `ls /bin` and
   `ping <gateway>` output.
5. Attach the log; record the first failing line if the boot stops. All board rows stay UNVERIFIED
   until then.

## 5. Open gaps

* REL1/getty/login is not on this base; the login gate cannot be exercised.
* brush and uutils `ls` have no aarch64 build (the ravenbsd toolchain has only the x86_64 std).
* The Pi 4 network path (GENET/platd/netd) and the EMMC2 root path remain UNVERIFIED on silicon.

## 6. Provenance

No donor code was read or copied for REL3; the profile, script, marker fixture and this document
are first-party BSD-3-Clause work.
