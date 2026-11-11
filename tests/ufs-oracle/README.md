# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
# UFS differential tests (B64, host half)

```sh
cargo test -p ufs-oracle-tests
```

By default the test runs 20 fixed seeds x 5,000 operations. `UFS_ORACLE_SEEDS`
overrides the seed count and `UFS_ORACLE_OPS` the operations per seed. The
golden image is `tests/ufs-vectors/golden/ufs2-b4096.img.gz`; set
`UFS_ORACLE_IMAGE` to a raw image or `UFS_VECTORS_DIR` to another vector
directory.

## What it does

The generator starts from a fresh writable mount and creates `/b64` (the
golden tree itself is never modified). It then applies a deterministic mix of
`create`, `mkdir`, `symlink`, `link`, `unlink`, `rmdir`, `write`, `truncate`,
`setattr` and `rename`, with a size cap that keeps the in-memory tree small.
Every 250 operations it compares the whole `/b64` subtree:

* path set, inode numbers, object types, modes and uid/gid;
* regular-file sizes and FNV-1a content digests;
* symlink targets and sizes;
* link counts derived from path references (files) or `2 + child dirs`
  (directories);
* directory inode sizes are checked to be a multiple of 512 but are not
  predicted exactly (directory packing is intentionally not modelled);
* `ufs_core::Ufs::verify_allocation()`.

The model in `src/model.rs` keeps its own path/inode tables and shares no code
with `ufs-core`.

At the end the image is cleanly unmounted and written to
`build/ufs-oracle-tests/seed-NN.img`. Two optional oracles run:

* `fsck_ffs -n` when `UFS_FSCK_FFS` points at a binary (or `UFS_ORACLE_DIR`
  contains `bin/fsck_ffs`); a clean unmount must exit 0.
* `tools/ufs-oracle/check-ufs.py` with a manifest generated from the model
  plus the untouched B63 baseline (the root link count is adjusted for the
  new `/b64` directory). This needs `python3` and `sha256sum`; when either is
  absent the seed records `absent`.

The summary line printed by the test reports the seed count, operations and
whole-tree/allocation checks. OBSERVED 2026-11-11 on this host: 20 seeds,
100,000 operations and 420 checks complete in about 36 seconds with
`image=ok` for every seed; `fsck=absent` because B63 could not build
`fsck_ffs`.

## Note on the oracle reader

`check-ufs.py` was corrected to use the upstream `size < maxsymlinklen` rule
for the historical UFS fast-symlink off-by-one; a 120-byte UFS2 target is a
slow symlink and lives in a data block.

## Licence

Fresh BSD-3-Clause test code. The golden image and `check-ufs.py` come from
B63; no donor source is copied into this directory.
