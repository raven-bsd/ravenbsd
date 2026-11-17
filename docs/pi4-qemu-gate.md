<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Pi 4 QEMU release-image gate

Recorded host/QEMU sweep of the Pi 4 release surface at commit `d8f32734`
(`dave/A8c`, from `origin/master`).

This is **not** a board claim. The Pi 4 serial/GENET gate is still blocked on the missing USB-TTL
adapter; everything below runs on the host with QEMU's `virt` machine or checks the built image.
Board rows are marked `UNAVAILABLE`, never "pass".

## 1. Image build and host check

The release image must exist before `--check`; a clean clone has only the sources.

```
RAVEN_RPI_FIRMWARE=/home/bo/work/ravenos-workspace/ravenbsd-resources/rpi-firmware/boot \
    cargo xtask image --arch aarch64 --platform pi4 --profile pi4-release
```

Result: exit 0, `build/aarch64/ravenbsd-pi4.img`, 335 544 320 bytes,
sha256 `ac543abcaa1a13fb96447bc00c4576f6b2bbaf0a790b6c9999a77ca43baa2196`.

```
sh scripts/pi4-release.sh --check
```

Result: exit 0, `pi4-release: image check PASS (board gates remain UNVERIFIED)`.

* `pi4-manifest: PASSED` — firmware set (`bootcode.bin`, `start4.elf`, `fixup4.dat`,
  `bcm2711-rpi-4-b.dtb`, `LICENCE.broadcom`, `overlays/disable-bt.dtbo`) and the flat
  `kernel8.img` (142 745 744 bytes) with the banner marker.
* 19 initfs paths verified: `/sbin/{vfsd,netd,platd,genet,pcid,devd}`,
  `/bin/{rsh,login,getty,brush,ls,ping}`, `/etc/{passwd,group,shadow,profile,motd,drivers.toml,pi4.dtb}`.
* `/etc/pi4.dtb` FDT header/totalsize/CRC-32 match the P8 fixture
  (`totalsize=371 crc32=4047538b`).
* The manifest linter's devd/netd/platd/genet/svcd/drivers-table rules passed (the script fails the
  check when any of them is wrong).

## 2. QEMU cases

Command set:

```
cargo xtask qtest --case <case> --arch aarch64
```

| Case | Exit | Result | Coverage / limit |
|---|---|---|---|
| `pi4-root` | 0 | pass | J5: EMMC2 **host-backed fake card** → blockd → fsd-ufs → vfsd, mount/read. QEMU has no BCM2711 EMMC2 model, so MMIO/IRQ/SDMA are not covered. |
| `pi4-devd` | 0 | pass | P8: devd decodes the fixture DTB and hands MMIO/DMA/IRQ handles to a fake platform driver. Device-memory access is not exercised. |
| `pi4-usb` | 0 | pass | D2: xHCI driver in **fake-keyboard** mode; D1 ring seam and enumeration without PCI/MMIO. VL805 board path not covered. |
| `pi4-emmc2` | 0 | pass | J4: real EMMC2 driver in **host-backed test mode** against a memory disk (disk scheme, block framing, pattern/checksum). Board MMIO/DMA not covered. |
| `pi4-platd` | — | **UNAVAILABLE** on aarch64 | The fixture is x86_64-only by design (`pi4-platd fixture is x86_64-only (requested aarch64)`, pre step exit 2). On its native arch: `cargo xtask qtest --case pi4-platd --arch x86_64` → exit 0, pass, so the P12 platd launch/`plat:` scheme path is covered as a host test. |

No case timed out.

## 3. Board-only rows (UNAVAILABLE)

| Gate | Why UNAVAILABLE |
|---|---|
| Serial console capture (`scripts/pi4-release.sh --device /dev/ttyUSB0` or `--log`) | No USB-TTL adapter on this host; `--check` never opens a serial port. |
| Board release markers (kernel banner, `login root/raven`, `raven#`, `ls /bin`, `ping <gateway>`, clean exit) | Require the captured board log above. |
| GENET v5 networking on the board | Needs the Pi 4 and the adapter; QEMU `virt` has no BCM2711 GENET model. |
| Real EMMC2 MMIO/IRQ/SDMA path | QEMU has no BCM2711 EMMC2 model; only the host-backed card path runs. |
| VL805 xHCI on the board | The QEMU case runs the driver in fake-keyboard mode with no PCI/MMIO. |
| Live firmware DTB | The image stages the P8 fixture DTB, not the firmware-provided tree. |

## 4. Environment note (clone state, not a product defect)

`scripts/pi4-release.sh --check` does not build; it fails with
`pi4-release: image not found: build/aarch64/ravenbsd-pi4.img` until the image exists.

On this clone the first image build also failed because the staged sysroot
`build/aarch64/libc.a` (dated 2026-11-15) predated master's libc exports, so `brush` could not link
(`undefined symbol: pipe2`, `confstr`, `tcgetpgrp`, `tcsetattr`, `killpg`, `gethostname`,
`errno_location`, ...). `xtask initfs::ensure_libc` only checks that `libc.a`/`crt*.o` *exist*, not
that they are current. Deleting those four generated files forced `libc-pack` to rebuild them, after
which the image built and every check passed. Nothing in the image, profiles, kernel or tests was
edited.

## 5. Outcome

Host-side release image, manifest, initfs and `/etc/pi4.dtb` checks pass; the four aarch64 Pi 4
QEMU cases pass on their host-backed paths; `pi4-platd` is x86_64-only and is recorded as
UNAVAILABLE here (it passes on x86_64). Every board-only row stays UNAVAILABLE. Board
serial/GENET remains **UNVERIFIED** until a real captured log exists.
