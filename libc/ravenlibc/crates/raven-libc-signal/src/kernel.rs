// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The kernel seam [`SigKernel`] and the production `SysKernel`.
//!
//! The kernel only kicks (PG-7): it never reads a mask or a disposition. Every missing primitive
//! is represented by a capability flag and a documented fallback; `SysKernel` forwards to
//! `raven-libc-sys` where a wrapper exists and returns `ENOSYS` otherwise. No syscall number is
//! invented here: `SIGRETURN` 0x82 and the kick bits are the labelled PROPOSED PG-7 values.

use raven_libc_abi::errno::Errno;
use raven_libc_abi::structs::SiginfoT;

use crate::errors::{EINVAL_ERR, ENOSYS_ERR, ETIMEDOUT_ERR};
use crate::raw;

/// Kick bit 0: look at the pending sets.
pub const KICK_CHECK: u32 = 1;
/// Kick bit 1: cancellation (hand to C57's `on_kick`).
pub const KICK_CANCEL: u32 = 2;
/// Kick bit 2: `synccall`.
pub const KICK_SYNCCALL: u32 = 4;
/// Kick bit 3: POSIX timer.
pub const KICK_TIMER: u32 = 8;
/// Every bit this crate may set (PROPOSED, RD-new 2).
pub const KICK_KNOWN: u32 = KICK_CHECK | KICK_CANCEL | KICK_SYNCCALL | KICK_TIMER;

/// The synchronous fault kinds of PG-7.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum FaultKind {
    /// Page fault with no mapping.
    PageNomap = 1,
    /// Page fault with a permission violation.
    PagePerm = 2,
    /// Integer divide error.
    Divide = 3,
    /// Illegal instruction.
    Illegal = 4,
    /// Misaligned or bus error.
    Misalign = 5,
    /// Breakpoint.
    Breakpoint = 6,
}

impl FaultKind {
    /// Decode a kernel record kind.
    #[must_use]
    pub const fn from_u32(kind: u32) -> Option<Self> {
        match kind {
            1 => Some(Self::PageNomap),
            2 => Some(Self::PagePerm),
            3 => Some(Self::Divide),
            4 => Some(Self::Illegal),
            5 => Some(Self::Misalign),
            6 => Some(Self::Breakpoint),
            _ => None,
        }
    }
}

/// The kernel's fault record (PROPOSED PG-7).
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FaultRecord {
    /// One of [`FaultKind`].
    pub kind: u32,
    /// The faulting address (`si_addr`).
    pub addr: usize,
    /// The interrupted program counter.
    pub pc: usize,
}

/// What the kernel can do today.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SigCaps {
    /// PG-7 `SIG_SETUP`, `THREAD_KICK`, `SIGRETURN` and the entry stack.
    pub entry: bool,
    /// PG-7 `THREAD_KICK`.
    pub kick: bool,
    /// PG-7 `SIGRETURN` (0x82).
    pub sigreturn: bool,
    /// PG-7 optional `SIGRESEND` (0x84).
    pub resend: bool,
    /// PG-8 `-EINTR` and the sticky kick word.
    pub eintr: bool,
    /// PG-44 procd `kill`.
    pub proc_kill: bool,
    /// PG-10 stop/continue.
    pub stop: bool,
    /// PG-12 FP/SIMD state in the frame.
    pub fp_in_frame: bool,
}

impl SigCaps {
    /// The production capability set: no PG-7, PG-8, PG-10, PG-12 or PG-44 primitive exists.
    pub const PRODUCTION: Self = Self {
        entry: false,
        kick: false,
        sigreturn: false,
        resend: false,
        eintr: false,
        proc_kill: false,
        stop: false,
        fp_in_frame: false,
    };

    /// Every capability available (host fake only).
    pub const HOST: Self = Self {
        entry: true,
        kick: true,
        sigreturn: true,
        resend: true,
        eintr: true,
        proc_kill: true,
        stop: true,
        fp_in_frame: false,
    };
}

impl Default for SigCaps {
    fn default() -> Self {
        Self::PRODUCTION
    }
}

/// The seam every signal operation uses; the host fake implements the same trait.
pub trait SigKernel: Sync {
    /// The capability set.
    fn caps(&self) -> SigCaps;
    /// The calling thread's thread pointer.
    fn thread_pointer(&self) -> usize;
    /// The calling thread's kernel handle (0 when unassigned).
    fn thread_handle(&self) -> u32;
    /// Give up the CPU.
    fn yield_now(&self);
    /// Monotonic nanoseconds.
    fn now_ns(&self) -> u64;
    /// PG-7 `SIG_SETUP`.
    fn sig_setup(&self, entry: usize, ctx_area_offset: usize, flags: u32) -> Result<(), Errno>;
    /// PG-7 `THREAD_KICK`.
    fn kick(&self, handle: u32, bits: u32) -> Result<(), Errno>;
    /// PG-7 `SIGRETURN`; never returns.
    fn sigreturn(&self) -> !;
    /// PG-7 optional `SIGRESEND`.
    fn resend(&self, sig: i32, info: &SiginfoT, mask: u64) -> Result<(), Errno>;
    /// PG-44 procd `kill`/`killpg`/`sigqueue`.
    fn proc_kill(&self, pid: i32, sig: i32, info: Option<&SiginfoT>) -> Result<(), Errno>;
    /// Take one process-directed pending signal whose bit is in `deliverable`.
    fn take_process_pending(&self, deliverable: u64) -> Option<(i32, SiginfoT)>;
    /// Wait for a kick; `None` waits forever (PG-8).
    fn wait_for_kick(&self, deadline_ns: Option<u64>) -> Result<(), Errno>;
    /// Apply a default terminate/core action; never returns.
    fn terminate(&self, sig: i32, core: bool) -> !;
}

