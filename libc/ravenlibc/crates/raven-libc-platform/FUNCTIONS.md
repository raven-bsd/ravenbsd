# C55: `raven-libc-platform` function table (T1-T9)
SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

This is the machine-checked copy of `donor-map/11-syscall-platform.md` section 3.9 for the
functions this crate (and its direct seams) own. Status is **verified against this repository at
the current revision**, not copied from the document: a doc row that claimed a primitive that is
absent here is corrected and named in the C55 report.

* **EXISTS** - implemented and exercised against the current kernel/protocol crates.
* **PARTIAL** - implemented with the deviation named in the gap column.
* **TBD** - the primitive is absent; the symbol is a registered class-A stub from `src/stubs.rs`
  returning the errno shown there (`ENOSYS`, or `ENOTSUP` for xattr/ACL and `EPERM` for chroot).
* **SKIP** - class B (not exported from `libc.so.1`) or owned by another area.

`owner` is the lane that makes the primitive exist; `dave` is this crate. Rows marked
`external` are implemented by the named ticket and are listed here only so the checklist closes.
`T10` process/signal/thread hooks are appended for the same reason; their mechanics belong to
C53/C57/C58.

## T1. Entry, errno, startup
| function | Raven primitive today | status | PG or G id | owner lane | donor reference |
|---|---|---|---|---|---|
| `syscall` | syscall0..6 | EXISTS | C54 | kevin | musl:src/misc/syscall.c |
| `errno` | TCB slot | TBD | PG-16 | C57 | musl:src/errno/__errno_location.c |
| `__errno_location` | TCB slot | TBD | PG-16 | C57 | musl:src/errno/__errno_location.c |
| `h_errno` | none | SKIP | class B | C57 | musl:src/errno |
| `_exit` | S:EXIT | EXISTS | C54 | kevin | picolibc _exit |
| `getauxval` | stack auxv | EXISTS | C53 | kevin | musl:src/misc/getauxval.c |
| `__libc_start_main` | C8 stack | EXISTS | C53 | kevin | userland/rt/src/entry.rs |

