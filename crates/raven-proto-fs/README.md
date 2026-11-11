<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# raven-proto-fs

Wire contract between the RavenBSD filesystem servers (`initfsd`, `ramfsd`, `fsd-ufs`) and their
clients. It is the typed layer above the S11 scheme packets described in
`architecture/03-system-servers.md` section 2 and `architecture/16-first-shell.md` section 3.

The crate is `no_std`, has one dependency (`raven-abi` for errno values), forbids `unsafe`, never
allocates in an encoder and never panics while decoding. Every integer is little-endian and is
written explicitly with `to_le_bytes`. Every length is checked before use. Every table below is
pinned by a golden test in the module tests or in `tests/fuzz.rs`.

## Rules

* `OPEN`, `READ`, `WRITE`, `CLOSE`, `DUP`, `FSTAT` and `CALL` keep the S11 opcodes.
* `OPEN` carries a path relative to the server root (no leading `/` required, at most 255 bytes)
  and the compact wire open-flag set in [`open`](src/open.rs), **not** Linux `O_*` numbers:
  READ, WRITE, CREATE, EXCL, TRUNC, APPEND, DIRECTORY and NOFOLLOW. Unknown bits and invalid
  combinations are rejected with `EINVAL`. `OpenFlags::from_linux`/`to_linux` is the translation
  layer for client APIs. `O_CREAT` uses mode `0o644` (0o666 masked by the server umask 0o022);
  protocol version 1 has no per-client umask or mode on the wire.
* `READ`/`WRITE` on files are sequential: the server keeps the cursor in the open-file description
  and the wire offset must be zero. `DUP` shares the description; the server releases it at the
  last close. `READ` on a directory returns packed directory entries and uses the same per-OFD
  sequential cursor (a change to the directory invalidates it with `EINVAL`). `FSTAT` returns a
  `Stat` record.
* Servers reject `.`/`..` path components; `vfsd` normalises absolute paths first with
  `normalize_request`, which also reports a trailing-slash or final-dot directory requirement.
* Control path operations use `CALL` with the versioned layout below. Unknown versions and
  operations are `-ENOSYS`.
* Responses use a negative Linux errno (`-4095..=-1`) or zero. The crate's `ProtoError::errno`
  maps framing and validation failures to `EPROTO`, `EINVAL` or `ENOSYS`.
* Names and paths are UTF-8 bytes, never NUL-terminated, and never contain NUL. Directory entry
  names additionally forbid `/`.

## `Stat` (`FSTAT`)

Exactly 64 bytes, little-endian.

| Offset | Size | Field |
|---|---|---|
| 0 | 8 | `ino` |
| 8 | 8 | `size` |
| 16 | 8 | `blocks` (512-byte units) |
| 24 | 8 | `mtime` (seconds, signed) |
| 32 | 8 | `ctime` (seconds, signed) |
| 40 | 8 | `atime` (seconds, signed) |
| 48 | 4 | `mode` (Linux `st_mode`) |
| 52 | 4 | `nlink` |
| 56 | 4 | `uid` |
| 60 | 4 | `gid` |

File-type values in `mode`: `S_IFREG 0o100000`, `S_IFDIR 0o040000`, `S_IFLNK 0o120000`,
`S_IFCHR 0o020000`, `S_IFBLK 0o060000`, `S_IFIFO 0o010000`, `S_IFSOCK 0o140000`; mask
`S_IFMT 0o170000`. Timestamps are whole seconds in protocol version 1.

## Directory entries

`READ` on a directory returns one or more records. Each record is a 16-byte header followed by
`name_len` name bytes, padded with zeroes to the next 8-byte boundary:

| Offset | Size | Field |
|---|---|---|
| 0 | 8 | `ino: u64` |
| 8 | 1 | `kind` (`DT_*`) |
| 9 | 1 | `name_len` (`1..=255`) |
| 10 | 6 | zero padding |
| 16 | `name_len` | name bytes |
| ... | 0..7 | zero padding to 8-byte alignment |

`kind` values: `DT_UNKNOWN 0`, `DT_FIFO 1`, `DT_CHR 2`, `DT_DIR 4`, `DT_BLK 6`, `DT_REG 8`,
`DT_LNK 10`, `DT_SOCK 12`. `record_len(name_len) = align8(16 + name_len)`.

