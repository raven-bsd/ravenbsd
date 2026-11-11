# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
# ufs-fsck

Check and repair UFS1/UFS2 images without host `fsck_ffs`. The phases follow
NetBSD `sbin/fsck_ffs`:

1. inode type, size, block pointers, duplicate and out-of-range blocks;
2. directory entries, `.`/`..`, entries to free inodes, entry types;
3. connectivity: unreferenced directories are reconnected to `/lost+found`;
4. link counts and unreferenced inodes;
5. cylinder-group bitmaps, free counters, fragment histograms, cluster maps and
   the superblock summary.

```sh
ufs-fsck -n build/root.img     # report only; never writes
ufs-fsck -p build/root.img     # preen: repair and verify
```

Exit status: `0` clean, `1` repaired, `4` unrepairable or uncorrected (also what `-n`
returns when it finds something), `8` usage or open error.

`-p` repairs the normal preen inconsistencies: wrong link counts, block counts,
missing or wrong `.`/`..`, entries to unallocated inodes, orphaned directories
when `/lost+found` exists, bitmap and summary mismatches. An inode with a bad or
duplicate block is cleared wholesale, as `fsck_ffs` does. A missing
`/lost+found` is reported as unrepairable rather than created, because creating
a directory requires allocation outside the checker's read/repair model.

After repairing, `ufs-fsck` runs a read-only pass over the result and sets
`clean_after` from its findings. The superblock clean flag is set only when that
verification is clean.

## Library

The library entry point is `ufs_core::check::check(&mut disk, Mode)`; it returns
`Report { findings, fixed, clean_after }`. `Mode::ReadOnly` performs no writes.
`Finding` names the phase, the problem, and the inode or block involved. The
checker only uses the `BlockIo` trait and has no OS dependencies; the CLI in
this directory supplies a file-backed device.

## Tests

`cargo test -p ufs-core --test check` covers a clean image for UFS1/UFS2 and
4096/8192 block sizes, all four B63 golden images, byte-surgery corruption for
the classes above, repair followed by a clean read-only pass, and 2,000 random
single-byte mutations. `cargo test -p ufs-fsck` covers the CLI and its exit
codes. When `fsck_ffs` is available (`UFS_FSCK_FFS` or `$UFS_ORACLE_DIR/bin/`),
the oracle test also compares its verdict on a generated image; otherwise it
prints `ORACLE ABSENT` and passes.

## Provenance

B63 marks every `sbin/fsck_ffs` file `reference-only`, so this implementation is
fresh code written from the on-disk definitions in `sys/ufs/ffs/fs.h`,
`sys/ufs/ufs/dinode.h` and `dir.h`, and from reading the phase behaviour. No
`fsck_ffs` code or notice text is reproduced; there are no new `[[import]]`
entries in `PROVENANCE.toml`.