## T2. Open, close, read, write, seek
| function | Raven primitive today | status | PG or G id | owner lane | donor reference |
|---|---|---|---|---|---|
| `open` | S:OPEN | PARTIAL | PG-24 | kevin | netbsd:lib/libc/sys/open.2 |
| `openat` | S:OPEN | PARTIAL | PG-24 | kevin | netbsd:lib/libc/sys/open.2 |
| `creat` | S:OPEN | PARTIAL | PG-24 | kevin | netbsd:lib/libc/sys/open.2 |
| `open64` | S:OPEN | PARTIAL | PG-24 | kevin | musl:src/fcntl/open.c |
| `openat64` | S:OPEN | PARTIAL | PG-24 | kevin | musl:src/fcntl/open.c |
| `creat64` | S:OPEN | PARTIAL | PG-24 | kevin | musl:src/fcntl/open.c |
| `close` | FD + S:CLOSE | EXISTS | - | dave | musl:src/unistd/close.c |
| `posix_close` | FD + S:CLOSE | EXISTS | - | dave | musl:src/unistd/close.c |
| `closefrom` | FD loop | EXISTS | - | dave | netbsd:tests/lib/libc/gen/t_closefrom.c |
| `close_range` | FD loop | EXISTS | - | dave | netbsd:tests/lib/libc/gen/t_closefrom.c |
| `read` | S:READ | EXISTS | - | dave | netbsd:lib/libc/sys/read.2 |
| `readv` | S:READ bounce | EXISTS | - | dave | netbsd:lib/libc/sys/read.2 |
| `pread` | S:PREAD* | TBD | PG-22 | stuart | netbsd:lib/libc/sys/read.2 |
| `pread64` | S:PREAD* | TBD | PG-22 | stuart | netbsd:lib/libc/sys/read.2 |
| `preadv` | S:PREAD* | TBD | PG-22 | stuart | netbsd:lib/libc/sys/read.2 |
| `preadv2` | S:PREAD* | TBD | PG-22 | stuart | netbsd:lib/libc/sys/read.2 |
| `write` | S:WRITE | EXISTS | - | dave | netbsd:lib/libc/sys/write.2 |
| `writev` | S:WRITE loop | EXISTS | - | dave | netbsd:lib/libc/sys/write.2 |
| `pwrite` | S:PWRITE* | TBD | PG-22 | stuart | netbsd:lib/libc/sys/write.2 |
| `pwrite64` | S:PWRITE* | TBD | PG-22 | stuart | netbsd:lib/libc/sys/write.2 |
| `pwritev` | S:PWRITE* | TBD | PG-22 | stuart | netbsd:lib/libc/sys/write.2 |
| `pwritev2` | S:PWRITE* | TBD | PG-22 | stuart | netbsd:lib/libc/sys/write.2 |
| `lseek` | S:SEEK* | TBD | PG-22 | stuart | netbsd:lib/libc/sys/lseek.2 |
| `lseek64` | S:SEEK* | TBD | PG-22 | stuart | netbsd:lib/libc/sys/lseek.2 |
| `ftruncate` | CALL(OP_TRUNCATE) | PARTIAL | PG-23 | dave | netbsd:lib/libc/sys/truncate.2 |
| `ftruncate64` | CALL(OP_TRUNCATE) | PARTIAL | PG-23 | dave | netbsd:lib/libc/sys/truncate.2 |
| `truncate` | open+OP_TRUNCATE | PARTIAL | PG-31 | dave | netbsd:lib/libc/sys/truncate.2 |
| `truncate64` | open+OP_TRUNCATE | PARTIAL | PG-31 | dave | netbsd:lib/libc/sys/truncate.2 |
| `fsync` | CALL(OP_SYNC) | PARTIAL | PG-21 | dave | netbsd:lib/libc/sys/fsync.2 |
| `fdatasync` | CALL(OP_SYNC) | PARTIAL | PG-21 | dave | netbsd:lib/libc/sys/fsync.2 |
| `sync` | CALL(OP_SYNC) file:/ | EXISTS | - | dave | userland/rt/src/fs.rs:291 |
| `syncfs` | CALL(OP_SYNC) file:/ | PARTIAL | PG-21 | dave | userland/rt/src/fs.rs:291 |
| `sync_file_range` | none | TBD | PG-21 | dave | netbsd:lib/libc/sys/fsync.2 |
| `readahead` | none | TBD | PG-21 | dave | musl:src/fcntl/posix_fadvise.c |
| `posix_fadvise` | L: advice | PARTIAL | PG-21 | dave | musl:src/fcntl/posix_fadvise.c |
| `posix_fadvise64` | L: advice | PARTIAL | PG-21 | dave | musl:src/fcntl/posix_fadvise.c |
| `fallocate` | CALL(OP_FALLOCATE*) | TBD | PG-21 | dave | pjdfstest/tests/posix_fallocate |
| `posix_fallocate` | CALL(OP_FALLOCATE*) | TBD | PG-21 | dave | pjdfstest/tests/posix_fallocate |
| `posix_fallocate64` | CALL(OP_FALLOCATE*) | TBD | PG-21 | dave | pjdfstest/tests/posix_fallocate |
| `sendfile` | libc loop 64 KiB | TBD | PG-21 | dave | mlibc sysdep-tags sys_sendfile |
| `sendfile64` | libc loop 64 KiB | TBD | PG-21 | dave | mlibc sysdep-tags sys_sendfile |
| `copy_file_range` | libc loop 64 KiB | TBD | PG-21 | dave | mlibc sysdep-tags sys_copy_file_range |
| `splice` | libc pipe loop | TBD | PG-21 | dave | mlibc sysdep-tags sys_splice |
| `tee` | libc pipe loop | TBD | PG-35 | dave | mlibc sysdep-tags sys_splice |
| `vmsplice` | none | SKIP | class B | dave | linux-only |

