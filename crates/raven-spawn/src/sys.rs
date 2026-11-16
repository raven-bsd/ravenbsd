// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The small raw-syscall surface the loader needs.
//!
//! `raven-libc-sys` owns the per-architecture stubs; this module keeps the shared crate free of
//! duplicated assembly while preserving the `Sys` seam used by `raven-rt` and ravenlibc.

pub use raven_libc_sys::arch::{syscall1, syscall2, syscall3, syscall4, syscall5, syscall6};

#[cfg(any(target_os = "none", test))]
use raven_abi::errno::Errno;
#[cfg(target_os = "none")]
use raven_abi::types::Handle;
#[cfg(any(target_os = "none", test))]
use raven_abi::types::Timespec;

/// Read a clock (`clock::REALTIME` or `clock::MONOTONIC`).
#[cfg(any(target_os = "none", test))]
pub fn clock_get(clock_id: u32) -> Result<Timespec, Errno> {
    raven_libc_sys::sys::clock_get(clock_id).map_err(|error| Errno(error.0))
}

/// Raw `PROC_HANDLE_INFO` (P9b): map a process handle to its kernel pid.
#[cfg(target_os = "none")]
pub const PROC_HANDLE_INFO: usize = 0x3E;

/// Map a process handle owned by the caller to its pid.
#[cfg(target_os = "none")]
pub fn pid_for_handle(handle: u32) -> Result<i32, Errno> {
    // SAFETY: PROC_HANDLE_INFO takes one scalar handle and returns the pid, or a negated errno.
    let raw = unsafe { syscall1(PROC_HANDLE_INFO, handle as usize) };
    raven_libc_sys::errno::ret_i32(raw).map_err(|error| Errno(error.0))
}

/// Wait for `handle` to exit and return the kernel's raw exit code.
#[cfg(target_os = "none")]
pub fn wait_process(handle: u32) -> Result<i32, Errno> {
    raven_libc_sys::sys::proc_wait(Handle(handle)).map_err(|error| Errno(error.0))
}

/// Close one process handle.
#[cfg(target_os = "none")]
pub fn close_handle(handle: u32) -> Result<(), Errno> {
    raven_libc_sys::sys::handle_close(Handle(handle)).map_err(|error| Errno(error.0))
}
