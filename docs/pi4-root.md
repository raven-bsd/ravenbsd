<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Pi 4 EMMC2 root path

Status: the EMMC2 `disk:` scheme, blockd, fsd-ufs read-write and the vfsd mount are wired. The SD2
`/root` regressions and the K182 UFS `/` pivot proof execute on the host-backed fake card; the
**real BCM2711 MMIO/DMA/IRQ path, the real-card pivot and cross-boot persistence are UNVERIFIED**
until SD3 captures a board log. No board result is claimed here.

## 1. Test stack

`cargo xtask qtest --case pi4-root --arch aarch64` builds `tests/fixtures/pi4-root/build.sh` and
boots the standalone `pi4-root-test` profile:

1. `emmc2 test` loads `initfs:/etc/pi4-root.img` and registers the `disk:` scheme.
2. `blockd` opens `disk:0`, scans the GPT and serves `block:0`.
3. `fsd-ufs` mounts partition 1 read-only and serves `ufs:`.
4. `vfsd` mounts the tree; svcd mounts `ufs:` at `/root` and the one-shot probe reads the known
   files through `file:/ufs/...`, then unmounts.
5. The probe checks the known files, directory listing, symlink, stat, error cases and umount;
   then svcd settles and init exits 0.

Serial markers:

```
emmc2: backend=memory image=initfs:/etc/pi4-root.img sectors=32835
emmc2: registered disk
emmc2: serving
blockd: gpt ok partitions=1
blockd: part 1 ufs-root 34..32801
fsd-ufs: mounted block:0 partition 1 as ufs: (16777216 bytes)
fsd-ufs: mode=ro journal=none
svcd: root mounted at /root
fsd-ufs: probe mount ok
fsd-ufs: probe ten.txt=0123456789
fsd-ufs: probe ok
init exited (0)
```

The K54a vfsd proxy fix is required for the `file:` read; `vfs-aarch64` and the literal
`fsd-ufs --arch aarch64` case remain the regression checks.

## 2. Read-write fake-card regression

`cargo xtask qtest --case pi4-root-rw --arch aarch64` builds
`tests/fixtures/pi4-root/build-rw.sh` and boots `pi4-root-rw-test`:

1. The same EMMC2 host-backed GPT+UFS image is served by `emmc2 test persist`.
2. `fsd-ufs` mounts partition 1 **read-write** (`FSD_UFS_READ_ONLY=0`).
3. svcd mounts `ufs:` at `/root`; the one-shot `pi4-root-rw` probe:
   - writes `/root/rw-persist.txt` and reads it back,
   - syncs, unmounts `/root`, remounts it through vfsd,
   - reads the file back again and verifies the known `ten.txt`,
   - exits 0.

Serial markers:

```
emmc2: backend=memory image=initfs:/etc/pi4-root.img sectors=32835
emmc2: serving
blockd: gpt ok partitions=1
fsd-ufs: mounted block:0 partition 1 as ufs: (16777216 bytes)
fsd-ufs: mode=rw journal=none
svcd: root mounted at /root
pi4-root-rw: mount ok
pi4-root-rw: write ok
pi4-root-rw: umount ok
pi4-root-rw: remount ok
pi4-root-rw: read back ok
pi4-root-rw: known file ok
pi4-root-rw: probe ok
svcd: settled
init exited (0)
```

The `persist` token keeps the fake card serving after the UFS sync/flush. The one-shot
`pi4-emmc2` case still uses the default stop-on-flush and is unchanged.

## 3. UFS `/` pivot proof (K182)

The Pi 4 `pi4-rootfs` SD profile stages `/etc/raven/root-mode` = `pivot`, the post-pivot
`/etc/mounts`, the UFS-only proof marker and the release rc/svcd policy. The real stack is
`devd` -> `emmc2` -> `blockd` -> `fsd-ufs` (RW) -> `vfsd` -> `svcd` pivot; `/dev`, `/tmp` and
the bootstrap handles survive.

`cargo xtask qtest --case pi4-root-pivot --arch aarch64` boots a bootstrap initfs against the
host-backed EMMC2 test card (a 48 MiB GPT+UFS image; the staged payload is 33 MiB). Markers:

```
emmc2: backend=memory image=initfs:/etc/pi4-root.img sectors=98371
blockd: gpt ok partitions=1
blockd: part 1 ufs-root 34..98337
fsd-ufs: mounted block:0 partition 1 as ufs: (50331648 bytes)
fsd-ufs: mode=rw journal=none
svcd: root pivoted to UFS at /
pi4-ufs-root
k182-persist
init exited (0)
```

`pi4-ufs-root` is the UFS-only marker read through `/`; the second marker is written and read in
the same boot. `login`, `getty` and `brush` run from the UFS root.

### Explicit limits (not proven by the fake card)

* **Cross-boot persistence is NOT proven.** The emmc2 `test` backend loads
  `initfs:/etc/pi4-root.img` into a `MemoryDisk`; the `persist` token only keeps the card serving
  after a same-boot UFS flush. A second boot of the same image pivots, but the first boot's
  writes are gone. Real-card persistence is SD3; a host-backed writable emmc2 backend is a
  separate ticket.
* **External spawned commands from the pivoted aarch64 brush fail** with
  `Bad file descriptor (os error 9)` (`/bin/cat`, `/bin/uname`, `/sbin/mount`). The proof
  therefore uses the shell's `read`/`echo` builtins for the marker I/O. The spawn defect is a
  libc/spawn owner ticket, not this profile.
* The emmc2 test backend refuses images above 64 MiB, so the proof root is 48 MiB. The real SD
  rootfs stays 256 MiB.

The SD2 `pi4-root`/`pi4-root-rw` cases remain the `/root` mount regressions: a UFS tree without
`/etc/services.d` keeps the `/root` mount and the initfs plan, while an image with no `ufs:`
scheme (the release/initfs image) keeps the original no-root-device path.

## 4. Real EMMC2 platform handoff

The real image uses the `devd` platform launcher, not the `svcd` bootstrap, for the card driver:

| Step | Owner | Contract |
|---|---|---|
| Device match | `devd` | `brcm,bcm2711-emmc2` from `/etc/pi4.dtb`; `bar=0`, `resources=dma+irq` |
| Handle grant | `devd` | child handles 3 (MMIO), 4 (DMA), 5 (IRQ); `bar=0`, `bar_size=`, `dma=`, `irq=` tokens |
| Card init | `emmc2` | reset, 3.3 V, 400 kHz identify, 25 MHz 4-bit default timing |
| Data path | `emmc2` | SDMA over the mapped DMA buffer; bus address checked against the `emmc2bus` window |
| Partitions | `blockd` | GPT scan, partition 1, `block:` scheme |
| Filesystem | `fsd-ufs` | UFS2 partition 1 mounted read-write, journal recovered, `ufs:` scheme |
| Root view | `vfsd` | `file:` namespace; svcd mounts `ufs:` at `/` for the `pivot` marker |
| Service set | `svcd` | merge `/etc/services.d` and `/etc/rc.conf` from the UFS root before the initfs plan |

`emmc2` accepts both launcher spellings: the platform tokens (`bar=`, `bar_size=`, `dma=`, `irq=`)
and the explicit `mmio_handle=`, `dma_handle=`, `irq_handle=` form. The defaults are handles 3/4/5,
a 0x100-byte register window, a 64 KiB DMA buffer and the Pi 4 `emmc2bus` window
(`0xc0000000`/`0`/1 GiB). The platform entry is in `profiles/pi4-rootfs.d/drivers.toml`.

The root profile fragments are:

* `profiles/pi4-rootfs.d/20-storage.toml` — `/etc/drivers.toml`, blockd, fsd-ufs RW and vfsd.
* `profiles/pi4-rootfs.d/blockd.manifest` — `BLOCKD_DISK=disk:0`.
* `profiles/pi4-rootfs.d/fsd-ufs-rw.manifest` — `FSD_UFS_READ_ONLY=0`, partition 1.
* `profiles/pi4-rootfs.d/vfsd.manifest` — waits for `fsd-ufs` before vfsd snapshots `ufs:`.

### SD1 dependency

The Pi 4 rootfs spec (`profiles/pi4-rootfs/rootfs.toml`) and the profile are in the tree. The
parent profile must:

* include `/sbin/devd` (with `grants = ["platform-root"]`), `/sbin/emmc2`, `/sbin/blockd`,
  `/sbin/fsd-ufs` and `/sbin/vfsd`;
