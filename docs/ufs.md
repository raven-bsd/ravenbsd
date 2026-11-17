<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# UFS crash-consistency evidence

This note records the deterministic write-fault matrix for the UFS backend and the exact
recovery gap that remains. It does not claim that a dirty non-journaled image is always clean.

## Harness

* `tests/fs-fuzz/replay/src/lib.rs::crash_matrix()` mounts the B63 `ufs2-b4096` golden image
  read-write through a `BlockIo` wrapper that fails the Nth write, runs a fixed sequence
  (mkdir, create, write, hard link, symlink, nested mkdir, large write with indirect blocks,
  rename, unlink, rmdir), then remounts and runs `ufs_core::check`.
* `tools/fs-fuzz/tests/crash.rs` is the stable host test; `cargo test -p raven-fs-fuzz` runs it.
* `cargo run -p raven-fs-fuzz -- crash` prints the same matrix.
* `tests/fixtures/k55-ufs/probe` adds an on-target checkpoint to `fsd-ufs-rw`: write a file
  without syncing, stop `fsd-ufs`, run `check(Mode::ReadOnly)`, restart, and attempt a remount.

The wrapper rejects every out-of-range write with `IoError`; the UFS parser and the remount are
inside the test process, so a panic fails the test. No silent corruption is assumed: every cut
is followed by a read-only check on the resulting image.

## Non-journaled fault matrix (golden `ufs2-b4096`, 157 writes, 158 cuts)

| Stage | Cuts | Recovered (clean or repaired) | Refused |
|---|---:|---:|---:|
| mkdir | 10 | 10 | 0 |
| create | 5 | 5 | 0 |
| write | 3 | 3 | 0 |
| link | 3 | 3 | 0 |
| symlink | 6 | 6 | 0 |
| mkdir-sub | 10 | 10 | 0 |
| write-indirect | 50 | 50 | 0 |
| rename | 6 | 6 | 0 |
| unlink | 17 | 17 | 0 |
| unlink-large | 26 | 26 | 0 |
| rmdir | 18 | 18 | 0 |
| mount (budget 0..2) | 3 | 0 | 3 |
| none (golden, no writes) | 1 | 1 | 0 |
| **total** | **158** | **155** | **3** |

* 2 cuts were already clean; 153 additional cuts were repaired by `check(Mode::Repair)` and
  passed a second read-only check.
* 3 cuts (0..2) could not complete the initial writable mount and therefore failed closed; they
  are refused, not mounted.
* The previous 20 `repair-incomplete` cuts are now repaired: `check(Mode::Repair)` recreates a
  missing `/lost+found` and reconnects orphaned directories and unreferenced inodes, fixes the
  root link count it changes, and releases indirect data blocks by their true fragment count so
  the cylinder-group and superblock summaries reconcile.

## Journaled fault matrix (`format_journaled`, 17 MiB = 16 MiB fs + 1 MiB log, 27 writes, 28 cuts)

| Result | Cuts |
|---|---:|
| journal replayed, committed baseline readable, post-replay fsck clean | 22 |
| mount refused (write budget too small for the journal start, or a typed replay error) | 6 |
| failed data/allocation/fsck check | 0 |
| **total** | **28** |

Each journaled cut mounts the image read-write, replays the log, reads the committed `stable`
file, verifies allocation, unmounts, and then passes a read-only `check`. A cut before the commit
header does not expose the uncommitted transaction, while a committed prefix replays; the
committed baseline is never lost. The counts are separate from the non-journaled 158-cut matrix.

The on-target `fsd-ufs-rw` checkpoint reports:

```
fsd-ufs-rw: probe crash findings=1 clean=false
fsd-ufs-rw: probe crash replay=mounted
fsd-ufs-rw: probe crash ok
fsd-ufs-rw: probe refuse ok
fsd-ufs-rw: probe refuse-clean ok
fsd-ufs: mode=rw journal=active
fsd-ufs-rw: probe journal mount ok
fsd-ufs-rw: probe ok
```

## Guarantees

* A failed write returns `IoError`; no out-of-range write is performed.
* The write paths order metadata so a visible directory entry never points at an
  uninitialized inode; namespace operations remove the entry before freeing the target.
* `check(Mode::ReadOnly)` detects the remaining inconsistencies in every non-clean cut; the
  matrix does not label a findings cut as clean.
* `ufs_core::check::recover` replays a dirty WAPBL journal before the checker on a writable
  mount (`Recovery::Replayed`), then runs the read-only check and the preen repair; it refuses
  an image whose follow-up check is not clean. A dirty read-only mount is refused with
  `Unsupported("dirty journal on read-only mount")`.
* WAPBL is not enabled on the golden non-journaled image; the qtest formats a journaled image
  for the `journal=active` transcript.

## Exact remaining recovery gap

The only unresolved non-journaled cuts are the three budget-0..2 cases that cannot even perform
the initial writable mount; they fail closed and are refused. Every cut that produces a
mountable image is either clean after a read-only check or repaired to clean by
`check(Mode::Repair)`, so `fsd-ufs` never serves a repair-incomplete image.

A dirty journal is now replayed on a writable mount and refused on a read-only mount. Remaining
work: the 158-cut matrix uses the non-journaled golden image, so the journaled counts are a
separate 28-cut matrix, and hardware power-cut validation is out of scope for this ticket.