## T3. Metadata
| function | Raven primitive today | status | PG or G id | owner lane | donor reference |
|---|---|---|---|---|---|
| `fstat` | S:FSTAT | PARTIAL | PG-23 | dave | netbsd:lib/libc/sys/stat.2 |
| `fstat64` | S:FSTAT | PARTIAL | PG-23 | dave | netbsd:lib/libc/sys/stat.2 |
| `stat` | open+FSTAT | PARTIAL | PG-31 | dave | netbsd:lib/libc/sys/stat.2 |
| `lstat` | open(NOFOLLOW)+FSTAT | PARTIAL | PG-31 | dave | netbsd:lib/libc/sys/stat.2 |
| `stat64` | open+FSTAT | PARTIAL | PG-31 | dave | netbsd:lib/libc/sys/stat.2 |
| `lstat64` | open(NOFOLLOW)+FSTAT | PARTIAL | PG-31 | dave | netbsd:lib/libc/sys/stat.2 |
| `fstatat` | open+FSTAT | PARTIAL | PG-31 | dave | musl:src/stat/fstatat.c |
| `fstatat64` | open+FSTAT | PARTIAL | PG-31 | dave | musl:src/stat/fstatat.c |
| `statx` | CALL(OP_STAT*) | TBD | PG-30 | dave | netbsd:lib/libc/sys/stat.2 |
| `newfstatat` | open+FSTAT | PARTIAL | PG-31 | dave | musl:src/stat/fstatat.c |
| `access` | stat+mode bits | PARTIAL | PG-28 | dave | netbsd:lib/libc/sys/access.2 |
| `faccessat` | stat+mode bits | PARTIAL | PG-28 | dave | netbsd:lib/libc/sys/access.2 |
| `eaccess` | stat+mode bits | PARTIAL | PG-28 | dave | netbsd:lib/libc/sys/access.2 |
| `euidaccess` | stat+mode bits | PARTIAL | PG-28 | dave | netbsd:lib/libc/sys/access.2 |
| `chmod` | CALL(OP_CHMOD) | EXISTS | - | dave | userland/rt/src/fs.rs:283 |
| `fchmod` | CALL(OP_CHMOD*) | TBD | PG-21 | dave | musl:src/stat/fchmodat.c |
| `fchmodat` | CALL(OP_CHMOD) | PARTIAL | PG-21 | dave | musl:src/stat/fchmodat.c |
| `lchmod` | CALL(OP_CHMOD*) | PARTIAL | PG-21 | dave | musl:src/stat/fchmodat.c |
| `chown` | CALL(OP_CHOWN*) | TBD | PG-31 | dave | netbsd:lib/libc/sys/chown.2 |
| `fchown` | CALL(OP_CHOWN*) | TBD | PG-31 | dave | netbsd:lib/libc/sys/chown.2 |
| `lchown` | CALL(OP_CHOWN*) | TBD | PG-31 | dave | netbsd:lib/libc/sys/chown.2 |
| `fchownat` | CALL(OP_CHOWN*) | TBD | PG-31 | dave | netbsd:lib/libc/sys/chown.2 |
| `utime` | CALL(OP_UTIMES*) | TBD | PG-31 | dave | pjdfstest/tests/utimensat |
| `utimes` | CALL(OP_UTIMES*) | TBD | PG-31 | dave | pjdfstest/tests/utimensat |
| `futimes` | CALL(OP_UTIMES*) | TBD | PG-31 | dave | pjdfstest/tests/utimensat |
| `lutimes` | CALL(OP_UTIMES*) | TBD | PG-31 | dave | pjdfstest/tests/utimensat |
| `futimens` | CALL(OP_UTIMES*) | TBD | PG-31 | dave | pjdfstest/tests/utimensat |
| `utimensat` | CALL(OP_UTIMES*) | TBD | PG-31 | dave | pjdfstest/tests/utimensat |
| `link` | CALL(OP_LINK*) | TBD | PG-31 | dave | pjdfstest/tests/link |
| `linkat` | CALL(OP_LINK*) | TBD | PG-31 | dave | pjdfstest/tests/link |
| `symlink` | CALL(OP_SYMLINK) | EXISTS | - | dave | userland/rt/src/fs.rs:255 |
| `symlinkat` | CALL(OP_SYMLINK) | EXISTS | - | dave | userland/rt/src/fs.rs:255 |
| `readlink` | CALL(OP_READLINK) | EXISTS | - | dave | userland/rt/src/fs.rs:263 |
| `readlinkat` | CALL(OP_READLINK) | EXISTS | - | dave | userland/rt/src/fs.rs:263 |
| `rename` | CALL(OP_RENAME) | PARTIAL | PG-31 | dave | pjdfstest/tests/rename |
| `renameat` | CALL(OP_RENAME) | PARTIAL | PG-31 | dave | pjdfstest/tests/rename |
| `renameat2` | CALL(OP_RENAME) | TBD | PG-31 | dave | pjdfstest/tests/rename |
| `unlink` | CALL(OP_UNLINK) | EXISTS | - | dave | userland/rt/src/fs.rs:223 |
| `unlinkat` | CALL(OP_UNLINK/OP_RMDIR) | EXISTS | - | dave | userland/rt/src/fs.rs:223 |
| `rmdir` | CALL(OP_RMDIR) | EXISTS | - | dave | userland/rt/src/fs.rs:223 |
| `mkdir` | CALL(OP_MKDIR) | EXISTS | - | dave | userland/rt/src/fs.rs:235 |
| `mkdirat` | CALL(OP_MKDIR) | EXISTS | - | dave | userland/rt/src/fs.rs:235 |
| `mkfifo` | CALL(OP_MKNOD*) | TBD | PG-31 | dave | pjdfstest/tests/mkfifo |
| `mkfifoat` | CALL(OP_MKNOD*) | TBD | PG-31 | dave | pjdfstest/tests/mkfifo |
| `mknod` | CALL(OP_MKNOD*) | TBD | PG-31 | dave | pjdfstest/tests/mknod |
| `mknodat` | CALL(OP_MKNOD*) | TBD | PG-31 | dave | pjdfstest/tests/mknod |
| `umask` | L: atomic cell | EXISTS | - | dave | netbsd:lib/libc/sys/umask.2 |
| `fstatfs` | CALL(OP_STATFS) | PARTIAL | PG-30 | dave | musl:src/stat/statvfs.c |
| `statfs` | open+CALL(OP_STATFS) | PARTIAL | PG-30 | dave | musl:src/stat/statvfs.c |
| `fstatfs64` | CALL(OP_STATFS) | PARTIAL | PG-30 | dave | musl:src/stat/statvfs.c |
| `statfs64` | open+CALL(OP_STATFS) | PARTIAL | PG-30 | dave | musl:src/stat/statvfs.c |
| `fstatvfs` | CALL(OP_STATFS) | PARTIAL | PG-30 | dave | musl:src/stat/statvfs.c |
| `statvfs` | open+CALL(OP_STATFS) | PARTIAL | PG-30 | dave | musl:src/stat/statvfs.c |
| `fstatvfs64` | CALL(OP_STATFS) | PARTIAL | PG-30 | dave | musl:src/stat/statvfs.c |
| `statvfs64` | open+CALL(OP_STATFS) | PARTIAL | PG-30 | dave | musl:src/stat/statvfs.c |
| `getxattr` | none | TBD | PG-31 | dave | musl:src/linux/xattr.c |
| `lgetxattr` | none | TBD | PG-31 | dave | musl:src/linux/xattr.c |
| `fgetxattr` | none | TBD | PG-31 | dave | musl:src/linux/xattr.c |
| `setxattr` | none | TBD | PG-31 | dave | musl:src/linux/xattr.c |
| `lsetxattr` | none | TBD | PG-31 | dave | musl:src/linux/xattr.c |
| `fsetxattr` | none | TBD | PG-31 | dave | musl:src/linux/xattr.c |
| `listxattr` | none | TBD | PG-31 | dave | musl:src/linux/xattr.c |
| `llistxattr` | none | TBD | PG-31 | dave | musl:src/linux/xattr.c |
| `flistxattr` | none | TBD | PG-31 | dave | musl:src/linux/xattr.c |
| `removexattr` | none | TBD | PG-31 | dave | musl:src/linux/xattr.c |
| `lremovexattr` | none | TBD | PG-31 | dave | musl:src/linux/xattr.c |
| `fremovexattr` | none | TBD | PG-31 | dave | musl:src/linux/xattr.c |
| `acl_get_file` | none | TBD | PG-31 | dave | musl:src/linux/xattr.c |
| `acl_set_file` | none | TBD | PG-31 | dave | musl:src/linux/xattr.c |
| `chflags` | none | TBD | PG-31 | dave | freebsd:lib/libsys/chflags.2 |
| `fchflags` | none | TBD | PG-31 | dave | freebsd:lib/libsys/chflags.2 |

