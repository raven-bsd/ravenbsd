// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The strong members of the weak-hook table (`donor-map/16` 3.A.4.3).
//!
//! One file per hook; every file defines exactly one exported `__rl_*_impl` symbol and one
//! `install` function. The weak cell itself is declared in the file of its caller
//! (`raven_libc_sys::weak_hook!`, C54's chosen mechanism: the spike showed that a weak
//! `global_asm!` symbol cannot be overridden by a fat-LTO consumer, so the shipped macro is the
//! function-pointer cell installed by `__raven_libc_init`). C61 calls [`install_all`] from the
//! start path; until then each cell keeps its documented dummy.

pub(crate) mod fmt_float;
pub(crate) mod fmt_wide;
pub(crate) mod funcs_on_exit;
pub(crate) mod malloc_debug;
pub(crate) mod malloc_impl;
pub(crate) mod scan_float;
pub(crate) mod stdio_exit;
pub(crate) mod tz_load;

/// Install every strong hook of the core set.
///
/// `__raven_libc_init` (C61) calls this once. It is not called on this branch, so the cells keep
/// their dummies and the strong members are not pulled into a program.
#[allow(dead_code)] // called by C61's `__raven_libc_init`
pub(crate) fn install_all() {
    funcs_on_exit::install();
    stdio_exit::install();
    fmt_float::install();
    fmt_wide::install();
    scan_float::install();
    tz_load::install();
    malloc_impl::install();
    malloc_debug::install();
}
