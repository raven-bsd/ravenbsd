// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `syscall(2)`.
//!
//! RavenBSD has no Linux syscall table. The Linux-numbered calls the brush dependency graph
//! names (`SYS_gettid`, `SYS_memfd_create`, `SYS_execveat`, `SYS_pivot_root`) have no Raven
//! translation yet, so the C ABI symbol reports `ENOSYS` instead of forwarding a number that
//! would mean a different operation in the Raven ABI. The definition is non-variadic: the
//! C declaration is variadic, but x86-64 passes the first argument in `rdi` and the return is
//! in `rax` either way, so the extra arguments are ignored rather than read.

use core::ffi::c_long;

/// Report that the Linux-numbered syscall is not implemented on RavenBSD.
#[unsafe(no_mangle)]
pub extern "C" fn syscall(_number: c_long) -> c_long {
    crate::ffi::errno::set(raven_libc_abi::errno::Errno(
        raven_libc_abi::errno::ENOSYS as u16,
    ));
    -1
}