## T4. Descriptors, locking, ioctl, terminals
| function | Raven primitive today | status | PG or G id | owner lane | donor reference |
|---|---|---|---|---|---|
| `dup` | FD | EXISTS | - | dave | 3.5 |
| `dup2` | FD | EXISTS | - | dave | 3.5 |
| `dup3` | FD | EXISTS | - | dave | 3.5 |
| `fcntl` | FD + CALL(OP_SETFL*) | PARTIAL | PG-21 | dave | netbsd:lib/libc/sys/fcntl.2 |
| `flock` | CALL(OP_FLOCK*) | TBD | PG-21 | dave | netbsd:lib/libc/sys/flock.2 |
| `lockf` | fcntl record locks | TBD | PG-21 | dave | openbsd:lib/libc/gen/lockf.c |
| `lockf64` | fcntl record locks | TBD | PG-21 | dave | openbsd:lib/libc/gen/lockf.c |
| `pipe` | pipe: server | TBD | PG-35 | kevin | netbsd:tests/lib/libc/sys/t_pipe.c |
| `pipe2` | pipe: server | TBD | PG-35 | kevin | netbsd:tests/lib/libc/sys/t_pipe2.c |
| `socketpair` | uds: server | TBD | PG-30 | bob | 4.3BSD socketpair |
| `ioctl` | libc dispatch | PARTIAL | PG-54 | dave | musl:src/misc/ioctl.c |
| `select` | EVQ shim | PARTIAL | PG-25 | dave | mlibc:sys-select.cpp |
| `pselect` | EVQ + mask swap | TBD | PG-26 | dave | mlibc:sys-select.cpp |
| `poll` | EVQ shim | PARTIAL | PG-25 | dave | freebsd:lib/libc/sys/poll.c |
| `ppoll` | EVQ + mask swap | TBD | PG-26 | dave | freebsd:lib/libc/sys/poll.c |
| `epoll_create` | EVQ_CREATE | PARTIAL | PG-25 | dave | musl:src/linux/epoll.c |
| `epoll_create1` | EVQ_CREATE | PARTIAL | PG-25 | dave | musl:src/linux/epoll.c |
| `epoll_ctl` | EVQ_CTL | PARTIAL | PG-25 | dave | musl:src/linux/epoll.c |
| `epoll_wait` | EVQ_WAIT | PARTIAL | PG-25 | dave | musl:src/linux/epoll.c |
| `epoll_pwait` | EVQ_WAIT + mask | TBD | PG-26 | dave | musl:src/linux/epoll.c |
| `epoll_pwait2` | EVQ_WAIT + mask | TBD | PG-26 | dave | musl:src/linux/epoll.c |
| `kqueue` | EVQ shim | TBD | PG-33 | kevin | freebsd:lib/libc/sys/kevent.c |
| `kevent` | EVQ shim | TBD | PG-33 | kevin | freebsd:lib/libc/sys/kevent.c |
| `eventfd` | EVENT_CREATE | TBD | PG-14 | dave | musl:src/linux/eventfd.c |
| `eventfd_read` | EVENT read | TBD | PG-14 | dave | musl:src/linux/eventfd.c |
| `eventfd_write` | EVENT_SIGNAL | TBD | PG-14 | dave | musl:src/linux/eventfd.c |
| `timerfd_create` | TIMER_CREATE | TBD | PG-14 | dave | musl:src/linux/timerfd.c |
| `timerfd_settime` | TIMER_SET | TBD | PG-14 | dave | musl:src/linux/timerfd.c |
| `timerfd_gettime` | TIMER read | TBD | PG-14 | dave | musl:src/linux/timerfd.c |
| `signalfd` | none | TBD | PG-5 | dave | signals area |
| `inotify_init` | none | TBD | PG-33 | dave | fs notifications |
| `inotify_init1` | none | TBD | PG-33 | dave | fs notifications |
| `inotify_add_watch` | none | TBD | PG-33 | dave | fs notifications |
| `inotify_rm_watch` | none | TBD | PG-33 | dave | fs notifications |
| `fanotify_init` | none | SKIP | class B | dave | linux-only |
| `isatty` | tcgetattr | PARTIAL | PG-54 | dave | bionic:isatty.cpp COPY |
| `ttyname` | OFD path | PARTIAL | PG-21 | dave | netbsd t_ttyname.c |
| `ttyname_r` | OFD path | PARTIAL | PG-21 | dave | netbsd t_ttyname.c |
| `tcgetattr` | tty OP_GET_MODE | PARTIAL | PG-54 | dave | mlibc:termios.cpp |
| `tcsetattr` | tty OP_SET_MODE | PARTIAL | PG-54 | dave | mlibc:termios.cpp |
| `tcdrain` | tty OP_DRAIN* | TBD | PG-54 | dave | mlibc:termios.cpp |
| `tcflow` | tty flow op | TBD | PG-54 | dave | mlibc:termios.cpp |
| `tcflush` | tty OP_FLUSH | PARTIAL | PG-54 | dave | mlibc:termios.cpp |
| `tcsendbreak` | tty OP_BREAK* | TBD | PG-54 | dave | mlibc:termios.cpp |
| `tcgetpgrp` | tty OP_PGRP_GET* | TBD | PG-54 | dave | mlibc:termios.cpp |
| `tcsetpgrp` | tty OP_PGRP_SET* | TBD | PG-54 | dave | mlibc:termios.cpp |
| `tcgetsid` | tty OP_SID_GET* | TBD | PG-54 | dave | mlibc:termios.cpp |
| `tcgetwinsize` | tty OP_WINSZ_GET* | TBD | PG-54 | dave | mlibc:termios.cpp |
| `tcsetwinsize` | tty OP_WINSZ_SET* | TBD | PG-54 | dave | mlibc:termios.cpp |
| `cfmakeraw` | L | EXISTS | - | dave | mlibc:termios.cpp |
| `cfgetispeed` | L | EXISTS | - | dave | mlibc:termios.cpp |
| `cfgetospeed` | L | EXISTS | - | dave | mlibc:termios.cpp |
| `cfsetispeed` | L | EXISTS | - | dave | mlibc:termios.cpp |
| `cfsetospeed` | L | EXISTS | - | dave | mlibc:termios.cpp |
| `openpty` | pty: scheme | TBD | PG-54 | carl | musl:src/misc/openpty.c |
| `forkpty` | pty: scheme | TBD | PG-54 | carl | musl:src/misc/openpty.c |
| `login_tty` | pty: scheme | TBD | PG-54 | carl | musl:src/misc/openpty.c |
| `posix_openpt` | pty: scheme | TBD | PG-54 | carl | musl:src/misc/openpty.c |
| `grantpt` | pty: scheme | TBD | PG-54 | carl | musl:src/misc/openpty.c |
| `unlockpt` | pty: scheme | TBD | PG-54 | carl | musl:src/misc/openpty.c |
| `ptsname` | pty: scheme | TBD | PG-54 | carl | musl:src/misc/openpty.c |
| `ptsname_r` | pty: scheme | TBD | PG-54 | carl | musl:src/misc/openpty.c |