Directory reads are sequential in protocol version 1. The server keeps the per-open-file-description
cursor; the wire `offset` must be zero. A zero-byte `READ` means end of directory. A buffer smaller
than the next complete record is `EINVAL`; a server must not return a partial record. On the first
read the server records the directory generation, and a create/unlink/rename between reads makes the
next read `EINVAL` (documented invalidation). `DirCookie` is the standard `(generation, index)`
server-side encoding for that cursor; it is not a live byte position and is not sent on the wire in
protocol version 1.

Example record for inode 7, regular file, name `abc` (24 bytes):

```text
07 00 00 00 00 00 00 00 08 03 00 00 00 00 00 00 61 62 63 00 00 00 00 00
ino                     kind len pad............... a  b  c  pad........
```

## `CALL` request

Common 16-byte header, then up to two length-prefixed paths:

| Offset | Size | Field |
|---|---|---|
| 0 | 2 | `version` = 1 |
| 2 | 2 | `op` |
| 4 | 4 | `flags` (reserved, send 0) |
| 8 | 4 | `arg0` |
| 12 | 4 | `arg1` |
| 16 | 2 | first path length (`1..=255`) |
| 18 | N | first path bytes |
| 18+N | 2 | second path length, when the operation has two paths |
| 20+N | M | second path bytes |

`TRUNCATE` uses `arg0` as the low 32 bits and `arg1` as the high 32 bits of a `u64` size. Other
operations put a mode in `arg0` and leave `arg1` zero. Unknown `op` or version is `-ENOSYS`.

| Op | Code | `arg0` | `arg1` | Paths |
|---|---|---|---|---|
| MKDIR | 1 | mode | 0 | `path` |
| UNLINK | 2 | 0 | 0 | `path` |
| RMDIR | 3 | 0 | 0 | `path` |
| RENAME | 4 | 0 | 0 | `path_from`, `path_to` |
| TRUNCATE | 5 | size low | size high | none (open file) |
| SYMLINK | 6 | 0 | 0 | `path`, `target` |
| READLINK | 7 | 0 | 0 | `path` |
| CHMOD | 8 | mode | 0 | `path` |
| SYNC | 9 | 0 | 0 | none |
| MOUNT | 10 | 0 | 0 | `path`, `scheme` |
| UMOUNT | 11 | 0 | 0 | `path` |
| STATFS | 12 | 0 | 0 | none |

Example `MKDIR(mode = 0o755, path = "hello")` (23 bytes):

```text
01 00 01 00 00 00 00 00 ed 01 00 00 00 00 00 00 05 00 68 65 6c 6c 6f
version op  flags       arg0=mode    arg1        len  h  e  l  l  o
```

`MOUNT` and `UMOUNT` are implemented only by `vfsd`.

## `CALL` response

The first four bytes are always `status: i32`. A negative status has no payload. On success:

| Op | Payload after status |
|---|---|
| READLINK | `target_len: u16` (`1..=255`), then `target_len` bytes |
| STATFS | `StatFs` record (36 bytes, below) |
| all others | none |

`StatFs` payload (36 bytes, little-endian):

| Offset | Size | Field |
|---|---|---|
| 0 | 4 | `block_size` |
| 4 | 8 | `blocks` |
| 12 | 8 | `free_blocks` |
| 20 | 8 | `files` |
| 28 | 8 | `free_files` |

Example successful `READLINK` of `abc` (9 bytes):

```text
00 00 00 00 03 00 61 62 63
status      len  a  b  c
```

## Path helpers

* `split_first_component(path)` strips leading `/`, collapses redundant slashes, rejects an
  empty path, a first component of `.` or `..`, a component longer than 255 bytes, a total path
  longer than 255 bytes, or NUL. It returns the first component and the remainder (which never
  starts with `/`). Servers call it repeatedly and reject later dot components.
* `normalize_absolute(path, out)` canonicalises an absolute path in place: removes empty and `.`
  components, pops `..`, rejects `..` above `/`, bounds each component and the 255-byte total,
  and returns a `&str` borrowed from `out`. It does not touch the filesystem and does not resolve
  symlinks.
