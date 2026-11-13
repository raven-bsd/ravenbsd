# raven-libc-platform

SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

The POSIX platform layer of ravenlibc: the process-local fd table and OFDs, path resolution and
cwd, the scheme clients, `poll`/`select`/`epoll` over `EVQ`, `ioctl`/termios dispatch, `stat` and
`statvfs` conversion, `sysconf`/`pathconf`/`confstr`, `sysctl`, entropy, memory glue and the
registered class-A stubs.

The contract is `ravenbsd-prjmgmt/architecture/raven-libc/donor-map/11-syscall-platform.md`.
`FUNCTIONS.md` is the verified T1-T9 function table; `src/registry_ut.rs` parses it and fails when
the document, the registry or the source tree drift apart.

## Seams

* `SysBackend` is the only way to the kernel; `RealSys` uses `raven-libc-sys`, `FakeSys` is the
  deterministic host world in `src/backend.rs` (also used by `tests/support/fake_sys.rs`).
* `Lock` is supplied by the shell; tests use `NoLock`. No lock is held across a backend call.
* `#![forbid(unsafe_code)]`, `no_std` plus `alloc`. Raw pointers and C-ABI conversions belong to
  the shell.

## Status

`cargo test -p raven-libc-platform` (and `--features fdsan`) covers the fd table, fdsan, path
resolution (80+ cases), open flags, I/O, stat conversion, poll/epoll, sysconf/pathconf/confstr,
sysctl, entropy, memory glue, the unimplemented policy and the `FUNCTIONS.md` coverage check.