## T5. Memory mapping
| function | Raven primitive today | status | PG or G id | owner lane | donor reference |
|---|---|---|---|---|---|
| `mmap` | S:MAP | PARTIAL | PG-36 | dave | netbsd t_mmap.c |
| `mmap64` | S:MAP | PARTIAL | PG-36 | dave | netbsd t_mmap.c |
| `munmap` | S:UNMAP | EXISTS | - | dave | netbsd t_mmap.c |
| `mprotect` | S:PROTECT | EXISTS | - | dave | netbsd t_mprotect.c |
| `mremap` | libc remap | PARTIAL | PG-13 | dave | musl:src/mman/mremap.c |
| `msync` | CALL(OP_MSYNC*) | TBD | PG-22 | dave | netbsd t_msync.c |
| `madvise` | S:MADVISE* / L | PARTIAL | PG-15 | dave | netbsd t_madvise.c |
| `posix_madvise` | S:MADVISE* / L | PARTIAL | PG-15 | dave | posix |
| `mincore` | S:MINCORE* | TBD | PG-15 | dave | netbsd |
| `mlock` | L: no-op | PARTIAL | PG-15 | dave | netbsd t_mlock.c |
| `munlock` | L: no-op | PARTIAL | PG-15 | dave | netbsd t_mlock.c |
| `mlockall` | L: no-op | PARTIAL | PG-15 | dave | netbsd t_mlock.c |
| `munlockall` | L: no-op | PARTIAL | PG-15 | dave | netbsd t_mlock.c |
| `mlock2` | L: no-op | PARTIAL | PG-15 | dave | netbsd t_mlock.c |
| `brk` | none | SKIP | class B | dave | musl:src/mman |
| `sbrk` | none | SKIP | class B | dave | musl:src/mman |
| `getpagesize` | L: 4096 | EXISTS | - | dave | bionic:getpagesize.cpp |

