<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Linux metadata syscalls in `linuxd`

Status: B25 implements the Linux metadata batch over the synthetic tree (`/proc`, `/sys` and the
per-process descriptors). The Linux root tree is **not** served yet; B33 owns the mounted
`/compat/linux` root. This page records the layout, the metadata model and the exact boundary.

## 1. Syscall table

| x86-64 nr | name | behaviour |
|---|---|---|
| 4 | `stat` | Resolve the path (following a final symlink) and copy an x86-64 `struct stat` (144 bytes). |
| 5 | `fstat` | Same layout for an open descriptor. 0/1/2 are synthetic character devices. |
| 6 | `lstat` | Like `stat`, but the final symlink itself is reported. |
| 21 | `access` | Check `F_OK`/`R_OK`/`W_OK`/`X_OK` against the synthetic mode bits. |
| 76 | `truncate` | Resolve the path; a file answers `EROFS`, a directory `EISDIR`. |
| 260 | `fchownat` | Resolve the path; `EROFS`. |
| 262 | `newfstatat` | `stat`/`lstat` with `dirfd`, `AT_SYMLINK_NOFOLLOW`, `AT_NO_AUTOMOUNT` and `AT_EMPTY_PATH`. |
| 268 | `fchmodat` | Resolve the path; `EROFS`. |
| 280 | `utimensat` | Resolve the path; `EROFS` (timestamps are never changed). |
| 332 | `statx` | Copy an x86-64 `struct statx` (256 bytes) with `stx_mask = STATX_BASIC_STATS`. |

`fstat`, `newfstatat`, `statx` and the `*at` mutators first validate the `AT_*` flags they accept;
an unknown flag is `EINVAL`. A relative path uses `dirfd` when it names an open synthetic
directory. With `AT_FDCWD` the synthetic current directory is `/`; absolute paths ignore `dirfd`.
Only one symlink hop is needed for the current tree, but the resolver follows up to 40 links and
returns `ELOOP` beyond that.

## 2. Layout

The encoders live in `userland/servers/linuxd/src/stat.rs`. They take the field offsets from
`raven_linux_abi::structs::x86_64::{Stat, Statx}` (whose compile-time layout assertions are owned
by `raven-linux-abi`) and write little-endian values into a zeroed buffer. No pointer casts and no
second copy of the layout are used.

`struct stat` (144 bytes):

| field | value |
|---|---|
| `st_dev`, `st_rdev` | 0 |
| `st_ino` | deterministic FNV-1a of the normalized path (non-zero) |
| `st_nlink` | 2 for a directory, 1 otherwise |
| `st_mode` | type bits plus synthetic permissions |
| `st_uid`, `st_gid` | 0 |
| `st_size` | file/symlink bytes; 4096 for a directory |
| `st_blksize` | 4096 |
| `st_blocks` | `ceil(st_size / 512)` |
| `st_atime`, `st_mtime`, `st_ctime` | 0 (no synthetic clock) |

`struct statx` (256 bytes) carries the same values in `stx_*`, with `stx_mask = 0x7ff`
(`STATX_BASIC_STATS`), `stx_attributes = 0`, `stx_attributes_mask = 0` and zero timestamps.
Unsupported/unknown `statx` mask bits are not claimed.

## 3. Synthetic metadata

| resource | mode | size | `fstat` |
|---|---|---|---|
| regular synthetic file | `S_IFREG \| 0444` | content length | open descriptor |
| synthetic directory | `S_IFDIR \| 0555` | 4096 | open descriptor |
| symlink | `S_IFLNK \| 0777` | target length | open descriptor (if it could be opened) |
| fd 0 | `S_IFCHR \| 0666` | 0 | `/dev/null` |
| fd 1/2 | `S_IFCHR \| 0666` | 0 | `tty:console` |

The tree is owned by uid/gid 0 but is **read-only**. `access(W_OK)` therefore answers `EACCES`
even for uid 0, and every metadata mutator resolves the path first and then answers `EROFS`
(`ENOENT` for a missing path, `EISDIR` for `truncate` on a directory). This is intentional: the
server has no write path and must not pretend a `chmod`/`chown` succeeded.

`fstat` on 0/1/2 does not answer `EBADF`; the server owns those descriptors as synthetic character
devices. Any unopened descriptor answers `EBADF`.

## 4. Symlinks and paths

`stat` follows the final symlink; `lstat` and `newfstatat(..., AT_SYMLINK_NOFOLLOW, ...)` report
the link. A target outside the synthetic tree (for example `/proc/self/exe` pointing at `/hello`,
which is only a loaded image, not a synthetic file) answers `ENOENT`, exactly as a missing real
target would. `/proc/self/fd/<n>` is a symlink whose target is the opened path.

Path normalization removes `.`/`..` and duplicate/trailing slashes. The trap loop copies a bounded
128-byte path window; the pointer is argument 0 for `stat`, `lstat`, `access` and `truncate` and
argument 1 for the `*at` calls and `statx` (`trap::copy_in_arg`).

## 5. Tests

`cargo test -p linuxd` covers:

* the 144/256-byte layouts and field offsets; mode/inode/nlink/size/blocks/blksize, zero
  uid/gid/timestamps, `statx` mask and 16-bit mode;
* `stat`, `lstat` and the symlink-follow `ENOENT` boundary; relative paths;
* `fstat` on 0/1/2, an open file and a closed descriptor;
* `newfstatat` absolute, directory-relative, `AT_SYMLINK_NOFOLLOW`, `AT_EMPTY_PATH`, unknown flags
  and `EBADF`;
* `statx` basic stats and empty-path `fstat` semantics;
* `access` read/write/execute on files and directories, invalid mode bits and missing paths;
* `fchmodat`, `fchownat`, `utimensat`, `truncate` (`EROFS`, `ENOENT`, `EISDIR`, `EINVAL`);
* the trap copy-in table: every path-taking number and the path pointer index.

The qtest `cargo xtask qtest --case linux-stat --arch x86_64` runs a static musl probe that checks
the same fields through raw syscalls and prints one line per case.

## 6. BusyBox boundary

With the B32a Alpine root staged, BusyBox reaches `stat(".")` and the B25 handler answers it, but
the synthetic tree has no root directory. The recorded serial output changes from:

```
ls: .: Function not implemented
linuxd: unimplemented nr=4 name=stat hits=1
```

to:

```
ls: .: No such file or directory
linuxd: syscall stat calls=1 errors=1
```

`linux-root.toml` still asserts the old B32a boundary and is outside this ticket's Owns. A full
`busybox ls` needs the initfs-backed `/compat/linux` root (B33); B25 deliberately does not invent
one.

## 7. Provenance

The layout values come from the project's `raven-linux-abi` UAPI tables (derived from
`linux-raw-sys` as documented there). No Linux kernel source, donor implementation or BusyBox code
was copied, translated or closely paraphrased. The stat probe and all handler code are first-party
BSD-3-Clause.
