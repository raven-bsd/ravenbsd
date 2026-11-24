<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# In-place UFS updates (`raven-ufs`)

Status: **OBSERVED (host).** `scripts/raven-ufs` updates an existing Pi 4 GPT image or a UFS
partition in place. It never copies the whole image, never writes the ESP/FAT partition, and
refuses mounted/root block devices or ambiguous GPT layouts. A Pi 4 board boot is not claimed
here; this is host-side tooling and host evidence.

## 1. Commands

```text
raven-ufs [--yes] ls IMAGE|DEVICE PATH
raven-ufs [--yes] put HOST DEST IMAGE|DEVICE
raven-ufs [--yes] [--recursive] rm PATH IMAGE|DEVICE
raven-ufs [--yes] mkdir PATH IMAGE|DEVICE
raven-ufs [--yes] [--delete] sync HOST_TREE DEST IMAGE|DEVICE
raven-ufs [--yes] check IMAGE|DEVICE
raven-ufs [--yes] repair IMAGE|DEVICE
```

`put` preserves the host mode, uid, gid and mtime and replaces an existing regular file.
`sync` adds/replaces directories, files and symlinks, preserving mode/uid/gid/mtime; `--delete`
removes destination entries absent from the host tree. `rm --recursive` removes a directory
tree. Every write operation runs the read-only UFS checker and fails if the result is not clean;
`check` is the same checker without a write. `repair` preens the UFS slice in place (the
`ufs-fsck` preen phase set) and verifies the result; use it after a guest shutdown left the rw
root dirty.

`--offset N --size N` overrides auto-detection for a known slice, but both are required together.

## 2. Target detection and safety

* A **raw UFS image** (no GPT signature) is used at offset 0.
* A **whole Pi 4 image or whole disk** must be a validated GPT with exactly two partitions:
  an ESP/FAT partition followed by a FreeBSD-UFS partition. The protective MBR, primary and
  backup headers, header/entry CRCs, usable range, partition types and overlap are checked.
* The **QEMU UFS-root disk** is a validated GPT with exactly one FreeBSD-UFS partition; the same
  offset/size checks apply. Any other count or type is refused; the tool does not guess.
* A **partition device** such as `/dev/sdX2` is used at offset 0, but it must pass the UFS check
  before any write.
* Every block-device operation requires `--yes`. A block device that is mounted, or that is the
  running root or contains it, is refused even with `--yes`.
* Only the UFS slice (`--offset/--size`) is opened by `ufs-inject`. The ESP/FAT bytes are never
  passed to a writer. The selftest hashes the ESP before/after and compares the inode and size of
  the image to prove the update was in place.

## 3. Whole Pi 4 SD image

```sh
cargo xtask image --arch aarch64 --platform pi4 --profile pi4-rootfs
image=build/aarch64/ravenbsd-pi4-sd.img

cargo build -p ufs-inject -p ufs-fsck
scripts/raven-ufs check "$image"
scripts/raven-ufs put etc/motd /etc/motd "$image"
scripts/raven-ufs sync build/pi4-rootfs / "$image"
scripts/raven-ufs check "$image"
```

The FAT/kernel path is separate from the UFS root. Update `kernel8.img`, the DTB or firmware with
`scripts/pi4-update.sh --kernel PATH --dtb PATH` (or mtools), then write the completed image to the
card with the normal flashing flow. `raven-ufs` does not touch that partition.

For a mirror update with deletion:

```sh
scripts/raven-ufs --delete sync build/pi4-rootfs / "$image"
```

Use `--delete` only on a tree that is the complete intended root; it removes every destination
entry outside the tree, including unrelated files.

## 3a. QEMU UFS-root disk

The K176 image build assembles `build/x86_64/qemu-ufs-root.img` as a one-partition GPT with a
256 MiB UFS2 root. Update it in place after the image build and boot the same root:

```sh
cargo xtask image --arch x86_64 --profile qemu-ufs --output build/x86_64/qemu-ufs.img
scripts/raven-ufs check build/x86_64/qemu-ufs-root.img
scripts/raven-ufs put build/motd /etc/ufs-update.txt build/x86_64/qemu-ufs-root.img
scripts/raven-ufs check build/x86_64/qemu-ufs-root.img
```

