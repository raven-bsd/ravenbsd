// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K529 fixture syscall surface: the raw calls the authority qtest needs.
//!
//! The fixture is standalone (no libc); every call is the native number the kernel dispatches.
//! `CLOCK_SET` and the `DTRACE_*` controls are raw constants because the ticket does not own the
//! ABI contract crate, matching the existing dtrace fixture.

#![no_std]

use core::arch::asm;

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Timespec, clock};

/// Raw `clock_set(clock_id, ts)` (S72 reviewer-assigned).
pub const CLOCK_SET: usize = 0x09;
/// `dtrace_probe_list(out, max)`.
pub const DTRACE_PROBE_LIST: usize = 0x90;
/// `dtrace_enable(probe, flags, event)`.
pub const DTRACE_ENABLE: usize = 0x91;
/// `dtrace_disable(probe)`.
pub const DTRACE_DISABLE: usize = 0x92;
/// `dtrace_read(out, max)`.
pub const DTRACE_READ: usize = 0x93;
/// `dtrace_stats(out, max)`.
pub const DTRACE_STATS: usize = 0x94;
/// `dtrace_clear()`.
pub const DTRACE_CLEAR: usize = 0x95;

/// The static syscall probe id (`syscall::entry`).
pub const SYSCALL_PROBE: u32 = 1;

/// One drainable DTrace record, matching `raven_dtrace_core::model::Record`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Record {
    /// Global sequence number.
    pub sequence: u64,
    /// Kernel timestamp.
    pub timestamp_ns: u64,
    /// First syscall argument.
    pub arg0: u64,
    /// Second syscall argument.
    pub arg1: u64,
    /// Third syscall argument.
    pub arg2: u64,
    /// Fourth syscall argument.
    pub arg3: u64,
    /// Syscall number.
    pub syscall: u32,
    /// Thread id.
    pub tid: u32,
    /// CPU index.
    pub cpu: u16,
    /// Probe id.
    pub probe: u16,
}

/// One per-CPU ring statistic, matching `raven_dtrace_core::model::CpuStats`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CpuStats {
    /// Records produced.
    pub produced: u64,
    /// Records consumed.
    pub consumed: u64,
    /// Records dropped.
    pub dropped: u64,
    /// Ring capacity.
    pub capacity: u64,
}

/// One probe descriptor, matching `raven_dtrace_core::model::ProbeInfo`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProbeInfo {
    /// Probe id.
    pub id: u32,
    /// Provider name.
    pub provider: [u8; 16],
    /// Probe name.
    pub name: [u8; 32],
}

const _: () = assert!(core::mem::size_of::<Record>() == 64);
const _: () = assert!(core::mem::size_of::<CpuStats>() == 32);
const _: () = assert!(core::mem::size_of::<ProbeInfo>() == 52);

#[cfg(target_arch = "x86_64")]
#[inline(always)]
unsafe fn syscall6(
    number: usize,
    a0: usize,
    a1: usize,
    a2: usize,
    a3: usize,
    a4: usize,
    a5: usize,
) -> isize {
    let result: isize;
    // SAFETY: scalar syscall arguments; the kernel validates handles and pointers.
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") number => result,
            in("rdi") a0,
            in("rsi") a1,
            in("rdx") a2,
            in("r10") a3,
            in("r8") a4,
            in("r9") a5,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    result
}

/// Issue one native syscall; scalar arguments only.
#[cfg(target_arch = "x86_64")]
pub fn sys(
    number: usize,
    a0: usize,
    a1: usize,
    a2: usize,
    a3: usize,
    a4: usize,
    a5: usize,
) -> Result<usize, Errno> {
    // SAFETY: scalar syscall arguments; the kernel validates handles and pointers.
    let raw = unsafe { syscall6(number, a0, a1, a2, a3, a4, a5) };
    if raw < 0 {
        Err(Errno(raw.unsigned_abs() as u16))
    } else {
        Ok(raw as usize)
    }
}

/// Read the realtime clock.
///
/// # Errors
/// [`Errno`] from `CLOCK_GET`.
pub fn clock_get() -> Result<Timespec, Errno> {
    let mut value = Timespec::default();
    sys(
        Syscall::CLOCK_GET as usize,
        clock::REALTIME as usize,
        &mut value as *mut Timespec as usize,
        0,
        0,
        0,
        0,
    )?;
    Ok(value)
}

/// Set the realtime clock from seconds/nanoseconds.
///
/// # Errors
/// [`Errno`] from `CLOCK_SET`.
pub fn clock_set(seconds: i64, nanos: i64) -> Result<(), Errno> {
    let value = Timespec {
        tv_sec: seconds,
        tv_nsec: nanos,
    };
    sys(
        CLOCK_SET,
        clock::REALTIME as usize,
        &value as *const Timespec as usize,
        0,
        0,
        0,
        0,
    )
    .map(|_| ())
}

/// Issue one DTrace control call with up to three arguments.
///
/// # Errors
/// [`Errno`] from the control syscall.
pub fn dtrace(number: usize, a0: usize, a1: usize, a2: usize) -> Result<usize, Errno> {
    sys(number, a0, a1, a2, 0, 0, 0)
}
