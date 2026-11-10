<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->
# UFS golden vectors

Input for the first `ufs-core` read ticket (K51): four real UFS images made
by NetBSD `makefs` and a manifest that describes every path in them. The
images are committed as `golden/<name>.img.gz`; the manifests are in
`expected/<name>.manifest`.

OBSERVED 2026-11-10: all four images are 16 MiB raw, 253-263 KiB gzipped,
and pass `tools/ufs-oracle/check-ufs.py` with `OK: 1639 paths match`.

## Images

| Name | Version | Block size | Fragment size | Raw | gzip |
|---|---|---:|---:|---:|---:|
| `ufs1-b4096` | UFS1 | 4096 | 512 | 16 MiB | 262715 |
| `ufs1-b8192` | UFS1 | 8192 | 1024 | 16 MiB | 254576 |
| `ufs2-b4096` | UFS2 | 4096 | 512 | 16 MiB | 256249 |
| `ufs2-b8192` | UFS2 | 8192 | 1024 | 16 MiB | 253351 |

`makefs` uses `SBLOCK_UFS1` (8192) even for UFS2 images made for small
filesystems, so the primary superblock is at 8192 in every image. UFS2
images are made with `extattr=0`; the default would produce the
`FS_UFS2EA_MAGIC` extended-attribute variant, which K51 explicitly does not
cover.

## Source tree

`gen-tree.sh` creates the same tree for every image (1639 paths including
the root):

| Case | Paths |
|---|---|
| empty file, 10-byte file | `empty`, `ten.txt` |
| fragments 1, 3 and 7 KiB | `frag1k.bin`, `frag3k.bin`, `frag7k.bin` |
| one block, 12 and 13 blocks | `block1.bin`, `blocks12.bin`, `blocks13.bin` |
| 600 blocks (single indirect at 8 KiB; double at 4 KiB) | `blocks600.bin` |
| 3 MiB (double indirect at 4 KiB; single at 8 KiB) | `big3m.bin` |
| hard links (same inode, nlink 2) | `hardlink-a.txt`, `hardlink-b.txt` |
| fast symlink (11-byte target) | `fastlink` |
| slow symlink (200-byte target) | `slowlink` |
| empty directory, 1/100/1500-entry directories | `emptydir`, `dir1`, `dir100`, `dir1500` |
| 255-byte file name | `nnn...nnn.txt` |
| 12 nested directories | `deep/d01/.../d12/leaf.txt` |
| setuid and private modes | `mode4755.bin`, `mode600.txt` |
| uid 1000/gid 100, character device, FIFO | every path, `cdev`, `fifo` |

The large files are text with the 4 KiB logical block number on every
line, so they compress well and are easy to inspect by eye.

`gen-tree.sh` re-executes itself under `fakeroot` when it is not root. That
is how the tree gets uid 1000/gid 100 and a character device without
privileges. If neither root nor `fakeroot` is available the script fails
instead of silently producing different metadata.

## Manifest format

One line per path, sorted with `LC_ALL=C`, with nine `|`-separated fields:

```
path|type|mode|uid|gid|size|sha256|link-target|nlink
```

* `path` starts with `/`; the root is `/`.
* `type` is `file`, `dir`, `symlink`, `char`, `block`, `fifo` or `socket`.
* `mode` is four octal digits including setuid/setgid (for example `4755`).
* `size` is the regular-file length; for a symlink it is the target length;
  for a device or FIFO it is `0`; for a directory it is `-` because the host
  directory size is not the image directory size.
* `sha256` is the file content hash, or `-` for non-regular files.
* `link-target` is the symlink target, or `-`.
* `nlink` is the source link count. makefs preserves it, including hard
  links.
* Symlink modes are recorded as `0755`: NetBSD makefs normalises them
  (`walk.c`: "force them to 0755"); Linux `lstat` reports `0777`.

The four manifests are byte-identical by design: they describe the same
source tree. The images differ in superblock, inode and block layout, and
`make-vectors.sh` checks each image against its manifest with
`tools/ufs-oracle/check-ufs.py` before committing the gzip.

`check-ufs.py` is a small independent UFS1/UFS2 reader used only as a
test oracle: it reads the superblock at 8192, maps direct, single, double
and triple indirect blocks, walks the directory tree and compares every
field. It is not a mount implementation.

## Regeneration

```sh
sh tools/ufs-oracle/build-oracles.sh       # builds makefs (8 s, see its README)
sh tests/ufs-vectors/make-vectors.sh       # tree, four images, manifests, self-check
```

`make-vectors.sh` uses `makefs -B little -T 1767225600` and `gzip -9 -n`,
so the output does not depend on the wall clock, the host name or the
absolute path. OBSERVED: running it twice produces the same SHA-256 for
all four images and all four manifests.

Final observed hashes:

```
e06ed81d9d1fb827023fbeb814197c3e87a51c84519193b856bca2c273464f1f  ufs1-b4096.img.gz
5d652091e63d5b9e239e8b9f60f16bc9b367b5ad3ab8c7915ba86e111d7cb2ee  ufs1-b8192.img.gz
74383f506838952c99bf3979d6022c06528369fc0b0ced2a41040d91ce9622f1  ufs2-b4096.img.gz
3b555e258055375e2895fcc4a3cf8644574c2c1313387524cf562f6fef86d6d1  ufs2-b8192.img.gz
2965403c63dc78f44392e9f7f484d9204559b79b7438a975e4df2d8a9782b717  <each manifest>
```

Committed payload: 1,749,811 bytes, below the 8 MiB budget.

## fsck_ffs status

`fsck_ffs` could not be built on this host; see
`tools/ufs-oracle/README.md` for the exact blocker and the proposed
fallbacks. `make-vectors.sh` has a branch that runs `fsck_ffs -n -F` and
records exit codes in `$UFS_ORACLE_DIR/fsck-results.txt` if a binary is
ever placed there; today it prints `fsck_ffs not built`.

## Licence audit

`LICENCE-AUDIT.md`, `licence-audit.tsv` and `NOTICE-UFS.txt` are generated
by `tools/ufs-oracle/audit.py`. The images are generated data; no NetBSD
source is copied into this directory.
