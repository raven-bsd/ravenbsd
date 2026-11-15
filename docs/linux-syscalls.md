<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# linuxd syscall batches: metadata (B25)

Status: B25, 2026-11-15. Evidence tags: **OBSERVED** (run on the target or read in the source),
**PROPOSED** (not implemented), **UNVERIFIED** (not exercised).

## 1. Scope

B32/B32a stage the Alpine minirootfs and load its PIE BusyBox; `ls` stopped at the missing
`stat` batch. B25 serves the metadata surface for the synthetic `/proc`/`/sys` files and a
metadata view of the staged tree under `/compat/linux`. File **contents** in the staged tree are
not served yet; only their metadata and directory entries are.

| nr (x86-64) | name | state |
|---|---|---|
| 4 | `stat` | x86-64 layout from the root tree/synthetic files |
| 5 | `fstat` | open descriptors and stdio (character device) |
| 6 | `lstat` | same metadata as `stat` (see the symlink note) |
| 21 | `access` | `R_OK`/`X_OK` from mode bits; `W_OK` is `-EROFS` |
| 76 | `truncate` | path validation; `-EROFS` (read-only root) |
| 260 | `fchownat` | path validation; `-EROFS` |
| 262 | `newfstatat` | dirfd/path, `AT_EMPTY_PATH`, `AT_SYMLINK_NOFOLLOW` |
| 268 | `fchmodat` | mode/flag validation; `-EROFS` |
| 280 | `utimensat` | path validation; `-EROFS` |
| 332 | `statx` | 256-byte x86-64 layout, `STATX_BASIC_STATS` mask |

`writev` (20) is assembled in the trap loop because BusyBox's stdio prints the `ls` listing with
it; it is not a dispatcher handler and is bounded to 16 iovecs / 64 KiB.

`stat`/`lstat` are included even though the B25 ticket names `fstat`/`newfstatat`: BusyBox's musl
build issues the legacy x86-64 `stat` (4) and `lstat` (6) syscalls (verified with `strace` on the
staged binary), so `ls` cannot work without them.

## 2. The staged root tree

`main.rs` parses the initfs and turns every entry under `/compat/linux/` into a `RootEntry`
(path, size, mode). `rootfs::RootTree` derives the implicit directories, adds `/`, `/proc` and
`/sys`, and assigns deterministic non-zero inodes in sorted-path order. The tree is
metadata-only: `open` of a staged **file** is `-EACCES` (contents stay in the initfs);
directories return `getdents64` entries.

`LinuxProcess.cwd` defaults to `/compat/linux`. Relative paths resolve against it (`AT_FDCWD`) or
against an open directory descriptor; `.`/`..` are normalized without leaving `/`. This is what
makes the fixture's `busybox ls` (no path argument) list the staged top-level entries.

The loader now maps writable image pages and zero pages as **shared memory objects** (file data
pages with `PROT_WRITE`, and every `StepKind::Zero` page). Writable `BSS`/data buffers are then
visible to the server's copy-in/copy-out, and `stat`/`lstat` on BusyBox's path buffers works.
Read-only text remains a private mapping.

## 3. `struct stat` and `struct statx`

`crates/raven-linux-abi` already owns the x86-64 layouts. The server serializes them without
casting:

* `struct stat` (144 bytes): `st_dev=0`, `st_ino`, `st_nlink`, `st_mode`, `st_uid=0`, `st_gid=0`,
  `st_rdev=0`, `st_size`, `st_blksize=4096`, `st_blocks=(size+511)/512`; times are zero.
* `struct statx` (256 bytes): `stx_mask` is the requested mask intersected with
  `STATX_BASIC_STATS` (0x7ff), `stx_blksize`, `stx_nlink`, `stx_uid/gid=0`, `stx_mode`,
  `stx_ino`, `stx_size`, `stx_blocks`; times/attributes are zero.

Modes: staged directories `S_IFDIR|0755` (permission bits come from the initfs entry for files),
regular files `S_IFREG|<mode>`, synthetic proc/sys files `S_IFREG|0644`, synthetic symlinks
`S_IFLNK|0777`, stdio `S_IFCHR|0600`. Directories report `st_nlink=2`, other nodes 1.

## 4. Behaviour and limits

* `stat`/`lstat`: a relative or absolute path; empty path is `-ENOENT`. The staged tree has no
  symlinks, and the synthetic `/proc/self/exe`/`fd` symlinks are reported as symlinks by both
  calls (`lstat` and `stat` do not differ yet).
* `newfstatat`/`statx`: `AT_EMPTY_PATH` stats the descriptor, `AT_SYMLINK_NOFOLLOW` is accepted,
  unknown flag bits are `-EINVAL`; `statx` masks are intersected with the basic set.
* `fstat`: fd 0/1/2 are a character device; open synthetic/root files and directories report their
  kind and size; closed descriptors are `-EBADF`.
* `fchmodat`/`fchownat`/`utimensat`/`truncate`: validate the path/mode/flags and return `-EROFS`;
  the root is read-only. Missing paths stay `-ENOENT`.
* `access`: `R_OK`/`X_OK` check the owner mode bits; `W_OK` is `-EROFS`; an unknown mode is
  `-EINVAL`.
* Times are zero (no file clock source yet); the staged root has no symlinks and no file contents;
  the synthetic procfs/sysfs files keep their deterministic generated sizes.

## 5. qtest and observed evidence

`tests/qtest/cases/linux-stat.toml` reuses the B32 recipe initfs (same `/init` fixture and staged
tree) and reads the new behaviour. **OBSERVED** serial output
(`build/qtest/linux-stat.log`):

```
linux-root: release=3.24.4
linux-root: busybox bytes=804616 loader bytes=670312
linux-root: attempt path=/compat/linux/bin/busybox argv1=ls
linuxd: entry=0x18000006c185 phdr=0x100000000040 phnum=12 brk=0x1000000c6000
linuxd: dynamic=1 interp-entry=0x18000006c185 interp-bias=0x180000000000
linuxd: mapped 369 regions, sp=0x7fffffffee60
bin
etc
lib
sbin
usr
...
linuxd: syscall lstat calls=5 errors=0
linuxd: syscall stat calls=1 errors=0
linuxd: hello exit=0
linux-root: linuxd exit=0
init exited (0)
```

`cargo xtask qtest --case linux-stat --arch x86_64` passes. The older `linux-root` case was the
B32a loader-boundary test and still asserts `ls: .: Function not implemented` plus
`unimplemented nr=4`; after B25 that boundary is gone. The case file is not in B25's Owns
(`tests/qtest/cases/linux-root*` belongs to B32a), so it needs a small follow-up update to the new
listing/status expectations; `linux-stat` is the B25 gate.

## 6. Provenance

First-party BSD-3-Clause code. The syscall numbers, `S_IF*` bits and `struct stat`/`statx` layouts
come from `raven-linux-abi` (generated from `linux-raw-sys`, Apache-2.0 OR MIT). No donor code was
copied. The Alpine tarball remains user-supplied and uncommitted.
