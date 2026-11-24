<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Fast in-place Pi 4 SD update

Status: `tools/ufs-inject` and `scripts/pi4-update.sh` are host-side tooling. The FAT+UFS image
layout comes from SD1; the real board turnaround remains SD3/UI1. No board result is claimed here.

## 1. Why

Reflashing the whole 500 MiB image for every Rust program or kernel iteration is slow and wears the
card. Linux cannot reliably mount UFS read-write, so the update path is:

* FAT boot partition: `mtools` (`mcopy`, `mtype`) on the whole image or SD device.
* UFS2 root: `tools/ufs-inject`, a host-side writer built on the existing `ufs-core` port.
* No kernel mount, no C in the product, no full reflash.

## 2. `ufs-inject`

The tool opens a UFS1/UFS2 image or a byte slice of a GPT image/block device, mounts it read-write
through `ufs-core`, changes one object, syncs/unmounts, runs the read-only fsck and reads the object
back. Every write is followed by fsck and a read-back comparison.

```text
usage: ufs-inject [--offset BYTES] [--size BYTES] IMAGE COMMAND ...

  put HOST DEST [--mode OCTAL] [--uid N] [--gid N] [--parents]
  mkdir PATH [--mode OCTAL] [--uid N] [--gid N] [--parents]
  symlink TARGET PATH [--uid N] [--gid N] [--parents]
  cat PATH
  list PATH
  stat PATH
  check
```

`--offset`/`--size` select the UFS partition inside a whole SD image; the installer layout has the
UFS partition at byte `452608 * 512`. A raw UFS image/partition uses the defaults (offset 0, size
to the end). `put` preserves the host mode/uid/gid unless overridden; `--parents` creates missing
directories. `check` exits 0 only when the read-only fsck is clean.

## 3. `scripts/pi4-update.sh`

```text
scripts/pi4-update.sh --check [-i IMAGE]
scripts/pi4-update.sh [-i IMAGE | -d DEVICE] [options]

  --check              verify GPT, FAT, UFS and update sources; no writes
  --dry-run            print the plan; no writes
  --program PKG[:DEST] build an aarch64 Rust program and inject it
  --file SRC:DEST      inject a host file into the UFS root
  --staging DIR        inject a rootfs staging tree (dirs/files/symlinks)
  --fat SRC:DEST       copy one file into the FAT boot partition
  --kernel PATH        copy PATH to FAT kernel8.img
  --dtb PATH           copy PATH to FAT /<basename>
  --uid N              UFS owner uid for injected objects (default 0)
  --gid N              UFS owner gid for injected objects (default 0)
  --backup             copy the whole image first (file image)
  --backup-dir DIR     save FAT/UFS slices first (device or image)
  -y, --yes            skip the block-device confirmation
```

### Fast loop

```sh
# Build the current image once (SD1 layout).
RAVEN_RPI_FIRMWARE=/path/to/rpi-firmware/boot SOURCE_DATE_EPOCH=1700000000 \
  cargo xtask image --arch aarch64 --platform pi4 --profile pi4-rootfs

# Verify the current image and the tool.
sh scripts/pi4-update.sh --check

# Update one program and one boot file, with a backup.
sh scripts/pi4-update.sh \
  --program raven-echo:/sbin/raven-echo \
  --kernel build/aarch64/kernel8.img \
  --backup

# Inspect the plan without writing.
sh scripts/pi4-update.sh --program raven-echo:/sbin/raven-echo --dry-run

# Update a real SD card in place (owner/root, unmounted).
sh scripts/pi4-update.sh --device /dev/sdX \
  --program raven-echo:/sbin/raven-echo \
  --kernel build/aarch64/kernel8.img \
  --backup-dir build/pi4-backup
```

The program path builds `cargo xtask build user --arch aarch64 --release -p PKG` and injects the
artifact (default destination `/sbin/<artifact>`). The UFS root keeps the new binary; the FAT
partition keeps the firmware/kernel/DTB files. Injected objects default to uid/gid `0:0` and keep
the host permission bits; use `--uid`/`--gid` for a non-root service account.

### Profile update

`--staging build/aarch64/rootfs-staging` walks the resolved rootfs tree and updates directories,
regular files and symlinks in the UFS root. It is the "new profile" path: rebuild the image or
staging tree, then inject the changed tree without reflashing. Use `--dry-run` first; the walker
prints each `mkdir`/`put`/`symlink` it would perform.

### E-Ink iteration

The E-Ink service package is not in this branch yet; the update tool is program-agnostic. When its
package lands, the loop is:

