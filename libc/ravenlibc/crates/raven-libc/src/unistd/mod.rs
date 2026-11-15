// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The `<unistd.h>`/`<fcntl.h>` I/O core (`02-api-surface` P50: `_exit read write close lseek
//! pread pwrite dup dup2 readv writev fcntl`), the `<sys/stat.h>` `fstatat`/`fchmod`/`mkdir`,
//! the `<unistd.h>` `fsync`/`isatty`, and the `<stdio.h>` `rename` (C64e). `open`/`openat`
//! carry a variadic mode argument that C55 owns; they are DEFERRED with the reason in
//! `expected/DEFERRED.txt`.

pub(crate) mod __raven_va_fcntl;
pub(crate) mod _exit;
pub(crate) mod close;
pub(crate) mod dup;
pub(crate) mod dup2;
pub(crate) mod fchmod;
pub(crate) mod fcntl;
pub(crate) mod fcntl_args;
pub(crate) mod fstatat;
pub(crate) mod fsync;
pub(crate) mod isatty;
pub(crate) mod lseek;
pub(crate) mod mkdir;
pub(crate) mod pread;
pub(crate) mod pwrite;
pub(crate) mod read;
pub(crate) mod readv;
pub(crate) mod rename;
pub(crate) mod write;
pub(crate) mod writev;
