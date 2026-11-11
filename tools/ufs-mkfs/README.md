# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
# ufs-mkfs

A pure-Rust UFS1/UFS2 formatter and host-tree populator. It does not call
`newfs`, `makefs` or any host filesystem tool.

```sh
cargo run -p ufs-mkfs -- \
    -O 2 -b 4096 -f 512 -s 64m -T 1767225600 -d staging/ build/root.img
```

Options:

```
-O 1|2              UFS version (default 2)
-b bytes            block size (default 4096)
-f bytes            fragment size (default 512)
-s size             image size, optional k/m/g suffix (default: existing file size)
-d host-dir         populate the image from this directory
-T epoch            filesystem timestamp (default SOURCE_DATE_EPOCH or clock)
```

The library entry point is `ufs_core::mkfs::format(disk, MkfsOptions)`. It
writes the superblock, duplicate superblocks, cylinder groups, summary array,
inode table, root directory and `/lost+found`, then mounts the result. The
`ufs_mkfs::populate` function copies a host tree through the K53/K59 API:
files, directories and symlinks in sorted name order with owner 0:0 and the
host permission bits. `ufs_mkfs::format_image` combines both for the CLI.

## Geometry

The geometry follows `usr.sbin/makefs/ffs/mkfs.c`, the implementation that
made the B63 golden images: `fs_sblkno`, `fs_cblkno`, `fs_iblkno`,
`fs_dblkno`, `fs_sbsize`, block/fragment shifts, `fs_nindir`, `fs_inopb`,
`fs_maxcontig`, `fs_maxbpg`, `fs_maxbsize`, `fs_cgsize` and the
cluster-summary fields match the golden images for the same version/block/
fragment sizes. `fs_ipg`, `fs_fpg`, `fs_dblkno` and `fs_ncg` are density
dependent; the formatter uses `MkfsOptions::bytes_per_inode` as makefs's
density input.

Free counts, fragment histograms, cluster maps/summaries and inode maps are
computed from the bitmaps; the K52 `verify_allocation` check passes before and
after a remount. A fixed timestamp, byte-per-inode value and tree produce
identical bytes across runs.

## Limits

- Device size must be a multiple of the block size and fit the target.
- Only little-endian UFS1/UFS2 without extended attributes.
- The populator copies regular files, directories and symlinks; devices,
  sockets and FIFOs in a staging tree are rejected.
- Hard links in the host tree become independent files.
- The UFS2 superblock primary is at 8192 (makefs's small-image layout), not
  65536; duplicate superblocks are written at every cylinder group.
- `fsck_ffs` is an optional external oracle; it is not built by this ticket.