```sh
# Build the program on the host.
cargo xtask build user --arch aarch64 --release -p einkd

# Inject it into the existing card image; the service manifest is already on the UFS root.
sh scripts/pi4-update.sh --image build/aarch64/ravenbsd-pi4-sd.img \
  --program einkd:/sbin/einkd --backup

# Reboot the Pi and start einkd from the existing service set.
```

For the current tree the same flow is proven with `raven-echo` (see the acceptance report).
A kernel or DTB iteration uses `--kernel build/aarch64/kernel8.img` and/or `--dtb
build/fixtures/pi4-release/pi4.dtb`; only the FAT partition changes.

## 4. Safety and failure handling

* `--check` never writes; `--dry-run` prints the plan and leaves the target hash unchanged.
* A file-image write with `--backup` copies the image to `<image>.upd1-backup` and restores it
  automatically if any write or the final fsck fails.
* A block-device write requires root and confirmation, and refuses the root disk/partition and any
  mounted partition (`/proc/mounts`). Images attached to a loop device are refused too.
* `--backup-dir` saves the FAT and UFS slices before a device write; it is required for a device
  `--backup`. The script never auto-writes to a device after a failure; the saved slices are the
  recovery path.
* `ufs-inject put` truncates and replaces a regular file, follows intermediate directories, and
  rejects `..` paths and non-regular collisions. `mkdir` is idempotent with `--parents`.
* Every UFS write is followed by a read-only fsck and a byte-for-byte read-back. A missing UFS
  root, bad GPT, bad FAT file, dirty fsck or short write fails visibly. No fake root and no silent
  fallback.
* The FAT update uses `mcopy -o`; the kernel/config/cmdline/DTB checks are the same as the
  installer (`tests/fixtures/pi4-sd/manifest.txt`).

## 5. Optional QEMU fallback

The primary path is host-side. If `mtools` or the host cannot touch a card (for example a card
reader that does not expose the partitions, or a host without `python3`), boot the image under
QEMU and write through Raven's own `fsd-ufs`/`fsd-ufs-rw` service:

1. Start the image with `cargo xtask run --arch aarch64 --profile pi4-rootfs` (or the SD3 image
   under QEMU with the `pi4-root-rw-test` profile).
2. In the guest, mount `ufs:` at `/root` (svcd already does) and copy the new file with `cp` or the
   `fsd-ufs` write path; the K55 RW probe is the reference implementation.
3. Unmount/remount and read the file back; the guest fsd-ufs fsck is the same `ufs-core` check.

This fallback is slower and needs a bootable QEMU image; it is documented only as a recovery path,
not the primary update loop. The host path is the one the acceptance exercises.

## 6. Donor reconciliation (DR-57 / DR-52)

| Topic | Illumos finding | BSD file/licence | Linux finding | Decision |
|---|---|---|---|---|
| UFS2 in-place file write | `ufs`/`ffs` write path and `fsck` state machine (CDDL, read-only) | NetBSD `sys/ufs/ffs`/`ufs`; already ported into `ufs-core` with the per-file BSD-2 notice (`36eddeb…`) | `fs/ufs` write path (GPL, read-only); `debugfs`/`e2fsprogs` not used | First-party `ufs-inject` over the existing `ufs-core` port; no new donor code |
| FAT boot partition | no FAT tool in the checkout | `mtools` is a host utility; no code copied | `mtools` package | Invoke `mcopy`/`mtype`; the product has no C and no linked FAT library |
| Backups/atomicity | `fsck`/mount safety model | NetBSD `fsck_ffs` preen model (behaviour only) | `e2fsck`/`fsck` outcomes | File-image backup + auto-restore; device slice backup; fsck after every write |
| Verify/read-back | `fsck` phases | `ufs-core::check` (BSD-2 port) | `fs/ufs` check behaviour | Read-only fsck plus byte-for-byte `cat` comparison before success |

Donor revisions: Illumos `586678ac1bcbd77644a6ec4dac7f79d676cfb4ec`; NetBSD
`36eddebcc36cb0775aa2962005b9c05419a6ff01`; Linux `adc218676eef25575469234709c2d87185ca223a`.
No CDDL/GPL code was copied, translated or closely paraphrased. The FAT path only invokes the
host `mtools` binary; it does not link or copy it.

## 7. Acceptance

```sh
cargo nextest run -p ufs-inject
sh scripts/pi4-update.sh --check
make check-fast
```

The report records the write/replace/read-back, fsck, offset, staging and program-injection
transcripts. Board turnaround and persistence are SD3/UI1.
