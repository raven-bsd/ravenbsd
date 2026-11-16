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

## Fault matrix (golden `ufs2-b4096`, 157 writes, 158 cuts)

| Stage | Cuts | Recovered (clean or repaired) | Unresolved |
|---|---:|---:|---:|
| mkdir | 10 | 9 | 1 |
| create | 5 | 4 | 1 |
| write | 3 | 3 | 0 |
| link | 3 | 3 | 0 |
| symlink | 6 | 5 | 1 |
| mkdir-sub | 10 | 9 | 1 |
| write-indirect | 50 | 49 | 1 |
| rename | 6 | 6 | 0 |
| unlink | 17 | 13 | 4 |
| unlink-large | 26 | 23 | 3 |
| rmdir | 18 | 10 | 8 |
| mount (budget 0..2) | 3 | 0 | 3 |
| none (golden, no writes) | 1 | 1 | 0 |
| **total** | **158** | **135** | **23** |

* 2 cuts were already clean; 133 additional cuts were repaired by `check(Mode::Repair)` and
  passed a second read-only check.
* 3 cuts (0..2) could not complete the initial writable mount and therefore failed closed.
* 20 cuts have a residual `repair-incomplete` finding after the preen repair: 11, 16, 27, 38,
  43, 100, 101, 108, 109, 114, 115, 116, 140, 141, 142, 143, 149, 150, 151, 152.

The on-target `fsd-ufs-rw` checkpoint reports:

```
fsd-ufs-rw: probe crash findings=1 clean=false
fsd-ufs-rw: probe crash replay=fail-closed
fsd-ufs-rw: probe crash ok
```

## Guarantees

* A failed write returns `IoError`; no out-of-range write is performed.
* The write paths order metadata so a visible directory entry never points at an
  uninitialized inode; namespace operations remove the entry before freeing the target.
* `check(Mode::ReadOnly)` detects the remaining inconsistencies in every non-clean cut; the
  matrix does not label a findings cut as clean.
* WAPBL remains available as an existing on-disk journal; it is not enabled for the golden
  image or the `fsd-ufs-rw` profile (`journal=none`).

## Exact remaining recovery gap

`fsd-ufs` does **not** run `check(Mode::Repair)` on mount, does not automatically replay a dirty
WAPBL log in this non-journaled qtest, and does not refuse a non-journaled dirty image that
`check` finds inconsistent. After a crash, the server may restart and serve an image that the
read-only check flags; the on-target qtest observes that case as `replay=fail-closed` when the
remount itself fails, but the general policy is not implemented.

The required follow-up is a mount-time recovery policy: detect the unclean flag/WAPBL log, run
the preen repair (or a journal replay for a journaled image), refuse to serve if the second
read-only check is not clean, and report the residual findings. The 20 `repair-incomplete`
cuts are the exact cases that need the stronger repair policy; the current preen repair is not
sufficient.