* ship the FAT boot files (`kernel8.img`, `config.txt`, `cmdline.txt`, DTB) and the UFS2 root
  partition populated from the SD1 rootfs spec;
* keep `/etc/services.d` and `/etc` on the UFS root, with the initfs bootstrap remaining usable
  when the card/root mount fails.

Board evidence is SD3. The exact image SHA, partition layout and serial transcript belong to that
ticket.

## 5. Failure handling

| Condition | Expected behaviour | Evidence |
|---|---|---|
| No card / card init failure | `emmc2` prints `card init: ...` and exits nonzero; blockd never starts; svcd keeps the initfs bootstrap and logs the missing root | `init_card` host tests; board step required |
| Bad GPT | `blockd` prints the GPT error and does not provide `block:`; fsd-ufs never mounts | `raven-blockd` host tests; board step required |
| UFS mount/recovery error | `fsd-ufs` refuses the image and exits nonzero; vfsd/svcd do not fake a root | `fsd-ufs` recovery tests; RW qtest exercises the clean path |
| Read-only card | `emmc2` reports `read_only`; blockd/fsd-ufs mount read-only or reject writes with `EROFS`; the root is not writable | `emmc2` and `fsd-ufs` host tests |
| Full disk | `fsd-ufs` returns `ENOSPC` on allocation; the probe fails visibly, no silent truncation | `ufs-core` allocation tests |
| Missing `/etc/services.d` on the card | svcd keeps the initfs plan and reports the missing root service set (the pivot is not faked) | `pi4-root` serial |

No path silently substitutes a fake root. The fake-card qtests are explicitly named and never claim
a board result.

## 6. Board steps (SD3, UNVERIFIED)

1. Build the SD1 image (`pi4-rootfs`) with the storage fragments and flash it to the owner-provided
   card.
2. Capture UART 115200 and verify the firmware/kernel banner.
3. Confirm `devd: dtb launch emmc2 bar=0 ...` and `emmc2: card rca=... capacity=...`.
4. Confirm `blockd: gpt ok`, `fsd-ufs: mounted block:0 partition 1 ... mode=rw` and
   `svcd: root pivoted to UFS at /`.
5. Log in, `ls /bin`, `df`, write a file under `/`, reboot, verify the file persists (real card).
6. Record the image SHA, SD device and the serial log path; hand the log to SD3.

A missing or failed step is reported with the first failure, not retried into a pass.

## 7. DR-57 / DR-52 donor reconciliation

The SDHCI/EMMC2 register model, card state machine, timing and SDMA/ADMA2 encoders were studied
from the donors in J1/J2/J7; no C code was copied, translated or closely paraphrased. This ticket
adds only first-party launch parsing, profile wiring and the read-write probe.

| Topic | Illumos (read-only) | BSD (read-only) | Linux (read-only) | Decision |
|---|---|---|---|---|
| SDHCI/EMMC2 register sequence | `sdhci`/`sd` driver design (CDDL) | NetBSD `sdmmc`/`sdhci`, FreeBSD `mmc`/`sdhci` (BSD-2) | `drivers/mmc/host/sdhci*` (GPL) | First-party `raven-pi4` + `emmc2`; no code copied |
| DMA window | no direct EMMC2 equivalent | NetBSD/FreeBSD DMA tag behaviour | `dma-ranges`/`swiotlb` outcomes | First-party `raven-pi4::dma`; bus address checked against the DTB window |
| Root mount / service set | `vfs_mountroot`/`vfs_mount` design | NetBSD `vfs_mountroot`, FreeBSD `vfs_mountroot` | `mount_root`/`initramfs` | First-party `svcd` `/` pivot and service merge |
| Read-write UFS | no UFS server in the checkout | NetBSD `ufs`/`ffs` (BSD-2, study only) | `fs/ufs` (GPL, study only) | First-party `ufs-core`/`fsd-ufs`; no code copied |

Donor revisions: Illumos `586678ac1bcbd77644a6ec4dac7f79d676cfb4ec`; NetBSD
`36eddebcc36cb0775aa2962005b9c05419a6ff01`; Linux `adc218676eef25575469234709c2d87185ca223a`.