## T6. Directories, cwd, mounts
| function | Raven primitive today | status | PG or G id | owner lane | donor reference |
|---|---|---|---|---|---|
| `getcwd` | L: cwd string | EXISTS | - | dave | bionic:getcwd.cpp COPY |
| `get_current_dir_name` | L: cwd string | EXISTS | - | dave | glibc |
| `getwd` | none | SKIP | class B | dave | bsd |
| `chdir` | resolve+OPEN(O_DIRECTORY) | EXISTS | - | dave | 3.4 |
| `fchdir` | OFD remembered path | EXISTS | - | dave | 3.4 |
| `chroot` | per-process root | TBD | PG-33 | dave | class C EPERM |
| `opendir` | READ dir records | PARTIAL | PG-22 | kevin | C46 |
| `readdir` | READ dir records | PARTIAL | PG-22 | kevin | C46 |
| `rewinddir` | S:SEEK* | TBD | PG-22 | kevin | C46 |
| `seekdir` | S:SEEK* | TBD | PG-22 | kevin | C46 |
| `telldir` | L: cookie | EXISTS | - | kevin | C46 |
| `scandir` | READ dir records | PARTIAL | PG-22 | kevin | C46 |
| `getdents` | S:READ raw | PARTIAL | PG-22 | dave | netbsd |
| `getdirentries` | S:READ raw | PARTIAL | PG-22 | dave | BSD |
| `mount` | CALL(OP_MOUNT) | EXISTS | - | dave | userland/rt/src/fs.rs:296 |
| `umount` | CALL(OP_UMOUNT) | EXISTS | - | dave | userland/rt/src/fs.rs:304 |
| `umount2` | CALL(OP_UMOUNT) | EXISTS | - | dave | userland/rt/src/fs.rs:304 |
| `unmount` | CALL(OP_UMOUNT) | EXISTS | - | dave | userland/rt/src/fs.rs:304 |
| `getmntinfo` | read /etc/mounts | TBD | PG-32 | dave | raven-vfs-core |
| `getfsstat` | read /etc/mounts | TBD | PG-32 | dave | raven-vfs-core |
| `setmntent` | read /etc/mounts | TBD | PG-32 | dave | raven-vfs-core |
| `getmntent` | read /etc/mounts | TBD | PG-32 | dave | raven-vfs-core |
| `pivot_root` | none | SKIP | class B | dave | linux-only |
| `fsopen` | none | SKIP | class B | dave | linux-only |
| `fsmount` | none | SKIP | class B | dave | linux-only |
| `open_tree` | none | SKIP | class B | dave | linux-only |
| `move_mount` | none | SKIP | class B | dave | linux-only |
| `unshare` | none | SKIP | class B | dave | linux-only |
| `setns` | none | SKIP | class B | dave | linux-only |