The ESP (`build/x86_64/qemu-ufs.img`, or the qtest case ESP) is a separate file and is never
passed to `raven-ufs`. The `ufs-update` qtest hashes the baseline ESP before/after the update; the
observed value was `160ae00e9dbdbc44a7f25980303cddc0658aa115852335e5cb42fe85e1a2bab6`
(unchanged), and the boot read `k185-first` through `/`. The `ufs-update-persist` pre step repairs
the dirty post-boot UFS, replaces the same file, and the next boot reads `k185-second`.

## 4. `/dev/sdX2` partition

Unmount every partition of the card before writing. The UFS partition must not be the running
root. Identify the UFS partition; do not pass the ESP/FAT partition.

```sh
lsblk -o NAME,SIZE,FSTYPE,MOUNTPOINT /dev/sdX
sudo umount /dev/sdX1 /dev/sdX2 2>/dev/null || true

cargo build -p ufs-inject -p ufs-fsck
scripts/raven-ufs --yes check /dev/sdX2
sudo scripts/raven-ufs --yes put build/motd /etc/motd /dev/sdX2
sudo scripts/raven-ufs --yes sync build/pi4-rootfs / /dev/sdX2
sudo scripts/raven-ufs --yes check /dev/sdX2
sync
```

The device must be passed whole (`/dev/sdX2`), not a mounted filesystem path. A wrong or mounted
device is refused before `ufs-inject` opens it. Boot the Pi 4 only after `sync`; a board run is
not part of this ticket.

## 5. In-place evidence

`timeout 120 sh scripts/raven-ufs-selftest.sh` builds an 8 MiB UFS2 image, wraps it in a GPT with
a 1 MiB ESP, and performs put/rm/mkdir/sync/check. The observed run:

```text
raven-ufs-selftest: corrupt-GPT refusal PASS
raven-ufs-selftest: unconfirmed block-device refusal PASS
raven-ufs-selftest: mounted block-device refusal PASS
raven-ufs-selftest: ESP sha256 16c7f1d8a38b4b84560e558ab03b13c82e2ff374d87eaacb4df22f03604e7a4f (unchanged)
raven-ufs-selftest: inode N size 10502656 (unchanged)
raven-ufs-selftest: PASS
```

The unchanged ESP hash, inode and size show the FAT bytes were not written and the image was
modified in place rather than replaced. `ufs-inject`/`ufs-fsck` host tests cover create, replace,
read-back, metadata, partition offsets, rm and fsck.

## 6. Scope

- No kernel, libc, UFS-base or boot change; the tool is a host front end over the existing
  `ufs-inject` writer and checker.
- No board result, no HDMI/UART claim and no release image is produced by this ticket.
- The FAT/ESP partition is never opened for writing; kernel/firmware updates use the separate
  `pi4-update.sh` path.

## 7. Donor reconciliation (DR-52)

| Topic | Illumos finding | BSD file/licence | Linux finding | Decision |
|---|---|---|---|---|
| In-place UFS mutation | UFS `fsck`/write path is behavioural reference only (CDDL). | NetBSD `sbin/fsck_ffs` and FFS inode/directory routines are the existing `ufs-core` port (BSD-3-Clause); no new code copied. | `e2fsprogs` debugfs outcome only (GPL, read-only). | Reuse the existing first-party `ufs-inject` calls; no new on-disk algorithm. |
| GPT/partition detection | No GPT in the UFS donor path. | NetBSD `gpt(8)`/disklabel outcomes only. | `util-linux` GPT layout/validation outcome only. | First-party GPT parser matching the Pi 4 ESP+UFS layout; refuse ambiguity. |
| Safe device writes | Not applicable. | `fsck_ffs` preen outcome only. | Mounted-device/root protection behaviour only. | Require `--yes`, parse `lsblk`/`/proc/mounts`, refuse mounted/root; hash ESP in the test. |
