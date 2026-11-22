<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# UFS durability gates — ENOSPC and sync persistence

Status: **blocked on the virtio-blk MSI-X completion path (K427)**. The two
guest cases and the probe are implemented and committed; on `fb1112c1` the block
driver floods the console with `irq: unclaimed vector 0x41` under disk I/O, so
the filesystem rows cannot be exercised reliably. No UFS defect was observed.

## Rows

| Row | Contract | Case |
|---|---|---|
| 8 ENOSPC | fill a small UFS2 image; the failing write returns `ENOSPC`, acknowledged bytes stay readable, free blocks reach the documented floor | `tests/qtest/cases/ufs-enospc.toml` |
| 9 sync persistence | write a file, a directory entry and a rename with two syncs each, kill QEMU, remount the same image and compare every byte | `tests/qtest/cases/ufs-fsync-persist.toml` + `ufs-fsync-check.toml` |

The probe is `tests/fixtures/ufs-durability/probe` (static `no_std`,
`raven-ufs-durability-probe`). `tests/fixtures/ufs-durability/build.sh <4m|16m>`
builds it, creates a one-partition GPT/UFS2 disk with `ufs-mkfs` +
`raven-k54-mkimage`, and injects the probe at `/bin/ufs-probe` (mode 0755) with
`scripts/raven-ufs put`. Both cases boot the `ufs-demo` profile, mount the disk
at `/mnt` and run `/mnt/bin/ufs-probe <mode>`.

`ufs-fsync-persist` sends the file, directory-entry and rename writes, prints the
checksums, then ends without unmounting; the case's timeout kills QEMU, which is
the power cut. `ufs-fsync-check` boots the same image and verifies every 4 KiB
chunk against the same deterministic pattern.

## Current blocker

On `fb1112c1` the first block I/O produces:

```text
devd: driver virtio-blk msix vector=65 table=0x0 bar=1
virtio-blk: msix table=0x0 bar=1 vector=1
virtio-blk: completion polled=5
irq: unclaimed vector 0x41 (eoi)
irq: unclaimed vector 0x41 (eoi)
...
```

K427 (`b0a58419`) closes `IRQ_HANDLE` unconditionally in
`userland/drivers/virtio-blk/src/main.rs` `fn run` at line 232. When `msix=1`
that handle is the MSI-X vector handle; the device's MSI-X table entry stays
programmed, the queue is poll-driven, and every completion raises vector `0x41`
with no kernel owner. The console flood makes the boot and the fill
unreliable; `ufs-enospc` never reaches its marker and `ufs-fsync-check` did not
reach `login:` within 60 s. The write half did once print

```text
k466: persist write bytes=65536 checksum=4090c52047982d5a sync=ok
k466: persist dir=ok
k466: persist rename=ok
```

so the probe and the UFS path are working; the block driver is the blocker.

The fix belongs to K427: close the legacy IRQ only when `msix.is_none()`, or mask
the programmed MSI-X entry before closing the handle. K466's Owns does not
include `userland/drivers/**`.

## Donor reconciliation

| Topic | Illumos finding | BSD file/licence | Linux finding | Decision |
|---|---|---|---|---|
| block allocation failure | `hat_i86` design; no copy | NetBSD `sys/ufs/ffs/ffs_alloc.c` `ffs_alloc` (lines 175-292): `cs_nbfree == 0` and `freespace(fs, fs_minfree) <= 0` → `nospace` → `ENOSPC`; BSD-2 | `ext4` outcome only | Raven `ufs-core` `has_room`/`hash_alloc` already mirrors it; no change |
| sync / fsync | ZIL behaviour only | NetBSD `sys/ufs/ffs/ffs_vnops.c` `ffs_fsync` (290), `sys/ufs/ffs/ffs_vfsops.c` `ffs_vfs_fsync` (2536) and `ffs_full_fsync`: flush pages, WAPBL, cache sync; BSD-2 | `ext4_sync_file` outcome only | Raven `ufs-core` `Ufs::sync` writes summaries/superblock, commits the journal and calls `disk.flush`; `fsd-ufs` `view::sync` is the S11 `SYNC` entry |
| FreeBSD lineage | — | `sys/ufs/ffs/ffs_alloc.c`, `sys/ufs/ffs/ffs_vfsops.c` at `1333444...` are **not present** in the sparse `ravenbsd-resources/freebsd` checkout (no `sys/ufs`); the behaviour was read from NetBSD `36eddebc...` | — | record the sparse-checkout blocker; no FreeBSD source was copied |

## Limits

- The persistence test proves the observable contract (write/rename + sync +
  power-cut + remount + byte compare). A no-op `sync` mutation is expected to
  fail the check only if the disk path defers writes; the current stack is
  write-through and QEMU's host page cache is shared across processes, so the
  mutation result will be recorded honestly when the blocker is fixed.
- The cases use the external virtio-blk demo disk; the RAM-disk `fsd-ufs-rw`
  profile cannot prove persistence across a VM kill.
