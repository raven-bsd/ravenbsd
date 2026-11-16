<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# linuxd syscall batches: metadata (B25) and clone (B21b)

Status: B25 and B21b, 2026-11-16. Evidence tags: **OBSERVED** (run on the target or read in the
source), **PROPOSED** (not implemented), **UNVERIFIED** (not exercised).

This document covers two linuxd batches:

* the metadata surface for the staged Alpine root (B25), which lets BusyBox `ls` list the tree;
* `clone` from the B21a trap frame (B21b), including the child trampoline and the shared signal
  page used by the clone fixture.

# Part I: metadata (B25)

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

The loader maps writable image pages and zero pages as **shared memory objects** (file data pages
with `PROT_WRITE`, and every `StepKind::Zero` page). Writable `BSS`/data buffers are then visible
to the server's copy-in/copy-out, and `stat`/`lstat` on BusyBox's path buffers works. Read-only
text remains a private mapping.

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

`cargo xtask qtest --case linux-stat --arch x86_64` passes. B25 also updates
`tests/qtest/cases/linux-root.toml` from the B32a `stat`-failure boundary to the same successful
listing and exit 0, because B25's contract explicitly says to run `linux-root` and requires
`busybox ls` to list the staged tree. That case file is outside B25's Owns-globs; the exception is
recorded in the B25 report.

## 6. Provenance

First-party BSD-3-Clause code. The syscall numbers, `S_IF*` bits and `struct stat`/`statx` layouts
come from `raven-linux-abi` (generated from `linux-raw-sys`, Apache-2.0 OR MIT). No donor code was
copied. The Alpine tarball remains user-supplied and uncommitted.

# Part II: clone (B21b)

## 7. Scope

B21b implements `clone` in `linuxd` from the B21a trap frame. The supporting process/thread calls
needed by the static fixture are served too: `gettid`, `set_tid_address`, `set_robust_list`,
`prctl` (`PR_SET_NAME`/`PR_GET_NAME`) and the existing `arch_prctl`.

`clone` covers two shapes:

| shape | flags | native mechanism | child return |
|---|---|---|---|
| thread | `CLONE_VM\|CLONE_THREAD\|CLONE_SIGHAND` | `THREAD_CREATE_EX` in the same process | `rax = 0`, new rsp |
| fork-like | signal byte only (e.g. `SIGCHLD`) | `PROC_FORK` + `THREAD_CREATE_EX` | `rax = 0`, COW stack |

`CLONE_SETTLS`, `CLONE_PARENT_SETTID`, `CLONE_CHILD_SETTID` and `CLONE_CHILD_CLEARTID` are
honoured. Unknown flags, `CLONE_THREAD` without `CLONE_VM`/`CLONE_SIGHAND`, sharing flags without
`CLONE_VM`, a zero/misaligned thread stack, and a fork-like `CLONE_CHILD_SETTID` are `EINVAL`; a
valid `clone` trap without a B21a frame is `ENOSYS`.

## 8. Child frame and trampoline

The kernel delivers the saved user frame in the scheme packet's path field (B21a). linuxd copies it,
sets `gprs[RAX] = 0` and chooses `rsp`:

* thread: the caller's `child_stack` (already 16-byte aligned);
* fork-like: the captured parent `rsp` (the COW copy contains the same stack), unless the caller
  supplied an explicit stack.

linuxd then builds one executable page containing a RIP-relative trampoline and the frame table.
The code loads all 15 general registers and `rsp` from the page, then jumps through a
`jmp qword ptr [rip+disp32]` to the captured RIP. It never touches the stack, so the eight bytes
below the child's `rsp` (the SysV red zone) survive; the `ret` in the fixture's raw-clone stub sees
exactly the parent's stack.

The child TLS base is installed at thread creation (`THREAD_CREATE_EX.tp`); the child does not need
an `arch_prctl` trap.

## 9. Addresses and the signal page

`linuxd` maps one shared, zero-filled page at `0x0000_4800_0000_0000` before the Linux thread
starts. A clone child cannot be attributed to a process by the trap packet (the packet carries no
sender identity), so the fixture children signal the parent through this page and spin instead of
making a syscall. A fork-like child inherits the shared mapping through `PROC_FORK`; a thread child
shares it directly.

Each clone maps a fresh trampoline page at `0x0000_4800_0000_1000 + n * 4096`, after creating the
memory object and populating it through a shared write mapping in linuxd. The target mapping is
created read/write and changed to read/execute with `PROTECT`, because generic memory objects carry
no `EXEC` right in ABI v1.

## 10. Ownership and cleanup

`LinuxProcess` records every native task in `clones` with its Linux pid/tid and native thread id.
When the parent exits (`exit_group`), linuxd terminates and reaps every fork-like child process,
closes the thread handles, and then terminates the Linux process. A thread child needs no separate
termination: terminating the process kills all of its threads.

`CLONE_CHILD_CLEARTID` is passed to the kernel as the new thread's clear-on-exit word. The main
thread's `set_tid_address` word is cleared by the trap loop at `exit_group`, because the thread was
created before the program called `set_tid_address` and the kernel has no later setter.

## 11. Clone evidence

**OBSERVED** from `build/qtest/linux-clone.log`:

```
linux-clone: gettid tid=6
linux-clone: set_tid_address=6 same=1
linux-clone: set_robust_list ok=1
linux-clone: prctl name=clone-fixture ok=1
linux-clone: arch_prctl fs=4280664 ok=1
linux-clone: clone bad-flags errno=22
linux-clone: clone thread pid=2 child=0
linux-clone: clone thread flag ok
linux-clone: clone fork pid=3 child=0
linux-clone: clone fork flag ok
linux-clone: done
linuxd: hello exit=0
init exited (0)
```

The thread child and the fork child both resumed at the parent's return RIP with `rax = 0` and
wrote the shared page. The fork child was terminated and reaped when the parent exited.

## 12. Clone limits

* **Trap attribution**: the scheme packet identifies the handler, not the sending process. A clone
  child that made a syscall would be served as its parent. B21b's children signal through the
  shared page and do not trap; per-task attribution is the B21 follow-up.
* `clone` with `CLONE_VM` but without `CLONE_THREAD` (a shared-VM process), namespaces, `CLONE_VFORK`,
  `CLONE_PIDFD` and the other unsupported bits are `EINVAL`, not silently accepted.
* `wait4` is not part of B21b; a fork-like child is reaped by linuxd at parent exit.
* The task-name attributes beyond `PR_SET_NAME`/`PR_GET_NAME` are `EINVAL`; B21 owns the full
  process/thread batch.
* aarch64 keeps the compact trap without a frame; `clone` there is `ENOSYS` until S56 adds frame
  parity.

## 13. Provenance for clone

First-party BSD-3-Clause code. No donor code was copied. The flag values and `ThreadCreateEx`
layout come from `raven-linux-abi` and `raven-abi`; the frame and trampoline are this project's
protocol and user-space code.
