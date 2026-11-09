// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! In-process Linux shim logic.
//!
//! The hot Linux operations that need no IPC are implemented here as pure logic over a
//! [`Host`] trait: anonymous private mappings (`mmap`/`munmap`/`mprotect`/`mremap`/`brk`),
//! futexes, time queries and the x86-64 `rt_sigframe`.
//!
//! The crate is `no_std` + `alloc`, forbids `unsafe` and calls no real syscall: every native
//! operation goes through `Host`, so the whole module is testable on the host.
//!
//! # Documented deviations
//!
//! * `PROT_WRITE | PROT_EXEC` is rejected with `EACCES` because the native kernel enforces
//!   W^X, unlike Linux.
//! * File-backed mappings return `ENODEV`; `MAP_SHARED | MAP_ANONYMOUS` returns `ENOSYS`.
//! * `mremap(..., MREMAP_MAYMOVE)` moves the VMA description but the current `Host` contract
//!   has no page-copy primitive, so page contents are not preserved by this pure layer.
//! * Shared mappings and memory objects are out of scope for this ticket.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

pub mod futex;
pub mod mm;
pub mod sigframe;
pub mod time;

use raven_abi::errno::Errno;

pub use mm::Mm;
pub use sigframe::{AltStack, Frame, Regs, SigInfo, build_rt_sigframe, restore_rt_sigframe};

/// Native operations the shim needs from the kernel or the host test harness.
pub trait Host {
    /// Map anonymous private pages. `fixed` requests exactly `addr`.
    fn map(&mut self, addr: usize, len: usize, prot: u32, fixed: bool) -> Result<usize, Errno>;

    /// Unmap a page-aligned range.
    fn unmap(&mut self, addr: usize, len: usize) -> Result<(), Errno>;

    /// Change protection on a page-aligned range.
    fn protect(&mut self, addr: usize, len: usize, prot: u32) -> Result<(), Errno>;

    /// Wait while the 32-bit word at `addr` equals `expected`.
    fn futex_wait(
        &mut self,
        addr: usize,
        expected: u32,
        timeout_ns: Option<u64>,
    ) -> Result<(), Errno>;

    /// Wake up to `n` waiters at `addr`; return the number woken.
    fn futex_wake(&mut self, addr: usize, n: u32) -> usize;

    /// Read a native clock. `native_clock` is 0 for realtime and 1 for monotonic.
    fn clock_get(&mut self, native_clock: u32) -> Result<(i64, i64), Errno>;

    /// Monotonic nanoseconds.
    fn now_ns(&mut self) -> u64;

    /// Read a 32-bit word for futex comparison.
    fn read_u32(&mut self, addr: usize) -> Result<u32, Errno>;
}