## T7. Time and sleeping
| function | Raven primitive today | status | PG or G id | owner lane | donor reference |
|---|---|---|---|---|---|
| `clock_gettime` | S:CLOCK_GET | PARTIAL | PG-17 | dave | bionic:clock.cpp |
| `gettimeofday` | S:CLOCK_GET(0) | PARTIAL | PG-17 | dave | bionic:clock.cpp |
| `time` | S:CLOCK_GET(0) | PARTIAL | PG-17 | dave | bionic:clock.cpp |
| `clock_getres` | L: 1 ns | EXISTS | - | dave | bionic:clock.cpp |
| `clock_settime` | S:CLOCK_SET* | TBD | PG-17 | dave | bionic:clock.cpp |
| `settimeofday` | S:CLOCK_SET* | TBD | PG-17 | dave | bionic:clock.cpp |
| `adjtime` | none | TBD | PG-17 | dave | bsd |
| `adjtimex` | none | TBD | PG-17 | dave | linux |
| `stime` | none | TBD | PG-17 | dave | bsd |
| `nanosleep` | S:NANOSLEEP | PARTIAL | PG-17 | dave | bionic:sleep.cpp COPY |
| `clock_nanosleep` | S:NANOSLEEP | PARTIAL | PG-17 | dave | bionic:clock.cpp |
| `sleep` | nanosleep | PARTIAL | PG-17 | dave | bionic:sleep.cpp COPY |
| `usleep` | nanosleep | PARTIAL | PG-17 | dave | openbsd:usleep.c COPY |
| `alarm` | TIMER_CREATE+signals | TBD | PG-17 | C58 | signals area |
| `ualarm` | TIMER_CREATE+signals | TBD | PG-17 | C58 | signals area |
| `setitimer` | TIMER_CREATE+signals | TBD | PG-17 | C58 | signals area |
| `getitimer` | TIMER_CREATE+signals | TBD | PG-17 | C58 | signals area |
| `timer_create` | TIMER_CREATE | TBD | PG-17 | C58 | signals area |
| `timer_settime` | TIMER_SET | TBD | PG-17 | C58 | signals area |
| `timer_gettime` | TIMER read | TBD | PG-14 | C58 | signals area |
| `timer_delete` | CLOSE | TBD | PG-17 | C58 | signals area |
| `timer_getoverrun` | none | TBD | PG-17 | C58 | signals area |
| `times` | times syscall | TBD | PG-17 | C57 | C44 |
| `getrusage` | procd | TBD | PG-28 | C57 | C44 |
| `clock_getcpuclockid` | clock ids 2,3 | TBD | PG-17 | C44 | C44 |

## T8. Identity and system information
| function | Raven primitive today | status | PG or G id | owner lane | donor reference |
|---|---|---|---|---|---|
| `uname` | S:UNAME | EXISTS | - | dave | freebsd:uname.c shape |
| `gethostname` | sys:hostname | EXISTS | - | dave | sysd |
| `sethostname` | sys:hostname | EXISTS | - | dave | netbsd t_sethostname.c |
| `getdomainname` | sys:domainname | TBD | PG-27 | dave | netbsd t_setdomainname.c |
| `setdomainname` | sys:domainname | TBD | PG-27 | dave | netbsd t_setdomainname.c |
| `gethostid` | sys:kern/hostid | TBD | PG-27 | dave | netbsd |
| `sethostid` | sys:kern/hostid | TBD | PG-27 | dave | netbsd |
| `sysconf` | L table + sysinfo | PARTIAL | PG-21 | dave | bionic:sysconf.cpp COPY |
| `pathconf` | stat + L table | EXISTS | - | dave | bionic:pathconf.cpp COPY |
| `fpathconf` | statfs + L table | EXISTS | - | dave | bionic:pathconf.cpp COPY |
| `confstr` | L table | EXISTS | - | dave | openbsd:confstr.c COPY |
| `sysinfo` | S:SYSINFO* | PARTIAL | PG-21 | dave | musl:src/linux/sysinfo.c |
| `get_nprocs` | S:SYSINFO* | PARTIAL | PG-21 | dave | musl:src/linux/sysinfo.c |
| `get_nprocs_conf` | S:SYSINFO* | PARTIAL | PG-21 | dave | musl:src/linux/sysinfo.c |
| `get_phys_pages` | S:SYSINFO* | PARTIAL | PG-21 | dave | musl:src/linux/sysinfo.c |
| `get_avphys_pages` | S:SYSINFO* | PARTIAL | PG-21 | dave | musl:src/linux/sysinfo.c |
| `getloadavg` | sys:kern/loadavg | TBD | PG-27 | dave | openbsd:getloadavg.c |
| `sysctl` | sys: shim | PARTIAL | PG-27 | dave | netbsd:sysctl.c |
| `sysctlbyname` | sys: shim | PARTIAL | PG-27 | dave | netbsd:sysctlbyname.c |
| `sysctlnametomib` | static MIB table | PARTIAL | PG-27 | dave | netbsd:sysctlnametomib.c |
| `sysctlgetmibinfo` | static MIB table | TBD | PG-27 | dave | netbsd:sysctlgetmibinfo.c |
| `getrlimit` | libc rlimit cell | PARTIAL | PG-28 | dave | musl:src/unistd |
| `setrlimit` | libc rlimit cell | PARTIAL | PG-28 | dave | musl:src/unistd |
| `prlimit` | libc rlimit cell | PARTIAL | PG-28 | dave | musl:src/unistd |
| `getrlimit64` | libc rlimit cell | PARTIAL | PG-28 | dave | musl:src/unistd |
| `getpriority` | procd | TBD | PG-28 | dave | openbsd:nice.c |
| `setpriority` | procd | TBD | PG-28 | dave | openbsd:nice.c |
| `nice` | procd / O-6 | PARTIAL | PG-28 | dave | openbsd:nice.c COPY |
| `getentropy` | rand: interim | PARTIAL | PG-52 | dave | bionic:getentropy.cpp COPY |
| `getrandom` | S:GETRANDOM* | TBD | PG-52 | dave | bionic:getentropy.cpp |
| `reboot` | svc: CALL | TBD | unassigned | kevin | svcd |
| `getlogin` | procd session | TBD | PG-28 | dave | bsd |
| `getlogin_r` | procd session | TBD | PG-28 | dave | bsd |
| `setlogin` | procd session | TBD | PG-28 | dave | bsd |