/// The production implementation over `raven-libc-sys`.
#[derive(Clone, Copy, Debug, Default)]
pub struct SysKernel;

impl SysKernel {
    /// A production kernel seam.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    fn notify(pid: i32, sig: i32, info: Option<&SiginfoT>) -> Result<(), Errno> {
        let _ = (pid, sig, info);
        Err(ENOSYS_ERR)
    }
}

impl SigKernel for SysKernel {
    fn caps(&self) -> SigCaps {
        SigCaps::PRODUCTION
    }

    fn thread_pointer(&self) -> usize {
        raven_libc_sys::tp::thread_pointer() as usize
    }

    fn thread_handle(&self) -> u32 {
        raw::thread_handle(self.thread_pointer())
    }

    fn yield_now(&self) {
        raven_libc_sys::sys::yield_now().ok();
    }

    fn now_ns(&self) -> u64 {
        match raven_libc_sys::sys::clock_get(1) {
            Ok(time) => (time.tv_sec.max(0) as u64)
                .saturating_mul(1_000_000_000)
                .saturating_add(time.tv_nsec.max(0) as u64),
            Err(_) => 0,
        }
    }

    fn sig_setup(&self, entry: usize, ctx_area_offset: usize, flags: u32) -> Result<(), Errno> {
        // PG-7 is absent: remember the address for the shell and report the gap. No number is
        // fabricated; the shell checks `caps().entry` before calling this.
        let _ = (ctx_area_offset, flags);
        raw::set_entry_function(entry);
        Err(ENOSYS_ERR)
    }

    fn kick(&self, handle: u32, bits: u32) -> Result<(), Errno> {
        let _ = (handle, bits);
        Err(ENOSYS_ERR)
    }

    fn sigreturn(&self) -> ! {
        raw::sigreturn()
    }

    fn resend(&self, sig: i32, info: &SiginfoT, mask: u64) -> Result<(), Errno> {
        let _ = (sig, info, mask);
        Err(ENOSYS_ERR)
    }

    fn proc_kill(&self, pid: i32, sig: i32, info: Option<&SiginfoT>) -> Result<(), Errno> {
        Self::notify(pid, sig, info)
    }

    fn take_process_pending(&self, deliverable: u64) -> Option<(i32, SiginfoT)> {
        let _ = deliverable;
        None
    }

    fn wait_for_kick(&self, deadline_ns: Option<u64>) -> Result<(), Errno> {
        loop {
            let tp = self.thread_pointer();
            let kick = raw::with_state(tp, |state| state.kick());
            if kick != 0 {
                raw::with_state(tp, |state| {
                    state.take_kick();
                });
                return Ok(());
            }
            if let Some(deadline) = deadline_ns
                && self.now_ns() >= deadline
            {
                return Err(ETIMEDOUT_ERR);
            }
            // No PG-8: a blocking call cannot be interrupted. Sleep in small slices so a kick
            // (which PG-7 would set) is still observed when it eventually exists.
            raven_libc_sys::sys::nanosleep(1_000_000).ok();
        }
    }

    fn terminate(&self, sig: i32, core: bool) -> ! {
        // "Core" sets the core flag in the wait status; v1 writes no core file.
        let status = 128i32.saturating_add(sig);
        let _ = core;
        if raw::call_exit_hook(status) {
            // The hook is `-> !`; this is unreachable, but keeps the type honest.
            core::hint::spin_loop();
        }
        raven_libc_sys::sys::exit(status)
    }
}

/// Seconds/nanoseconds to monotonic nanoseconds (the `raven-abi` record is not nameable here).
#[must_use]
pub fn nanosecond_value(seconds: i64, nanos: i64) -> u64 {
    (seconds.max(0) as u64)
        .saturating_mul(1_000_000_000)
        .saturating_add(nanos.max(0) as u64)
}

/// Validate a kick bit set (only the four PROPOSED bits).
#[must_use]
pub fn valid_kick_bits(bits: u32) -> bool {
    bits & !KICK_KNOWN == 0
}

/// `EINVAL` for a bad signal number.
#[must_use]
pub const fn bad_signal() -> Errno {
    EINVAL_ERR
}

#[cfg(test)]
#[path = "kernel_ut.rs"]
mod kernel_ut;
