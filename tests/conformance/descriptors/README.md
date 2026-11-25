<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->
# Descriptor conformance (S172)

`descriptor-conformance.c` asserts the POSIX/FreeBSD descriptor contract on the release image.
The shared runner in `../harness/runner.py` builds the probe, overlays it as a one-shot service,
boots the image and maps the rows to an exit status. It is test-only: no product behaviour is
changed here, and a red row is a fix ticket for the owning lane.

## Rows

| Row | Contract asserted |
|---|---|
| `dup-offset` | `dup` shares one file offset; the original continues where the duplicate stopped; `fstat` reports the same object |
| `dup2-dupfd` | `dup2`/`F_DUPFD` share the offset, `F_DUPFD` honours the minimum, out-of-range minimum is `EINVAL`, invalid source is `EBADF` without closing the target |
| `append` | `O_APPEND` writes at the current end after a `SEEK_SET`, and the resulting offset is the new end |
| `open-flags` | `O_CREAT|O_EXCL` refuses an existing file with `EEXIST`; `creat` and `O_TRUNC` truncate |
| `fd-flags` | `FD_CLOEXEC` is per descriptor; `O_APPEND` is per open file description and shared by `dup` |
| `cloexec` | `O_CLOEXEC`, `F_SETFD(FD_CLOEXEC)` and `F_DUPFD_CLOEXEC` disappear across `execve`; plain descriptors survive |
| `eof` | a read at EOF returns 0 and a second read still returns 0 |
| `short-read` | a pipe read returns the bytes available, then 0 after the writer closes |
| `eintr` | a handler without `SA_RESTART` gives `EINTR` on an empty pipe, and a partial transfer returns the bytes instead of `EINTR` |
| `nonblock` | `O_NONBLOCK` read of an empty pipe is `EAGAIN`, and `F_SETFL` changes the same open file description |
| `epipe-sigpipe` | a write to a pipe with no reader is `EPIPE` and raises `SIGPIPE`; ignoring `SIGPIPE` still returns `EPIPE` |
| `fstat-identity` | duplicated descriptors report the same device/inode and see each other's size changes |

## Controlled mutations

* `dup-copies-offset` -- the probe expects a duplicate to own an independent offset. The correct
  product shares it, so `dup-offset` turns red.
* `cloexec-survives` -- the probe expects a `FD_CLOEXEC` descriptor to survive `exec`. The correct
  product closes it, so `cloexec` turns red.

Both run in the same boot after the normal rows and are delimited by `CONFORM-MUTATION` lines.

## Donor map

| Topic | Illumos finding | BSD file + licence | Linux finding | Decision |
|---|---|---|---|---|
| dup/dup2/F_DUPFD, shared offset | file object shared by `fhold`; no offset copy | FreeBSD `133344405f532bd54ff3629a77d4c4e062f01fdb` `sys/kern/kern_descrip.c` `kern_dup` (FDDUP_NORMAL/FCNTL/FIXED) and `kern_fcntl` | `fs/file.c` read-only | test only: exact offset/identity rows |
| per-descriptor vs per-open-file flags | vnode/file flags are separate from descriptor flags | same file: `kern_fcntl` `F_GETFD`/`F_SETFD` vs `F_GETFL`/`F_SETFL`; `fde_flags` vs `f_flag` | read-only | test only |
| close-on-exec | exec closes CLOEXEC entries, keeps plain ones | same file `fdcloseexec` (UF_EXCLOSE); FreeBSD `sys/kern/kern_exec.c` exec cleanup | read-only | test only |
| EOF, short read, EINTR after partial | `dofileread`: zero resid returns 0; `EINTR`/`ERESTART`/`EWOULDBLOCK` after a partial transfer is suppressed | FreeBSD `sys/kern/sys_generic.c` `dofileread`/`dofilewrite` | read-only | test only |
| O_APPEND position | `IO_APPEND` from `f_flag & O_APPEND` at write time | FreeBSD `sys/kern/vfs_vnops.c` `vn_write` | read-only | test only |
| pipe EPIPE and SIGPIPE | write with no reader returns `EPIPE` and raises `SIGPIPE`; no `EPIPE` after a byte was written | FreeBSD `sys/kern/sys_pipe.c` `pipe_write`; `sys/kern/sys_generic.c` `dofilewrite` | read-only | test only |

No donor expression is copied; the probe asserts observable bytes, offsets and errno values.

## Run

```sh
python3 tests/conformance/harness/runner.py --topic tests/conformance/descriptors
python3 tests/conformance/harness/runner.py --topic tests/conformance/descriptors \
    --phase mutation --mutation dup-copies-offset
```
