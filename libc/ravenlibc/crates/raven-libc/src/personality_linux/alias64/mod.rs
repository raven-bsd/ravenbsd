// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The `*64` symbol aliases (ticket C66 5.F and `alias64.toml`).
//!
//! Every active row is one `global_asm!` `.set` so the alias and its base resolve to the
//! same address in `libc.so.1`; a pending row has no file and no export.

pub(crate) mod fstat64;
pub(crate) mod lseek64;
pub(crate) mod lstat64;
pub(crate) mod mmap64;
pub(crate) mod pread64;
pub(crate) mod pwrite64;
pub(crate) mod readdir64;
pub(crate) mod sendfile64;
pub(crate) mod stat64;