## T9. Sockets (primitives only)
| function | Raven primitive today | status | PG or G id | owner lane | donor reference |
|---|---|---|---|---|---|
| `socket` | OPEN net: | TBD | PG-29 | bob | C63 |
| `accept` | CALL(OP_ACCEPT) | TBD | PG-29 | bob | C63 |
| `accept4` | CALL(OP_ACCEPT) | TBD | PG-29 | bob | C63 |
| `bind` | CALL(OP_BIND) | TBD | PG-29 | bob | C63 |
| `listen` | CALL(OP_LISTEN) | TBD | PG-29 | bob | C63 |
| `connect` | CALL(OP_CONNECT) | TBD | PG-29 | bob | C63 |
| `shutdown` | CALL(OP_SHUTDOWN) | TBD | PG-29 | bob | C63 |
| `getsockname` | CALL(OP_GETSOCKNAME) | TBD | PG-29 | bob | C63 |
| `getpeername` | CALL(OP_GETPEERNAME) | TBD | PG-29 | bob | C63 |
| `getsockopt` | CALL(OP_GETSOCKOPT) | TBD | PG-29 | bob | C63 |
| `setsockopt` | CALL(OP_SETSOCKOPT) | TBD | PG-29 | bob | C63 |
| `send` | S:WRITE | TBD | PG-29 | bob | C63 |
| `sendto` | S:WRITE | TBD | PG-29 | bob | C63 |
| `sendmsg` | S:WRITE | TBD | PG-29 | bob | C63 |
| `recv` | S:READ | TBD | PG-29 | bob | C63 |
| `recvfrom` | S:READ | TBD | PG-29 | bob | C63 |
| `recvmsg` | S:READ | TBD | PG-29 | bob | C63 |
| `getifaddrs` | netd iface LIST | TBD | PG-29 | bob | C63 |
| `if_nameindex` | netd iface LIST | TBD | PG-29 | bob | C63 |
| `if_nametoindex` | netd iface GET | TBD | PG-29 | bob | C63 |
| `if_indextoname` | netd iface GET | TBD | PG-29 | bob | C63 |

## T10. Hooks owned by other tickets
| function | Raven primitive today | status | PG or G id | owner lane | donor reference |
|---|---|---|---|---|---|
| `fork` | PROC_FORK* | TBD | PG-44 | C57 | process area |
| `execve` | spawn + FdMap | PARTIAL | PG-50 | C53 | process area |
| `posix_spawn` | spawn + FdMap | PARTIAL | PG-50 | C53 | process area |
| `waitpid` | PROC_WAIT | PARTIAL | PG-44 | C53 | process area |
| `getpid` | procinfo page | TBD | PG-44 | C57 | process area |
| `kill` | signal contract | TBD | PG-5 | C58 | signals area |
| `pthread_create` | THREAD_CREATE | EXISTS | - | C57 | threads area |
