// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Reserved-but-not-yet-defined syscalls (`19-platform-gaps.md`).
//!
//! None of these has a number in `raven-abi`. They are safe typed stubs that return `ENOSYS`
//! until the kernel lands the primitive; the lead then edits this one file and adds the typed
//! wrapper to [`crate::sys`]. No syscall number is fabricated here.

use core::sync::atomic::AtomicU32;

use raven_abi::errno::Errno;
use raven_abi::types::{Handle, Timespec};

/// Every entry in this module is pending a kernel primitive.
pub const PENDING: bool = true;

/// `SYSINFO` record proposed by PG-21.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SysInfo {
    /// Configured CPUs.
    pub cpus_conf: u32,
    /// Online CPUs.
    pub cpus_online: u32,
    /// Total physical pages.
    pub mem_total_pages: u64,
    /// Free physical pages.
    pub mem_free_pages: u64,
    /// Boot time in nanoseconds since the Unix epoch.
    pub boot_time_ns: u64,
}

/// Set the calling thread's TLS base (PG-1, proposed `THREAD_SET_TLS` 0x23).
#[doc = "PG-1"]
pub fn thread_set_tls(_tp: *mut u8) -> Result<(), Errno> {
    Err(Errno::ENOSYS)
}

/// Kick a thread with signal bits (PG-7, proposed `THREAD_KICK` 0x81).
#[doc = "PG-7"]
pub fn thread_kick(_thread: Handle, _bits: u32) -> Result<(), Errno> {
    Err(Errno::ENOSYS)
}

/// Fill `buf` from the kernel CSPRNG (PG-52, proposed `GETRANDOM` 0x08).
#[doc = "PG-52"]
pub fn getrandom(_buf: &mut [u8], _flags: u32) -> Result<usize, Errno> {
    Err(Errno::ENOSYS)
}

/// Poll a futex once without sleeping (PG-3, proposed futex v2 `HAS_TIMEOUT`).
#[doc = "PG-3"]
pub fn poll_once(_addr: &AtomicU32, _expected: u32) -> Result<(), Errno> {
    Err(Errno::ENOSYS)
}

/// Fork the calling process (PG-45, proposed `PROC_FORK` 0x34).
#[doc = "PG-45"]
pub fn proc_fork(_src: Handle, _flags: usize) -> Result<Handle, Errno> {
    Err(Errno::ENOSYS)
}

/// Set a clock (PG-17, proposed `CLOCK_SET` 0x0A).
#[doc = "PG-17"]
pub fn clock_set(_clock_id: u32, _time: Timespec) -> Result<(), Errno> {
    Err(Errno::ENOSYS)
}

/// Read CPU and memory information (PG-21, proposed `SYSINFO` 0x09).
#[doc = "PG-21"]
pub fn sysinfo() -> Result<SysInfo, Errno> {
    Err(Errno::ENOSYS)
}

/// Give memory advice (PG-15, proposed `MADVISE` 0x44).
#[doc = "PG-15"]
pub fn madvise(_proc_: Handle, _addr: usize, _len: usize, _advice: u32) -> Result<(), Errno> {
    Err(Errno::ENOSYS)
}

#[cfg(test)]
#[path = "pending_ut.rs"]
mod tests;
